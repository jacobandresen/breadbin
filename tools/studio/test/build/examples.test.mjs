// Every example project builds, as the Studio builds it: gen/ from the
// assets and the runtime, then cc65, ca65 and ld65 in WebAssembly. And the
// Makefile in each builds the same program with a native cc65, when there is
// one (CC65_BIN, or cl65 on the PATH).
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { execFileSync } from 'node:child_process';
import { Toolchain } from '../../../../docs/play/studio/toolchain.js';
import { generate } from '../../../../docs/play/studio/codegen.js';

const DIR = new URL('../../../../docs/play/studio/examples/', import.meta.url);
const tc = await Toolchain.load();

function load(id) {
  const list = JSON.parse(fs.readFileSync(new URL(`${id}/files.json`, DIR), 'utf8'));
  return Object.fromEntries(list.map((p) => [p, fs.readFileSync(new URL(`${id}/${p}`, DIR), 'utf8')]));
}

export async function buildExample(id) {
  const files = load(id);
  const gen = await generate(files);
  const r = await tc.build({ ...files, ...gen });
  return { files, gen, r };
}

for (const id of ['blank', 'bubbler', 'bouncer']) {
  test(`${id} builds without errors or warnings`, async () => {
    const { r } = await buildExample(id);
    assert.equal(r.ok, true, r.diagnostics.map((d) => `${d.file}:${d.line}: ${d.message}`).join('\n'));
    assert.deepEqual(r.diagnostics, [], 'no warnings either');
    assert.ok(r.memory.used < r.memory.total * 0.6, `${id} uses ${r.memory.used} bytes`);
  });
}

const native = process.env.CC65_BIN || (() => {
  const local = new URL('../../../../target/cc65/bin/', import.meta.url);
  return fs.existsSync(new URL('cl65', local)) ? local.pathname : '';
})();

test('the Makefile builds the same program with a native cc65', { skip: !native && 'no native cc65 (set CC65_BIN)' }, async () => {
  const { files, gen, r } = await buildExample('bubbler');
  const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'studio-make-'));
  for (const [p, t] of Object.entries({ ...files, ...gen })) {
    fs.mkdirSync(path.dirname(path.join(dir, p)), { recursive: true });
    fs.writeFileSync(path.join(dir, p), t);
  }
  const home = path.resolve(native, '..');
  execFileSync('make', ['-s', '-C', dir], { env: { ...process.env, PATH: `${native}:${process.env.PATH}`, CC65_HOME: home }, stdio: 'pipe' });
  const prg = fs.readFileSync(path.join(dir, 'game.prg'));
  assert.equal(prg.length, r.prg.length, 'the same size');
  assert.ok(Buffer.from(r.prg).equals(prg), 'byte for byte the same program');
  fs.rmSync(dir, { recursive: true, force: true });
});
