// map-editor - the MAP mode: paint rooms, 40x25 screens of tiles, with the
// tiles from the TILES mode in their real colours. bb_room(ROOM_X) puts a room
// on the C64's screen. Saves to assets/map.json (map.js).
//
// Left button paints the chosen tile, right button paints a space. Keys:
// P pen, R box, F fill, I pick, G grid, [ ] the previous / next room.

import { h, toast } from '../ui.js';
import { History, drawRows, flood, line, rect } from './pixels.js';
import { roomTiles, charFor, blankRoom, COLS, ROWS } from './assets/map.js';
import { FONT, COUNT } from './assets/tiles.js';
import { PALETTE } from './assets/palette.js';
import { nextName } from './sprite-editor.js';

const SPACE = 32;
const TOOLS = [
  { id: 'pen', icon: '✎', label: 'Pen', key: 'p' },
  { id: 'rect', icon: '▬', label: 'Box', key: 'r' },
  { id: 'fill', icon: '◧', label: 'Fill', key: 'f' },
  { id: 'pick', icon: '◉', label: 'Pick', key: 'i' },
];
// a tint per flag bit, for the flags overlay
const FLAG_TINT = ['#ff5d73', '#7ab8ff', '#ffd36b', '#5fd3a6', '#d3869b', '#fe8019', '#8ec07c', '#ebdbb2'];

export class MapEditor {
  constructor(studio) {
    this.studio = studio;
    this.map = null;
    this.room = 0;
    this.tile = 64;
    this.tool = 'pen';
    this.grid = false;
    this.overlay = false;
    this.showFont = false;
    this.history = new History(60);
    this.cells = null; // the room being edited, as 1000 tile numbers
    this.cache = new Map(); // tile -> its 8x8 image

    const u = (this.ui = {});
    u.list = h('div.studio-assetitems');
    u.listTools = h('div.studio-listtools', {},
      h('button.studio-tool', { onclick: () => this.addRoom(), title: 'A new, empty room' }, '+ new'),
      h('button.studio-tool', { onclick: () => this.copyRoom(), title: 'A copy of this room' }, 'copy'),
      h('button.studio-tool', { onclick: () => this.moveRoom(-1), title: 'Move up' }, '↑'),
      h('button.studio-tool', { onclick: () => this.moveRoom(1), title: 'Move down' }, '↓'),
      h('button.studio-tool.danger', { onclick: () => this.removeRoom(), title: 'Delete this room' }, '×'));
    u.name = h('input.studio-input', { type: 'text', spellcheck: false, onchange: (e) => this.rename(e.target.value) });
    u.const = h('code.studio-constname');
    u.tools = h('div.studio-tools');
    u.canvas = h('canvas.studio-room', { width: 320, height: 200, tabIndex: 0 });
    u.where = h('span.studio-where');
    u.palette = h('div.studio-tilepalette');
    u.picked = h('div.studio-picked');
    u.fontToggle = h('label.studio-check', {}, h('input', { type: 'checkbox', onchange: (e) => ((this.showFont = e.target.checked), this.renderPalette()) }), ' the font too');
    u.foot = h('div.studio-ops', {},
      h('label.studio-check', {}, h('input', { type: 'checkbox', onchange: (e) => ((this.grid = e.target.checked), this.draw()) }), ' grid (G)'),
      h('label.studio-check', { title: 'Tint tiles by their flags: solid, ladder...' }, h('input', { type: 'checkbox', onchange: (e) => ((this.overlay = e.target.checked), this.draw()) }), ' flags'),
      h('span.studio-sep'),
      h('button.studio-tool', { onclick: () => this.undo(), title: 'Undo (Ctrl+Z)' }, '↶ undo'),
      h('button.studio-tool', { onclick: () => this.redo(), title: 'Redo (Ctrl+Y)' }, '↷ redo'),
      h('button.studio-tool', { onclick: () => this.clearRoom(), title: 'Fill the room with spaces' }, 'clear'));
    u.legend = h('div.studio-legendkey');

    this.el = h('div.studio-map.studio-editor', { tabIndex: -1 },
      h('aside.studio-assetlist', {}, h('div.studio-files-head', {}, 'Rooms', h('span.studio-count')), u.list, u.listTools),
      h('div.studio-canvasarea', {},
        h('div.studio-canvasbar', {}, u.tools, h('label.studio-field.inline', {}, h('span', {}, 'Name'), u.name), u.const),
        h('div.studio-canvasbox.studio-roombox', {}, u.canvas, u.where),
        h('div.studio-canvasfoot', {}, u.foot)),
      h('aside.studio-props', {}, h('div.studio-field', {}, h('span', {}, 'Paint with'), u.picked), u.fontToggle, u.palette, u.legend));

    u.canvas.addEventListener('contextmenu', (e) => e.preventDefault());
    u.canvas.addEventListener('pointerdown', (e) => this.down(e));
    u.canvas.addEventListener('pointermove', (e) => this.move(e));
    u.canvas.addEventListener('pointerup', () => this.up());
    u.canvas.addEventListener('pointerleave', () => ((this.hover = null), this.draw(), (u.where.textContent = '')));
    this.el.addEventListener('keydown', (e) => this.key(e));
    new ResizeObserver(() => this.fit()).observe(u.canvas.parentElement);
    this.renderTools();
  }

