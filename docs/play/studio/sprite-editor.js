// sprite-editor - the SPRITES mode: paint the C64's 24x21 hardware sprites,
// hires or multicolour, with a live preview at real size and an animation
// player for runs of frames. Saves to assets/sprites.json (sprites.js).
//
// Keys: 1-4 pick the colour slot, P F L R I the tools, H / V flip, arrows
// shift the image, [ ] the previous / next sprite, Ctrl+Z / Ctrl+Y undo.

import { h, toast } from '../ui.js';
import { PaintCanvas, History, colorPicker, toolbar, thumb, shift, flipH, flipV, drawRows, TOOLS, readImage, pickImage, sheetPng, saveBlob } from './pixels.js';
import { blankSprite, convert } from './assets/sprites.js';
import { cells, spriteFromCell } from './assets/image.js';
import { PALETTE, COLOR_NAMES } from './assets/palette.js';

export class SpriteEditor {
  constructor(studio) {
    this.studio = studio;
    this.sheet = null;
    this.index = 0;
    this.history = new History();
    this.slot = '1';
    this.anim = { from: 0, to: 0, fps: 8, timer: 0, frame: 0 };

    this.canvas = new PaintCanvas({
      onBegin: () => this.history.push(this.sheet),
      onChange: (rows, done) => {
        this.sprite.rows = rows;
        this.renderPreview();
        if (done) this.save();
        else this.renderItem(this.index);
      },
      onPick: (v) => this.pickSlot(v),
      onHover: (at) => (this.ui.where.textContent = at ? `x ${at[0] * (this.sprite.multi ? 2 : 1)}, y ${at[1]}` : ''),
    });

    const u = (this.ui = {});
    u.list = h('div.studio-assetitems');
    u.listTools = h('div.studio-listtools', {},
      h('button.studio-tool', { onclick: () => this.add(), title: 'A new sprite' }, '+ new'),
      h('button.studio-tool', { onclick: () => this.duplicate(), title: 'A copy of this sprite (for the next animation frame)' }, 'copy'),
      h('button.studio-tool', { onclick: () => this.move(-1), title: 'Move up' }, '↑'),
      h('button.studio-tool', { onclick: () => this.move(1), title: 'Move down' }, '↓'),
      h('button.studio-tool.danger', { onclick: () => this.remove(), title: 'Delete this sprite' }, '×'),
      h('button.studio-tool', { onclick: () => this.importPng(), title: 'Sprites from a picture: a 24x21 image, or a sheet of them' }, '⇧ PNG'),
      h('button.studio-tool', { onclick: () => this.exportPng(), title: 'All the sprites as one picture, 8 to a row: edit it elsewhere and import it back' }, '⇩ PNG'));
    u.tools = toolbar(this.canvas);
    u.slots = h('div.studio-slots');
    u.ops = h('div.studio-ops', {},
      h('button.studio-tool', { onclick: () => this.edit((r) => flipH(r)), title: 'Mirror left-right (H)' }, '⇋ flip'),
      h('button.studio-tool', { onclick: () => this.edit((r) => flipV(r)), title: 'Upside down (V)' }, '⇅ flip'),
      h('button.studio-tool', { onclick: () => this.edit((r) => shift(r, -1, 0)), title: 'Shift left (←)' }, '←'),
      h('button.studio-tool', { onclick: () => this.edit((r) => shift(r, 1, 0)), title: 'Shift right (→)' }, '→'),
      h('button.studio-tool', { onclick: () => this.edit((r) => shift(r, 0, -1)), title: 'Shift up (↑)' }, '↑'),
      h('button.studio-tool', { onclick: () => this.edit((r) => shift(r, 0, 1)), title: 'Shift down (↓)' }, '↓'),
      h('button.studio-tool', { onclick: () => this.edit((r) => r.map((x) => '.'.repeat(x.length))), title: 'Clear' }, 'clear'),
      h('span.studio-sep'),
      h('button.studio-tool', { onclick: () => this.undo(), title: 'Undo (Ctrl+Z)' }, '↶ undo'),
      h('button.studio-tool', { onclick: () => this.redo(), title: 'Redo (Ctrl+Y)' }, '↷ redo'),
      h('label.studio-check', {}, h('input', { type: 'checkbox', checked: true, onchange: (e) => ((this.canvas.grid = e.target.checked), this.canvas.draw()) }), ' grid'),
      h('label.studio-check', { title: 'Show the sprite before this one faintly underneath: for drawing the next frame of an animation (O)' },
        (u.onion = h('input', { type: 'checkbox', onchange: () => this.select(this.index) })), ' onion skin'));
    u.where = h('span.studio-where');
    u.name = h('input.studio-input', { type: 'text', spellcheck: false, onchange: (e) => this.rename(e.target.value) });
    u.mode = h('div.studio-seg', {},
      h('button', { onclick: () => this.setMulti(false) }, 'Hires 24×21'),
      h('button', { onclick: () => this.setMulti(true) }, 'Multicolour 12×21'));
    u.flip = h('input', { type: 'checkbox', onchange: (e) => (this.history.push(this.sheet), (this.sprite.flip = e.target.checked), this.save()) });
    u.own = colorPicker(1, (c) => this.setColor('color', c));
    u.mc1 = colorPicker(1, (c) => this.setColor('mc1', c));
    u.mc2 = colorPicker(1, (c) => this.setColor('mc2', c));
    u.const = h('code.studio-constname');
    u.preview = h('canvas.studio-preview', { width: 144, height: 84 });
    u.animFrom = h('input.studio-num', { type: 'number', min: 0, value: 0, onchange: () => this.animate() });
    u.animTo = h('input.studio-num', { type: 'number', min: 0, value: 0, onchange: () => this.animate() });
    u.animFps = h('input.studio-num', { type: 'number', min: 1, max: 50, value: 8, onchange: () => this.animate() });
    u.anim = h('canvas.studio-preview.studio-anim', { width: 144, height: 84 });

    this.el = h('div.studio-sprites.studio-editor', { tabIndex: -1 },
      h('aside.studio-assetlist', {}, h('div.studio-files-head', {}, 'Sprites', h('span.studio-count')), u.list, u.listTools),
      h('div.studio-canvasarea', {}, h('div.studio-canvasbar', {}, u.tools, u.slots), h('div.studio-canvasbox', {}, this.canvas.el, u.where), h('div.studio-canvasfoot', {}, u.ops)),
      h('aside.studio-props', {},
        h('label.studio-field', {}, h('span', {}, 'Name'), u.name), u.const,
        h('div.studio-field', {}, h('span', {}, 'Pixels'), u.mode),
        h('div.studio-field', {}, h('span', {}, 'Its colour'), u.own),
        h('div.studio-field.studio-mconly', {}, h('span', {}, 'Shared multicolour 2 (all sprites)'), u.mc1),
        h('div.studio-field.studio-mconly', {}, h('span', {}, 'Shared multicolour 3 (all sprites)'), u.mc2),
        h('label.studio-check', { title: 'The C64 can\'t mirror a sprite: this adds a mirrored copy for walking the other way' }, u.flip, ' Also mirrored, as ', h('code.studio-flipname')),
        h('div.studio-field', {}, h('span', {}, 'Real size, 2× and 3×'), u.preview),
        h('div.studio-field', {}, h('span', {}, 'Animation'), u.anim,
          h('div.studio-animrow', {}, 'frames ', u.animFrom, ' to ', u.animTo, ' at ', u.animFps, ' fps'))));
    this.el.addEventListener('keydown', (e) => this.key(e));
  }

