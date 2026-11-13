// image - pictures into C64 pixels, for "Import PNG" in the SPRITES and
// TILES editors: a sheet is cut into cells (24x21 for sprites, 8x8 for
// tiles), and each pixel becomes the nearest of the colours the cell may
// use. Works on plain RGBA arrays, so the tests need no canvas.
//
//   cells(rgba, width, height, 24, 21) -> [{ x, y, rgba }] (cells with anything in them)
//   toRows(cell, 24, 21, { multi, colors: [bg, own, mc1, mc2] }) -> rows of '.123'

import { rgb } from './palette.js';

const dist = (a, b) => (a[0] - b[0]) ** 2 + (a[1] - b[1]) ** 2 + (a[2] - b[2]) ** 2;

/** The C64 colour (0..15) nearest to [r, g, b]. */
export function nearest(c) {
  let best = 0;
  for (let i = 1; i < 16; i++) if (dist(c, rgb(i)) < dist(c, rgb(best))) best = i;
  return best;
}

/** Cut an RGBA image into cw x ch cells, left to right, top to bottom; empty (all clear) cells are skipped. */
export function cells(rgba, width, height, cw, ch) {
  const out = [];
  for (let y = 0; y + ch <= height; y += ch) {
    for (let x = 0; x + cw <= width; x += cw) {
      const cell = new Uint8ClampedArray(cw * ch * 4);
      let any = false;
      for (let r = 0; r < ch; r++) {
        const from = ((y + r) * width + x) * 4;
        cell.set(rgba.subarray(from, from + cw * 4), r * cw * 4);
      }
      for (let i = 3; i < cell.length; i += 4) if (cell[i] >= 128) any = true;
      if (any) out.push({ x: x / cw, y: y / ch, rgba: cell });
    }
  }
  return out;
}

/** The C64 colours a cell uses most, most first: [colour, count], clear pixels left out. */
export function histogram(rgba) {
  const counts = new Map();
  for (let i = 0; i < rgba.length; i += 4) {
    if (rgba[i + 3] < 128) continue;
    const c = nearest([rgba[i], rgba[i + 1], rgba[i + 2]]);
    counts.set(c, (counts.get(c) || 0) + 1);
  }
  return [...counts].sort((a, b) => b[1] - a[1]);
}

/**
 * A cell's pixels as rows of '.', '1', '2', '3'. `colors` are the C64
 * colours of '.', '1', '2', '3' ('.' null: see-through, so clear pixels are
 * '.' and the rest the nearest of 1-3). Multicolour pixels are two wide: the
 * pair's left pixel decides (or its right one, when the left is clear).
 */
export function toRows(rgba, cw, ch, { multi = false, colors = [null, 1, 2, 3] } = {}) {
  const slots = multi ? ['.', '1', '2', '3'] : ['.', '1'];
  const rows = [];
  for (let y = 0; y < ch; y++) {
    let row = '';
    for (let x = 0; x < cw; x += multi ? 2 : 1) {
      let i = (y * cw + x) * 4;
      if (multi && rgba[i + 3] < 128) i += 4;
      if (rgba[i + 3] < 128) {
        row += '.';
        continue;
      }
      const c = rgb(nearest([rgba[i], rgba[i + 1], rgba[i + 2]]));
      let best = null;
      for (const s of slots) {
        if (colors[slots.indexOf(s)] == null) continue;
        if (best == null || dist(c, rgb(colors[slots.indexOf(s)])) < dist(c, rgb(colors[slots.indexOf(best)]))) best = s;
      }
      row += best ?? '1';
    }
    rows.push(row);
  }
  return rows;
}

/**
 * A sprite from a 24x21 cell: hires when it has one colour, else multicolour
 * with its most common colour as its own (the shared `mc1`, `mc2` for the rest).
 */
export function spriteFromCell(rgba, { mc1, mc2 }, name) {
  const used = histogram(rgba);
  const own = used.find(([c]) => c !== mc1 && c !== mc2)?.[0] ?? used[0]?.[0] ?? 1;
  const multi = used.length > 1;
  return { name, multi, color: own, flip: false, rows: toRows(rgba, 24, 21, { multi, colors: [null, own, mc1, mc2] }) };
}

/** A tile from an 8x8 cell, over background `bg` (pixels near it become '.'). */
export function tileFromCell(rgba, { bg, mc1, mc2 }, name) {
  // clear pixels count as background
  const used = histogram(rgba).filter(([c]) => c !== bg);
  const multi = used.length > 1;
  const own = (used.find(([c]) => c !== mc1 && c !== mc2)?.[0] ?? used[0]?.[0] ?? 1) & (multi ? 7 : 15);
  return { name, multi, color: own, flags: 0, rows: toRows(rgba, 8, 8, { multi, colors: [bg, own, mc1, mc2] }) };
}