  get tiles() {
    return this.studio.asset('tiles') || { tiles: [], colors: { bg: 0, mc1: 1, mc2: 2, border: 0 }, flags: [] };
  }

  load(map) {
    if (!map) return;
    this.map = structuredClone(map);
    this.map.legend ??= { ' ': SPACE };
    this.map.rooms ??= [];
    if (!this.map.rooms.length) this.map.rooms.push(blankRoom(this.map, 'start'));
    this.room = Math.min(this.room, this.map.rooms.length - 1);
    this.cells = null; // what was being edited belongs to the old map
    this.cache.clear(); // the tiles may have changed
    this.renderList();
    this.renderPalette();
    this.selectRoom(this.room);
  }

  show() {
    requestAnimationFrame(() => this.fit());
  }

  save() {
    // the cells back into the room's text rows, through the legend
    const r = this.map.rooms[this.room];
    r.rows = [...Array(ROWS)].map((_, y) => [...Array(COLS)].map((_, x) => charFor(this.map, this.cells[y * COLS + x])).join(''));
    this.prune();
    this.studio.commit('map', this.map);
  }

  /** Drop legend entries no room uses any more. */
  prune() {
    const used = new Set(this.map.rooms.flatMap((r) => r.rows.join('').split('')));
    for (const ch of Object.keys(this.map.legend)) if (!used.has(ch) && ch !== ' ') delete this.map.legend[ch];
  }

  // ── rooms ─────────────────────────────────────────────────────────────────

  renderList() {
    this.el.querySelector('.studio-count').textContent = `${this.map.rooms.length}`;
    this.ui.list.replaceChildren(...this.map.rooms.map((r, i) => {
      const c = h('canvas.studio-thumb', { width: 80, height: 50 });
      this.drawRoom(c.getContext('2d'), roomTiles(this.map, r), 0.25);
      return h('button.studio-assetitem.studio-roomitem', { 'aria-selected': String(i === this.room), onclick: () => this.selectRoom(i), title: r.name },
        c, h('span.studio-assetname', {}, r.name || `room ${i}`), h('span.studio-assetidx', {}, `${i}`));
    }));
  }

