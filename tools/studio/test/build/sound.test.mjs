// The game's SID player (runtime/bbsound.s), assembled by the WebAssembly
// ca65 into a PSID and played by breadbin's own SID engine (breadbin.wasm,
// `make web`): the SID registers it writes, frame by frame.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { Toolchain } from '../../../../docs/play/studio/toolchain.js';
import { musicPsid, previewMusic } from '../../../../docs/play/studio/psid.js';
import { newMusic, EMPTY } from '../../../../docs/play/studio/assets/music.js';

const ENGINE = new URL('../../../../docs/play/breadbin.wasm', import.meta.url);
const skip = !fs.existsSync(ENGINE) && 'needs docs/play/breadbin.wasm (make web)';
const tc = await Toolchain.load();

/** Play `psid` for `frames` frames: each frame's 25 SID registers and loudness. */
async function play(psid, frames, song = 1) {
  const { instance } = await WebAssembly.instantiate(fs.readFileSync(ENGINE), { env: { host_peek: () => 0, host_poke() {}, host_event() {} } });
  const w = instance.exports;
  const p = w.bb_alloc(psid.length);
  new Uint8Array(w.memory.buffer, p, psid.length).set(psid);
  const player = w.bb_player_new(p, psid.length, song, 48000, 0);
  assert.ok(player, 'the engine took the PSID');
  const buf = w.bb_alloc(960 * 4);
  const regs = w.bb_alloc(32);
  const out = [];
  for (let f = 0; f < frames; f++) {
    w.bb_player_render(player, buf, 960); // one PAL frame at 48 kHz
    w.bb_player_regs(player, regs);
    const r = [...new Uint8Array(w.memory.buffer, regs, 25)];
    const pcm = new Int16Array(w.memory.buffer, buf, 1920);
    let e = 0;
    for (const v of pcm) e += v * v;
    const voice = (n) => ({ freq: r[n * 7] | (r[n * 7 + 1] << 8), pw: r[n * 7 + 2] | (r[n * 7 + 3] << 8), ctrl: r[n * 7 + 4], ad: r[n * 7 + 5], sr: r[n * 7 + 6] });
    out.push({ v: [voice(0), voice(1), voice(2)], vol: r[24], rms: Math.sqrt(e / pcm.length) });
  }
  return out;
}

/** The SID frequency of note `n` (0 = C-0) on a PAL C64. */
const pal = (n) => Math.min(65535, Math.round(16.3516 * 2 ** (n / 12) * 16777216 / 985248));
const [C3, C4, E4, G4, A4, C5, E5, C6, G6] = [36, 48, 52, 55, 57, 60, 64, 72, 79].map(pal);

const music = (patterns, songs, instruments) => ({ ...newMusic(), ...(instruments && { instruments }), patterns, songs });
const pat = (speed, ...steps) => ({ name: 'p', speed, steps });
const plain = { name: 'plain', wave: 'saw', a: 0, d: 0, s: 15, r: 0, pw: 8, pwm: 0, vib: 0, arp: [0, 0], slide: 0, drum: false };

test('notes play at PAL SID frequencies, with the instrument\'s waveform and envelope', { skip }, async () => {
  const m = music([pat(4, 'A-4 0 ...', EMPTY, 'C-5 0 ...', EMPTY)], [{ name: 's', loop: 0, rows: [[0, -1, -1]] }], [plain]);
  const f = await play(await musicPsid(tc, m), 20);
  assert.equal(f[1].v[0].freq, 0x1d45, 'A-4 is 440 Hz');
  assert.equal(A4, 0x1d45);
  assert.equal(f[1].v[0].ctrl, 0x21, 'saw, gate on');
  assert.equal(f[1].v[0].sr, 0xf0);
  assert.equal(f[1].vol & 15, 15);
  assert.equal(f[10].v[0].freq, C5, 'C-5 is 523 Hz');
  assert.equal(f[17].v[0].freq, A4, 'the song loops');
  assert.ok(f[5].rms > 500, `it is audible (rms ${f[5].rms})`);
});

test('release lets the gate go; the voice falls silent', { skip }, async () => {
  const ins = { ...plain, r: 0 };
  const m = music([pat(5, 'A-4 0 ...', '=== . ...', EMPTY, EMPTY, EMPTY, EMPTY)], [{ name: 's', loop: -1, rows: [[0, -1, -1]] }], [ins]);
  const f = await play(await musicPsid(tc, m), 40);
  assert.equal(f[2].v[0].ctrl & 1, 1);
  assert.equal(f[7].v[0].ctrl & 1, 0, 'released on step 1');
  assert.ok(f[38].rms < 200, `silent (rms ${f[38].rms})`);
});

