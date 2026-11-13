// Pictures into sprites and tiles ("Import PNG"): cutting a sheet into
// cells and the nearest C64 colours.
import test from 'node:test';
import assert from 'node:assert/strict';
import { nearest, cells, histogram, toRows, spriteFromCell, tileFromCell } from '../../../../docs/play/studio/assets/image.js';
import { rgb } from '../../../../docs/play/studio/assets/palette.js';

/** An RGBA image w x h, from fn(x, y) -> C64 colour or null (clear). */
function image(w, h, fn) {
  const out = new Uint8ClampedArray(w * h * 4);
  for (let y = 0; y < h; y++) {
    for (let x = 0; x < w; x++) {
      const c = fn(x, y);
      if (c == null) continue;
      out.set([...rgb(c), 255], (y * w + x) * 4);
    }
  }
  return out;
}

test('colours go to the nearest of the 16', () => {
  for (let c = 0; c < 16; c++) assert.equal(nearest(rgb(c)), c);
  assert.equal(nearest([250, 250, 250]), 1, 'near white is white');
  assert.equal(nearest([10, 0, 5]), 0, 'near black is black');
});

test('a sheet is cut into cells, empty ones skipped', () => {
  // three cells across, the middle one empty
  const img = image(24, 8, (x) => (x < 8 ? 2 : x >= 16 ? 5 : null));
  const c = cells(img, 24, 8, 8, 8);
  assert.deepEqual(c.map((x) => [x.x, x.y]), [[0, 0], [2, 0]]);
  assert.deepEqual(histogram(c[1].rgba), [[5, 64]]);
});

test('a one-colour picture becomes a hires sprite; more colours, multicolour', () => {
  const ring = image(24, 21, (x, y) => (Math.abs(Math.hypot(x - 11.5, y - 10) - 8) < 1 ? 7 : null));
  const s = spriteFromCell(ring, { mc1: 0, mc2: 1 }, 'ring');
  assert.equal(s.multi, false);
  assert.equal(s.color, 7);
  assert.equal(s.rows.length, 21);
  assert.equal(s.rows[10][3], '1');
  assert.equal(s.rows[10][11], '.');
  // green with black outline and a white eye: multicolour, green its own colour
  const blob = image(24, 21, (x, y) => (y === 0 || y === 20 ? 0 : x > 16 && x < 20 && y > 4 && y < 8 ? 1 : 5));
  const m = spriteFromCell(blob, { mc1: 0, mc2: 1 }, 'blob');
  assert.equal(m.multi, true);
  assert.equal(m.color, 5);
  assert.equal(m.rows[0], '222222222222', 'black is the shared multicolour 2');
  assert.equal(m.rows[6].slice(9, 10), '3', 'white is the shared multicolour 3');
  assert.equal(m.rows[10][0], '1');
});

test('tiles: the background colour reads as background', () => {
  const brick = image(8, 8, (x, y) => (y === 7 || x === 7 ? 0 : 2)); // red with a black edge, over a black background
  const t = tileFromCell(brick, { bg: 0, mc1: 11, mc2: 12 }, 'brick');
  assert.equal(t.multi, false);
  assert.equal(t.color, 2);
  assert.equal(t.rows[0], '1111111.');
  assert.equal(t.rows[7], '........');
  const two = tileFromCell(image(8, 8, (x) => (x < 4 ? 2 : 11)), { bg: 0, mc1: 11, mc2: 12 }, '');
  assert.equal(two.multi, true);
  assert.deepEqual(two.rows[0], '1122', 'red its own, dark grey the shared multicolour');
});

test('multicolour pairs: the left pixel decides, or the right one when the left is clear', () => {
  const rgba = image(4, 1, (x) => [null, 5, 1, 0][x]);
  assert.deepEqual(toRows(rgba, 4, 1, { multi: true, colors: [null, 5, 1, 0] }), ['12']);
});