  selectRoom(i) {
    if (this.cells && this.map.rooms[this.room]) this.save();
    this.room = Math.max(0, Math.min(this.map.rooms.length - 1, i));
    const r = this.map.rooms[this.room];
    this.cells = roomTiles(this.map, r);
    for (const [k, el] of [...this.ui.list.children].entries()) el.setAttribute('aria-selected', String(k === this.room));
    this.ui.name.value = r.name;
    this.ui.const.textContent = `ROOM_${(r.name || 'unnamed').toUpperCase().replace(/[^A-Z0-9]+/g, '_').replace(/^_|_$/g, '')} = ${this.room}`;
    this.history.clear();
    this.draw();
  }

  addRoom() {
    this.save();
    this.map.rooms.splice(this.room + 1, 0, blankRoom(this.map, nextName('room 1', this.map.rooms.map((r) => r.name))));
    this.cells = null;
    this.renderList();
    this.selectRoom(this.room + 1);
    this.save();
  }

  copyRoom() {
    this.save();
    const copy = structuredClone(this.map.rooms[this.room]);
    copy.name = nextName(copy.name, this.map.rooms.map((r) => r.name));
    this.map.rooms.splice(this.room + 1, 0, copy);
    this.cells = null;
    this.renderList();
    this.selectRoom(this.room + 1);
    this.save();
  }

  moveRoom(d) {
    const j = this.room + d;
    if (j < 0 || j >= this.map.rooms.length) return;
    this.save();
    [this.map.rooms[this.room], this.map.rooms[j]] = [this.map.rooms[j], this.map.rooms[this.room]];
    this.cells = null;
    this.renderList();
    this.selectRoom(j);
    this.save();
  }

  removeRoom() {
    if (this.map.rooms.length === 1) return toast('A project keeps at least one room.');
    if (!confirm(`Delete the room ${this.map.rooms[this.room].name}?`)) return;
    this.map.rooms.splice(this.room, 1);
    this.cells = null;
    this.renderList();
    this.selectRoom(Math.min(this.room, this.map.rooms.length - 1));
    this.prune();
    this.studio.commit('map', this.map);
  }

  rename(name) {
    this.map.rooms[this.room].name = name.trim() || `room ${this.room}`;
    this.save();
    this.renderList();
    this.selectRoom(this.room);
  }

  clearRoom() {
    this.history.push([...this.cells]);
    this.cells.fill(SPACE);
    this.draw();
    this.save();
    this.renderList();
  }

  // ── the tile palette ──────────────────────────────────────────────────────

  renderPalette() {
    const set = this.tiles;
    const items = [];
    for (let i = this.showFont ? 0 : FONT; i < COUNT; i++) {
      const t = set.tiles[i];
      if (!t && i !== SPACE) continue;
      const c = h('canvas', { width: 24, height: 24 });
      const ctx = c.getContext('2d');
      ctx.drawImage(this.image(i), 0, 0, 24, 24);
      items.push(h('button.studio-paltile', { 'aria-pressed': String(i === this.tile), title: `${i}${t?.name ? ` · ${t.name}` : ''}`, onclick: () => this.pickTile(i) }, c));
    }
    if (!this.showFont && !items.length) items.push(h('p.studio-small', {}, 'No tiles of your own yet: draw some in TILES (tiles 64 and up).'));
    this.ui.palette.replaceChildren(...items);
    this.renderPicked();
  }

  renderPicked() {
    const t = this.tiles.tiles[this.tile];
    const c = h('canvas', { width: 40, height: 40 });
    c.getContext('2d').drawImage(this.image(this.tile), 0, 0, 40, 40);
    const flags = (this.tiles.flags || []).filter((_, b) => (t?.flags ?? 0) & (1 << b));
    this.ui.picked.replaceChildren(c, h('div', {}, h('b', {}, t?.name || (this.tile === SPACE ? 'space' : `tile ${this.tile}`)), h('span', {}, ` #${this.tile}`),
      h('div.studio-small', {}, flags.length ? flags.join(', ') : 'no flags')));
    for (const b of this.ui.palette.children) b.setAttribute?.('aria-pressed', String(b.title.split(' ')[0] === String(this.tile)));
    this.ui.legend.replaceChildren(h('span.studio-small', {}, 'Right button paints a space.'));
  }

