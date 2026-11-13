// tools/stamp.mjs: a deploy's module URLs get ?v=<version>, nothing else does.
import test from 'node:test';
import assert from 'node:assert/strict';

process.argv[3] = 'abc';
const { stampJs, stampHtml } = await import('../../../stamp.mjs');

test('relative imports, workers, worklets and wasm are stamped', () => {
  assert.equal(stampJs(`import { a } from './a.js';`), `import { a } from './a.js?v=abc';`);
  assert.equal(stampJs(`import * as e from '../emu.js'`), `import * as e from '../emu.js?v=abc'`);
  assert.equal(stampJs(`await import('./studio/studio.js')`), `await import('./studio/studio.js?v=abc')`);
  assert.equal(stampJs(`new URL('./worker.js', import.meta.url)`), `new URL('./worker.js?v=abc', import.meta.url)`);
  assert.equal(stampJs(`new URL('./breadbin.wasm', import.meta.url)`), `new URL('./breadbin.wasm?v=abc', import.meta.url)`);
});

test('packages, data, absolute URLs and stamped URLs are left alone', () => {
  for (const s of [`import fs from 'node:fs';`, `new URL('./verified.json', import.meta.url)`, `import x from 'https://a.b/c.js'`, `import a from './a.js?v=1'`, `const s = './a.js';`]) assert.equal(stampJs(s), s);
});

test('HTML: local scripts and stylesheets', () => {
  assert.equal(stampHtml('<script type="module" src="app.js"></script>'), '<script type="module" src="app.js?v=abc"></script>');
  assert.equal(stampHtml('<link rel="stylesheet" href="studio/studio.css">'), '<link rel="stylesheet" href="studio/studio.css?v=abc">');
  for (const s of ['<script src="https://cdn.x/y.js"></script>', '<a href="#games">', '<link href="//x/y.css">']) assert.equal(stampHtml(s), s);
});
