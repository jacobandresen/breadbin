// sprites - the SPRITES editor's images (assets/sprites.json) and their C64
// form: 24x21 pixels, 63 bytes, padded to 64 for the VIC's sprite pointers.
//
//   { "mc1": 11, "mc2": 1,                     // the shared multicolours
//     "sprites": [ { "name": "player", "multi": true, "color": 5, "flip": true,
//                    "rows": ["....1111....", ...21 rows] } ] }
//
// A hires sprite's rows are 24 pixels: '.' see-through, '1' its colour. A
// multicolour sprite's are 12 double-wide pixels: '1' its colour, '2' and '3'
// the shared multicolours ($D025, $D026). With "flip", the game also gets the
// image mirrored, as SPR_<NAME>_FLIP (the C64 can't mirror a sprite itself).

export const W = 24;
export const H = 21;

/** Pixels per row: 24 hires, 12 multicolour. */
export const width = (s) => (s.multi ? 12 : 24);

/** A new, empty sprite. */
export const blankSprite = (name = 'sprite', multi = true) => ({ name, multi, color: 1, flip: false, rows: Array(H).fill('.'.repeat(multi ? 12 : 24)) });

// multicolour bit pairs: '1' -> %10 (sprite colour), '2' -> %01 ($D025), '3' -> %11 ($D026)
const PAIR = { '.': 0, 1: 2, 2: 1, 3: 3 };
const UNPAIR = ['.', '2', '1', '3'];

/** The 64 bytes the VIC reads for `s`. */
export function encodeSprite(s) {
  const out = new Uint8Array(64);
  for (let y = 0; y < H; y++) {
    const row = (s.rows[y] || '').padEnd(width(s), '.');
    for (let b = 0; b < 3; b++) {
      let v = 0;
      if (s.multi) for (let i = 0; i < 4; i++) v |= (PAIR[row[b * 4 + i]] ?? 0) << (6 - 2 * i);
      else for (let i = 0; i < 8; i++) if (row[b * 8 + i] !== '.' && row[b * 8 + i] !== undefined) v |= 0x80 >> i;
      out[y * 3 + b] = v;
    }
  }
  return out;
}

/** The sprite in 63 or 64 VIC bytes, back as rows (for importing .spr/.bin files). */
export function decodeSprite(bytes, multi, name = 'sprite') {
  const rows = [];
  for (let y = 0; y < H; y++) {
    let row = '';
    for (let b = 0; b < 3; b++) {
      const v = bytes[y * 3 + b] || 0;
      if (multi) for (let i = 0; i < 4; i++) row += UNPAIR[(v >> (6 - 2 * i)) & 3];
      else for (let i = 0; i < 8; i++) row += v & (0x80 >> i) ? '1' : '.';
    }
    rows.push(row);
  }
  return { name, multi, color: 1, flip: false, rows };
}

/** `s` mirrored left to right. */
export const mirror = (s) => ({ ...s, rows: s.rows.map((r) => [...r].reverse().join('')) });

/** `s` in the other mode: hires pairs become one multicolour pixel and back. */
export function convert(s, multi) {
  if (multi === s.multi) return s;
  const rows = s.rows.map((r) => (multi
    ? [...Array(12)].map((_, i) => (r[2 * i] !== '.' || r[2 * i + 1] !== '.' ? '1' : '.')).join('')
    : [...r].map((p) => (p === '.' ? '..' : '11')).join('')));
  return { ...s, multi, rows };
}

/** The images the game sees, in order: each sprite, then its mirror if flipped. */
export function images(sheet) {
  const out = [];
  for (const s of sheet.sprites) {
    out.push({ ...s, constName: s.name });
    if (s.flip) out.push({ ...mirror(s), constName: `${s.name}_flip` });
  }
  return out;
}
