// tools - the player's 🔧 Tools: a running game's insides, as an Action
// Replay cartridge showed them.
//
//   Cheat finder  copy all of RAM, play, say what happened to the number
//                 (a life lost: "went down"), again until a few addresses
//                 are left (cheats.js)
//   Trainer       a found address frozen at a value (poked many times a
//                 frame), or the code that writes it patched out with NOPs
//                 (when the game checks the number right after changing it,
//                 only the patch wins)
//   Monitor       memory as hex or code (disasm.js), Pause, one-instruction
//                 Step, breakpoints at addresses, the CPU's registers
//
// It all runs in the emulator's page through the session (emu.js): bbtools
// (below) reads RAM and keeps the frozen values; bbdbg (studio/debugger.js)
// is the Studio's debugger, which stops the CPU through VirtualC64's
// RetroShell.

import { FRAME } from './studio/debugger.js';
import { CheatSearch, KINDS, findWriters, nops } from './cheats.js';
import { listing, hex2, hex4 } from './disasm.js';
import { h, toast } from './ui.js';

/** The emulator page's half: window.bbtools. */
const PAGE = String.raw`
if (!window.bbtools) window.bbtools = (() => {
  let frozen = [];
  setInterval(() => { for (const [a, v] of frozen) wasm_poke(a, v); }, 4);
  return {
    // all 64 KB of RAM: the CPU port switched to RAM everywhere for the read
    // (the C64 doesn't run during it), so the chips' registers aren't touched
    ram() {
      const port = wasm_peek(1);
      wasm_poke(1, (port & 0xf8) | 4);
      const out = new Uint8Array(65536);
      for (let a = 2; a < 65536; a++) out[a] = wasm_peek(a);
      wasm_poke(1, port);
      return out;
    },
    freeze(list) { frozen = list; return true; },
  };
})();`;

const MAX_SHOWN = 40;
const parseAddr = (s) => {
  const t = String(s).trim().replace(/^\$|^0x/i, '');
  const n = parseInt(t, 16);
  return /^[0-9a-f]{1,4}$/i.test(t) ? n : null;
};

export class GameTools {
  constructor() {
    this.session = null;
    this.timer = null;
    this.reset();
    this.build();
  }

  reset() {
    this.search = null;
    this.frozen = new Map(); // addr -> value
    this.patches = new Map(); // addr -> { bytes (the original), text }
    this.breaks = [];
    this.stop = null;
    this.writers = null;
    this.monAddr = 0x0801;
    this.monCode = false;
  }

