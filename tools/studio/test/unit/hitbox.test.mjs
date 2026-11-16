// The hitbox overlay's boxes (hitbox.js): sprites from the VIC's registers,
// the game's bb_overlap tests from the runtime's ring (bbfast.s).
import test from 'node:test';
import assert from 'node:assert/strict';
import { boxes } from '../../../../docs/play/studio/hitbox.js';

test('sprite boxes: on ones only, the 9th x bit, double width and height', () => {
  const vic = new Uint8Array(0x2f);
  vic[0x15] = 0b101; // sprites 0 and 2
  vic[0] = 100; vic[1] = 80;
  vic[4] = 10; vic[5] = 200; vic[0x10] = 0b100; // sprite 2 at x 266
  vic[0x1d] = 0b100; vic[0x17] = 0b001;
  assert.deepEqual(boxes(vic).sprites, [{ n: 0, x: 100, y: 80, w: 24, h: 42 }, { n: 2, x: 266, y: 200, w: 48, h: 21 }]);
});

test('bb_overlap boxes: screen coordinates to the VIC\'s, recent ones only, hits marked', () => {
  const hits = new Uint8Array(128);
  // slot 0: a = (10, 20, 24x10), b = (30, 25, 8x6), at frame 7: they overlap
  hits.set([8, 25, 0, 30, 0, 10, 24, 20, 0, 10, 0, 6, 7], 0);
  // slot 1: from frame 100: too old by frame 120
  hits.set([8, 0, 0, 200, 0, 8, 8, 0, 0, 0, 0, 8, 100], 16);
  const { tests } = boxes(new Uint8Array(0x2f), hits, 2, 12);
  assert.deepEqual(tests, [{ a: { x: 34, y: 70, w: 24, h: 10 }, b: { x: 54, y: 75, w: 8, h: 6 }, hit: true }]);
  assert.equal(boxes(new Uint8Array(0x2f), hits, 2, 120).tests.length, 0);
});
