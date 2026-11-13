// tile-editor - the TILES mode: the 256 characters of the screen, 8x8 each.
// Tiles 0-63 are the font (bb_text writes with them); the rest are yours.
// Each tile has a colour, hires or multicolour pixels and up to 8 flags
// (solid, ladder... - what bb_fget and bb_flags_at report). The screen's
// shared colours are here too. Saves to assets/tiles.json (tiles.js).

import { h, toast } from '../ui.js';
import { PaintCanvas, History, colorPicker, toolbar, shift, flipH, flipV, drawRows, TOOLS, readImage, pickImage, sheetPng, saveBlob } from './pixels.js';
import { cells, tileFromCell } from './assets/image.js';
import { blankTile, FONT, COUNT } from './assets/tiles.js';
import { PALETTE, COLOR_NAMES } from './assets/palette.js';
import { screenChar } from './assets/font.js';

const SHEET = 3; // screen pixels per C64 pixel in the tile sheet

export class TileEditor {
  constructor(studio) {
    this.studio = studio;
    this.set = null;
    this.index = FONT;
    this.slot = '1';
    this.history = new History();
    this.canvas = new PaintCanvas({
      onBegin: () => {
        this.history.push(this.set);
        this.ensure();
      },
      onChange: (rows, done) => {
        this.tile.rows = rows;
        this.drawSheetTile(this.index);
        this.renderPreview();
        if (done) this.save();
      },
      onPick: (v) => this.pickSlot(v),
    });
    const u = (this.ui = {});
    u.sheet = h('canvas.studio-tilesheet', { width: 16 * 8 * SHEET + 15, height: 16 * 8 * SHEET + 15 });
    u.sheet.addEventListener('click', (e) => this.sheetClick(e));
    u.sheetInfo = h('div.studio-sheetinfo');
    u.sheetTools = h('div.studio-listtools', {},
      h('button.studio-tool', { onclick: () => this.importPng(), title: 'Tiles from a picture, cut into 8x8: into the empty slots from the selected tile on' }, '⇧ PNG'),
      h('button.studio-tool', { onclick: () => this.exportPng(), title: 'The whole tile set as one picture, 16 to a row' }, '⇩ PNG'));
    u.tools = toolbar(this.canvas);
    u.slots = h('div.studio-slots');
    u.ops = h('div.studio-ops', {},
      h('button.studio-tool', { onclick: () => this.edit(flipH), title: 'Mirror (H)' }, '⇋ flip'),
      h('button.studio-tool', { onclick: () => this.edit(flipV), title: 'Upside down (V)' }, '⇅ flip'),
      h('button.studio-tool', { onclick: () => this.edit((r) => shift(r, -1, 0)), title: 'Shift left' }, '←'),
      h('button.studio-tool', { onclick: () => this.edit((r) => shift(r, 1, 0)), title: 'Shift right' }, '→'),
      h('button.studio-tool', { onclick: () => this.edit((r) => shift(r, 0, -1)), title: 'Shift up' }, '↑'),
      h('button.studio-tool', { onclick: () => this.edit((r) => shift(r, 0, 1)), title: 'Shift down' }, '↓'),
      h('button.studio-tool', { onclick: () => this.copyTile(), title: 'Copy this tile (Ctrl+C)' }, 'copy'),
      h('button.studio-tool', { onclick: () => this.pasteTile(), title: 'Paste onto this tile (Ctrl+V)' }, 'paste'),
      h('button.studio-tool', { onclick: () => this.clearTile(), title: 'Empty this slot' }, 'clear'),
      h('span.studio-sep'),
      h('button.studio-tool', { onclick: () => this.undo(), title: 'Undo (Ctrl+Z)' }, '↶ undo'),
      h('button.studio-tool', { onclick: () => this.redo(), title: 'Redo (Ctrl+Y)' }, '↷ redo'));
    u.name = h('input.studio-input', { type: 'text', spellcheck: false, placeholder: 'name it to get a TILE_ constant', onchange: (e) => this.change((t) => (t.name = e.target.value.trim())) });
    u.const = h('code.studio-constname');
    u.mode = h('div.studio-seg', {},
      h('button', { onclick: () => this.setMulti(false) }, 'Hires 8×8'),
      h('button', { onclick: () => this.setMulti(true) }, 'Multicolour 4×8'));
    u.own = colorPicker(1, (c) => this.change((t) => (t.color = t.multi ? c & 7 : c)));
    u.flags = h('div.studio-flags');
    u.bg = colorPicker(0, (c) => this.setShared('bg', c));
    u.mc1 = colorPicker(11, (c) => this.setShared('mc1', c));
    u.mc2 = colorPicker(12, (c) => this.setShared('mc2', c));
    u.border = colorPicker(0, (c) => this.setShared('border', c));
    u.preview = h('canvas.studio-preview', { width: 120, height: 120 });

    this.el = h('div.studio-tiles.studio-editor', { tabIndex: -1 },
      h('aside.studio-sheetbox', {}, h('div.studio-files-head', {}, 'Tile set', h('span.studio-count')), u.sheet, u.sheetInfo, u.sheetTools),
      h('div.studio-canvasarea', {}, h('div.studio-canvasbar', {}, u.tools, u.slots), h('div.studio-canvasbox', {}, this.canvas.el), h('div.studio-canvasfoot', {}, u.ops)),
      h('aside.studio-props', {},
        h('div.studio-tilenum'),
        h('label.studio-field', {}, h('span', {}, 'Name'), u.name), u.const,
        h('div.studio-field', {}, h('span', {}, 'Pixels'), u.mode),
        h('div.studio-field', {}, h('span', {}, 'Its colour'), u.own),
        h('div.studio-field', {}, h('span', {}, 'Flags (bb_fget, bb_flags_at)'), u.flags),
        h('div.studio-field', {}, h('span', {}, 'Tiled, as on the screen'), u.preview),
        h('details.studio-shared', { open: true }, h('summary', {}, 'Screen colours (all tiles)'),
          h('div.studio-field', {}, h('span', {}, 'Background'), u.bg),
          h('div.studio-field', {}, h('span', {}, 'Multicolour 2'), u.mc1),
          h('div.studio-field', {}, h('span', {}, 'Multicolour 3'), u.mc2),
          h('div.studio-field', {}, h('span', {}, 'Border'), u.border))));
    this.el.addEventListener('keydown', (e) => this.key(e));
  }

