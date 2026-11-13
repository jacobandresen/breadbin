// pixels - what the SPRITES, TILES and MAP editors share: drawing C64
// pixels in their real colours, a paint canvas with tools, a colour picker
// and an undo history.
//
// Pixel values are the characters of the asset files: '.' see-through (or
// the background), '1' the sprite's or tile's own colour, '2' and '3' the
// shared multicolours. A multicolour pixel is two screen pixels wide.

import { PALETTE } from './assets/palette.js';
import { h } from '../ui.js';

export const SLOTS = ['.', '1', '2', '3'];

/** The CSS colour of value `v` for `colors` = { bg, own, mc1, mc2 } (C64 colour numbers). */
export function colorOf(v, colors) {
  if (v === '1') return PALETTE[colors.own & 15];
  if (v === '2') return PALETTE[colors.mc1 & 15];
  if (v === '3') return PALETTE[colors.mc2 & 15];
  return colors.bg == null ? null : PALETTE[colors.bg & 15];
}

/** Draw `rows` at x, y on a 2D context, each pixel `pw` x `ph` (null colours stay clear). */
export function drawRows(ctx, rows, x, y, pw, ph, colors) {
  for (let r = 0; r < rows.length; r++) {
    const row = rows[r];
    for (let c = 0; c < row.length; c++) {
      const col = colorOf(row[c], colors);
      if (!col) continue;
      ctx.fillStyle = col;
      ctx.fillRect(x + c * pw, y + r * ph, pw, ph);
    }
  }
}

/** A small canvas showing `rows` (a thumbnail), `scale` screen pixels per C64 pixel. */
export function thumb(rows, { multi, colors, scale = 2, w = multi ? rows[0]?.length * 2 : rows[0]?.length, hgt = rows.length, checker = true }) {
  const c = h('canvas.studio-thumb', { width: w * scale, height: hgt * scale });
  const ctx = c.getContext('2d');
  if (checker) checkers(ctx, c.width, c.height, 4 * scale);
  drawRows(ctx, rows, 0, 0, (multi ? 2 : 1) * scale, scale, colors);
  return c;
}

/** The see-through pattern behind sprites. */
export function checkers(ctx, w, hgt, size = 8) {
  for (let y = 0; y < hgt; y += size) {
    for (let x = 0; x < w; x += size) {
      ctx.fillStyle = ((x + y) / size) % 2 ? '#2b2733' : '#221f29';
      ctx.fillRect(x, y, size, size);
    }
  }
}

/** Flood fill: the cells of `grid` (array of char arrays) connected to x, y with its value become `v`. */
export function flood(grid, x, y, v) {
  const from = grid[y]?.[x];
  if (from == null || from === v) return grid;
  const stack = [[x, y]];
  while (stack.length) {
    const [cx, cy] = stack.pop();
    if (grid[cy]?.[cx] !== from) continue;
    grid[cy][cx] = v;
    stack.push([cx + 1, cy], [cx - 1, cy], [cx, cy + 1], [cx, cy - 1]);
  }
  return grid;
}

/** The cells of a line from a to b (Bresenham). */
export function line(ax, ay, bx, by) {
  const out = [];
  const dx = Math.abs(bx - ax);
  const dy = -Math.abs(by - ay);
  const sx = ax < bx ? 1 : -1;
  const sy = ay < by ? 1 : -1;
  let err = dx + dy;
  for (;;) {
    out.push([ax, ay]);
    if (ax === bx && ay === by) return out;
    const e2 = 2 * err;
    if (e2 >= dy) {
      err += dy;
      ax += sx;
    }
    if (e2 <= dx) {
      err += dx;
      ay += sy;
    }
  }
}

/** The cells of a rectangle's outline (or, `filled`, its inside too). */
export function rect(ax, ay, bx, by, filled = false) {
  const out = [];
  const [x0, x1] = [Math.min(ax, bx), Math.max(ax, bx)];
  const [y0, y1] = [Math.min(ay, by), Math.max(ay, by)];
  for (let y = y0; y <= y1; y++) for (let x = x0; x <= x1; x++) if (filled || y === y0 || y === y1 || x === x0 || x === x1) out.push([x, y]);
  return out;
}

