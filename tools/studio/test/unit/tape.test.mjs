// The joystick tape (tape.js, bb.c's bb_tape): its frames, cutting it, where
// a replay from a frame stands, and writing it into a program.
import test from 'node:test';
import assert from 'node:assert/strict';
import { tapeFrames, cut, position, usedAfter, playWrites, patchPrg, hasTape } from '../../../../docs/play/studio/tape.js';

const tape = Uint8Array.from([0, 100, 8, 30, 0, 20]); // still 100 frames, right 30, still 20

test('frames, cut, and where a replay from a frame stands', () => {
  assert.equal(tapeFrames(tape), 150);
  assert.deepEqual([...cut(tape, 110)], [0, 100, 8, 10]);
  assert.deepEqual(position(tape, 0), { pos: 2, left: 100 });
  assert.deepEqual(position(tape, 105), { pos: 4, left: 25 });
  assert.deepEqual(position(tape, 150), { pos: 6, left: 0 });
  assert.equal(usedAfter(tape, 50), true);
  assert.equal(usedAfter(tape, 130), false, 'only standing still after frame 130');
});

test('a tape written into a program, at its variables\' addresses', () => {
  const labels = { _bb_tape_mode: 0x900, _bb_tape_len: 0x901, _bb_tape_pos: 0x903, _bb_tape_left: 0x905, _bb_tape_seed: 0x906, _bb_tape_frames: 0x908, _bb_tape: 0x90a };
  assert.ok(hasTape(labels));
  const prg = new Uint8Array(2 + 0x200);
  prg[0] = 0x01;
  prg[1] = 0x08; // loads at $0801
  const out = patchPrg(prg, playWrites(labels, tape, { seed: 0x1234 }));
  const at = (a) => out[a - 0x801 + 2];
  assert.equal(at(0x900), 2, 'mode: play');
  assert.equal(at(0x901), 6);
  assert.deepEqual([at(0x906), at(0x907)], [0x34, 0x12]);
  assert.deepEqual([...out.slice(0x90a - 0x801 + 2, 0x90a - 0x801 + 8)], [...tape]);
  assert.throws(() => patchPrg(prg, [[0x2000, [1]]]), /outside the program/);
});