  get tile() {
    return this.set.tiles[this.index];
  }

  /** The slot becomes a tile once painted on. */
  ensure() {
    if (!this.set.tiles[this.index]) this.set.tiles[this.index] = blankTile('', false);
  }

  load(set) {
    if (!set) return;
    this.set = structuredClone(set);
    this.set.tiles ??= [];
    while (this.set.tiles.length < COUNT) this.set.tiles.push(null);
    this.set.colors ??= { border: 0, bg: 0, mc1: 11, mc2: 12 };
    this.set.flags ??= ['solid', 'ladder', 'hurt', 'pickup', 'flag4', 'flag5', 'flag6', 'flag7'];
    this.drawSheet();
    this.select(this.index);
  }

  show() {
    requestAnimationFrame(() => this.canvas.fit());
  }

  save() {
    this.studio.commit('tiles', this.set);
  }

  colors(t = this.tile) {
    const c = this.set.colors;
    return { bg: c.bg, own: t ? (t.multi ? t.color & 7 : t.color) : 1, mc1: c.mc1, mc2: c.mc2 };
  }

  drawSheet() {
    const ctx = this.ui.sheet.getContext('2d');
    ctx.fillStyle = '#0b0912';
    ctx.fillRect(0, 0, this.ui.sheet.width, this.ui.sheet.height);
    for (let i = 0; i < COUNT; i++) this.drawSheetTile(i);
    this.el.querySelector('.studio-count').textContent = `${this.set.tiles.filter(Boolean).length - FONT} of ${COUNT - FONT} used`;
  }

  drawSheetTile(i) {
    const ctx = this.ui.sheet.getContext('2d');
    const x = (i % 16) * (8 * SHEET + 1);
    const y = Math.floor(i / 16) * (8 * SHEET + 1);
    const t = this.set.tiles[i];
    ctx.fillStyle = t ? PALETTE[this.set.colors.bg] : '#16131f';
    ctx.fillRect(x, y, 8 * SHEET, 8 * SHEET);
    if (t) drawRows(ctx, t.rows, x, y, (t.multi ? 2 : 1) * SHEET, SHEET, { ...this.colors(t), bg: null });
    if (i === this.index) {
      ctx.strokeStyle = '#ffd36b';
      ctx.lineWidth = 2;
      ctx.strokeRect(x + 1, y + 1, 8 * SHEET - 2, 8 * SHEET - 2);
    }
  }

  sheetClick(e) {
    const r = this.ui.sheet.getBoundingClientRect();
    const sx = this.ui.sheet.width / r.width;
    const x = Math.floor(((e.clientX - r.left) * sx) / (8 * SHEET + 1));
    const y = Math.floor(((e.clientY - r.top) * sx) / (8 * SHEET + 1));
    if (x >= 0 && x < 16 && y >= 0 && y < 16) this.select(y * 16 + x);
  }