/** Rows shifted by dx, dy, wrapping round. */
export function shift(rows, dx, dy) {
  const hgt = rows.length;
  return rows.map((_, y) => {
    const r = rows[(y - dy + hgt * 8) % hgt];
    const w = r.length;
    return [...r].map((_, x) => r[(x - dx + w * 8) % w]).join('');
  });
}

export const flipH = (rows) => rows.map((r) => [...r].reverse().join(''));
export const flipV = (rows) => [...rows].reverse();

/** An undo history of JSON snapshots. */
export class History {
  constructor(limit = 100) {
    this.past = [];
    this.future = [];
    this.limit = limit;
  }
  push(state) {
    const s = JSON.stringify(state);
    if (this.past.at(-1) === s) return;
    this.past.push(s);
    if (this.past.length > this.limit) this.past.shift();
    this.future = [];
  }
  undo(current) {
    if (!this.past.length) return null;
    this.future.push(JSON.stringify(current));
    return JSON.parse(this.past.pop());
  }
  redo(current) {
    if (!this.future.length) return null;
    this.past.push(JSON.stringify(current));
    return JSON.parse(this.future.pop());
  }
  clear() {
    this.past = [];
    this.future = [];
  }
}

/**
 * A paint canvas over `rows`. Left button paints the chosen value, right
 * button erases ('.'). Calls onBegin() before a stroke (for
 * undo), onChange(rows) during it, onPick(value) for the picker.
 */
export class PaintCanvas {
  constructor({ onBegin, onChange, onPick, onHover } = {}) {
    this.el = h('canvas.studio-paint', { tabIndex: 0 });
    this.ctx = this.el.getContext('2d');
    this.rows = [];
    this.multi = false;
    this.colors = { bg: null, own: 1, mc1: 2, mc2: 3 };
    this.tool = 'pen';
    this.value = '1';
    this.grid = true;
    this.cell = 16; // screen pixels per C64 pixel (height)
    this.hover = null;
    this.stroke = null;
    this.onBegin = onBegin;
    this.onChange = onChange;
    this.onPick = onPick;
    this.onHover = onHover;
    this.el.addEventListener('contextmenu', (e) => e.preventDefault());
    this.el.addEventListener('pointerdown', (e) => this.down(e));
    this.el.addEventListener('pointermove', (e) => this.move(e));
    this.el.addEventListener('pointerup', (e) => this.up(e));
    this.el.addEventListener('pointerleave', () => ((this.hover = null), this.draw(), this.onHover?.(null)));
    // the box it sits in decides its size: watch that (once the canvas is in it)
    this.watching = new ResizeObserver(() => this.fit());
    queueMicrotask(() => this.watching.observe(this.el.parentElement || this.el));
  }

  /** Show `rows` (strings), `multi` for double-wide pixels. */
  set(rows, { multi = this.multi, colors = this.colors } = {}) {
    this.rows = rows.slice();
    this.multi = multi;
    this.colors = colors;
    this.fit();
  }

  get cols() {
    return this.rows[0]?.length || 0;
  }

  /** Size the canvas to its box, keeping square screen pixels. */
  fit() {
    const parent = this.el.parentElement;
    if (!parent || !this.rows.length) return;
    // the box inside its padding
    const css = getComputedStyle(parent);
    const bw = parent.clientWidth - parseFloat(css.paddingLeft) - parseFloat(css.paddingRight) - 4;
    const bh = parent.clientHeight - parseFloat(css.paddingTop) - parseFloat(css.paddingBottom) - 4;
    if (bw <= 0 || bh <= 0) return;
    const wide = this.multi ? 2 : 1;
    const cell = Math.max(4, Math.floor(Math.min(bw / (this.cols * wide), bh / this.rows.length)));
    this.cell = cell;
    const dpr = devicePixelRatio || 1;
    const w = this.cols * wide * cell;
    const hgt = this.rows.length * cell;
    this.el.style.width = `${w}px`;
    this.el.style.height = `${hgt}px`;
    this.el.width = Math.round(w * dpr);
    this.el.height = Math.round(hgt * dpr);
    this.ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    this.draw();
  }