  pickTile(t) {
    this.tile = t;
    if (this.tool === 'pick') {
      this.tool = 'pen';
      this.renderTools();
    }
    this.renderPicked();
  }

  renderTools() {
    this.ui.tools.replaceChildren(...TOOLS.map((t) => h('button.studio-toolbtn', {
      'aria-pressed': String(this.tool === t.id), title: `${t.label} (${t.key.toUpperCase()})`, onclick: () => ((this.tool = t.id), this.renderTools()),
    }, h('span', {}, t.icon), h('small', {}, t.label))));
  }

  // ── drawing ───────────────────────────────────────────────────────────────

  /** Tile `t` as an 8x8 canvas in the screen's colours. */
  image(t) {
    let c = this.cache.get(t);
    if (c) return c;
    const set = this.tiles;
    c = document.createElement('canvas');
    c.width = 8;
    c.height = 8;
    const ctx = c.getContext('2d');
    const tile = set.tiles[t];
    const col = set.colors || {};
    ctx.fillStyle = PALETTE[col.bg ?? 0];
    ctx.fillRect(0, 0, 8, 8);
    if (tile) drawRows(ctx, tile.rows, 0, 0, tile.multi ? 2 : 1, 1, { bg: null, own: tile.multi ? tile.color & 7 : tile.color, mc1: col.mc1, mc2: col.mc2 });
    this.cache.set(t, c);
    return c;
  }

  fit() {
    const parent = this.ui.canvas.parentElement;
    if (!parent.clientWidth) return;
    // room for the canvas and its 10-pixel border, inside the box's padding
    const scale = Math.max(1, Math.min((parent.clientWidth - 48) / 320, (parent.clientHeight - 48) / 200));
    const s = scale >= 2 ? Math.floor(scale) : scale; // whole pixels when there is room
    this.scale = s;
    const dpr = devicePixelRatio || 1;
    this.ui.canvas.style.width = `${320 * s}px`;
    this.ui.canvas.style.height = `${200 * s}px`;
    this.ui.canvas.width = Math.round(320 * s * dpr);
    this.ui.canvas.height = Math.round(200 * s * dpr);
    this.ui.canvas.style.borderColor = PALETTE[this.tiles.colors?.border ?? 0];
    this.draw();
  }

  drawRoom(ctx, cells, scale) {
    ctx.imageSmoothingEnabled = false;
    for (let i = 0; i < COLS * ROWS; i++) ctx.drawImage(this.image(cells[i]), (i % COLS) * 8 * scale, Math.floor(i / COLS) * 8 * scale, 8 * scale, 8 * scale);
  }

  draw() {
    if (!this.cells) return;
    const c = this.ui.canvas;
    const ctx = c.getContext('2d');
    const k = c.width / 320;
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    this.drawRoom(ctx, this.preview || this.cells, k);
    if (this.overlay) {
      const set = this.tiles;
      for (let i = 0; i < COLS * ROWS; i++) {
        const f = set.tiles[(this.preview || this.cells)[i]]?.flags ?? 0;
        if (!f) continue;
        const bit = Math.log2(f & -f);
        ctx.fillStyle = `${FLAG_TINT[bit]}66`;
        ctx.fillRect((i % COLS) * 8 * k, Math.floor(i / COLS) * 8 * k, 8 * k, 8 * k);
      }
    }
    if (this.grid) {
      ctx.strokeStyle = 'rgb(255 255 255 / 0.12)';
      ctx.lineWidth = 1;
      ctx.beginPath();
      for (let x = 0; x <= COLS; x++) (ctx.moveTo(Math.round(x * 8 * k) + 0.5, 0), ctx.lineTo(Math.round(x * 8 * k) + 0.5, c.height));
      for (let y = 0; y <= ROWS; y++) (ctx.moveTo(0, Math.round(y * 8 * k) + 0.5), ctx.lineTo(c.width, Math.round(y * 8 * k) + 0.5));
      ctx.stroke();
    }
    if (this.hover) {
      const [x, y] = this.hover;
      ctx.strokeStyle = '#ffd36b';
      ctx.lineWidth = Math.max(1, k * 0.75);
      ctx.strokeRect(x * 8 * k, y * 8 * k, 8 * k, 8 * k);
    }
  }

