// builder - the page's side of the build worker (worker.js): build a C
// project, assemble an ACME one, or make a PSID of the music. Results are
// the same shape for both languages: { ok, prg, diagnostics, log, memory, ms }.

import { generate } from './codegen.js';
import { meta, hasAssets } from './project.js';
import { parseAcme } from './acme-diag.js';

let worker = null;
let next = 1;
const waiting = new Map();

function call(msg) {
  worker ??= (() => {
    const w = new Worker(new URL('./worker.js', import.meta.url), { type: 'module' });
    w.onmessage = ({ data }) => {
      const p = waiting.get(data.id);
      waiting.delete(data.id);
      if (!p) return;
      data.error ? p.reject(new Error(data.error)) : p.resolve(data);
    };
    w.onerror = (e) => {
      for (const p of waiting.values()) p.reject(new Error(e.message || 'the build worker failed to start'));
      waiting.clear();
    };
    return w;
  })();
  const id = next++;
  return new Promise((resolve, reject) => {
    waiting.set(id, { resolve, reject });
    worker.postMessage({ ...msg, id });
  });
}

/** Start the worker and fetch the compiler now, so the first Run is quick. */
export const warmUp = () => call({ type: 'warm' }).catch(() => {});

/** Only what the compiler reads: sources, headers, assets; not docs or binaries it doesn't need. */
const forBuild = (files) => Object.fromEntries(Object.entries(files).filter(([p]) => /\.(c|h|s|inc)$/i.test(p)));

/**
 * Build `files`. For C, gen/ is made first and returned as `generated`, so
 * the caller can show it and write it to the project folder.
 */
export async function build(files) {
  const m = meta(files);
  if (m.lang === 'acme') return acme(files, m.entry);
  // plain cc65 (no assets): no runtime, and cc65's usual RAM, to $D000
  if (!hasAssets(files)) {
    const { result } = await call({ type: 'build', files: forBuild(files), options: { himem: 0xd000 } });
    return { ...result, generated: {} };
  }
  let generated;
  try {
    generated = await generate(files);
  } catch (e) {
    return { ok: false, prg: null, diagnostics: [{ file: /assets\/\w+\.json/.exec(e.message)?.[0] || '', line: 0, severity: 'error', message: e.message, tool: 'studio' }], log: e.message, memory: null, generated: {} };
  }
  const { result } = await call({ type: 'build', files: { ...forBuild(files), ...generated } });
  const warnings = (generated.warnings || []).map((message) => ({ file: 'assets/tiles.json', line: 0, severity: 'warning', message, tool: 'studio' }));
  return { ...result, diagnostics: [...warnings, ...result.diagnostics], generated };
}

/** The text of cc65's headers `names` ('stdlib.h', ...): { name: text } (the missing left out). */
export async function headers(names) {
  return (await call({ type: 'headers', names })).headers;
}

/** A PSID of `music` (assets/music.json parsed), for the MUSIC editor. */
export async function psid(music, metaInfo = {}) {
  return (await call({ type: 'psid', music, meta: metaInfo })).bytes;
}

// ── ACME, for the assembly projects (the classic intros) ─────────────────

let acmeModule = null;

/** Assemble an ACME project from `entry`. ACME is small: it runs on the page. */
async function acme(files, entry) {
  const t0 = Date.now();
  acmeModule ??= import('../acme.mjs').then((m) => m.default);
  const log = [];
  // a fresh instance per run: ACME keeps global state
  const a = await (await acmeModule)({ print: (s) => log.push(s), printErr: (s) => log.push(s) });
  for (const [path, data] of Object.entries(files)) {
    a.FS.mkdirTree(`/src/${path}`.replace(/\/[^/]*$/, ''));
    a.FS.writeFile(`/src/${path}`, data);
  }
  a.FS.chdir('/src');
  try {
    a.callMain(['-f', 'cbm', '-o', '/out.prg', entry]);
  } catch {}
  let prg = null;
  try {
    prg = a.FS.readFile('/out.prg');
  } catch {}
  const text = log.join('\n');
  const diagnostics = parseAcme(text);
  const ok = !!prg && !diagnostics.some((d) => d.severity === 'error');
  const memory = prg ? { segments: [], used: prg.length - 2, total: 0xffff - 0x0801, free: 0 } : null;
  return { ok, prg: ok ? prg : null, diagnostics, log: text, memory, ms: Date.now() - t0, generated: {} };
}