  cellAt(e) {
    const r = this.el.getBoundingClientRect();
    const pw = this.cell * (this.multi ? 2 : 1);
    const x = Math.floor((e.clientX - r.left) / pw);
    const y = Math.floor((e.clientY - r.top) / this.cell);
    return x >= 0 && y >= 0 && x < this.cols && y < this.rows.length ? [x, y] : null;
  }

  down(e) {
    const at = this.cellAt(e);
    if (!at) return;
    this.el.setPointerCapture(e.pointerId);
    const v = e.button === 2 || e.buttons === 2 ? '.' : this.value;
    if (this.tool === 'pick') {
      this.onPick?.(this.rows[at[1]][at[0]]);
      return;
    }
    this.onBegin?.();
    this.stroke = { from: at, last: at, v, base: this.rows.slice() };
    this.apply(at);
  }

  move(e) {
    const at = this.cellAt(e);
    this.hover = at;
    this.onHover?.(at);
    if (this.stroke && at) this.apply(at);
    else this.draw();
  }

  up() {
    if (this.stroke) this.onChange?.(this.rows, true);
    this.stroke = null;
  }

  /** Paint with the current tool, from the stroke's start to `at`. */
  apply(at) {
    const s = this.stroke;
    const grid = (s.base && (this.tool === 'line' || this.tool === 'rect') ? s.base : this.rows).map((r) => [...r]);
    let cells = [];
    if (this.tool === 'pen') cells = line(...s.last, ...at);
    else if (this.tool === 'line') cells = line(...s.from, ...at);
    else if (this.tool === 'rect') cells = rect(...s.from, ...at);
    else if (this.tool === 'fill') flood(grid, at[0], at[1], s.v);
    for (const [x, y] of cells) if (grid[y] && x < grid[y].length) grid[y][x] = s.v;
    s.last = at;
    this.rows = grid.map((r) => r.join(''));
    this.draw();
    this.onChange?.(this.rows, false);
  }

  draw() {
    const { ctx } = this;
    const wide = this.multi ? 2 : 1;
    const pw = this.cell * wide;
    const W = this.cols * pw;
    const H = this.rows.length * this.cell;
    ctx.clearRect(0, 0, W, H);
    if (this.colors.bg == null) checkers(ctx, W, H, this.cell);
    else {
      ctx.fillStyle = PALETTE[this.colors.bg & 15];
      ctx.fillRect(0, 0, W, H);
    }
    // the onion skin: another image (the frame before), faint, under this one
    if (this.under) {
      ctx.globalAlpha = 0.28;
      drawRows(ctx, this.under.rows, 0, 0, this.cell * (this.under.multi ? 2 : 1), this.cell, { ...this.under.colors, bg: null });
      ctx.globalAlpha = 1;
    }
    drawRows(ctx, this.rows, 0, 0, pw, this.cell, this.colors.bg == null ? this.colors : { ...this.colors, bg: null });
    if (this.grid && this.cell >= 6) {
      ctx.strokeStyle = 'rgb(255 255 255 / 0.07)';
      ctx.lineWidth = 1;
      ctx.beginPath();
      for (let x = 0; x <= this.cols; x++) (ctx.moveTo(x * pw + 0.5, 0), ctx.lineTo(x * pw + 0.5, H));
      for (let y = 0; y <= this.rows.length; y++) (ctx.moveTo(0, y * this.cell + 0.5), ctx.lineTo(W, y * this.cell + 0.5));
      ctx.stroke();
      // the byte boundaries of a sprite (every 8 screen pixels) a bit stronger
      ctx.strokeStyle = 'rgb(255 211 107 / 0.16)';
      ctx.beginPath();
      for (let x = 8 / wide; x < this.cols; x += 8 / wide) (ctx.moveTo(x * pw + 0.5, 0), ctx.lineTo(x * pw + 0.5, H));
      ctx.stroke();
    }
    if (this.hover && !this.stroke) {
      const [x, y] = this.hover;
      ctx.strokeStyle = '#ffd36b';
      ctx.lineWidth = 2;
      ctx.strokeRect(x * pw + 1, y * this.cell + 1, pw - 2, this.cell - 2);
    }
  }
}