  build() {
    const btn = (label, title, fn, cls = '') => h(`button.key.pill.small${cls}`, { onclick: fn, title }, label);
    const u = (this.ui = {});
    // the cheat finder
    u.start = btn('● Start a search', 'Copy all of RAM: every byte a candidate', () => this.startSearch(), '.primary');
    u.kinds = h('div.tools-row', {}, ...Object.entries(KINDS).filter(([k]) => k !== 'equal').map(([k, v]) => btn(v.label, `Keep the bytes that ${v.label.toLowerCase()} since the last step`, () => this.narrow(k))));
    u.equal = h('input.tools-num', { type: 'number', min: 0, max: 255, placeholder: 'n', 'aria-label': 'Value' });
    u.kinds.append(h('span.tools-is', {}, btn('Is', 'Keep the bytes that are this number now', () => this.narrow('equal', Number(u.equal.value))), u.equal));
    u.count = h('p.tools-count');
    u.found = h('div.tools-list');
    u.writers = h('div.tools-list');
    // the trainer
    u.trainer = h('div.tools-list');
    // the monitor
    u.addr = h('input.tools-addr', { value: '0801', 'aria-label': 'Address', onchange: () => this.goto(u.addr.value) });
    u.view = btn('Code', 'Show memory as instructions or as bytes', () => {
      this.monCode = !this.monCode;
      u.view.textContent = this.monCode ? 'Hex' : 'Code';
      this.refresh();
    });
    u.pause = btn('⏸ Pause', 'Stop the CPU where it is', () => this.togglePause());
    u.step = btn('Step', 'One instruction', () => this.step());
    u.breakAt = h('input.tools-addr', { placeholder: 'addr', 'aria-label': 'Breakpoint address' });
    u.breakList = h('span.tools-breaks');
    u.regs = h('p.tools-regs', {}, 'Running');
    u.mem = h('pre.tools-mem');
    this.el = h('aside.emu-tools', { hidden: true },
      h('div.tools-head', {}, h('b', {}, '🔧 Tools'), h('button.tools-close', { onclick: () => this.toggle(false), title: 'Close the tools', 'aria-label': 'Close' }, '×')),
      h('details.tools-sec', { open: true },
        h('summary', {}, 'Cheat finder'),
        h('p.tools-hint', {}, 'Find where the game keeps a number: lives, time, energy. Start a search, play until the number changes, then say how it changed. A few rounds leave a handful of addresses.'),
        h('div.tools-row', {}, u.start), u.kinds, u.count, u.found, u.writers),
      h('details.tools-sec', { open: true },
        h('summary', {}, 'Trainer'),
        h('p.tools-hint', {}, 'Freeze keeps a byte at its value. If the game still loses the life (it checks right after counting down), find the code that writes it and patch that out.'),
        u.trainer),
      h('details.tools-sec', {},
        h('summary', {}, 'Monitor'),
        h('div.tools-row', {}, h('span', {}, '$'), u.addr, u.view, u.pause, u.step),
        h('div.tools-row', {}, h('span', {}, 'Break at $'), u.breakAt, btn('Set', 'Stop when the CPU gets to this address', () => this.addBreak(u.breakAt.value)), u.breakList),
        u.regs, u.mem));
    this.showSearch();
    this.showTrainer();
  }

  /** A new game in the player (or none). */
  attach(session) {
    // the same emulator with a new program in it (the Studio's Run): its helpers stay, the freezes go
    const same = session && session === this.session && this.installed;
    if (same) session.call('bbtools.freeze', [], 8000).catch(() => {});
    this.session = session;
    this.installed = same;
    this.reset();
    this.ui.regs.textContent = 'Running';
    this.ui.pause.textContent = '⏸ Pause';
    this.showSearch();
    this.showTrainer();
    this.ui.writers.replaceChildren();
    if (!session) this.toggle(false);
  }

  get open() {
    return !this.el.hidden;
  }

  toggle(on = !this.open) {
    this.el.hidden = !on;
    this.onToggle?.(on);
    clearInterval(this.timer);
    this.timer = on ? setInterval(() => this.refresh(), 400) : null;
    if (on) this.refresh();
  }

  install() {
    if (this.installed || !this.session) return;
    this.session.script(FRAME);
    this.session.script(PAGE);
    this.installed = true;
  }

  call(fn, arg) {
    this.install();
    return this.session.call(fn, arg, 8000);
  }

  ram() {
    return this.call('bbtools.ram');
  }

  // ── the cheat finder ─────────────────────────────────────────────────────

  async startSearch() {
    if (!this.session?.started) return toast('Start the game first');
    this.search = new CheatSearch(await this.ram());
    this.writers = null;
    this.ui.writers.replaceChildren();
    this.showSearch();
  }

  async narrow(kind, n) {
    if (!this.search) return toast('Start a search first');
    if (kind === 'equal' && !Number.isFinite(n)) return toast('Type the number the game shows');
    this.search.filter(kind, await this.ram(), n);
    this.showSearch();
  }

