// tiles - the TILES editor's character set (assets/tiles.json): up to 256
// tiles of 8x8, the font in 0..63, and what the game needs to know of each.
//
//   { "colors": { "border": 0, "bg": 0, "mc1": 11, "mc2": 12 },
//     "flags": ["solid", "ladder", ...],        // names of flag bits 0..7
//     "tiles": [ { "name": "brick", "multi": false, "color": 2, "flags": 1,
//                  "rows": ["11111111", ...8 rows] }, ... ] }
//
// Hires rows are 8 pixels ('.' background, '1' the tile's colour);
// multicolour rows are 4 double-wide pixels: '1' the tile's colour (0..7
// only, a C64 limit), '2' and '3' the shared multicolours ($D022, $D023).

import { fontTiles } from './font.js';

export const COUNT = 256;
export const FONT = 64; // tiles 0..63 are the font

export const blankTile = (name = '', multi = false) => ({ name, multi, color: 1, flags: 0, rows: Array(8).fill(multi ? '....' : '........') });

// multicolour bit pairs: '1' -> %11 (colour RAM), '2' -> %01 ($D022), '3' -> %10 ($D023)
const PAIR = { '.': 0, 1: 3, 2: 1, 3: 2 };
const UNPAIR = ['.', '2', '3', '1'];

/** The 8 bytes of tile `t`. */
export function encodeTile(t) {
  const out = new Uint8Array(8);
  if (!t) return out;
  for (let y = 0; y < 8; y++) {
    const row = t.rows[y] || '';
    let v = 0;
    if (t.multi) for (let i = 0; i < 4; i++) v |= (PAIR[row[i]] ?? 0) << (6 - 2 * i);
    else for (let i = 0; i < 8; i++) if (row[i] && row[i] !== '.') v |= 0x80 >> i;
    out[y] = v;
  }
  return out;
}

/** Tile rows from 8 bytes (for importing a character set). */
export function decodeTile(bytes, multi = false, name = '') {
  const rows = [];
  for (let y = 0; y < 8; y++) {
    const v = bytes[y] || 0;
    rows.push(multi
      ? [0, 1, 2, 3].map((i) => UNPAIR[(v >> (6 - 2 * i)) & 3]).join('')
      : [...Array(8)].map((_, i) => (v & (0x80 >> i) ? '1' : '.')).join(''));
  }
  return { name, multi, color: 1, flags: 0, rows };
}

/** The 2048-byte character set. */
export function encodeCharset(set) {
  const out = new Uint8Array(COUNT * 8);
  set.tiles.slice(0, COUNT).forEach((t, i) => out.set(encodeTile(t), i * 8));
  return out;
}

/** The colour RAM value of each tile: its colour, +8 for multicolour. */
export const tileColors = (set) => [...Array(COUNT)].map((_, i) => {
  const t = set.tiles[i];
  return t ? (t.multi ? (t.color & 7) | 8 : t.color & 15) : 1;
});

export const tileFlags = (set) => [...Array(COUNT)].map((_, i) => (set.tiles[i]?.flags ?? 0) & 255);

/** A new tile set: the font, then empty tiles. */
export function newTileset() {
  const tiles = [...fontTiles()];
  while (tiles.length < COUNT) tiles.push(null);
  return { colors: { border: 0, bg: 0, mc1: 11, mc2: 12 }, flags: ['solid', 'ladder', 'hurt', 'pickup', 'flag4', 'flag5', 'flag6', 'flag7'], tiles };
}