/** The 16 C64 colours as buttons; `onPick(c)`. `only8` greys out 8-15 (multicolour tiles). */
export function colorPicker(value, onPick, { only8 = false } = {}) {
  const el = h('div.studio-colors', { role: 'radiogroup' });
  const render = (v) => {
    el.replaceChildren(...PALETTE.map((col, i) => h('button.studio-color', {
      style: { background: col },
      role: 'radio',
      'aria-checked': String(i === v),
      'aria-label': `colour ${i}`,
      title: `${i}`,
      disabled: only8 && i > 7,
      onclick: () => (render(i), onPick(i)),
    })));
  };
  render(value);
  el.setValue = render;
  return el;
}

/** The paint tools as buttons. */
export const TOOLS = [
  { id: 'pen', icon: '✎', label: 'Pen', key: 'p' },
  { id: 'fill', icon: '◧', label: 'Fill', key: 'f' },
  { id: 'line', icon: '╱', label: 'Line', key: 'l' },
  { id: 'rect', icon: '▭', label: 'Box', key: 'r' },
  { id: 'pick', icon: '◉', label: 'Pick', key: 'i' },
];

export function toolbar(canvas, onTool) {
  const el = h('div.studio-tools');
  const render = () => el.replaceChildren(...TOOLS.map((t) => h('button.studio-toolbtn', {
    'aria-pressed': String(canvas.tool === t.id), title: `${t.label} (${t.key.toUpperCase()})`,
    onclick: () => ((canvas.tool = t.id), render(), onTool?.(t.id)),
  }, h('span', {}, t.icon), h('small', {}, t.label))));
  render();
  el.render = render;
  return el;
}

/** The pixels of an image file (PNG, GIF...) as { rgba, width, height }. */
export async function readImage(file) {
  const bmp = await createImageBitmap(file);
  const c = document.createElement('canvas');
  c.width = bmp.width;
  c.height = bmp.height;
  const ctx = c.getContext('2d');
  ctx.drawImage(bmp, 0, 0);
  return { rgba: ctx.getImageData(0, 0, c.width, c.height).data, width: c.width, height: c.height };
}

/** Let the user pick an image file; resolves to it, or null. */
export function pickImage() {
  return new Promise((resolve) => {
    const input = h('input', { type: 'file', accept: 'image/png,image/gif,image/bmp,image/webp' });
    input.addEventListener('change', () => resolve(input.files[0] || null));
    input.addEventListener('cancel', () => resolve(null));
    input.click();
  });
}

/**
 * A PNG of `items` (each { rows, multi, colors }) in a grid of `perRow`,
 * each cell w x h C64 pixels at real size (see-through stays clear).
 */
export function sheetPng(items, { w, h: ch, perRow }) {
  const c = document.createElement('canvas');
  c.width = Math.min(items.length, perRow) * w;
  c.height = Math.ceil(items.length / perRow) * ch;
  const ctx = c.getContext('2d');
  items.forEach((it, i) => drawRows(ctx, it.rows, (i % perRow) * w, Math.floor(i / perRow) * ch, it.multi ? 2 : 1, 1, it.colors));
  return new Promise((resolve) => c.toBlob(resolve, 'image/png'));
}

/** Save `blob` as a download called `name`. */
export function saveBlob(blob, name) {
  const a = h('a', { href: URL.createObjectURL(blob), download: name });
  document.body.append(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(a.href), 1000);
}