  showSearch() {
    const u = this.ui;
    const s = this.search;
    for (const b of u.kinds.querySelectorAll('button')) b.disabled = !s;
    u.start.textContent = s ? 'Start again' : '● Start a search';
    u.count.textContent = !s ? 'No search yet.' : s.addrs.length === 0 ? 'Nothing left: start again, and say what happened each time.' : `${s.addrs.length.toLocaleString()} candidate${s.addrs.length === 1 ? '' : 's'}${s.rounds ? ` after ${s.rounds} step${s.rounds === 1 ? '' : 's'}` : ': play, then say how the number changed'}`;
    const shown = s && s.addrs.length <= MAX_SHOWN ? [...s.addrs] : [];
    u.found.replaceChildren(...shown.map((a) => h('div.tools-item', { 'data-addr': a },
      h('code', {}, `$${hex4(a)}`), h('span.tools-val', {}, '…'),
      h('button.tools-act', { onclick: () => this.freeze(a), title: 'Keep it at its value (a trainer)' }, 'freeze'),
      h('button.tools-act', { onclick: () => this.setValue(a), title: 'Change it' }, 'set'),
      h('button.tools-act', { onclick: () => this.findCode(a), title: 'The code that writes it' }, 'who writes?'))));
  }

  async setValue(a) {
    const v = prompt(`New value for $${hex4(a)} (0-255):`);
    if (v === null) return;
    const n = Number(v);
    if (!(n >= 0 && n <= 255)) return;
    this.session.poke(a, [n]);
    if (this.frozen.has(a)) this.freeze(a, n);
  }

  async freeze(a, value) {
    const v = value ?? (await this.ram())[a];
    this.frozen.set(a, v);
    await this.call('bbtools.freeze', [...this.frozen]);
    this.showTrainer();
    toast(`$${hex4(a)} frozen at ${v}`);
  }

  async unfreeze(a) {
    this.frozen.delete(a);
    await this.call('bbtools.freeze', [...this.frozen]);
    this.showTrainer();
  }

  /** The instructions that write address `a`, each to break on or patch out. */
  async findCode(a) {
    const ram = await this.ram();
    const found = findWriters(ram, a);
    this.writers = { addr: a, found };
    this.ui.writers.replaceChildren(
      h('p.tools-count', {}, found.length ? `Code that writes $${hex4(a)}:` : `No instruction writes $${hex4(a)} directly (it may go through a pointer, or a table with X or Y).`),
      ...found.map((w) => h('div.tools-item', {},
        h('code', {}, `$${hex4(w.at)}`), h('span.tools-code', {}, w.text),
        h('button.tools-act', { onclick: () => this.addBreak(hex4(w.at)), title: 'Stop when the game runs it: the one that does it stops when the number changes' }, 'break'),
        h('button.tools-act', { onclick: () => this.patch(w), title: 'Make it do nothing (NOPs)' }, 'patch out'))));
  }

  async patch(w) {
    if (this.patches.has(w.at)) return;
    const ram = await this.ram();
    this.patches.set(w.at, { bytes: [...ram.slice(w.at, w.at + w.size)], text: w.text });
    this.session.poke(w.at, nops(w.size));
    this.showTrainer();
    toast(`${w.text} at $${hex4(w.at)} patched out`);
  }

  unpatch(at) {
    const p = this.patches.get(at);
    if (!p) return;
    this.session.poke(at, p.bytes);
    this.patches.delete(at);
    this.showTrainer();
  }

  showTrainer() {
    const rows = [
      ...[...this.frozen].map(([a, v]) => h('div.tools-item', {}, h('code', {}, `$${hex4(a)}`), h('span', {}, `frozen at ${v}`), h('button.tools-act', { onclick: () => this.unfreeze(a), title: 'Let it change again' }, 'thaw'))),
      ...[...this.patches].map(([a, p]) => h('div.tools-item', {}, h('code', {}, `$${hex4(a)}`), h('span.tools-code', {}, `${p.text} → NOP`), h('button.tools-act', { onclick: () => this.unpatch(a), title: 'Put the instruction back' }, 'undo'))),
    ];
    this.ui.trainer.replaceChildren(...(rows.length ? rows : [h('p.tools-count', {}, 'Nothing frozen or patched.')]));
  }

  // ── the monitor ──────────────────────────────────────────────────────────

  goto(text) {
    const a = parseAddr(text);
    if (a === null) return toast('An address in hex: 0801, $C000');
    this.monAddr = a;
    this.refresh();
  }

