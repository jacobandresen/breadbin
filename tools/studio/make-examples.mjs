// make-examples - writes the Studio's example projects into
// docs/play/studio/examples/<id>/: the C sources from tools/studio/examples/,
// and the assets (sprites, tiles, rooms, music) made here, in code, so they
// are easy to review and change. Bubbler's rounds and theme come from blip's
// Bubbler (crates/bubbler/src/levels.rs and tunes.rs), Bouncer's from blip's
// Bouncer.
//
//   node tools/studio/make-examples.mjs

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { newTileset } from '../../docs/play/studio/assets/tiles.js';
import { blankRoom, setCell, COLS, ROWS } from '../../docs/play/studio/assets/map.js';
import { EMPTY } from '../../docs/play/studio/assets/music.js';
import { glyph } from '../../docs/play/studio/assets/font.js';
import { pretty } from '../../docs/play/studio/project.js';

const here = path.dirname(fileURLToPath(import.meta.url));
const OUT = path.join(here, '../../docs/play/studio/examples');

// ── pixels ──────────────────────────────────────────────────────────────────

/** Check an image's rows: 21 rows of 12 (multicolour) or 24 (hires) pixels. */
function rows(name, list, w) {
  const out = [...list];
  while (out.length < 21) out.unshift('.'.repeat(w));
  out.forEach((r, i) => {
    if (r.length !== w) throw new Error(`${name}: row ${i} is ${r.length} pixels, not ${w}: "${r}"`);
  });
  if (out.length !== 21) throw new Error(`${name}: ${out.length} rows`);
  return out;
}
const mc = (name, color, list, flip = false) => ({ name, multi: true, color, flip, rows: rows(name, list, 12) });
const hires = (name, color, list, flip = false) => ({ name, multi: false, color, flip, rows: rows(name, list, 24) });

/** A 24x21 picture from fn(x, y) -> '.', '1', '2' or '3', at hires or (multi) 12x21. */
function paint(fn, multi) {
  const w = multi ? 12 : 24;
  return [...Array(21)].map((_, y) => [...Array(w)].map((_, x) => fn(multi ? x * 2 + 1 : x + 0.5, y + 0.5)).join(''));
}

// ── Bubbler ─────────────────────────────────────────────────────────────────

const BUB_TOP = [
  '....2222....',
  '..22111122..',
  '.2111113332.',
  '.2111113232.',
  '21211113332.',
  '21111111112.',
  '.2111111112.',
];
const BUB_BODY = [
  '.2111122222.',
  '.21111111122',
  '.2211111112.',
  '2112111112..',
  '.2.21111112.',
  '...2111112..',
];
const bub = (top, body, legs) => [...top, ...body, ...legs];
const LEGS_STAND = ['...2111112..', '..221..221..', '..222..222..'];
const LEGS_WALK = ['..21111112..', '.221...2212.', '.222....222.'];
const LEGS_JUMP = ['.2211111122.', '.22.2222.22.', '............'];
const BUB_BLOW_BODY = ['.2111111222.', '.21111123332', '.2111111222.', '2112111112..', '.2.21111112.', '...2111112..'];
const BUB_HURT_TOP = ['....2222....', '..22111122..', '.2111113.3..', '.21111113...', '2121113.3...', '21111111112.', '.2111111112.'];

const ROBOT_HEAD = ['.....2......', '.....3......', '...222222...', '..21111112..', '..21133312..', '..21322312..', '..21111112..', '..22222222..',
  '.2111111112.', '21211111121.', '21211111121.', '.2111111112.', '..22222222..'];
const BUNNY = ['...22..22...', '...21..21...', '...21..21...', '...21..21...', '...21..21...', '..22111122..', '.2111111112.', '.2132111321.',
  '.2111111112.', '.2111221112.', '..21111112..', '..22111122..', '.2111111112.', '211113311112', '211133331112', '.2111331112.', '.2111111112.',
  '..21111112..', '.2211..1122.', '.222....222.'];
const GHOST = ['....2222....', '..22111122..', '.2111111112.', '.2133113312.', '.2132113212.', '211111111112', '211111111112', '211122221112',
  '211111111112', '211111111112', '211111111112', '211111111112', '211111111112', '21221221122.', '2..2..2..2..', '............', '............', '............'];

const ring = (cx, cy, r0, r1) => (x, y) => {
  const d = Math.hypot(x - cx, y - cy);
  return d >= r0 && d <= r1;
};

