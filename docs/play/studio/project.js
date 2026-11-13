// project - a Studio project: a folder of files, as on disk.
//
//   project.json        { "name": "Bubbler", "lang": "c", "about": "..." }
//   src/*.c, *.h, *.s   the program (lang "c": cc65; lang "acme": ACME assembly)
//   assets/*.json       sprites, tiles, map and music, from the editors
//   gen/                written by every build (the runtime and the assets as C)
//   Makefile            builds the same program with a native cc65
//   README.md           what the project is
//
// In memory a project is { meta, files }: `files` maps paths to text (or
// bytes for binary files), `meta` is project.json parsed.

export const TEXT = /\.(c|h|s|asm|a|inc|json|md|txt|cfg|mk|bas)$|(^|\/)(Makefile|README|LICENSE|\.gitignore)$/i;
export const isText = (path) => TEXT.test(path);
/**
 * What the Studio neither shows nor reads back from disk: folders of tools and
 * builds, and what builds make (game.prg, game.map, object files), so a
 * `make` in the folder isn't taken for an edit. (Other .prg files stay: an
 * ACME project may include one.)
 */
export const SKIP = /(^|\/)(\.git|node_modules|\.build|build)(\/|$)|(^|\/)game\.(prg|map)$|\.o$/;
/** Written by builds: shown, but not edited (an edit would be overwritten). */
export const isGenerated = (path) => /^gen\//.test(path);

const dec = new TextDecoder();

/**
 * Text files as strings, the rest as bytes. With `unwrap` (for a zip of
 * "bubbler/src/main.c"...), a top folder that holds everything is dropped.
 */
export function normalise(files, { unwrap = false } = {}) {
  const paths = Object.keys(files).filter((p) => !SKIP.test(p));
  const tops = new Set(paths.map((p) => p.split('/')[0]));
  const strip = unwrap && tops.size === 1 && paths.every((p) => p.includes('/')) ? `${[...tops][0]}/` : '';
  const out = {};
  for (const p of paths) {
    const d = files[p];
    out[p.slice(strip.length)] = isText(p) && typeof d !== 'string' ? dec.decode(d) : d;
  }
  return out;
}

/** project.json of `files`, with defaults; lang is guessed from the sources if unsaid. */
export function meta(files, { name = 'untitled' } = {}) {
  let m = {};
  try {
    m = JSON.parse(files['project.json'] || '{}');
  } catch {}
  const paths = Object.keys(files);
  const lang = m.lang || (paths.some((p) => /\.c$/.test(p)) ? 'c' : 'acme');
  // without a project.json (a plain cc65 project): its main.c, wherever it is
  const entry = lang === 'c'
    ? ('src/main.c' in files ? 'src/main.c' : paths.find((p) => /(^|\/)main\.c$/.test(p)) || paths.find((p) => /\.c$/.test(p)) || 'src/main.c')
    : paths.find((p) => /\.(asm|a|s)$/i.test(p)) || 'main.asm';
  return { name, entry, ...m, lang };
}

/** The asset editors' files. */
export const ASSET_PATHS = ['assets/sprites.json', 'assets/tiles.json', 'assets/map.json', 'assets/music.json'];

/** Does the project have the Studio's assets (and so its runtime), or is it plain cc65? */
export const hasAssets = (files) => ASSET_PATHS.some((p) => p in files);

/** Has the folder's text anything to build: C or assembly? */
export const hasSources = (files) => Object.keys(files).some((p) => /\.(c|s|asm|a)$/i.test(p));

/** The order the file list shows: project.json, sources, assets, the rest, gen/ last. */
export function sortPaths(paths) {
  const rank = (p) => (p === 'project.json' ? 0 : /^src\//.test(p) ? 1 : /^assets\//.test(p) ? 3 : isGenerated(p) ? 5 : /^tests?\//.test(p) ? 2 : 4);
  return [...paths].sort((a, b) => rank(a) - rank(b) || a.localeCompare(b));
}

/** A file name for a project's downloads: "Bubbler" -> "bubbler". */
export const slug = (name) => String(name || 'project').toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, '') || 'project';

/**
 * JSON for the asset files, laid out to read well and diff well in git:
 * pixel rows one per line, lists of numbers on one line, small objects of
 * plain values (an instrument) on one line, runs of nulls together.
 */
export function pretty(value, indent = '') {
  const inner = `${indent}  `;
  const flat = (v) => v === null || typeof v !== 'object';
  if (flat(value)) return JSON.stringify(value);
  if (Array.isArray(value)) {
    if (!value.length) return '[]';
    if (value.every((v) => typeof v === 'number' || (Array.isArray(v) && v.every((x) => typeof x === 'number')))) {
      const one = `[${value.map((v) => (Array.isArray(v) ? `[${v.join(', ')}]` : v)).join(', ')}]`;
      if (one.length < 100) return one;
    }
    const lines = [];
    let nulls = [];
    const flush = () => nulls.length && (lines.push(`${inner}${nulls.join(', ')}`), (nulls = []));
    for (const v of value) {
      if (v === null) {
        nulls.push('null');
        if (nulls.length === 16) flush();
        continue;
      }
      flush();
      lines.push(inner + pretty(v, inner));
    }
    flush();
    return `[\n${lines.join(',\n')}\n${indent}]`;
  }
  const entries = Object.entries(value);
  if (!entries.length) return '{}';
  const one = `{ ${entries.map(([k, v]) => `${JSON.stringify(k)}: ${JSON.stringify(v)}`).join(', ')} }`;
  if (entries.every(([, v]) => flat(v) || (Array.isArray(v) && v.length <= 4 && v.every(flat))) && one.length < 140) return one;
  return `{\n${entries.map(([k, v]) => `${inner}${JSON.stringify(k)}: ${pretty(v, inner)}`).join(',\n')}\n${indent}}`;
}
