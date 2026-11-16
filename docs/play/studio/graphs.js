// graphs - a running game's variables over time, under the C64: click a
// variable's name beside the code and its last seconds show as a line, one
// point per frame. Made for tuning: a jump's arc, a speed, a timer.
//
// The emulator's page samples the variables 50 times a second (`SAMPLER`,
// installed with session.script); the Studio takes the samples a few times a
// second (session.call('bbgraphTake')) and draws them.

import { h } from '../ui.js';
import { token } from './theme.js';
import { number } from './variables.js';

const MAX = 3; // graphs at once
const KEEP = 250; // samples kept: five seconds at 50 a second
const COLORS = ['--code-aqua', '--code-yellow', '--code-purple']; // the theme's tokens

/** The sampler, in the emulator's page: every 20 ms, each variable's bytes. */
export const sampler = (specs) => `
  clearInterval(window.bbgraphTimer);
  window.bbgraph = { specs: ${JSON.stringify(specs)}, rows: [] };
  window.bbgraphTake = async () => { const r = bbgraph.rows; bbgraph.rows = []; return r; };
  window.bbgraphTimer = setInterval(() => {
    const g = window.bbgraph;
    g.rows.push(g.specs.map(([a, n]) => n === 1 ? wasm_peek(a) : wasm_peek(a) | (wasm_peek(a + 1) << 8)));
    if (g.rows.length > 500) g.rows.shift();
  }, 20);`;

export class Graphs {
  constructor() {
    this.list = []; // [{ v, data: [numbers], el, canvas, value }]
    this.el = h('div.studio-graphs', { hidden: true });
    this.changed = true; // the sampler must be told
  }

  has(name) {
    return this.list.some((g) => g.v.name === name);
  }

  /** Graph variable `v` ({ name, addr, size, unit, signed }), or stop graphing it. */
  toggle(v) {
    if (this.has(v.name)) return this.remove(v.name);
    if (this.list.length === MAX) this.remove(this.list[0].v.name);
    const color = COLORS.find((c) => !this.list.some((g) => g.color === c)) || COLORS[0];
    const canvas = h('canvas.studio-graph-canvas', { width: 600, height: 120 });
    const value = h('span.studio-graph-value');
    const range = h('span.studio-graph-range');
    const el = h('div.studio-graph', {},
      h('div.studio-graph-head', {}, h('i', { style: { background: `var(${color})` } }), h('b', {}, v.name), value, range,
        h('button.studio-tool', { title: `Stop graphing ${v.name}`, onclick: () => this.remove(v.name) }, '×')),
      canvas);
    this.list.push({ v, data: [], el, canvas, value, range, color });
    this.el.append(el);
    this.el.hidden = false;
    this.changed = true;
  }

  remove(name) {
    const i = this.list.findIndex((g) => g.v.name === name);
    if (i < 0) return;
    this.list[i].el.remove();
    this.list.splice(i, 1);
    this.el.hidden = !this.list.length;
    this.changed = true;
  }

  /** A new build: keep the graphs of variables it still has (at their new addresses). */
  rebind(vars) {
    for (const g of [...this.list]) {
      const v = vars.find((x) => x.name === g.v.name && x.size <= 2);
      if (v) {
        g.v = v;
        g.data = [];
      } else this.remove(g.v.name);
    }
    this.changed = true;
  }

  /** [address, bytes] for the sampler. */
  specs() {
    return this.list.map((g) => [g.v.addr, Math.min(g.v.size, 2)]);
  }

  /** Samples from the sampler: rows of raw numbers, one per graph. */
  push(rows) {
    for (const row of rows) {
      this.list.forEach((g, i) => {
        if (row[i] == null) return;
        g.data.push(number([row[i] & 255, row[i] >> 8], g.v));
      });
    }
    for (const g of this.list) if (g.data.length > KEEP) g.data.splice(0, g.data.length - KEEP);
    this.draw();
  }

  draw() {
    for (const g of this.list) {
      const { canvas: cv, data } = g;
      const ctx = cv.getContext('2d');
      const W = cv.width;
      const H = cv.height;
      ctx.clearRect(0, 0, W, H);
      if (!data.length) continue;
      let lo = Math.min(...data);
      let hi = Math.max(...data);
      if (lo === hi) (lo -= 1), (hi += 1);
      const pad = 10;
      const y = (n) => H - pad - ((n - lo) / (hi - lo)) * (H - 2 * pad);
      // zero, when it is in sight
      if (lo < 0 && hi > 0) {
        ctx.strokeStyle = token(cv, '--code-gray');
        ctx.globalAlpha = 0.4;
        ctx.setLineDash([4, 4]);
        ctx.beginPath();
        ctx.moveTo(0, y(0));
        ctx.lineTo(W, y(0));
        ctx.stroke();
        ctx.setLineDash([]);
        ctx.globalAlpha = 1;
      }
      ctx.strokeStyle = token(cv, g.color);
      ctx.lineWidth = 2.5;
      ctx.lineJoin = 'round';
      ctx.beginPath();
      const step = W / (KEEP - 1);
      const x0 = W - (data.length - 1) * step;
      data.forEach((n, i) => (i ? ctx.lineTo(x0 + i * step, y(n)) : ctx.moveTo(x0, y(n))));
      ctx.stroke();
      g.value.textContent = String(data[data.length - 1]);
      g.range.textContent = `${Math.min(...data)} … ${Math.max(...data)}`;
    }
  }
}