  get sprite() {
    return this.sheet.sprites[this.index];
  }

  /** Show `sheet` (assets/sprites.json parsed). */
  load(sheet) {
    if (!sheet) return;
    const same = JSON.stringify(sheet) === JSON.stringify(this.sheet);
    this.sheet = structuredClone(sheet);
    this.sheet.sprites ??= [];
    if (!this.sheet.sprites.length) this.sheet.sprites.push(blankSprite('sprite'));
    if (!same) this.history.clear();
    this.index = Math.min(this.index, this.sheet.sprites.length - 1);
    this.renderList();
    this.select(this.index);
  }

  show() {
    requestAnimationFrame(() => this.canvas.fit());
    this.animate();
  }

  save() {
    this.studio.commit('sprites', this.sheet);
  }

  colors(s = this.sprite) {
    return { bg: null, own: s.color, mc1: this.sheet.mc1 ?? 1, mc2: this.sheet.mc2 ?? 2 };
  }

  renderList() {
    this.el.querySelector('.studio-count').textContent = `${this.sheet.sprites.length}`;
    this.ui.list.replaceChildren(...this.sheet.sprites.map((s, i) => this.item(s, i)));
  }

  item(s, i) {
    return h('button.studio-assetitem', { 'aria-selected': String(i === this.index), onclick: () => this.select(i), title: s.name },
      thumb(s.rows, { multi: s.multi, colors: this.colors(s), scale: 2 }),
      h('span.studio-assetname', {}, s.name || `#${i}`), h('span.studio-assetidx', {}, s.flip ? `${i} ⇋` : `${i}`));
  }

