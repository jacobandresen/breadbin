// The asset formats and their C64 encodings: sprites, tiles, the font, rooms.
import test from 'node:test';
import assert from 'node:assert/strict';
import { encodeSprite, decodeSprite, mirror, convert, images, blankSprite } from '../../../../docs/play/studio/assets/sprites.js';
import { encodeTile, decodeTile, encodeCharset, tileColors, tileFlags, newTileset, FONT } from '../../../../docs/play/studio/assets/tiles.js';
import { glyph, screenChar, fontTiles } from '../../../../docs/play/studio/assets/font.js';
import { rle, unrle, roomTiles, setCell, charFor, blankRoom } from '../../../../docs/play/studio/assets/map.js';

const rows = (first, w, fill = '.') => [first, ...Array(20).fill(fill.repeat(w))];

test('a hires sprite row is three bytes, leftmost pixel in bit 7', () => {
  const s = { multi: false, rows: rows('1.......' + '........' + '.......1', 24) };
  const b = encodeSprite(s);
  assert.equal(b.length, 64);
  assert.deepEqual([...b.subarray(0, 3)], [0x80, 0x00, 0x01]);
  assert.ok(b.subarray(3).every((v) => v === 0));
});

test('multicolour sprite pixels: 1 = its colour (%10), 2 = $D025 (%01), 3 = $D026 (%11)', () => {
  const s = { multi: true, rows: rows('1230' + '....' + '...3', 12) };
  assert.deepEqual([...encodeSprite(s).subarray(0, 3)], [0b10011100, 0, 0b00000011]);
});

test('sprites decode back to the same rows', () => {
  for (const multi of [false, true]) {
    const w = multi ? 12 : 24;
    const chars = multi ? '.123' : '.1';
    const s = { multi, rows: [...Array(21)].map((_, y) => [...Array(w)].map((_, x) => chars[(x * 7 + y * 3) % chars.length]).join('')) };
    assert.deepEqual(decodeSprite(encodeSprite(s), multi).rows, s.rows);
  }
});

test('mirror reverses rows; flip adds an image named _flip right after', () => {
  const s = { name: 'hero', multi: true, color: 5, flip: true, rows: rows('12..........', 12) };
  assert.equal(mirror(s).rows[0], '..........21');
  const list = images({ sprites: [s, { ...s, name: 'enemy', flip: false }] });
  assert.deepEqual(list.map((i) => i.constName), ['hero', 'hero_flip', 'enemy']);
  assert.equal(list[1].rows[0], '..........21');
});

test('converting between hires and multicolour keeps the shape', () => {
  const hires = { multi: false, rows: rows('11..11' + '.'.repeat(18), 24) };
  const multi = convert(hires, true);
  assert.equal(multi.rows[0], '1.1.........');
  assert.equal(convert(multi, false).rows[0], '11..11' + '.'.repeat(18));
  assert.equal(blankSprite('x').rows.length, 21);
});

test('tiles: hires bits, multicolour pairs 1 = colour RAM (%11), 2 = $D022 (%01), 3 = $D023 (%10)', () => {
  assert.equal(encodeTile({ multi: false, rows: ['1......1'] })[0], 0x81);
  assert.equal(encodeTile({ multi: true, rows: ['123.'] })[0], 0b11011000);
  assert.deepEqual(decodeTile(encodeTile({ multi: true, rows: ['123.', '3..1'] }), true).rows.slice(0, 2), ['123.', '3..1']);
});

test('tile colours: multicolour tiles get +8 and only colours 0-7', () => {
  const set = { tiles: [{ multi: false, color: 14, rows: [] }, { multi: true, color: 13, rows: [] }, null] };
  const c = tileColors(set);
  assert.deepEqual(c.slice(0, 3), [14, 13 & 7 | 8, 1]);
  assert.equal(c.length, 256);
  assert.equal(encodeCharset(set).length, 2048);
  assert.deepEqual(tileFlags({ tiles: [{ flags: 3 }, null] }).slice(0, 2), [3, 0]);
});

test('the font: 64 distinct glyphs in screen-code order, space empty', () => {
  assert.equal(screenChar(1), 'A');
  assert.equal(screenChar(48), '0');
  const all = [...Array(64)].map((_, n) => glyph(n).join('/'));
  assert.equal(new Set(all).size, 64);
  assert.ok(glyph(32).every((r) => r === '........'));
  assert.ok(glyph(1).some((r) => r.includes('1')));
  // glyphs keep column 0, columns 6-7 and row 7 clear: letters never touch
  for (let n = 0; n < 64; n++) {
    const g = glyph(n);
    assert.equal(g[7], '........', screenChar(n));
    assert.ok(g.every((r) => r[0] === '.' && r[6] === '.' && r[7] === '.'), screenChar(n));
  }
  const set = newTileset();
  assert.equal(set.tiles.length, 256);
  assert.equal(set.tiles.slice(0, FONT).filter(Boolean).length, 64);
  assert.equal(fontTiles()[1].name, 'font A');
});

test('rooms run-length code and decode back', () => {
  const tiles = new Uint8Array(1000);
  for (let i = 0; i < 1000; i++) tiles[i] = i < 400 ? 32 : i % 7 === 0 ? 0xff : (i * 13) % 5;
  const packed = rle(tiles);
  assert.ok(packed.length < 1000);
  assert.deepEqual([...unrle(packed)], [...tiles]);
  assert.deepEqual([...rle(Uint8Array.of(1, 1, 1, 1, 1))], [0xff, 5, 1]);
  assert.deepEqual([...rle(Uint8Array.of(2, 2, 2))], [2, 2, 2]);
  assert.deepEqual([...rle(Uint8Array.of(0xff))], [0xff, 1, 0xff], 'a lone $FF tile is escaped');
  const long = new Uint8Array(1000).fill(9);
  assert.deepEqual([...unrle(rle(long))], [...long], 'runs longer than 255 split');
});

test('map cells go through the legend, which grows as tiles are used', () => {
  const map = { legend: {}, rooms: [] };
  const room = blankRoom(map, 'start');
  assert.deepEqual(map.legend, { ' ': 32 });
  setCell(map, room, 3, 2, 65);
  assert.equal(charFor(map, 65), '#');
  assert.equal(room.rows[2][3], '#');
  const t = roomTiles(map, room);
  assert.equal(t[2 * 40 + 3], 65);
  assert.equal(t[0], 32);
  assert.equal(roomTiles(map, { rows: ['?'] })[0], 32, 'unknown characters are spaces');
});

test('the legend has a character for every one of the 256 tiles', async () => {
  const { LEGEND_CHARS } = await import('../../../../docs/play/studio/assets/map.js');
  assert.ok(new Set(LEGEND_CHARS).size >= 256);
  assert.equal(new Set(LEGEND_CHARS).size, LEGEND_CHARS.length, 'no character twice');
  const map = { legend: {}, rooms: [] };
  const room = blankRoom(map, 'all');
  for (let t = 0; t < 256; t++) setCell(map, room, t % 40, Math.floor(t / 40), t);
  const tiles = roomTiles(JSON.parse(JSON.stringify(map)), JSON.parse(JSON.stringify(room)));
  for (let t = 0; t < 256; t++) assert.equal(tiles[Math.floor(t / 40) * 40 + (t % 40)], t);
});