function bubblerSprites() {
  const bubble = paint((x, y) => {
    const d = Math.hypot(x - 12, y - 10.5);
    if (d > 8.3 && d < 9.8) return '1';
    if (Math.hypot(x - 8, y - 6.5) < 1.6) return '1'; // the shine
    if (Math.hypot(x - 10.5, y - 5.2) < 0.8) return '1';
    return '.';
  }, false);
  const pop = paint((x, y) => (ring(12, 10.5, 8.6, 10.2)(x, y) && Math.floor(Math.atan2(y - 10.5, x - 12) * 3) % 2 === 0 ? '1' : '.'), false);
  const trapped = (squash) => paint((x, y) => {
    const d = Math.hypot((x - 12) * squash, (y - 10.5) / squash);
    if (d > 8.4 && d < 10.2) return '1';
    if (Math.hypot(x - 8, y - 6) < 1.8) return '3';
    const e = Math.hypot((x - 12) * 1.1, y - 11.5);
    if (e < 5.2) return Math.hypot(x - 10, y - 10) < 1.3 || Math.hypot(x - 14, y - 10) < 1.3 ? '2' : '3';
    return '.';
  }, true);
  const cherry = paint((x, y) => {
    const a = Math.hypot(x - 8, y - 15);
    const b = Math.hypot(x - 16, y - 14);
    if (Math.hypot(x - 6.5, y - 13.5) < 1.2 || Math.hypot(x - 14.5, y - 12.5) < 1.2) return '3';
    if (a < 4.2 || b < 4.2) return '1';
    if (a < 5.3 || b < 5.3) return '2';
    // the stems
    const t1 = (y - 4) / 7;
    if (y > 4 && y < 11.5 && Math.abs(x - (14 - 5.5 * t1)) < 1.1) return '2';
    const t2 = (y - 4) / 6;
    if (y > 4 && y < 10.5 && Math.abs(x - (14 + 2 * t2)) < 1.1) return '2';
    if (y > 2.5 && y < 5 && x > 13 && x < 19) return y < 3.8 ? '2' : '1';
    return '.';
  }, true);
  return {
    mc1: 0, // shared multicolour 2: black outlines
    mc2: 1, // shared multicolour 3: white eyes and shine
    sprites: [
      mc('bub stand', 13, bub(BUB_TOP, BUB_BODY, LEGS_STAND), true),
      mc('bub walk', 13, bub(BUB_TOP, BUB_BODY, LEGS_WALK), true),
      mc('bub jump', 13, bub(BUB_TOP, BUB_BODY, LEGS_JUMP), true),
      mc('bub blow', 13, bub(BUB_TOP, BUB_BLOW_BODY, LEGS_STAND), true),
      mc('bub hurt', 10, bub(BUB_HURT_TOP, BUB_BODY, LEGS_JUMP), true),
      hires('bubble', 3, bubble),
      hires('bubble pop', 3, pop),
      mc('robot', 15, [...ROBOT_HEAD, '...21..12...', '...21..12...', '..221..122..', '..222..222..'], true),
      mc('robot step', 15, [...ROBOT_HEAD, '...21...12..', '..21....21..', '.221....122.', '.222....222.'], true),
      mc('bunny', 10, BUNNY, true),
      mc('ghost', 14, GHOST, true),
      mc('trapped', 3, trapped(1)),
      mc('trapped wobble', 3, trapped(1.08)),
      mc('cherry', 2, cherry),
    ],
  };
}

/** A bevelled multicolour block: light top-left, its colour, a dark bottom-right. */
const BLOCK = ['3333', '3112', '3112', '3112', '3112', '3112', '3112', '2222'];
const BRICK = ['3332', '1112', '1112', '2222', '3233', '1211', '1211', '2222'];
const MARK = (letter) => glyph(letter.charCodeAt(0) - 64).map((r) => r);

function bubblerTiles() {
  const set = newTileset();
  set.colors = { border: 11, bg: 0, mc1: 11, mc2: 15 };
  set.flags = ['solid', 'wall', 'flag2', 'flag3', 'flag4', 'flag5', 'flag6', 'flag7'];
  const SOLID = 1;
  const WALL = 2;
  // four looks for the rounds: purple, blue, orange, green
  const looks = [['a', 4], ['b', 6], ['c', 2], ['d', 5]];
  let i = 64;
  for (const [k, color] of looks) {
    set.tiles[i++] = { name: `wall ${k}`, multi: true, color, flags: SOLID | WALL, rows: BRICK };
    set.tiles[i++] = { name: `block ${k}`, multi: true, color, flags: SOLID, rows: BLOCK };
  }
  // spawn markers: shown in the MAP editor, swapped for spaces by the game
  set.tiles[72] = { name: 'spawn player', multi: false, color: 13, flags: 0, rows: MARK('P') };
  set.tiles[73] = { name: 'spawn robot', multi: false, color: 15, flags: 0, rows: MARK('R') };
  set.tiles[74] = { name: 'spawn bunny', multi: false, color: 10, flags: 0, rows: MARK('B') };
  set.tiles[75] = { name: 'spawn ghost', multi: false, color: 14, flags: 0, rows: MARK('G') };
  // the title's big letters: quarter blocks, a 2x2 pixel of the font each
  for (let q = 1; q < 16; q++) {
    const r = [...Array(8)].map((_, y) => [...Array(8)].map((_, x) => (q & (1 << ((y >= 4 ? 2 : 0) + (x >= 4 ? 1 : 0))) ? '1' : '.')).join(''));
    set.tiles[80 + q] = { name: q === 15 ? 'logo' : '', multi: false, color: 7, flags: 0, rows: r };
  }
  return set;
}

