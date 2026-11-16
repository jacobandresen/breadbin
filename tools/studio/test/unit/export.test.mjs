// The web page export: the font its C64 shows for a C64's own characters.
import test from 'node:test';
import assert from 'node:assert/strict';
import { charRom } from '../../../../docs/play/studio/export.js';

test('the character ROM: the Studio font, reversed in the upper half, both sets', () => {
  const rom = charRom();
  assert.equal(rom.length, 4096);
  const a = [...rom.subarray(8, 16)]; // screen code 1: A
  assert.ok(a.some((b) => b));
  assert.deepEqual([...rom.subarray(129 * 8, 130 * 8)], a.map((b) => b ^ 0xff));
  assert.deepEqual([...rom.subarray(2048 + 8, 2048 + 16)], a);
});
