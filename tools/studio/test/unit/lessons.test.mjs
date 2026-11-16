// The lessons (lessons.js), on the Empty game's files: each step's Show me
// makes its step done, the steps that wait on the running game wait for it,
// and the coins lesson's files build.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { LESSONS } from '../../../../docs/play/studio/lessons.js';
import { Toolchain } from '../../../../docs/play/studio/toolchain.js';
import { generate } from '../../../../docs/play/studio/codegen.js';

const DIR = new URL('../../../../docs/play/studio/examples/blank/', import.meta.url);
const blank = () => Object.fromEntries(JSON.parse(fs.readFileSync(new URL('files.json', DIR), 'utf8')).map((p) => [p, fs.readFileSync(new URL(p, DIR), 'utf8')]));
const ctx = (files, more = {}) => ({ files, vars: [], running: false, value: () => undefined, ...more });

for (const lesson of LESSONS) {
  test(`lesson "${lesson.title}": each step is not done until it is, then done`, () => {
    const files = blank();
    for (const [i, st] of lesson.steps.entries()) {
      assert.equal(!!st.done(ctx(files)), false, `step ${i + 1} is not done at the start`);
      if (st.show) Object.assign(files, st.show(files));
      else if (lesson.id === 'jump' && i === 1) assert.ok(st.done(ctx(files, { running: true, vars: [{ name: 'jump' }] })));
      else if (lesson.id === 'jump' && i === 2) files['src/main.c'] = files['src/main.c'].replace(/^signed char jump = -6;/m, 'signed char jump = -10;');
      else if (lesson.id === 'coins' && i === 4) assert.ok(st.done(ctx(files, { value: (n) => (n === 'coins' ? '1' : undefined) })));
      if (!(lesson.id === 'jump' && i === 1) && !(lesson.id === 'coins' && i === 4)) assert.ok(st.done(ctx(files)), `step ${i + 1} is done after it`);
    }
  });
}

test('the lessons\' files build without a warning', async () => {
  const tc = await Toolchain.load();
  for (const lesson of LESSONS) {
    const files = blank();
    for (const st of lesson.steps) if (st.show) Object.assign(files, st.show(files));
    const gen = await generate(files);
    const r = await tc.build({ ...files, ...gen });
    assert.equal(r.ok, true, r.diagnostics.map((d) => `${d.file}:${d.line}: ${d.message}`).join('\n'));
    assert.deepEqual(r.diagnostics, [], lesson.id);
  }
});