// blip's Bubbler rounds (crates/bubbler/src/levels.rs): 26 columns x 24 rows.
// '#' block, A / B player spawns, w walker, h hopper, g ghost.
const BLIP_LEVELS = (() => {
  const src = fs.readFileSync(path.join(here, 'examples/bubbler/levels.txt'), 'utf8');
  return src.trim().split(/\n\s*\n/).map((block) => block.trim().split('\n').map((l) => l.trim()));
})();

function bubblerMap(tiles) {
  const map = { legend: { ' ': 32 }, rooms: [] };
  const named = (n) => tiles.tiles.findIndex((t) => t?.name === n);
  // the title
  const title = blankRoom(map, 'title');
  const text = (x, y, s) => [...s].forEach((ch, i) => setCell(map, title, x + i, y, ch.charCodeAt(0) >= 64 ? ch.charCodeAt(0) - 64 : ch.charCodeAt(0)));
  const word = 'BUBBLER';
  const x0 = Math.floor((COLS - (word.length * 3 + word.length - 1)) / 2);
  [...word].forEach((ch, k) => {
    const g = glyph(ch.charCodeAt(0) - 64); // 8 rows; columns 1..5 hold the letter
    for (let ty = 0; ty < 4; ty++) {
      for (let tx = 0; tx < 3; tx++) {
        let q = 0;
        for (let dy = 0; dy < 2; dy++) for (let dx = 0; dx < 2; dx++) if (g[ty * 2 + dy]?.[1 + tx * 2 + dx] === '1') q |= 1 << (dy * 2 + dx);
        if (q) setCell(map, title, x0 + k * 4 + tx, 3 + ty, 80 + q);
      }
    }
  });
  text(9, 9, 'A ONE-SCREEN PLATFORMER');
  text(5, 12, 'UP JUMPS, FIRE BLOWS A BUBBLE.');
  text(5, 13, 'CATCH THE MONSTERS, THEN TOUCH');
  text(5, 14, 'THEM TO POP THEM INTO CHERRIES.');
  for (let x = 0; x < COLS; x++) setCell(map, title, x, 24, named('block a'));
  for (let x = 0; x < COLS; x++) setCell(map, title, x, 0, named('wall a'));
  map.rooms.push(title);

  // the rounds: blip's 26 columns spread over the C64's 40
  const look = ['a', 'b', 'c', 'd'];
  BLIP_LEVELS.forEach((level, n) => {
    const room = blankRoom(map, `round ${n + 1}`);
    const wall = named(`wall ${look[n % 4]}`);
    const block = named(`block ${look[n % 4]}`);
    level.forEach((line, r) => {
      [...line].forEach((ch, c) => {
        const from = Math.floor((c * COLS) / 26);
        const to = Math.floor(((c + 1) * COLS) / 26);
        for (let x = from; x < to; x++) {
          const y = r + 1; // row 0 is the score line
          if (ch === '#') setCell(map, room, x, y, c === 0 || c === 25 ? wall : block);
          else if (x === from && ch === 'A') setCell(map, room, x, y, named('spawn player'));
          else if (x === from && ch === 'w') setCell(map, room, x, y, named('spawn robot'));
          else if (x === from && ch === 'h') setCell(map, room, x, y, named('spawn bunny'));
          else if (x === from && ch === 'g') setCell(map, room, x, y, named('spawn ghost'));
        }
      });
    });
    map.rooms.push(room);
  });
  return map;
}

