// music - the MUSIC editor's instruments, patterns and songs
// (assets/music.json), and gen/music.s, the data bbsound.s plays.
//
//   { "instruments": [ { "name": "lead", "wave": "pulse", "a": 0, "d": 9, "s": 6, "r": 4,
//                        "pw": 8, "pwm": 2, "vib": 3, "arp": [0, 0], "slide": 0, "drum": false } ],
//     "patterns": [ { "name": "intro", "speed": 6, "steps": ["C-4 0 ...", "--- . ...", ...] } ],
//     "songs": [ { "name": "theme", "loop": 0, "rows": [[0, 1, 2], [3, 1, -1]] } ] }
//
// An instrument is a SID voice setting: a waveform, the envelope (attack,
// decay, sustain, release: 0..15 each), the pulse width (0..15, for pulse)
// and how fast it sweeps (pwm), a delayed vibrato (vib, depth 0..15), an
// arpeggio (two semitone offsets played on alternate frames: a chord on one
// voice), a constant pitch slide (signed, for drums and lasers) and "drum",
// a noise click on the first frames.
//
// A pattern is up to 64 steps of `speed` frames each (6 is 125 steps a minute).
// A step is written as in a tracker, "NNN I FPP":
//   NNN  the note: C-4, C#4 ... B-7, "---" nothing new, "===" release
//   I    the instrument, 0..F, or "." for none
//   FPP  an effect and its parameter in hex: U slide up, D slide down,
//        V vibrato, C cut after PP frames, "..." none
//
// A song is a list of rows; each row plays a pattern on each of the three
// voices (-1 rests). The row ends with its first voice's pattern. "loop" is
// the row to go back to at the end, or -1 to stop.
//
// Patterns double as sound effects: bb_sfx(SFX_JUMP) plays a pattern on the
// third voice.

export const NOTE_NAMES = ['C-', 'C#', 'D-', 'D#', 'E-', 'F-', 'F#', 'G-', 'G#', 'A-', 'A#', 'B-'];
export const WAVES = { tri: 0x10, saw: 0x20, pulse: 0x40, noise: 0x80, 'tri+saw': 0x30, 'tri+pulse': 0x50, 'saw+pulse': 0x60 };
export const EFFECTS = { '.': 0, U: 1, D: 2, V: 3, C: 4 };
export const MAX_STEPS = 64;
export const EMPTY = '--- . ...';

