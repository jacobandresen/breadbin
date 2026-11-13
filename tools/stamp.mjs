// Stamp a deploy: every relative module import, worker, worklet and wasm URL
// in docs/ (and the scripts and stylesheets in its HTML) gets ?v=<version>.
//
//   node tools/stamp.mjs docs <version>
//
// Why: GitHub Pages lets browsers keep each file for 10 minutes, one by one.
// The Studio is many small modules, imported lazily, so a browser right after
// a deploy could join a new studio.js to an old code-editor.js and break
// ("this.code.setBreakpoints is not a function"), even through a hard reload.
// Stamped, each deploy's files only ever load each other. Run on the copy
// that is deployed (pages.yml), not on the repository.
import fs from 'node:fs';
import path from 'node:path';

const [root = 'docs', version = String(Date.now())] = process.argv.slice(2);
const v = encodeURIComponent(version);

// './x.js', '../x.mjs', './breadbin.wasm' after from, import, import(, new URL(
const JS = /((?:\bfrom|\bimport)\s*\(?\s*|\bnew URL\(\s*)(['"])(\.{1,2}\/[^'"?#\s]+\.(?:m?js|wasm))\2/g;
// src="app.js", href="style.css" (relative, not http:, not data:)
const HTML = /(\b(?:src|href)=)(["'])((?!https?:|data:|#|\/\/|mailto:)[^"'?#\s]+\.(?:m?js|css))\2/g;

export function stampJs(text) {
  return text.replace(JS, (_, pre, q, url) => `${pre}${q}${url}?v=${v}${q}`);
}
export function stampHtml(text) {
  return text.replace(HTML, (_, pre, q, url) => `${pre}${q}${url}?v=${v}${q}`);
}

function walk(dir) {
  for (const e of fs.readdirSync(dir, { withFileTypes: true })) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) {
      if (e.name !== 'vendor' && e.name !== 'node_modules') walk(p);
      continue;
    }
    const stamp = /\.m?js$/.test(e.name) ? stampJs : /\.html$/.test(e.name) ? stampHtml : null;
    if (!stamp) continue;
    const text = fs.readFileSync(p, 'utf8');
    const out = stamp(text);
    if (out !== text) fs.writeFileSync(p, out);
  }
}

if (import.meta.url === `file://${process.argv[1]}`) {
  walk(root);
  console.log(`stamped ${root} with v=${version}`);
}
