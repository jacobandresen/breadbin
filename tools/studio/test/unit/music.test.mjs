// The tracker notation and the player's data format.
import test from 'node:test';
import assert from 'node:assert/strict';
import {
  parseNote, noteName, parseStep, formatStep, stepBytes, patternBytes, songBytes, instrumentTables,
  musicSource, newMusic, songFrames, EMPTY,
} from '../../../../docs/play/studio/assets/music.js';

test('note names', () => {
  assert.equal(parseNote('C-0'), 0);
  assert.equal(parseNote('A-4'), 57);
  assert.equal(parseNote('C#4'), 49);
  assert.equal(parseNote('B-7'), 95);
  assert.equal(parseNote('C-8'), null);
  assert.equal(parseNote('H-4'), null);
  for (let n = 0; n < 96; n++) assert.equal(parseNote(noteName(n)), n);
});

test('steps parse and print the same', () => {
  for (const s of ['C-4 1 ...', 'A#3 F V04', '=== . ...', EMPTY, '--- 2 C10', 'G-5 . U03']) assert.equal(formatStep(parseStep(s)), s);
  assert.deepEqual(parseStep('A-4 2 D1F'), { note: 57, release: false, ins: 2, fx: 2, param: 0x1f });
});

test('a step is three bytes: note+1 / $FF release / 0 nothing, effect<<4|instrument, parameter', () => {
  assert.deepEqual(stepBytes('A-4 2 V05'), [58, 0x32, 5]);
  assert.deepEqual(stepBytes('=== . ...'), [0xff, 0, 0]);
  assert.deepEqual(stepBytes(EMPTY), [0, 0, 0]);
});

test('a pattern is speed, length and its steps; a note without an instrument keeps the last', () => {
  const b = patternBytes({ speed: 4, steps: ['C-4 3 ...', 'D-4 . ...', EMPTY] });
  assert.deepEqual(b, [4, 3, 49, 3, 0, 51, 3, 0, 0, 0, 0]);
  assert.equal(patternBytes({ speed: 0, steps: [] })[0], 6, 'speed 0 means the default');
  assert.equal(patternBytes({ speed: 1, steps: Array(100).fill(EMPTY) })[1], 64, 'at most 64 steps');
});

test('a song is rows, loop and three patterns a row; rests and missing patterns are $FF', () => {
  assert.deepEqual(songBytes({ loop: 1, rows: [[0, 1, -1], [2, 9, 0]] }, 3), [2, 1, 0, 1, 0xff, 2, 0xff, 0]);
  assert.equal(songBytes({ loop: -1, rows: [[0]] })[1], 0xff);
  assert.equal(songBytes({ loop: 5, rows: [[0]] })[1], 0xff, 'a loop past the end plays once');
});

test('instrument tables pack the envelope and arpeggio into nybbles', () => {
  const t = instrumentTables({ instruments: [{ wave: 'saw', a: 1, d: 2, s: 3, r: 4, pw: 20, arp: [4, 7], slide: -2, drum: true }] });
  assert.deepEqual([t.wave[0], t.ad[0], t.sr[0], t.pw[0], t.arp[0], t.slide[0], t.flags[0]], [0x20, 0x12, 0x34, 15, 0x47, 0xfe, 1]);
  assert.equal(instrumentTables({ instruments: [] }).wave.length, 1, 'there is always an instrument');
});

test('gen/music.s exports what bbsound.s imports', () => {
  const s = musicSource(newMusic());
  for (const name of ['bbs_ins_wave', 'bbs_ins_flags', 'bbs_pat_lo', 'bbs_pat_hi', 'bbs_song_lo', 'bbs_song_count']) assert.match(s, new RegExp(`^${name}:`, 'm'));
  assert.match(s, /^pat0: {3}; pattern 0/m);
  assert.match(musicSource({ instruments: [], patterns: [], songs: [] }), /bbs_song_count:\n {8}\.byte 0/);
});

test('a song lasts its leading patterns', () => {
  const m = { patterns: [{ speed: 6, steps: Array(32).fill(EMPTY) }, { speed: 3, steps: Array(16).fill(EMPTY) }] };
  assert.equal(songFrames(m, { rows: [[0, 1, -1], [-1, 1, 0]] }), 32 * 6 + 16 * 3);
});

test('deleting an instrument: its notes play 0, the later ones move down', async () => {
  // music-editor.js needs a DOM only to run; its remapInstruments is plain
  globalThis.document ??= { createElement: () => ({}) };
  const { remapInstruments } = await import('../../../../docs/play/studio/music-editor.js');
  const p = remapInstruments([{ name: 'p', speed: 6, steps: ['C-4 0 ...', 'D-4 1 V02', 'E-4 2 ...', '--- . ...', '=== . ...'] }], 1);
  assert.deepEqual(p[0].steps, ['C-4 0 ...', 'D-4 0 V02', 'E-4 1 ...', '--- . ...', '=== . ...']);
});