/** "C#4" -> 49 (0 = C-0), or null for anything else. */
export function parseNote(s) {
  const m = /^([A-G])([-#])(\d)$/.exec(s);
  if (!m) return null;
  const i = NOTE_NAMES.indexOf(m[1] + m[2]);
  if (i < 0) return null;
  const n = Number(m[3]) * 12 + i;
  return n < 96 ? n : null;
}

/** 49 -> "C#4" */
export const noteName = (n) => `${NOTE_NAMES[n % 12]}${Math.floor(n / 12)}`;

/**
 * A step string as { note, release, ins, fx, param }: `note` 0..95 or null
 * (nothing new), `release` true for "===", `ins` 0..15 or null.
 */
export function parseStep(s) {
  const [n = '---', i = '.', f = '...'] = String(s || EMPTY).trim().split(/\s+/);
  const release = n === '===';
  const note = release ? null : parseNote(n);
  const ins = /^[0-9a-f]$/i.test(i) ? parseInt(i, 16) : null;
  const fx = EFFECTS[f[0]] ?? 0;
  const param = fx ? parseInt(f.slice(1), 16) || 0 : 0;
  return { note, release, ins, fx, param };
}

/** The inverse of parseStep. */
export function formatStep({ note = null, release = false, ins = null, fx = 0, param = 0 }) {
  const n = release ? '===' : note == null ? '---' : noteName(note);
  const i = ins == null ? '.' : ins.toString(16).toUpperCase();
  const fxch = Object.keys(EFFECTS).find((k) => EFFECTS[k] === fx) || '.';
  const f = fx ? fxch + param.toString(16).toUpperCase().padStart(2, '0') : '...';
  return `${n} ${i} ${f}`;
}

/** The three bytes of a step in the player's format. */
export function stepBytes(s) {
  const st = typeof s === 'string' ? parseStep(s) : s;
  const note = st.release ? 0xff : st.note == null ? 0 : st.note + 1;
  return [note, ((st.fx & 15) << 4) | ((st.ins ?? 0) & 15), st.param & 255];
}

const waveByte = (w) => (typeof w === 'number' ? w & 0xf0 : WAVES[w] ?? 0x40);
const nyb = (v) => Math.max(0, Math.min(15, v | 0));
const signed = (v) => (Math.max(-128, Math.min(127, v | 0)) + 256) & 255;

/** The instrument tables, a byte per instrument each (at least one instrument). */
export function instrumentTables(music) {
  const list = music.instruments?.length ? music.instruments : [defaultInstrument()];
  return {
    wave: list.map((i) => waveByte(i.wave)),
    ad: list.map((i) => (nyb(i.a) << 4) | nyb(i.d)),
    sr: list.map((i) => (nyb(i.s) << 4) | nyb(i.r)),
    pw: list.map((i) => nyb(i.pw ?? 8)),
    pwm: list.map((i) => nyb(i.pwm)),
    vib: list.map((i) => nyb(i.vib)),
    arp: list.map((i) => (nyb(i.arp?.[0]) << 4) | nyb(i.arp?.[1])),
    slide: list.map((i) => signed(i.slide)),
    flags: list.map((i) => (i.drum ? 1 : 0)),
  };
}

/** The bytes of pattern `p`: speed, length, then 3 per step (a note without an instrument keeps the last). */
export function patternBytes(p) {
  const steps = (p.steps || []).slice(0, MAX_STEPS).map((s) => (typeof s === 'string' ? parseStep(s) : s));
  // a note without an instrument keeps the one before it, as in a tracker
  let ins = 0;
  for (const st of steps) if (st.note != null) st.ins == null ? (st.ins = ins) : (ins = st.ins);
  return [Math.max(1, Math.min(255, p.speed | 0 || 6)), steps.length, ...steps.flatMap(stepBytes)];
}

/** The bytes of song `s`: rows, loop row ($FF none), then three patterns a row ($FF rests). */
export function songBytes(s, patterns = Infinity) {
  const rows = (s.rows || []).slice(0, 84); // the player indexes a song with one byte
  const pat = (v) => (v == null || v < 0 || v >= patterns ? 0xff : v);
  const loop = s.loop == null || s.loop < 0 || s.loop >= rows.length ? 0xff : s.loop;
  return [rows.length, loop, ...rows.flatMap((r) => [pat(r[0]), pat(r[1]), pat(r[2])])];
}

const bytes = (list) => {
  const out = [];
  for (let i = 0; i < list.length; i += 16) out.push(`        .byte ${list.slice(i, i + 16).map((b) => `$${b.toString(16).padStart(2, '0')}`).join(', ')}`);
  return out.length ? out.join('\n') : '';
};

/** gen/music.s: the tables bbsound.s imports. */
export function musicSource(music) {
  const t = instrumentTables(music);
  const patterns = music.patterns?.length ? music.patterns : [{ name: 'silence', speed: 6, steps: [EMPTY] }];
  const songs = music.songs || [];
  const lines = [
    '; gen/music.s - written by the breadbin Studio from assets/music.json.',
    '; Edit the music in the MUSIC editor: this file is made again on every build.',
    '',
    '        .export bbs_ins_wave, bbs_ins_ad, bbs_ins_sr, bbs_ins_pw, bbs_ins_pwm',
    '        .export bbs_ins_vib, bbs_ins_arp, bbs_ins_slide, bbs_ins_flags',
    '        .export bbs_pat_lo, bbs_pat_hi, bbs_song_lo, bbs_song_hi, bbs_song_count',
    '',
    '        .segment "RODATA"',
  ];
  for (const k of ['wave', 'ad', 'sr', 'pw', 'pwm', 'vib', 'arp', 'slide', 'flags']) lines.push(`bbs_ins_${k}:`, bytes(t[k]));
  lines.push('bbs_pat_lo:', `        .lobytes ${patterns.map((_, i) => `pat${i}`).join(', ')}`);
  lines.push('bbs_pat_hi:', `        .hibytes ${patterns.map((_, i) => `pat${i}`).join(', ')}`);
  lines.push('bbs_song_count:', `        .byte ${songs.length}`);
  lines.push('bbs_song_lo:', songs.length ? `        .lobytes ${songs.map((_, i) => `song${i}`).join(', ')}` : '');
  lines.push('bbs_song_hi:', songs.length ? `        .hibytes ${songs.map((_, i) => `song${i}`).join(', ')}` : '');
  patterns.forEach((p, i) => lines.push(`pat${i}:   ; ${clean(p.name)}`, bytes(patternBytes(p))));
  songs.forEach((s, i) => lines.push(`song${i}:  ; ${clean(s.name)}`, bytes(songBytes(s, patterns.length))));
  return `${lines.filter((l) => l !== '').join('\n')}\n`;
}

const clean = (s) => String(s ?? '').replace(/[^\x20-\x7e]/g, '');

export const defaultInstrument = () => ({ name: 'lead', wave: 'pulse', a: 0, d: 9, s: 6, r: 4, pw: 8, pwm: 2, vib: 3, arp: [0, 0], slide: 0, drum: false });

/** A new, empty piece of music: a few instruments to start from. */
export function newMusic() {
  return {
    instruments: [
      defaultInstrument(),
      { name: 'bass', wave: 'saw', a: 0, d: 6, s: 10, r: 2, pw: 8, pwm: 0, vib: 0, arp: [0, 0], slide: 0, drum: false },
      { name: 'chord', wave: 'pulse', a: 0, d: 4, s: 8, r: 6, pw: 4, pwm: 4, vib: 0, arp: [4, 7], slide: 0, drum: false },
      { name: 'kick', wave: 'tri', a: 0, d: 8, s: 0, r: 0, pw: 8, pwm: 0, vib: 0, arp: [0, 0], slide: -24, drum: true },
      { name: 'snare', wave: 'noise', a: 0, d: 7, s: 0, r: 6, pw: 8, pwm: 0, vib: 0, arp: [0, 0], slide: 0, drum: false },
    ],
    patterns: [{ name: 'pattern 0', speed: 6, steps: Array(32).fill(EMPTY) }],
    songs: [{ name: 'song 0', loop: 0, rows: [[0, -1, -1]] }],
  };
}

/** Frames a song plays before it loops or stops (rows timed by their first voice). */
export function songFrames(music, s) {
  let frames = 0;
  for (const row of s.rows || []) {
    const lead = row.find((v) => v != null && v >= 0);
    const p = music.patterns[lead];
    if (p) frames += (p.steps?.length || 0) * Math.max(1, p.speed | 0 || 6);
  }
  return frames;
}