  cellAt(e) {
    // inside the border: the drawn screen is the canvas's content box
    const c = this.ui.canvas;
    const r = c.getBoundingClientRect();
    const x = Math.floor(((e.clientX - r.left - c.clientLeft) / c.clientWidth) * COLS);
    const y = Math.floor(((e.clientY - r.top - c.clientTop) / c.clientHeight) * ROWS);
    return x >= 0 && y >= 0 && x < COLS && y < ROWS ? [x, y] : null;
  }

  down(e) {
    const at = this.cellAt(e);
    if (!at) return;
    this.ui.canvas.setPointerCapture(e.pointerId);
    const t = e.button === 2 ? SPACE : this.tile;
    if (this.tool === 'pick') return this.pickTile(this.cells[at[1] * COLS + at[0]]);
    this.history.push([...this.cells]);
    this.stroke = { from: at, last: at, t };
    this.paint(at);
  }

  move(e) {
    const at = this.cellAt(e);
    this.hover = at;
    if (at) {
      const t = this.cells[at[1] * COLS + at[0]];
      const name = this.tiles.tiles[t]?.name;
      this.ui.where.textContent = `cell ${at[0]}, ${at[1]} · pixel ${at[0] * 8}, ${at[1] * 8} · tile ${t}${name ? ` (${name})` : ''}`;
    }
    if (this.stroke && at) this.paint(at);
    else this.draw();
  }

  up() {
    if (!this.stroke) return;
    if (this.preview) this.cells = this.preview;
    this.preview = null;
    this.stroke = null;
    this.draw();
    this.save();
    this.renderList();
  }

  paint(at) {
    const s = this.stroke;
    if (this.tool === 'pen') {
      for (const [x, y] of line(...s.last, ...at)) this.cells[y * COLS + x] = s.t;
    } else if (this.tool === 'rect') {
      this.preview = Uint8Array.from(this.cells);
      for (const [x, y] of rect(...s.from, ...at, true)) this.preview[y * COLS + x] = s.t;
    } else if (this.tool === 'fill') {
      const grid = [...Array(ROWS)].map((_, y) => [...this.cells.subarray(y * COLS, y * COLS + COLS)]);
      flood(grid, at[0], at[1], s.t);
      this.cells = Uint8Array.from(grid.flat());
    }
    s.last = at;
    this.draw();
  }

  undo() {
    const s = this.history.undo([...this.cells]);
    if (s) (this.cells = Uint8Array.from(s), this.draw(), this.save());
  }

  redo() {
    const s = this.history.redo([...this.cells]);
    if (s) (this.cells = Uint8Array.from(s), this.draw(), this.save());
  }

  key(e) {
    if (e.target.matches('input, select, textarea')) return;
    const k = e.key.toLowerCase();
    if ((e.ctrlKey || e.metaKey) && k === 'z') return (e.preventDefault(), e.shiftKey ? this.redo() : this.undo());
    if ((e.ctrlKey || e.metaKey) && k === 'y') return (e.preventDefault(), this.redo());
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    const tool = TOOLS.find((t) => t.key === k);
    if (tool) (this.tool = tool.id), this.renderTools();
    else if (k === 'g') (this.grid = !this.grid), this.draw();
    else if (k === '[') this.selectRoom(this.room - 1);
    else if (k === ']') this.selectRoom(this.room + 1);
    else return;
    e.preventDefault();
  }
}
