// quality - breadbin's code, measured: lines of code by part, ESLint's
// findings (mistakes, and the warnings for long, deep or branchy functions),
// exports nobody imports, very long lines, clippy's warnings. Each run prints the numbers and
// what changed since the last run, and adds a line to quality-history.jsonl.
//
//   node tools/quality.mjs            (make quality)
//   node tools/quality.mjs --no-rust  (skip clippy, which builds the crate)

import fs from 'node:fs';
import path from 'node:path';
import { execFileSync, spawnSync } from 'node:child_process';

const ROOT = path.resolve(path.dirname(new URL(import.meta.url).pathname), '..');
const HISTORY = path.join(ROOT, 'tools/quality-history.jsonl');
const SKIP = /node_modules|\/vendor\/|\/cc65\/|\/examples\/|acme\.mjs$|\/target\//;

const walk = (dir) => fs.readdirSync(path.join(ROOT, dir), { withFileTypes: true }).flatMap((e) => {
  const p = `${dir}/${e.name}`;
  if (SKIP.test(`/${p}`)) return [];
  return e.isDirectory() ? walk(p) : [p];
});
const files = [...walk('docs'), ...walk('tools'), ...walk('crates')];
const read = (p) => fs.readFileSync(path.join(ROOT, p), 'utf8');

// the parts, and the files in each
const PARTS = {
  player: (p) => /^docs\/play\/[^/]+\.(js|mjs)$/.test(p),
  studio: (p) => /^docs\/play\/studio\/.*\.js$/.test(p) && !p.includes('/runtime/'),
  runtime: (p) => /^docs\/play\/studio\/runtime\/.*\.(c|h|s)$/.test(p),
  css: (p) => /^docs\/.*\.css$/.test(p),
  html: (p) => /^docs\/.*\.html$/.test(p),
  rust: (p) => /^crates\/.*\.rs$/.test(p),
  tools: (p) => /^tools\/.*\.(mjs|js|py|sh)$/.test(p) && !p.includes('/test/'),
  tests: (p) => /^tools\/.*\/test\/.*\.mjs$/.test(p),
};

/** Lines of code and of comments in `text` (a C-like or # language). */
function count(text, p) {
  let code = 0;
  let comment = 0;
  let inBlock = false;
  const hash = /\.(py|sh)$/.test(p);
  const semi = /\.s$/.test(p);
  for (const raw of text.split('\n')) {
    const l = raw.trim();
    if (!l) continue;
    if (inBlock) {
      comment++;
      if (l.includes('*/')) inBlock = false;
    } else if (l.startsWith('//') || (hash && l.startsWith('#')) || (semi && l.startsWith(';'))) comment++;
    else if (l.startsWith('/*')) {
      comment++;
      inBlock = !l.includes('*/');
    } else code++;
  }
  return { code, comment };
}

const parts = {};
for (const [name, test] of Object.entries(PARTS)) {
  const mine = files.filter(test);
  const sum = { files: mine.length, code: 0, comment: 0, biggest: '' };
  let most = 0;
  for (const p of mine) {
    const c = count(read(p), p);
    sum.code += c.code;
    sum.comment += c.comment;
    if (c.code > most) [most, sum.biggest] = [c.code, `${p.split('/').pop()} ${c.code}`];
  }
  parts[name] = sum;
}

// ESLint: errors (mistakes) and the measures' warnings, by rule
const eslintBin = path.join(ROOT, 'tools/studio/node_modules/.bin/eslint');
let lint = { errors: 0, warnings: 0, rules: {} };
if (fs.existsSync(eslintBin)) {
  let out;
  try {
    out = execFileSync(eslintBin, ['docs/play', 'tools', '--format', 'json'], { cwd: ROOT, encoding: 'utf8', maxBuffer: 1 << 26 });
  } catch (e) {
    out = e.stdout; // eslint exits 1 when it finds errors
  }
  for (const f of JSON.parse(out)) {
    for (const m of f.messages) {
      lint[m.severity === 2 ? 'errors' : 'warnings']++;
      lint.rules[m.ruleId] = (lint.rules[m.ruleId] || 0) + 1;
    }
  }
} else lint = null;