// blip's Bubbler theme (crates/bubbler/src/tunes.rs): 152 bpm in eighths, C major.
const H = -2;
const RR = -1;
const LEAD = [
  [76, H, 79, H, 84, H, 79, 76], [81, H, 76, H, 72, H, 76, 81], [77, H, 81, 84, H, 81, 77, H], [79, H, H, 83, 86, H, 83, H],
  [76, 79, 84, H, 88, H, 86, 84], [81, H, 84, H, 88, H, 86, 84], [81, H, 77, H, 79, H, 83, 86], [84, H, H, H, RR, 79, 81, 83],
  [84, H, 81, H, 77, H, 81, 84], [86, H, 83, H, 79, H, 83, 86], [88, H, 83, H, 79, 83, 88, H], [84, H, 81, H, 76, H, 81, H],
  [77, 81, 84, 89, H, 88, 86, 84], [86, H, 83, 79, H, 81, 83, 86], [88, H, 86, H, 84, H, 79, H], [84, H, H, H, RR, H, RR, H],
];
// the chord roots per half bar: C Am F G C Am F/G C | C F G Em Am F G C/G C
const ROOTS = [[48, 48], [45, 45], [41, 41], [43, 43], [48, 48], [45, 45], [41, 43], [48, 48],
  [48, 48], [41, 41], [43, 43], [40, 40], [45, 45], [41, 41], [43, 43], [48, 43]];
const NAMES = ['C-', 'C#', 'D-', 'D#', 'E-', 'F-', 'F#', 'G-', 'G#', 'A-', 'A#', 'B-'];
const note = (n, ins) => `${NAMES[n % 12]}${Math.floor(n / 12)} ${ins.toString(16).toUpperCase()} ...`;

function bubblerMusic() {
  // MIDI 60 (middle C) is our C-4: the tracker's C-0 is MIDI 12; blip plays the lead an octave down
  const lead = [];
  const bass = [];
  for (let p = 0; p < 4; p++) {
    const ls = [];
    const bs = [];
    for (let b = p * 4; b < p * 4 + 4; b++) {
      for (const n of LEAD[b]) ls.push(n === H ? EMPTY : n === RR ? '=== . ...' : note(n - 24, 0));
      for (const root of ROOTS[b]) for (const off of [0, 12, 7, 12]) bs.push(note(root - 12 + off, 1));
    }
    lead.push({ name: `theme lead ${p + 1}`, speed: 10, steps: ls });
    bass.push({ name: `theme bass ${p + 1}`, speed: 10, steps: bs });
  }
  const drums = [];
  for (let b = 0; b < 4; b++) {
    for (let s = 0; s < 8; s++) {
      if (b === 3 && s >= 6) drums.push(note(60, 3)); // a fill into the next phrase
      else if (s % 4 === 0) drums.push(note(36, 2));
      else if (s % 4 === 2) drums.push(note(60, 3));
      else drums.push(note(84, 4));
    }
  }
  const sfx = (name, speed, steps) => ({ name, speed, steps });
  const patterns = [
    ...lead, ...bass, { name: 'theme drums', speed: 10, steps: drums },
    sfx('jump', 1, ['C-4 5 U28', ...Array(9).fill(EMPTY), '=== . ...', ...Array(4).fill(EMPTY)]),
    sfx('blow', 1, ['G-6 6 D20', EMPTY, EMPTY, EMPTY, EMPTY, '=== . ...', EMPTY, EMPTY]),
    sfx('catch', 2, ['C-5 7 ...', 'E-5 7 ...', 'G-5 7 ...', 'C-6 7 ...', '=== . ...', EMPTY]),
    sfx('pop', 1, ['C-6 6 ...', EMPTY, 'G-5 6 ...', EMPTY, '=== . ...', EMPTY, EMPTY, EMPTY]),
    sfx('yum', 3, ['G-5 8 ...', 'C-6 8 ...', 'E-6 8 ...', 'G-6 8 ...', 'C-7 8 ...', EMPTY, '=== . ...', EMPTY]),
    sfx('ouch', 2, ['C-5 5 D20', ...Array(14).fill(EMPTY), '=== . ...', EMPTY, EMPTY]),
    sfx('clear', 5, ['C-5 8 ...', 'E-5 8 ...', 'G-5 8 ...', 'C-6 8 ...', EMPTY, 'G-5 8 ...', 'C-6 8 ...', EMPTY, EMPTY, '=== . ...', EMPTY, EMPTY]),
  ];
  return {
    instruments: [
      { name: 'lead', wave: 'pulse', a: 0, d: 9, s: 7, r: 5, pw: 6, pwm: 2, vib: 3, arp: [0, 0], slide: 0, drum: false },
      { name: 'bass', wave: 'pulse', a: 0, d: 7, s: 2, r: 3, pw: 3, pwm: 0, vib: 0, arp: [0, 0], slide: 0, drum: false },
      { name: 'kick', wave: 'tri', a: 0, d: 7, s: 0, r: 0, pw: 8, pwm: 0, vib: 0, arp: [0, 0], slide: -40, drum: true },
      { name: 'snare', wave: 'noise', a: 0, d: 7, s: 0, r: 6, pw: 8, pwm: 0, vib: 0, arp: [0, 0], slide: 0, drum: false },
      { name: 'hat', wave: 'noise', a: 0, d: 2, s: 0, r: 2, pw: 8, pwm: 0, vib: 0, arp: [0, 0], slide: 0, drum: false },
      { name: 'zap', wave: 'tri', a: 0, d: 8, s: 0, r: 5, pw: 8, pwm: 0, vib: 0, arp: [0, 0], slide: 0, drum: false },
      { name: 'puff', wave: 'noise', a: 0, d: 5, s: 0, r: 4, pw: 8, pwm: 0, vib: 0, arp: [0, 0], slide: 0, drum: false },
      { name: 'bling', wave: 'pulse', a: 0, d: 6, s: 4, r: 6, pw: 4, pwm: 3, vib: 0, arp: [0, 0], slide: 0, drum: false },
      { name: 'chime', wave: 'tri', a: 0, d: 5, s: 6, r: 6, pw: 8, pwm: 0, vib: 2, arp: [4, 7], slide: 0, drum: false },
    ],
    patterns,
    songs: [{ name: 'theme', loop: 0, rows: [0, 1, 2, 3].map((i) => [i, 4 + i, 8]) }],
  };
}

