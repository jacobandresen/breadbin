// Bundles CodeMirror 6 for the Studio: tools/studio/codemirror.entry.js ->
// docs/play/studio/vendor/codemirror.js (one minified ES module).
//   cd tools/studio && npm ci && node bundle.mjs
import { build } from 'esbuild';
import { fileURLToPath } from 'node:url';

const here = (p) => fileURLToPath(new URL(p, import.meta.url));
await build({
  entryPoints: [here('./codemirror.entry.js')],
  outfile: here('../../docs/play/studio/vendor/codemirror.js'),
  bundle: true,
  format: 'esm',
  minify: true,
  legalComments: 'eof',
  target: 'es2022',
  logLevel: 'info',
});