// exports that no other file mentions (dead code, or an API kept for nothing)
const js = files.filter((p) => /^docs\/play\/.*\.(js|mjs)$/.test(p));
const everything = files.filter((p) => /\.(js|mjs|html)$/.test(p)).map((p) => [p, read(p)]);
const unused = [];
for (const p of js) {
  for (const m of read(p).matchAll(/^export (?:async )?(?:function\*? |const |let |class )(\w+)/gm)) {
    const re = new RegExp(`\\b${m[1]}\\b`);
    if (!everything.some(([q, t]) => q !== p && re.test(t))) unused.push(`${p.replace('docs/play/', '')}: ${m[1]}`);
  }
}

// lines over 180 characters in the page's JavaScript (hard to read side by side)
const longLines = js.reduce((n, p) => n + read(p).split('\n').filter((l) => l.length > 180).length, 0);

// clippy's warnings (the crate built for checking)
let clippy = null;
if (!process.argv.includes('--no-rust')) {
  // (clippy writes its findings to stderr; every target, the tests too)
  const r = spawnSync('cargo', ['clippy', '--message-format', 'short', '--all-targets'], { cwd: ROOT, encoding: 'utf8' });
  if (r.status === 0) clippy = new Set((r.stderr.match(/^\S+:\d+:\d+: warning: .*$/gm) || [])).size;
}

const git = (...a) => {
  try {
    return execFileSync('git', a, { cwd: ROOT, encoding: 'utf8' }).trim();
  } catch {
    return '';
  }
};
const now = {
  at: new Date().toISOString(),
  commit: git('rev-parse', '--short', 'HEAD') + (git('status', '--porcelain') ? '+' : ''),
  parts: Object.fromEntries(Object.entries(parts).map(([k, v]) => [k, { files: v.files, code: v.code, comment: v.comment }])),
  lint: lint && { errors: lint.errors, warnings: lint.warnings, rules: lint.rules },
  unusedExports: unused.length,
  longLines,
  clippy,
};

// the report, with what changed since the last run
const history = fs.existsSync(HISTORY) ? fs.readFileSync(HISTORY, 'utf8').trim().split('\n').filter(Boolean).map((l) => JSON.parse(l)) : [];
const last = history.at(-1);
const delta = (a, b) => (b === undefined || b === null || a === b ? '' : ` (${a - b > 0 ? '+' : ''}${a - b})`);
const rows = Object.entries(parts).map(([k, v]) => `  ${k.padEnd(8)} ${String(v.files).padStart(4)} files ${String(v.code).padStart(6)} code${delta(v.code, last?.parts?.[k]?.code).padEnd(9)} ${String(v.comment).padStart(5)} comment   biggest: ${v.biggest}`);
const total = Object.values(parts).reduce((a, v) => a + v.code, 0);
const lastTotal = last && Object.values(last.parts).reduce((a, v) => a + v.code, 0);
console.log(`breadbin quality, ${now.at.slice(0, 16).replace('T', ' ')} (${now.commit})\n`);
console.log(rows.join('\n'));
console.log(`  ${'total'.padEnd(8)}            ${String(total).padStart(6)} code${delta(total, lastTotal)}\n`);
if (lint) {
  console.log(`ESLint: ${lint.errors} errors${delta(lint.errors, last?.lint?.errors)}, ${lint.warnings} warnings${delta(lint.warnings, last?.lint?.warnings)}`);
  for (const [r, n] of Object.entries(lint.rules).sort((a, b) => b[1] - a[1])) console.log(`  ${r}: ${n}${last?.lint ? delta(n, last.lint.rules[r] ?? 0) : ''}`);
}
console.log(`Unused exports: ${unused.length}${delta(unused.length, last?.unusedExports)}`);
for (const u of unused) console.log(`  ${u}`);
console.log(`Lines over 180 characters: ${longLines}${delta(longLines, last?.longLines)}`);
if (clippy !== null) console.log(`Clippy warnings: ${clippy}${delta(clippy, last?.clippy)}`);
fs.appendFileSync(HISTORY, `${JSON.stringify(now)}\n`);