  async togglePause() {
    if (!this.session?.started) return;
    if (this.stop) {
      this.stop = null;
      await this.call('bbdbg.go', []);
    } else this.stopped(await this.call('bbdbg.halt'));
    this.showStop();
  }

  async step() {
    if (!this.stop) return this.togglePause();
    this.stopped(await this.call('bbdbg.step'));
  }

  async addBreak(text) {
    const a = parseAddr(text);
    if (a === null) return toast('An address in hex: 0801, $C000');
    if (!this.breaks.includes(a)) this.breaks.push(a);
    await this.call('bbdbg.users', this.breaks);
    if (!this.stop) await this.call('bbdbg.go', []);
    this.showBreaks();
    this.ui.mem.closest('details').open = true;
  }

  async removeBreak(a) {
    this.breaks = this.breaks.filter((b) => b !== a);
    await this.call('bbdbg.users', this.breaks);
    this.showBreaks();
  }

  showBreaks() {
    this.ui.breakList.replaceChildren(...this.breaks.map((a) => h('button.tools-break', { onclick: () => this.removeBreak(a), title: 'Remove this breakpoint' }, `$${hex4(a)} ×`)));
  }

  stopped(s) {
    this.stop = s;
    if (s) {
      this.monAddr = s.pc;
      this.monCode = true;
      this.ui.view.textContent = 'Hex';
      this.ui.mem.closest('details').open = true;
    }
    this.showStop();
    this.refresh();
  }

  showStop() {
    const s = this.stop;
    const r = s?.regs || {};
    const v = (x) => (x == null ? '··' : hex2(x));
    this.ui.pause.textContent = s ? '▶ Run' : '⏸ Pause';
    this.ui.regs.textContent = s ? `Stopped · PC ${hex4(s.pc)}  A ${v(r.a)}  X ${v(r.x)}  Y ${v(r.y)}  SP ${v(r.sp)}` : this.breaks.length ? 'Running, until a breakpoint' : 'Running';
    this.ui.regs.classList.toggle('stopped', !!s);
  }

  /** The live parts: the candidates' values, the monitor's memory, a breakpoint's stop. */
  async refresh() {
    const s = this.session;
    if (!s?.started || document.hidden) return;
    if (this.refreshing) {
      this.again = true; // (asked while one was under way: one more after it, with what changed)
      return;
    }
    this.refreshing = true;
    this.again = false;
    try {
      const items = [...this.ui.found.children];
      const ranges = [...items.map((el) => [Number(el.dataset.addr), 1]), [this.monAddr, 128]];
      const got = await s.peek(ranges);
      if (!got) return;
      items.forEach((el, i) => (el.querySelector('.tools-val').textContent = String(got[i][0])));
      this.showMemory(got[got.length - 1]);
      if (this.breaks.length && !this.stop && this.installed) {
        const at = await this.call('bbdbg.status').catch(() => null);
        if (at) this.stopped(at);
      }
    } finally {
      this.refreshing = false;
      if (this.again) this.refresh();
    }
  }

  showMemory(bytes) {
    const at = this.monAddr;
    if (this.monCode) {
      const rows = listing((a) => bytes[(a - at) & 0xffff] ?? 0, at, 16).filter((d) => ((d.addr - at) & 0xffff) < 125);
      this.ui.mem.textContent = rows.map((d) => `${this.stop?.pc === d.addr ? '▸' : ' '}${hex4(d.addr)}  ${d.bytes.map(hex2).join(' ').padEnd(8)}  ${d.text}`).join('\n');
    } else {
      const lines = [];
      for (let row = 0; row < 16; row++) {
        const b = bytes.slice(row * 8, row * 8 + 8);
        const text = b.map((c) => (c >= 32 && c < 127 ? String.fromCharCode(c) : c >= 1 && c <= 26 ? String.fromCharCode(c + 64) : '.')).join('');
        lines.push(`${hex4((at + row * 8) & 0xffff)}  ${b.map(hex2).join(' ')}  ${text}`);
      }
      this.ui.mem.textContent = lines.join('\n');
    }
  }
}