// ── the empty game ──────────────────────────────────────────────────────────

function blankAssets() {
  const tiles = newTileset();
  tiles.colors = { border: 6, bg: 0, mc1: 11, mc2: 12 };
  tiles.tiles[64] = { name: 'ground', multi: true, color: 5, flags: 1, rows: BLOCK };
  tiles.tiles[65] = { name: 'brick', multi: true, color: 2, flags: 1, rows: BRICK };
  const map = { legend: { ' ': 32 }, rooms: [] };
  const room = blankRoom(map, 'start');
  for (let x = 0; x < COLS; x++) for (const y of [ROWS - 2, ROWS - 1]) setCell(map, room, x, y, 64);
  for (let x = 6; x < 14; x++) setCell(map, room, x, 17, 65);
  for (let x = 24; x < 34; x++) setCell(map, room, x, 13, 65);
  map.rooms.push(room);
  return {
    sprites: { mc1: 0, mc2: 1, sprites: [mc('hero', 13, bub(BUB_TOP, BUB_BODY, LEGS_STAND), true), mc('hero walk', 13, bub(BUB_TOP, BUB_BODY, LEGS_WALK), true)] },
    tiles,
    map,
    music: {
      instruments: [
        { name: 'lead', wave: 'pulse', a: 0, d: 9, s: 6, r: 4, pw: 8, pwm: 2, vib: 3, arp: [0, 0], slide: 0, drum: false },
        { name: 'bass', wave: 'saw', a: 0, d: 6, s: 4, r: 2, pw: 8, pwm: 0, vib: 0, arp: [0, 0], slide: 0, drum: false },
        { name: 'blip', wave: 'tri', a: 0, d: 6, s: 0, r: 4, pw: 8, pwm: 0, vib: 0, arp: [0, 0], slide: 0, drum: false },
      ],
      patterns: [
        { name: 'tune', speed: 8, steps: ['C-4 0 ...', EMPTY, 'E-4 0 ...', EMPTY, 'G-4 0 ...', EMPTY, 'E-4 0 ...', EMPTY, 'A-4 0 ...', EMPTY, 'G-4 0 ...', EMPTY, 'E-4 0 ...', EMPTY, '=== . ...', EMPTY] },
        { name: 'bass', speed: 8, steps: ['C-2 1 ...', EMPTY, 'C-3 1 ...', EMPTY, 'G-2 1 ...', EMPTY, 'C-3 1 ...', EMPTY, 'A-2 1 ...', EMPTY, 'A-3 1 ...', EMPTY, 'G-2 1 ...', EMPTY, 'G-3 1 ...', EMPTY] },
        { name: 'blip', speed: 1, steps: ['C-5 2 U20', EMPTY, EMPTY, EMPTY, EMPTY, EMPTY, '=== . ...', EMPTY] },
      ],
      songs: [{ name: 'tune', loop: 0, rows: [[0, 1, -1]] }],
    },
  };
}

// ── Bouncer ─────────────────────────────────────────────────────────────────