  select(i) {
    const old = this.index;
    this.index = Math.max(0, Math.min(COUNT - 1, i));
    this.drawSheetTile(old);
    this.drawSheetTile(this.index);
    const t = this.tile;
    const multi = !!t?.multi;
    this.canvas.set(t ? t.rows : blankTile().rows, { multi, colors: this.colors(t) });
    this.el.querySelector('.studio-tilenum').replaceChildren(h('b', {}, `Tile ${this.index}`), ' ',
      h('span', {}, this.index < FONT ? `· the font's “${screenChar(this.index)}”` : t ? '' : '· empty: paint to use it'));
    this.ui.name.value = t?.name || '';
    this.ui.name.disabled = this.index < FONT;
    const n = t?.name ? `TILE_${t.name.toUpperCase().replace(/[^A-Z0-9]+/g, '_').replace(/^_|_$/g, '')}` : '';
    this.ui.const.textContent = this.index < FONT ? `bb_text() draws it` : n ? `${n} = ${this.index}` : `tile ${this.index}`;
    for (const [k, b] of [...this.ui.mode.children].entries()) b.setAttribute('aria-pressed', String(k === (multi ? 1 : 0)));
    this.ui.own.replaceWith((this.ui.own = colorPicker(t?.color ?? 1, (c) => this.change((x) => (x.color = x.multi ? c & 7 : c)), { only8: multi })));
    this.el.classList.toggle('multi', multi);
    this.renderFlags();
    const c = this.set.colors;
    this.ui.bg.setValue(c.bg);
    this.ui.mc1.setValue(c.mc1);
    this.ui.mc2.setValue(c.mc2);
    this.ui.border.setValue(c.border);
    if (!multi && /[23]/.test(this.slot)) this.slot = '1';
    this.canvas.value = this.slot;
    this.renderSlots();
    this.renderPreview();
    this.ui.sheetInfo.textContent = `${this.index < FONT ? 'font' : 'yours'} · click a tile to edit it`;
  }

  renderFlags() {
    const t = this.tile;
    this.ui.flags.replaceChildren(...this.set.flags.map((name, bit) => h('label.studio-flag', { title: `FLAG_${name.toUpperCase()} = ${1 << bit}` },
      h('input', { type: 'checkbox', checked: !!((t?.flags ?? 0) & (1 << bit)), disabled: !t, onchange: (e) => this.change((x) => (x.flags = e.target.checked ? (x.flags | (1 << bit)) : (x.flags & ~(1 << bit)))) }),
      h('input.studio-flagname', { value: name, spellcheck: false, title: 'Rename this flag (for every tile)', onchange: (e) => this.renameFlag(bit, e.target.value) }))));
  }

  renderSlots() {
    const t = this.tile;
    const multi = !!t?.multi;
    const c = this.colors(t);
    const slots = multi ? ['.', '1', '2', '3'] : ['.', '1'];
    const names = { '.': `background (${COLOR_NAMES[c.bg]})`, 1: `its colour (${COLOR_NAMES[c.own]})`, 2: `multicolour 2 (${COLOR_NAMES[c.mc1]})`, 3: `multicolour 3 (${COLOR_NAMES[c.mc2]})` };
    const col = { '.': c.bg, 1: c.own, 2: c.mc1, 3: c.mc2 };
    this.ui.slots.replaceChildren(...slots.map((v, i) => h('button.studio-slot', {
      'aria-pressed': String(v === this.slot), title: `${names[v]} (${i + 1})`, style: { background: PALETTE[col[v]] },
      onclick: () => this.pickSlot(v),
    }, h('span', {}, String(i + 1)))));
  }

  pickSlot(v) {
    if (!this.tile?.multi && /[23]/.test(v)) return;
    this.slot = v;
    this.canvas.value = v;
    if (this.canvas.tool === 'pick') {
      this.canvas.tool = 'pen';
      this.ui.tools.render();
    }
    this.renderSlots();
  }

  renderPreview() {
    const p = this.ui.preview;
    const ctx = p.getContext('2d');
    const t = this.tile;
    ctx.fillStyle = PALETTE[this.set.colors.bg];
    ctx.fillRect(0, 0, p.width, p.height);
    if (!t) return;
    for (let y = 0; y < 5; y++) for (let x = 0; x < 5; x++) drawRows(ctx, t.rows, x * 24, y * 24, (t.multi ? 2 : 1) * 3, 3, { ...this.colors(t), bg: null });
  }

  // ── changes ───────────────────────────────────────────────────────────────

  change(fn) {
    this.history.push(this.set);
    this.ensure();
    fn(this.tile);
    this.drawSheetTile(this.index);
    this.select(this.index);
    this.save();
  }

  edit(fn) {
    if (!this.tile) return;
    this.change((t) => (t.rows = fn(t.rows)));
  }