  renderItem(i) {
    const old = this.ui.list.children[i];
    if (old) old.replaceWith(this.item(this.sheet.sprites[i], i));
  }

  select(i) {
    this.index = Math.max(0, Math.min(this.sheet.sprites.length - 1, i));
    const s = this.sprite;
    for (const [k, el] of [...this.ui.list.children].entries()) el.setAttribute('aria-selected', String(k === this.index));
    this.ui.list.children[this.index]?.scrollIntoView({ block: 'nearest' });
    const before = this.sheet.sprites[this.index - 1];
    this.canvas.under = this.ui.onion.checked && before ? { rows: before.rows, multi: before.multi, colors: this.colors(before) } : null;
    this.canvas.set(s.rows, { multi: s.multi, colors: this.colors() });
    this.ui.name.value = s.name;
    this.ui.flip.checked = !!s.flip;
    const cname = `SPR_${(s.name || 'unnamed').toUpperCase().replace(/[^A-Z0-9]+/g, '_').replace(/^_|_$/g, '')}`;
    this.ui.const.textContent = cname;
    this.el.querySelector('.studio-flipname').textContent = `${cname}_FLIP`;
    for (const [k, b] of [...this.ui.mode.children].entries()) b.setAttribute('aria-pressed', String(k === (s.multi ? 1 : 0)));
    this.el.classList.toggle('multi', !!s.multi);
    this.ui.own.setValue(s.color);
    this.ui.mc1.setValue(this.sheet.mc1 ?? 1);
    this.ui.mc2.setValue(this.sheet.mc2 ?? 2);
    if (!s.multi && /[23]/.test(this.slot)) this.slot = '1';
    this.canvas.value = this.slot;
    this.renderSlots();
    this.renderPreview();
  }

  renderSlots() {
    const s = this.sprite;
    const slots = s.multi ? ['.', '1', '2', '3'] : ['.', '1'];
    const names = { '.': 'see-through', 1: `its colour (${COLOR_NAMES[s.color]})`, 2: `multicolour 2 (${COLOR_NAMES[this.sheet.mc1 ?? 1]})`, 3: `multicolour 3 (${COLOR_NAMES[this.sheet.mc2 ?? 2]})` };
    const col = { '.': null, 1: s.color, 2: this.sheet.mc1 ?? 1, 3: this.sheet.mc2 ?? 2 };
    this.ui.slots.replaceChildren(...slots.map((v, i) => h('button.studio-slot', {
      'aria-pressed': String(v === this.slot), title: `${names[v]} (${i + 1}; right button erases)`,
      style: { background: col[v] == null ? undefined : PALETTE[col[v]] }, className: `studio-slot${v === '.' ? ' clear' : ''}`,
      onclick: () => this.pickSlot(v),
    }, h('span', {}, String(i + 1)))));
  }

  pickSlot(v) {
    if (!this.sprite.multi && /[23]/.test(v)) return;
    this.slot = v;
    this.canvas.value = v;
    if (this.canvas.tool === 'pick') {
      this.canvas.tool = 'pen';
      this.ui.tools.render();
    }
    this.renderSlots();
  }

