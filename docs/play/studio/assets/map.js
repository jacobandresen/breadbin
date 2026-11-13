// map - the MAP editor's rooms (assets/map.json): screens of 40x25 tiles,
// written as text, one character per cell, through a legend:
//
//   { "legend": { " ": 32, "#": 64, "=": 65 },
//     "rooms": [ { "name": "round 1", "rows": ["########...", ...25 rows] } ] }
//
// The game gets each room run-length coded (bb_room in bb.c): a byte is a
// tile, except $FF, which is followed by a count and the tile to repeat.

export const COLS = 40;
export const ROWS = 25;

/** The legend's characters, in the order the editor hands them out. */
// The printable ASCII ones first (easy to read and type), then Latin-1 and
// Latin Extended-A letters, so all 256 tiles can have one.
export const LEGEND_CHARS = ' #=-+*%&@$o0123456789abcdefghijklmnpqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ!?<>[]{}()/\\|^~:;,._`\'"' +
  String.fromCharCode(...Array.from({ length: 0x17f - 0xa1 + 1 }, (_, i) => 0xa1 + i).filter((c) => c !== 0xad)); // (not the soft hyphen, which hides)

/** The tiles of room `room` as a 1000-entry array (unknown characters are tile 32, a space). */
export function roomTiles(map, room) {
  const out = new Uint8Array(COLS * ROWS).fill(32);
  for (let y = 0; y < ROWS; y++) {
    const row = room.rows[y] || '';
    for (let x = 0; x < COLS; x++) {
      const t = map.legend[row[x] ?? ' '];
      out[y * COLS + x] = t ?? 32;
    }
  }
  return out;
}

/** The legend character for tile `t`, adding one if the legend lacks it. */
export function charFor(map, t) {
  for (const [ch, v] of Object.entries(map.legend)) if (v === t) return ch;
  const ch = [...LEGEND_CHARS].find((c) => !(c in map.legend));
  if (!ch) throw new Error('the map uses more different tiles than its legend can name');
  map.legend[ch] = t;
  return ch;
}

/** Put tile `t` at x, y of `room` (mutates the room's rows and maybe the legend). */
export function setCell(map, room, x, y, t) {
  const ch = charFor(map, t);
  const row = (room.rows[y] || '').padEnd(COLS, ' ');
  room.rows[y] = row.slice(0, x) + ch + row.slice(x + 1);
}

export const blankRoom = (map, name = 'room') => ({ name, rows: Array(ROWS).fill(charFor(map, 32).repeat(COLS)) });

/** Run-length code 1000 tiles (runs of 4 or more, and any $FF, become $FF, count, tile). */
export function rle(tiles) {
  const out = [];
  for (let i = 0; i < tiles.length; ) {
    let n = 1;
    while (i + n < tiles.length && tiles[i + n] === tiles[i] && n < 255) n++;
    if (n >= 4 || tiles[i] === 0xff) {
      out.push(0xff, n, tiles[i]);
      i += n;
    } else {
      out.push(tiles[i]);
      i += 1;
    }
  }
  return Uint8Array.from(out);
}

/** The inverse of rle() (what bb_room does on the C64), for the tests. */
export function unrle(bytes, n = COLS * ROWS) {
  const out = [];
  for (let i = 0; out.length < n && i < bytes.length; ) {
    if (bytes[i] === 0xff) {
      for (let k = 0; k < bytes[i + 1]; k++) out.push(bytes[i + 2]);
      i += 3;
    } else out.push(bytes[i++]);
  }
  return Uint8Array.from(out.slice(0, n));
}