test('three voices play their own patterns; the row ends with the first voice', { skip }, async () => {
  const m = music([pat(2, 'C-4 0 ...', EMPTY), pat(2, 'E-4 0 ...'), pat(2, 'G-4 0 ...', EMPTY, EMPTY, EMPTY, EMPTY, EMPTY), pat(2, 'C-3 0 ...', EMPTY)],
    [{ name: 's', loop: -1, rows: [[0, 1, 2], [3, -1, -1]] }], [plain]);
  const f = await play(await musicPsid(tc, m), 12);
  assert.deepEqual(f[1].v.map((v) => v.freq), [C4, E4, G4], 'C-4, E-4, G-4');
  assert.equal(f[5].v[0].freq, C3, 'row 2 starts after the first voice\'s 2 steps x 2 frames: C-3');
  assert.equal(f[5].v[1].ctrl & 1, 0, 'the resting voice is released');
});

test('slides, vibrato, arpeggio and cut change the voice frame by frame', { skip }, async () => {
  const ins = [plain, { ...plain, vib: 8 }, { ...plain, arp: [4, 7] }];
  const tune = (step) => music([pat(30, step)], [{ name: 's', loop: 0, rows: [[0, -1, -1]] }], ins);
  const up = await play(await musicPsid(tc, tune('A-4 0 U10')), 6);
  assert.ok(up[4].v[0].freq > up[1].v[0].freq + 100, 'slide up');
  const down = await play(await musicPsid(tc, tune('A-4 0 D10')), 6);
  assert.ok(down[4].v[0].freq < down[1].v[0].freq - 100, 'slide down');
  const vib = await play(await musicPsid(tc, tune('A-4 1 ...')), 30);
  assert.equal(new Set(vib.slice(1, 8).map((x) => x.v[0].freq)).size, 1, 'the instrument\'s vibrato waits 8 frames');
  assert.ok(new Set(vib.slice(10, 30).map((x) => x.v[0].freq)).size >= 3, 'then wobbles');
  const arp = await play(await musicPsid(tc, tune('C-4 2 ...')), 9);
  assert.deepEqual(new Set(arp.slice(2, 8).map((x) => x.v[0].freq)), new Set([C4, E4, G4]), 'C, E, G in turn');
  const cut = await play(await musicPsid(tc, tune('A-4 0 C05')), 10);
  assert.equal(cut[2].v[0].ctrl & 1, 1);
  assert.equal(cut[8].v[0].ctrl & 1, 0, 'cut after 5 frames');
});

test('a drum clicks with noise first, then its own waveform', { skip }, async () => {
  const kick = { ...plain, wave: 'tri', drum: true, slide: -20 };
  const m = music([pat(20, 'C-3 0 ...')], [{ name: 's', loop: 0, rows: [[0, -1, -1]] }], [kick]);
  const f = await play(await musicPsid(tc, m), 8);
  assert.equal(f[0].v[0].ctrl & 0xf0, 0x80, 'noise on the first frame');
  assert.equal(f[4].v[0].ctrl & 0xf0, 0x10, 'triangle after');
  assert.ok(f[6].v[0].freq < f[3].v[0].freq, 'the pitch drops');
});

test('a sound effect borrows the third voice and gives it back', { skip }, async () => {
  const m = music([pat(50, 'C-4 0 ...'), pat(2, 'C-6 0 ...', 'G-6 0 ...')], [{ name: 's', loop: 0, rows: [[0, 0, 0]] }], [plain]);
  const f = await play(await musicPsid(tc, m, { sfx: 1 }), 12);
  assert.equal(f[1].v[0].freq, C4, 'voice 1 plays the song');
  assert.equal(f[1].v[2].freq, C6, 'voice 3 plays the effect: C-6');
  assert.equal(f[2].v[2].freq, G6);
  assert.equal(f[3].v[2].freq, G6, 'G-6 for all of its 2 frames');
  assert.equal(f[8].v[2].freq, C4, 'and then the song again');
});

test('previews: one pattern, one note', { skip }, async () => {
  const base = music([pat(3, 'C-4 0 ...'), pat(3, 'E-5 0 ...')], [{ name: 's', loop: 0, rows: [[0, -1, -1]] }], [plain]);
  const p = await play(await musicPsid(tc, previewMusic(base, { pattern: 1 })), 3);
  assert.equal(p[1].v[0].freq, E5, 'pattern 1 alone: E-5');
  const n = await play(await musicPsid(tc, previewMusic(base, { note: 57, ins: 0 })), 40);
  assert.equal(n[2].v[0].freq, A4);
  assert.equal(n[30].v[0].ctrl & 1, 0, 'a previewed note is released');
});

test('a row where every voice rests passes, and the song goes on', { skip }, async () => {
  const m = music([pat(2, 'C-4 0 ...', EMPTY), pat(2, 'E-4 0 ...', EMPTY)], [{ name: 's', loop: 0, rows: [[0, -1, -1], [-1, -1, -1], [1, -1, -1]] }], [plain]);
  const f = await play(await musicPsid(tc, m), 12);
  assert.equal(f[1].v[0].freq, C4);
  assert.ok(f.slice(4, 8).some((x) => x.v[0].freq === E4), 'the third row plays after the empty one');
});
