// export - the game as a web page that runs without the Studio: one HTML
// file (export/player.html) with breadbin's own small C64
// (crates/breadbin/src/machine.rs, the engine's WebAssembly), the built PRG
// and the font written into it as base64. It opens in any browser, offline.

import { engineBytes } from '../engine.js';
import { glyph } from './assets/font.js';

/**
 * The character ROM the exported C64 shows where a C64 shows its own (banks 0
 * and 2): the Studio's font, both sets alike, with the reversed letters in the
 * upper half as the C64's ROM has them. (Studio games bring their own tiles.)
 */
export function charRom() {
  const rom = new Uint8Array(4096);
  const letters = [...Array(64)].map((_, n) => glyph(n).map((r) => parseInt(r.replace(/\./g, '0'), 2)));
  for (let set = 0; set < 2; set++) {
    const put = (n, rows) => rows.forEach((b, y) => {
      rom[set * 2048 + n * 8 + y] = b;
      rom[set * 2048 + (n + 128) * 8 + y] = b ^ 0xff;
    });
    letters.forEach((rows, n) => put(n, rows));
    // the lower-case set has its capitals at 65-90 (its 1-26, the small letters, show as capitals too)
    if (set === 1) for (let n = 1; n <= 26; n++) put(64 + n, letters[n]);
    else for (const [n, rows] of Object.entries(LINES)) put(Number(n), rows);
    for (const [n, rows] of Object.entries(BLOCKS)) put(Number(n), rows);
  }
  return rom;
}

// The C64's graphics characters, drawn for the Studio (not copied from the
// ROM): rows of 8 pixels, by screen code.
const V = 0x18; // a vertical line through the middle, two pixels wide
const rows = (f) => [...Array(8)].map((_, y) => f(y) & 0xff);
const span = (a, b, v) => rows((y) => (y >= a && y <= b ? v : 0));
const or = (...rs) => rows((y) => rs.reduce((acc, r) => acc | r[y], 0));
const H = (v = 0xff) => span(3, 4, v); // a horizontal line through the middle
const pic = (...lines) => lines.map((l) => parseInt(l.replace(/\./g, '0').replace(/#/g, '1'), 2));
const BLOCKS = {
  96: span(0, 7, 0),
  97: span(0, 7, 0xf0),
  98: span(4, 7, 0xff),
  99: span(0, 0, 0xff),
  100: span(7, 7, 0xff),
  101: span(0, 7, 0x80),
  102: rows((y) => (y & 2 ? 0x33 : 0xcc)),
  103: span(0, 7, 0x01),
  104: rows((y) => (y < 4 ? 0 : y & 2 ? 0x33 : 0xcc)),
  105: rows((y) => 0xff << y),
  106: span(0, 7, 0x03),
  107: or(span(0, 7, V), H(0x1f)),
  108: span(4, 7, 0x0f),
  109: or(span(0, 4, V), H(0x1f)),
  110: or(span(3, 7, V), H(0xf8)),
  111: span(6, 7, 0xff),
  112: or(span(3, 7, V), H(0x1f)),
  113: or(span(0, 4, V), H()),
  114: or(span(3, 7, V), H()),
  115: or(span(0, 7, V), H(0xf8)),
  116: span(0, 7, 0xc0),
  117: span(0, 7, 0xe0),
  118: span(0, 7, 0x07),
  119: span(0, 1, 0xff),
  120: span(0, 2, 0xff),
  121: span(5, 7, 0xff),
  122: rows((y) => 0xff >> (7 - y)),
  123: span(4, 7, 0xf0),
  124: span(0, 3, 0x0f),
  125: or(span(0, 4, V), H(0xf8)),
  126: span(0, 3, 0xf0),
  127: or(span(0, 3, 0xf0), span(4, 7, 0x0f)),
};
const LINES = {
  64: H(),
  65: pic('...#....', '..###...', '.#####..', '#######.', '#######.', '...#....', '..###...', '........'),
  66: span(0, 7, V),
  67: span(2, 2, 0xff),
  68: span(1, 1, 0xff),
  69: span(5, 5, 0xff),
  70: span(6, 6, 0xff),
  71: span(0, 7, 0x20),
  72: span(0, 7, 0x04),
  73: or(span(4, 7, V), span(3, 3, 0xf0)),
  74: or(span(0, 3, V), span(4, 4, 0x0f)),
  75: or(span(0, 3, V), span(4, 4, 0xf0)),
  76: or(span(0, 7, 0x80), span(7, 7, 0xff)),
  77: rows((y) => 0x80 >> y),
  78: rows((y) => 0x01 << y),
  79: or(span(0, 7, 0x80), span(0, 0, 0xff)),
  80: or(span(0, 7, 0x01), span(0, 0, 0xff)),
  81: pic('..####..', '.######.', '########', '########', '########', '########', '.######.', '..####..'),
  82: span(5, 5, 0xff),
  83: pic('.##.##..', '#######.', '#######.', '#######.', '.#####..', '..###...', '...#....', '........'),
  84: span(0, 7, 0x40),
  85: or(span(4, 7, V), span(3, 3, 0x0f)),
  86: or(rows((y) => 0x80 >> y), rows((y) => 0x01 << y)),
  87: pic('..####..', '.#....#.', '#......#', '#......#', '#......#', '#......#', '.#....#.', '..####..'),
  88: pic('..##....', '..##....', '##..##..', '##..##..', '..##....', '..##....', '.####...', '........'),
  89: span(0, 7, 0x02),
  90: pic('...#....', '..###...', '.#####..', '#######.', '.#####..', '..###...', '...#....', '........'),
  91: or(span(0, 7, V), H()),
  92: rows((y) => (y & 2 ? 0x30 : 0xc0)),
  93: span(0, 7, V),
  94: pic('........', '........', '.######.', '#.#..#..', '..#..#..', '..#..#..', '.##..##.', '........'),
  95: rows((y) => 0xff >> y),
};

const escapeHtml = (s) => s.replace(/[&<>"]/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' })[c]);

async function fetchBytes(url, what) {
  const r = await fetch(url, { cache: 'no-cache' }); // (not stamped by a deploy: always this deploy's)
  if (!r.ok) throw new Error(`${what} is missing here (${r.status})`);
  return new Uint8Array(await r.arrayBuffer());
}

/** The game as one HTML page (text). */
export async function webPage(prg, name) {
  const template = new TextDecoder().decode(await fetchBytes(new URL('./export/player.html', import.meta.url), 'The web page player'));
  const wasm = new Uint8Array(await engineBytes());
  const data = JSON.stringify({ name, prg: prg.toBase64(), font: charRom().toBase64(), wasm: wasm.toBase64() });
  return template.replaceAll('{{NAME}}', escapeHtml(name)).replace('/*BB_DATA*/{}', data);
}