  renderPreview() {
    const s = this.sprite;
    const bg = this.studio.asset('tiles')?.colors?.bg ?? 0;
    const p = this.ui.preview;
    const ctx = p.getContext('2d');
    ctx.fillStyle = PALETTE[bg];
    ctx.fillRect(0, 0, p.width, p.height);
    let x = 4;
    for (const scale of [1, 2, 3]) {
      drawRows(ctx, s.rows, x, 4, (s.multi ? 2 : 1) * scale, scale, { ...this.colors(s), bg: null });
      x += 24 * scale + 6;
    }
    p.width !== x && ((p.width = x), this.renderPreview());
  }

  /** Play frames from..to of the sheet, at fps, in the animation box. */
  animate() {
    clearInterval(this.anim.timer);
    const n = this.sheet?.sprites.length || 0;
    if (!n) return;
    const from = Math.max(0, Math.min(n - 1, Number(this.ui.animFrom.value) || 0));
    const to = Math.max(from, Math.min(n - 1, Number(this.ui.animTo.value) || from));
    const fps = Math.max(1, Math.min(50, Number(this.ui.animFps.value) || 8));
    let f = from;
    const draw = () => {
      if (!this.el.isConnected || this.el.closest('[hidden]')) return;
      const s = this.sheet.sprites[f] || this.sprite;
      const c = this.ui.anim;
      const ctx = c.getContext('2d');
      ctx.fillStyle = PALETTE[this.studio.asset('tiles')?.colors?.bg ?? 0];
      ctx.fillRect(0, 0, c.width, c.height);
      drawRows(ctx, s.rows, (c.width - 72) / 2, (c.height - 63) / 2, (s.multi ? 2 : 1) * 3, 3, { ...this.colors(s), bg: null });
      f = f >= to ? from : f + 1;
    };
    draw();
    this.anim.timer = setInterval(draw, 1000 / fps);
  }

  // ── changes ───────────────────────────────────────────────────────────────

  edit(fn) {
    this.history.push(this.sheet);
    this.sprite.rows = fn(this.sprite.rows);
    this.canvas.set(this.sprite.rows);
    this.renderItem(this.index);
    this.renderPreview();
    this.save();
  }

  rename(name) {
    this.history.push(this.sheet);
    this.sprite.name = name.trim() || `sprite ${this.index}`;
    this.renderItem(this.index);
    this.select(this.index);
    this.save();
  }

  setMulti(multi) {
    if (multi === !!this.sprite.multi) return;
    this.history.push(this.sheet);
    this.sheet.sprites[this.index] = convert(this.sprite, multi);
    this.renderItem(this.index);
    this.select(this.index);
    this.save();
  }

  setColor(which, c) {
    this.history.push(this.sheet);
    if (which === 'color') this.sprite.color = c;
    else this.sheet[which] = c;
    if (which === 'color') this.renderItem(this.index);
    else this.renderList();
    this.select(this.index);
    this.save();
  }

  add() {
    this.history.push(this.sheet);
    const multi = this.sprite?.multi ?? true;
    this.sheet.sprites.splice(this.index + 1, 0, { ...blankSprite(`sprite ${this.sheet.sprites.length}`, multi), color: this.sprite?.color ?? 1 });
    if (this.sheet.sprites.length > 128) {
      this.sheet.sprites.splice(this.index + 1, 1);
      return toast('128 sprite images fill the C64\'s sprite memory.');
    }
    this.renderList();
    this.select(this.index + 1);
    this.save();
  }

  duplicate() {
    this.history.push(this.sheet);
    const copy = structuredClone(this.sprite);
    copy.name = nextName(copy.name, this.sheet.sprites.map((s) => s.name));
    this.sheet.sprites.splice(this.index + 1, 0, copy);
    this.renderList();
    this.select(this.index + 1);
    this.save();
  }

  move(d) {
    const j = this.index + d;
    if (j < 0 || j >= this.sheet.sprites.length) return;
    this.history.push(this.sheet);
    const list = this.sheet.sprites;
    [list[this.index], list[j]] = [list[j], list[this.index]];
    this.renderList();
    this.select(j);
    this.save();
    toast('Moved: sprite numbers changed, the SPR_ names did not.');
  }