function bouncerAssets() {
  const tiles = newTileset();
  tiles.colors = { border: 6, bg: 0, mc1: 11, mc2: 1 };
  tiles.flags = ['solid', 'brick', 'steel', 'flag3', 'flag4', 'flag5', 'flag6', 'flag7'];
  // a brick is two tiles wide: a left and a right half, in five colours
  const LEFT = ['3333', '3111', '3111', '3111', '3111', '3111', '2222', '....'];
  const RIGHT = ['3332', '1112', '1112', '1112', '1112', '1112', '2222', '....'];
  // multicolour tiles can only have colours 0-7 of their own (a C64 limit)
  const colours = [['red', 2], ['purple', 4], ['yellow', 7], ['green', 5], ['blue', 6]];
  let i = 64;
  for (const [name, color] of colours) {
    tiles.tiles[i++] = { name: `${name} brick`, multi: true, color, flags: 3, rows: LEFT };
    tiles.tiles[i++] = { name: `${name} brick right`, multi: true, color, flags: 3, rows: RIGHT };
  }
  tiles.tiles[74] = { name: 'steel', multi: true, color: 7, flags: 5, rows: LEFT.map((r) => r.replace(/1/g, '3')) };
  tiles.tiles[75] = { name: 'steel right', multi: true, color: 7, flags: 5, rows: RIGHT.map((r) => r.replace(/1/g, '3')) };
  tiles.tiles[76] = { name: 'frame', multi: true, color: 6, flags: 1, rows: ['3332', '3112', '3112', '3112', '3112', '3112', '3112', '2222'] };
  const map = { legend: { ' ': 32 }, rooms: [] };
  // the layouts, a character per brick: r p y g b colours, S steel, . none
  const LAYOUTS = [
    ['rrrrrrrrrrrrrrrrr', 'ppppppppppppppppp', 'yyyyyyyyyyyyyyyyy', 'ggggggggggggggggg', 'bbbbbbbbbbbbbbbbb'],
    ['r.r.r.r.r.r.r.r.r', '.p.p.p.p.p.p.p.p.', 'y.y.y.y.y.y.y.y.y', '.g.g.g.g.g.g.g.g.', 'b.b.b.b.b.b.b.b.b', '.................', 'SSSS.........SSSS'],
    ['........r........', '.......rpr.......', '......rpypr......', '.....rpygypr.....', '....rpygbgypr....', '.....rpygypr.....', '......rpypr......', '.......rpr.......', '........r........'],
    ['rrrrr...S...rrrrr', 'ppppp...S...ppppp', 'yyyyy...S...yyyyy', 'SSSSS...S...SSSSS', 'ggggg.......ggggg', 'bbbbb.......bbbbb'],
  ];
  const brick = { r: 64, p: 66, y: 68, g: 70, b: 72, S: 74 };
  LAYOUTS.forEach((layout, n) => {
    const room = blankRoom(map, `level ${n + 1}`);
    // the frame: top row 1, sides at columns 2 and 37
    for (let x = 2; x < 38; x++) setCell(map, room, x, 1, 76);
    for (let y = 1; y < 25; y++) (setCell(map, room, 2, y, 76), setCell(map, room, 37, y, 76));
    layout.forEach((line, r) => [...line].forEach((ch, c) => {
      if (!(ch in brick)) return;
      const x = 3 + c * 2;
      setCell(map, room, x, 4 + r, brick[ch]);
      setCell(map, room, x + 1, 4 + r, brick[ch] + 1);
    }));
    map.rooms.push(room);
  });
  const ball = paint((x, y) => {
    const d = Math.hypot(x - 4, y - 4);
    if (Math.hypot(x - 3, y - 2.8) < 1.1) return '3';
    if (d < 3.2) return '1';
    if (d < 4.1) return '2';
    return '.';
  }, true);
  const bat = (w) => paint((x, y) => {
    if (y < 2 || y > 7 || x > w) return '.';
    if (y < 3) return '3';
    if (y > 6) return '2';
    return x < 3 || x > w - 2 ? '2' : '1';
  }, true);
  const capsule = (letter, colour) => {
    const g = glyph(letter.charCodeAt(0) - 64);
    return mc(`pickup ${letter.toLowerCase()}`, colour, paint((x, y) => {
      if (y < 1 || y > 10 || x > 21) return '.';
      const gx = Math.floor((x - 7) / 1.4);
      const gy = Math.floor(y - 1.5);
      if (gx >= 0 && gx < 8 && gy >= 0 && gy < 8 && g[gy]?.[gx + 1] === '1' && y > 2 && y < 9) return '3';
      if (y < 2 || y > 9) return '2';
      return '1';
    }, true));
  };
  return {
    sprites: {
      mc1: 0,
      mc2: 1,
      sprites: [
        mc('bat', 14, bat(23)),
        mc('ball', 7, ball),
        capsule('W', 5), capsule('M', 4), capsule('S', 3), capsule('L', 2),
      ],
    },
    tiles,
    map,
    music: {
      instruments: [
        { name: 'lead', wave: 'pulse', a: 0, d: 8, s: 6, r: 4, pw: 5, pwm: 3, vib: 2, arp: [0, 0], slide: 0, drum: false },
        { name: 'bass', wave: 'saw', a: 0, d: 6, s: 3, r: 2, pw: 8, pwm: 0, vib: 0, arp: [0, 0], slide: 0, drum: false },
        { name: 'kick', wave: 'tri', a: 0, d: 7, s: 0, r: 0, pw: 8, pwm: 0, vib: 0, arp: [0, 0], slide: -40, drum: true },
        { name: 'ping', wave: 'tri', a: 0, d: 6, s: 0, r: 4, pw: 8, pwm: 0, vib: 0, arp: [0, 0], slide: 0, drum: false },
        { name: 'thud', wave: 'noise', a: 0, d: 4, s: 0, r: 3, pw: 8, pwm: 0, vib: 0, arp: [0, 0], slide: 0, drum: false },
        { name: 'chime', wave: 'pulse', a: 0, d: 6, s: 6, r: 6, pw: 4, pwm: 3, vib: 0, arp: [4, 7], slide: 0, drum: false },
      ],
      patterns: [
        { name: 'march lead', speed: 7, steps: ['E-4 0 ...', EMPTY, 'G-4 0 ...', 'A-4 0 ...', 'B-4 0 ...', EMPTY, 'A-4 0 ...', 'G-4 0 ...', 'E-4 0 ...', EMPTY, 'D-4 0 ...', EMPTY, 'E-4 0 ...', EMPTY, '=== . ...', EMPTY,
          'E-4 0 ...', EMPTY, 'G-4 0 ...', 'A-4 0 ...', 'B-4 0 ...', EMPTY, 'D-5 0 ...', 'B-4 0 ...', 'A-4 0 ...', EMPTY, 'G-4 0 ...', EMPTY, 'A-4 0 ...', EMPTY, '=== . ...', EMPTY] },
        { name: 'march bass', speed: 7, steps: [...Array(4)].flatMap((_, k) => ['E-2 1 ...', EMPTY, 'E-3 1 ...', EMPTY, 'B-2 1 ...', EMPTY, 'E-3 1 ...', EMPTY].map((s) => (k === 3 ? s.replace('E-', 'D-').replace('B-2', 'A-2') : s))) },
        { name: 'march drums', speed: 7, steps: [...Array(8)].flatMap(() => ['C-3 2 ...', EMPTY, 'C-7 4 ...', EMPTY]) },
        { name: 'bat', speed: 1, steps: ['C-5 3 ...', EMPTY, EMPTY, '=== . ...', EMPTY, EMPTY] },
        { name: 'brick', speed: 1, steps: ['G-5 3 ...', EMPTY, '=== . ...', EMPTY, EMPTY] },
        { name: 'steel', speed: 1, steps: ['C-4 4 ...', EMPTY, EMPTY, '=== . ...', EMPTY] },
        { name: 'pickup', speed: 2, steps: ['C-5 5 ...', 'E-5 5 ...', 'G-5 5 ...', 'C-6 5 ...', '=== . ...', EMPTY] },
        { name: 'lost', speed: 2, steps: ['G-4 3 D18', ...Array(16).fill(EMPTY), '=== . ...', EMPTY] },
        { name: 'clear', speed: 5, steps: ['C-5 5 ...', 'E-5 5 ...', 'G-5 5 ...', 'C-6 5 ...', EMPTY, EMPTY, '=== . ...', EMPTY] },
      ],
      songs: [{ name: 'march', loop: 0, rows: [[0, 1, 2]] }],
    },
  };
}

