// sfx - sound effects made up for you, sfxr-style: pick a kind (jump, coin,
// laser...), get a pattern for bb_sfx() and the instrument it plays, with a
// little chance in them, so each press is another take. The MUSIC editor's
// 🎲 button (music-editor.js) adds one and plays it.

import { formatStep, parseNote, EFFECTS, EMPTY } from './assets/music.js';

const pick = (rnd, list) => list[Math.floor(rnd() * list.length)];
const between = (rnd, a, b) => a + Math.floor(rnd() * (b - a + 1));
const note = (name, ins, fx = '.', param = 0) => formatStep({ note: parseNote(name), ins, fx: EFFECTS[fx], param });

/**
 * The kinds: an instrument (wave and envelope, 0-15 each) and the pattern's
 * notes, each a function of the random numbers. `steps` gives [step, text]
 * pairs; the rest of the pattern is empty, and a release (===) ends it.
 */
export const KINDS = {
  jump: {
    label: 'Jump',
    ins: (r) => ({ wave: 'pulse', a: 0, d: 6, s: 0, r: 3, pw: between(r, 4, 8) }),
    pattern: (r, i) => ({ speed: 1, steps: [[0, note(pick(r, ['C-4', 'D-4', 'E-4', 'G-3']), i, 'U', between(r, 0x18, 0x38))]], release: between(r, 8, 12) }),
  },
  coin: {
    label: 'Coin',
    ins: () => ({ wave: 'pulse', a: 0, d: 5, s: 6, r: 4, pw: 8 }),
    pattern: (r, i) => {
      const [a, b] = pick(r, [['B-5', 'E-6'], ['A-5', 'E-6'], ['G-5', 'C-6'], ['C-6', 'G-6']]);
      return { speed: 2, steps: [[0, note(a, i)], [1, note(b, i)]], release: between(r, 4, 6) };
    },
  },
  laser: {
    label: 'Laser',
    ins: (r) => ({ wave: pick(r, ['saw', 'pulse']), a: 0, d: 8, s: 0, r: 2, pw: 3 }),
    pattern: (r, i) => ({ speed: 1, steps: [[0, note(pick(r, ['C-6', 'E-6', 'G-6']), i, 'D', between(r, 0x30, 0x60))]], release: between(r, 6, 10) }),
  },
  explosion: {
    label: 'Explosion',
    ins: (r) => ({ wave: 'noise', a: 0, d: between(r, 10, 13), s: 0, r: between(r, 8, 11) }),
    pattern: (r, i) => ({ speed: 2, steps: [[0, note(pick(r, ['C-3', 'E-3', 'G-2']), i, 'D', between(r, 0x04, 0x10))]], release: between(r, 8, 14) }),
  },
  powerup: {
    label: 'Power-up',
    ins: (r) => ({ wave: pick(r, ['tri', 'pulse']), a: 0, d: 4, s: 8, r: 3, pw: 6 }),
    pattern: (r, i) => {
      const root = pick(r, ['C', 'D', 'F', 'G']);
      const up = [`${root}-4`, `${root}-5`].map((n) => n);
      const third = { C: ['E', 'G'], D: ['F#', 'A'], F: ['A', 'C'], G: ['B', 'D'] }[root];
      const run = [up[0], `${third[0]}-4`, `${third[1]}-4`, up[1], `${third[0]}-5`];
      return { speed: 2, steps: run.map((n, k) => [k, note(n.replace('#-', '#'), i)]), release: 6 };
    },
  },
  hit: {
    label: 'Hit',
    ins: () => ({ wave: 'noise', a: 0, d: 5, s: 0, r: 2 }),
    pattern: (r, i) => ({ speed: 1, steps: [[0, note(pick(r, ['G-4', 'C-5', 'E-4']), i, 'D', between(r, 0x20, 0x40))]], release: between(r, 3, 5) }),
  },
  blip: {
    label: 'Blip',
    ins: (r) => ({ wave: 'pulse', a: 0, d: 3, s: 4, r: 2, pw: between(r, 2, 8) }),
    pattern: (r, i) => ({ speed: 1, steps: [[0, note(pick(r, ['C-5', 'E-5', 'G-5', 'C-6']), i)]], release: between(r, 2, 4) }),
  },
};

const unique = (base, taken) => {
  if (!taken.includes(base)) return base;
  for (let n = 2; ; n++) if (!taken.includes(`${base} ${n}`)) return `${base} ${n}`;
};

/**
 * Add a `kind` sound effect to `music` (changed in place): its instrument
 * ("sfx jump", made or remade) and a pattern named after the kind. Returns
 * { pattern, instrument }: their indexes.
 */
export function makeSfx(music, kind, rnd = Math.random) {
  const k = KINDS[kind];
  const name = `sfx ${kind}`;
  const ins = { name, a: 0, d: 0, s: 0, r: 0, pw: 8, pwm: 0, vib: 0, arp: [0, 0], slide: 0, drum: false, ...k.ins(rnd) };
  let i = music.instruments.findIndex((x) => x.name === name);
  if (i < 0) {
    if (music.instruments.length >= 16) i = music.instruments.length - 1; // (a step names 0-F: the last one is reused)
    else i = music.instruments.push(ins) - 1;
  }
  music.instruments[i] = ins;
  const p = k.pattern(rnd, i);
  const steps = Array(Math.max(8, p.release + 2)).fill(EMPTY);
  for (const [at, text] of p.steps) steps[at] = text;
  steps[p.release] = '=== . ...';
  music.patterns.push({ name: unique(kind, music.patterns.map((x) => x.name)), speed: p.speed, steps });
  return { pattern: music.patterns.length - 1, instrument: i };
}