  remove() {
    if (this.sheet.sprites.length === 1) return toast('A project keeps at least one sprite.');
    if (!confirm(`Delete the sprite ${this.sprite.name}? Code that uses its SPR_ name won't build until you change it.`)) return;
    this.history.push(this.sheet);
    this.sheet.sprites.splice(this.index, 1);
    this.renderList();
    this.select(this.index);
    this.save();
  }

  /** Sprites from a picture: each 24x21 cell with something in it, after the selected sprite. */
  async importPng() {
    const file = await pickImage();
    if (!file) return;
    try {
      const img = await readImage(file);
      const found = cells(img.rgba, img.width, img.height, 24, 21);
      if (!found.length) return toast(`Nothing to import: ${file.name} needs to be at least 24x21, with something in it.`);
      const room = 128 - this.sheet.sprites.length;
      const base = file.name.replace(/\.\w+$/, '').toLowerCase().replace(/[^a-z0-9]+/g, ' ').trim() || 'sprite';
      const added = found.slice(0, room).map((c, i) => spriteFromCell(c.rgba, { mc1: this.sheet.mc1 ?? 0, mc2: this.sheet.mc2 ?? 1 }, found.length > 1 ? `${base} ${i + 1}` : base));
      this.history.push(this.sheet);
      this.sheet.sprites.splice(this.index + 1, 0, ...added);
      this.renderList();
      this.select(this.index + 1);
      this.save();
      toast(`${added.length} sprite${added.length > 1 ? 's' : ''} from ${file.name}${found.length > room ? ` (${found.length - room} more didn't fit: 128 is the most)` : ''}. Colours went to the nearest the C64 has.`);
    } catch (e) {
      toast(`Couldn't read ${file.name}: ${e.message}`);
    }
  }

  async exportPng() {
    const items = this.sheet.sprites.map((s) => ({ rows: s.rows, multi: s.multi, colors: { ...this.colors(s), bg: null } }));
    saveBlob(await sheetPng(items, { w: 24, h: 21, perRow: 8 }), 'sprites.png');
  }

  undo() {
    const s = this.history.undo(this.sheet);
    if (s) this.restore(s);
  }

  redo() {
    const s = this.history.redo(this.sheet);
    if (s) this.restore(s);
  }

  restore(s) {
    this.sheet = s;
    this.renderList();
    this.select(Math.min(this.index, s.sprites.length - 1));
    this.save();
  }

  key(e) {
    if (e.target.matches('input, select, textarea')) return;
    const k = e.key.toLowerCase();
    if ((e.ctrlKey || e.metaKey) && k === 'z') return (e.preventDefault(), e.shiftKey ? this.redo() : this.undo());
    if ((e.ctrlKey || e.metaKey) && k === 'y') return (e.preventDefault(), this.redo());
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    const tool = TOOLS.find((t) => t.key === k);
    if (tool) {
      this.canvas.tool = tool.id;
      this.ui.tools.render();
    } else if (/^[1-4]$/.test(k)) this.pickSlot(['.', '1', '2', '3'][Number(k) - 1]);
    else if (k === 'h') this.edit(flipH);
    else if (k === 'v') this.edit(flipV);
    else if (k === 'arrowleft') this.edit((r) => shift(r, -1, 0));
    else if (k === 'arrowright') this.edit((r) => shift(r, 1, 0));
    else if (k === 'arrowup') this.edit((r) => shift(r, 0, -1));
    else if (k === 'arrowdown') this.edit((r) => shift(r, 0, 1));
    else if (k === '[') this.select(this.index - 1);
    else if (k === ']') this.select(this.index + 1);
    else if (k === 'o') (this.ui.onion.checked = !this.ui.onion.checked), this.select(this.index);
    else return;
    e.preventDefault();
  }
}

/** "walk 1" -> "walk 2", "hero" -> "hero 2", avoiding `taken`. */
export function nextName(name, taken) {
  const m = /^(.*?)(\d+)$/.exec(name || '');
  let base = m ? m[1] : `${name} `;
  let n = m ? Number(m[2]) + 1 : 2;
  if (!m && base === ' ') base = 'sprite ';
  while (taken.includes(`${base}${n}`)) n++;
  return `${base}${n}`;
}