  setMulti(multi) {
    this.change((t) => {
      if (multi === !!t.multi) return;
      t.multi = multi;
      t.rows = t.rows.map((r) => (multi
        ? [0, 1, 2, 3].map((i) => (r[2 * i] !== '.' || r[2 * i + 1] !== '.' ? '1' : '.')).join('')
        : [...r].map((p) => (p === '.' ? '..' : '11')).join('')));
      if (multi) t.color &= 7;
    });
  }

  setShared(which, c) {
    this.history.push(this.set);
    this.set.colors[which] = c;
    this.drawSheet();
    this.select(this.index);
    this.save();
  }

  renameFlag(bit, name) {
    const n = name.trim().toLowerCase().replace(/[^a-z0-9_]+/g, '_') || `flag${bit}`;
    this.history.push(this.set);
    this.set.flags[bit] = n;
    this.renderFlags();
    this.save();
    toast(`Flag ${bit} is FLAG_${n.toUpperCase()} in the code now.`);
  }

  copyTile() {
    this.clip = this.tile ? structuredClone(this.tile) : null;
    if (this.clip) toast(`Tile ${this.index} copied.`);
  }

  pasteTile() {
    if (!this.clip) return;
    this.history.push(this.set);
    this.set.tiles[this.index] = { ...structuredClone(this.clip), name: this.tile?.name || '' };
    this.drawSheetTile(this.index);
    this.select(this.index);
    this.save();
  }

  clearTile() {
    if (this.index < FONT) return toast('The font tiles stay: bb_text() needs them. Paint them instead.');
    this.history.push(this.set);
    this.set.tiles[this.index] = null;
    this.drawSheet();
    this.select(this.index);
    this.save();
  }

  /** Tiles from a picture: each 8x8 cell with something in it, into empty slots (64 and up) from the selected one. */
  async importPng() {
    const file = await pickImage();
    if (!file) return;
    try {
      const img = await readImage(file);
      const found = cells(img.rgba, img.width, img.height, 8, 8);
      if (!found.length) return toast(`Nothing to import from ${file.name}.`);
      this.history.push(this.set);
      let slot = Math.max(FONT, this.index);
      let n = 0;
      for (const c of found) {
        while (slot < COUNT && this.set.tiles[slot]) slot++;
        if (slot >= COUNT) break;
        this.set.tiles[slot] = tileFromCell(c.rgba, this.set.colors, '');
        n++;
      }
      this.drawSheet();
      this.select(this.index);
      this.save();
      toast(`${n} tile${n === 1 ? '' : 's'} from ${file.name}${n < found.length ? ` (${found.length - n} more didn't fit)` : ''}, in the screen's colours. Name the ones the code needs.`);
    } catch (e) {
      toast(`Couldn't read ${file.name}: ${e.message}`);
    }
  }

  async exportPng() {
    const items = this.set.tiles.map((t) => (t ? { rows: t.rows, multi: t.multi, colors: this.colors(t) } : { rows: Array(8).fill('........'), multi: false, colors: { bg: this.set.colors.bg } }));
    saveBlob(await sheetPng(items, { w: 8, h: 8, perRow: 16 }), 'tiles.png');
  }

  undo() {
    const s = this.history.undo(this.set);
    if (s) this.restore(s);
  }

  redo() {
    const s = this.history.redo(this.set);
    if (s) this.restore(s);
  }

  restore(s) {
    this.set = s;
    this.drawSheet();
    this.select(this.index);
    this.save();
  }

  key(e) {
    if (e.target.matches('input, select, textarea')) return;
    const k = e.key.toLowerCase();
    if ((e.ctrlKey || e.metaKey) && k === 'z') return (e.preventDefault(), e.shiftKey ? this.redo() : this.undo());
    if ((e.ctrlKey || e.metaKey) && k === 'y') return (e.preventDefault(), this.redo());
    if ((e.ctrlKey || e.metaKey) && k === 'c') return (e.preventDefault(), this.copyTile());
    if ((e.ctrlKey || e.metaKey) && k === 'v') return (e.preventDefault(), this.pasteTile());
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    const tool = TOOLS.find((t) => t.key === k);
    if (tool) {
      this.canvas.tool = tool.id;
      this.ui.tools.render();
    } else if (/^[1-4]$/.test(k)) this.pickSlot(['.', '1', '2', '3'][Number(k) - 1]);
    else if (k === 'h') this.edit(flipH);
    else if (k === 'v') this.edit(flipV);
    else if (k === 'arrowleft') this.select(this.index - 1);
    else if (k === 'arrowright') this.select(this.index + 1);
    else if (k === 'arrowup') this.select(this.index - 16);
    else if (k === 'arrowdown') this.select(this.index + 16);
    else return;
    e.preventDefault();
  }
}