// ── Pixels ──────────────────────────────────────────────────────────────────

function pixelsAssets() {
  const ufo = paint((x, y) => {
    const dome = Math.hypot(x - 12, (y - 8) * 1.3);
    if (dome < 5 && y < 9) return Math.hypot(x - 10, y - 5) < 1.6 ? '3' : '2';
    const body = Math.hypot((x - 12) / 11, (y - 11) / 3.2);
    if (body < 1) return y < 10 ? '1' : Math.floor(x / 4) % 2 ? '3' : '1';
    if (body < 1.15) return '2';
    return '.';
  }, true);
  const tiles = newTileset();
  const map = { legend: { ' ': 32 }, rooms: [] };
  map.rooms.push(blankRoom(map, 'start'));
  return {
    sprites: { mc1: 0, mc2: 1, sprites: [mc('ufo', 4, ufo)] },
    tiles,
    map,
    music: {
      instruments: [{ name: 'ping', wave: 'tri', a: 0, d: 6, s: 0, r: 5, pw: 8, pwm: 0, vib: 0, arp: [0, 0], slide: 0, drum: false }],
      patterns: [{ name: 'ping', speed: 1, steps: ['C-6 0 ...', EMPTY, EMPTY, '=== . ...', EMPTY, EMPTY] }],
      songs: [],
    },
  };
}

