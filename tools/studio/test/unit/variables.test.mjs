// The Variables panel: arrays' item types from the C, and the runtime's own
// variables left out.
import test from 'node:test';
import assert from 'node:assert/strict';
import { cTypes, globals, format } from '../../../../docs/play/studio/variables.js';

test('item types come from the C declarations', () => {
  const types = cTypes({
    'src/a.c': 'int ex[6], ey[6];\nunsigned char lives, *name;\nstatic signed char dx[4];\nconst int table[3] = {1, 2, 3};\nvoid f(void) {\n}\n',
    'README.md': 'int not_this;',
  });
  assert.deepEqual(types, { ex: 2, ey: 2, lives: 1, name: 2, dx: 1, table: 2 });
});

test('the runtime\'s variables are left out; int arrays read as ints', () => {
  const types = { ex: 2, lives: 1 };
  const memory = { segments: [{ name: 'BSS', start: 0x100, size: 0x40 }] };
  const sizes = { ex: { size: 12, unit: 1 }, rnd: { size: 2, unit: 1 }, lives: { size: 1, unit: 1 } };
  const vars = globals({ _ex: 0x100, _rnd: 0x10c, _lives: 0x10e }, memory, sizes, { hidden: ['rnd'], types });
  assert.deepEqual(vars.map((v) => [v.name, v.unit]), [['ex', 2], ['lives', 1]]);
  assert.equal(format([78, 0, 234, 0, 4, 1], 12, 2), '[78, 234, 260, …]');
});

test('live values: signs from the C, numbers in and out of the bytes', async () => {
  const { cDecls, number, range, bytesOf, brief } = await import('../../../../docs/play/studio/variables.js');
  const d = cDecls({ 'src/a.c': 'char c;\nsigned char s;\nint i;\nunsigned u;\nunsigned int w;\nint *p;\n' });
  assert.deepEqual(d.c, { unit: 1, signed: false });
  assert.deepEqual(d.s, { unit: 1, signed: true });
  assert.deepEqual(d.i, { unit: 2, signed: true });
  assert.deepEqual(d.w, { unit: 2, signed: false });
  assert.deepEqual(d.p, { unit: 2, signed: false });
  const sc = { size: 1, unit: 1, signed: true };
  assert.equal(number([0xff], sc), -1);
  assert.deepEqual(range(sc), [-128, 127]);
  assert.deepEqual(bytesOf(-2, sc), [0xfe]);
  assert.deepEqual(bytesOf(999, { size: 1, unit: 1 }), [255]);
  const si = { size: 2, unit: 2, signed: true };
  assert.equal(number([0x00, 0x80], si), -32768);
  assert.deepEqual(bytesOf(-1, si), [255, 255]);
  assert.equal(brief([1, 0, 2, 0, 3, 0, 4, 0], { size: 12, unit: 2 }), '[1,2,3,4,…]');
  assert.equal(brief([7, 8], { size: 2, unit: 1 }), '2055');
});