// ── writing the projects ───────────────────────────────────────────────────

const MAKEFILE = (name) => `# Builds ${name} with a native cc65 (https://cc65.github.io), the same way
# the breadbin Studio builds it in the browser:
#
#   make            # -> game.prg; run it with x64sc game.prg, or any C64 emulator
#
# The Studio writes gen/ (the runtime and the assets as C) on every build; it
# is in the folder so this Makefile has it too. The program ends below $C000:
# the runtime keeps $C000-$FFFF for the screen, the tiles and the sprites.
# -Cl keeps local variables at fixed addresses (much faster on a 6502; the
# one catch is that a function can't call itself), as the Studio does.

# (sorted: the order the Studio links them in, so both make the same program)
SOURCES = $(sort $(wildcard src/*.c src/*.s gen/*.c gen/*.s))

game.prg: $(SOURCES) $(wildcard src/*.h) $(wildcard gen/*.h)
\tcl65 -t c64 -Oirs -Cl -I src -I gen -Wl -D,__HIMEM__=0xC000 -m game.map -o $@ $(SOURCES)

clean:
\trm -f game.prg game.map src/*.o gen/*.o

.PHONY: clean
`;

const README = (name, about, extra) => `# ${name}

${about}

Made with the breadbin Studio: open it in the Hack tab of
https://jacobandresen.github.io/breadbin/play/#hack, or build it here with
\`make\` and a native cc65.

## Files

- \`src/\`: the game, in C (cc65)
- \`assets/sprites.json\`, \`tiles.json\`, \`map.json\`, \`music.json\`: the
  SPRITES, TILES, MAP and MUSIC editors' work, as text
- \`gen/\`: written by every build (the runtime \`bb.h\` and the assets as C)
- \`Makefile\`: the same build with a native cc65
- \`game.prg\`: the program, written by every build when the project is in a
  folder on your disk (it is in \`.gitignore\`)
${extra}`;

function write(id, files) {
  const dir = path.join(OUT, id);
  fs.rmSync(dir, { recursive: true, force: true });
  for (const [p, text] of Object.entries(files)) {
    fs.mkdirSync(path.dirname(path.join(dir, p)), { recursive: true });
    fs.writeFileSync(path.join(dir, p), text);
  }
  fs.writeFileSync(path.join(dir, 'files.json'), `${JSON.stringify(Object.keys(files).sort(), null, 1)}\n`);
  console.log(`${id}: ${Object.keys(files).length} files`);
}

function project(id, name, about, assets, extra = '') {
  const src = path.join(here, 'examples', id, 'src');
  const files = {
    'project.json': `${pretty({ name, lang: 'c', entry: 'src/main.c', about })}\n`,
    'README.md': README(name, about, extra),
    Makefile: MAKEFILE(name),
    '.gitignore': '# what builds make (the Studio writes game.prg into a folder it saves to)\ngame.prg\ngame.map\n*.o\n',
  };
  for (const f of fs.readdirSync(src)) files[`src/${f}`] = fs.readFileSync(path.join(src, f), 'utf8');
  for (const [k, v] of Object.entries(assets)) files[`assets/${k}.json`] = `${pretty(v)}\n`;
  write(id, files);
}

const tiles = bubblerTiles();
project('bubbler', 'Bubbler', 'A one-screen platformer: blow bubbles, trap the monsters, pop them for points. After blip\'s Bubbler.',
  { sprites: bubblerSprites(), tiles, map: bubblerMap(tiles), music: bubblerMusic() },
  '\n## Playing\n\nJoystick left and right to run, up to jump, fire to blow a bubble. A\nmonster caught in a bubble floats up: touch it to pop it into a cherry.\nClear the room to go on to the next round.\n');
project('bouncer', 'Bouncer', 'Bricks, a bat and a ball: catch the falling pickups. After blip\'s Bouncer.', bouncerAssets(),
  '\n## Playing\n\nJoystick left and right move the bat, fire launches the ball. Pickups:\nW a wide bat, M three balls, S a slower ball, L a life.\n');
project('pixels', 'Pixels', 'A tour of the bitmap mode: lines, circles, fills, blits racing plots, bouncing balls, and peek and poke.', pixelsAssets(),
  '\n## The parts\n\nFire goes on to the next: shapes, plot against blit (timed), bouncing balls\n(blits with the flip pen, pre-shifted to move a pixel at a time, under a\nhardware sprite), and peek and poke.\n');
project('blank', 'My game', 'A sprite you can steer and a room to walk in: the smallest start.', blankAssets());
