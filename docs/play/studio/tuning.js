// tuning - the Tuning panel under the C64: a slider for each of the game's
// "knobs". A knob is a global variable declared with a number and never
// assigned anywhere in the code (`unsigned char gravity = 3;`): a setting,
// not state. Dragging a slider changes the running game at once; letting go
// writes the number into its declaration, so the next Run keeps it.

import { h } from '../ui.js';

const DECL = /^(?:static\s+)?((?:unsigned|signed)\s+)?(char|int|short)\b\s+([^;()]+);/;

/**
 * The knobs in `files` (path -> text): [{ name, file, line, value, signed, size }],
 * in the order they are declared.
 */
export function tunables(files) {
  const found = [];
  const sources = Object.entries(files).filter(([p, t]) => /\.c$/.test(p) && !p.startsWith('gen/') && typeof t === 'string');
  for (const [file, text] of sources) {
    text.split('\n').forEach((l, i) => {
      const m = DECL.exec(l);
      if (!m) return;
      const size = m[2] === 'char' ? 1 : 2;
      const signed = m[2] === 'char' ? /^signed/.test(m[1] || '') : !/^unsigned/.test(m[1] || '');
      for (const part of m[3].split(',')) {
        const d = /^\s*([A-Za-z_]\w*)\s*=\s*(-?(?:0x[0-9a-fA-F]+|\d+))\s*$/.exec(part);
        if (d) found.push({ name: d[1], file, line: i + 1, value: Number(d[2]), signed, size });
      }
    });
  }
  // a variable the code changes is the game's state, not a knob
  return [...found.filter((k) => !sources.some(([file, text]) => text.split('\n').some((l, i) => {
    if (file === k.file && i + 1 === k.line) return false;
    const code = l.replace(/\/\/.*$/, '').replace(/\/\*.*?\*\//g, '');
    const n = k.name;
    return new RegExp(`\\b${n}\\s*(?:\\[[^\\]]*\\]\\s*)?(?:[-+*/%&|^]|<<|>>)?=(?!=)|(?:\\+\\+|--)\\s*${n}\\b|\\b${n}\\s*(?:\\+\\+|--)|&\\s*${n}\\b`).test(code);
  }))), ...asmTunables(files)];
}

// In assembly a knob is a label on one number (`speed !byte 3`, ca65's
// `speed: .byte 3`) that no instruction writes (sta, inc, asl... to it).
const ASM_DECL = /^([A-Za-z_]\w*)(:?\s+(?:!(?:by|byte|8|08|wo|word|16)|\.(?:byte|word))\s+)(-?(?:\$[0-9a-fA-F]+|\d+))\s*(?:;.*)?$/;
const asmNumber = (v) => (v.startsWith('$') ? parseInt(v.slice(1), 16) : v.startsWith('-$') ? -parseInt(v.slice(2), 16) : Number(v));

function asmTunables(files) {
  const sources = Object.entries(files).filter(([p, t]) => /\.(asm|s|a|inc)$/i.test(p) && !p.startsWith('gen/') && typeof t === 'string');
  const found = [];
  for (const [file, text] of sources) {
    text.split('\n').forEach((l, i) => {
      const m = ASM_DECL.exec(l);
      if (m && !/^(?:adc|and|asl|bit|cmp|cpx|cpy|dec|eor|inc|lda|ldx|ldy|lsr|ora|rol|ror|sbc|sta|stx|sty|jmp|jsr)$/i.test(m[1])) {
        found.push({ name: m[1].replace(/^_/, ''), label: m[1], file, line: i + 1, value: asmNumber(m[3]), signed: false, size: /w|16/i.test(m[2]) ? 2 : 1 });
      }
    });
  }
  return found.filter((k) => !sources.some(([, text]) => new RegExp(`^\\s*(?:[A-Za-z_]\\w*:?\\s+)?(?:sta|stx|sty|inc|dec|asl|lsr|rol|ror)\\s+${k.label}\\b`, 'im').test(text.replace(/;.*$/gm, ''))));
}

/** Line `line` (a knob's declaration) with the knob's number made `n`, in the number's own form; null if it is not there. */
export function setKnob(line, k, n) {
  const asm = ASM_DECL.exec(line);
  if (asm && asm[1].replace(/^_/, '') === k.name) {
    const v = asm[3];
    const text = v.startsWith('$') && n >= 0 ? `$${n.toString(16).toUpperCase().padStart(v.length - 1, '0')}` : String(n);
    return line.slice(0, asm[1].length + asm[2].length) + text + line.slice(asm[1].length + asm[2].length + v.length);
  }
  const re = new RegExp(`(\\b${k.name}\\s*=\\s*)(-?(?:0x[0-9a-fA-F]+|\\d+))`);
  return re.test(line) ? line.replace(re, `$1${n}`) : null;
}

/** The slider's range for knob `k`: around its value, inside what its type holds. */
export function sliderRange(k) {
  const lo = k.signed ? -(1 << (8 * k.size - 1)) : 0;
  const hi = k.signed ? (1 << (8 * k.size - 1)) - 1 : (1 << (8 * k.size)) - 1;
  const reach = Math.max(10, Math.abs(k.value) * 2);
  return [Math.max(lo, k.value < 0 ? k.value - reach : Math.max(0, k.value - reach)), Math.min(hi, k.value + reach)];
}

export class TuningPanel {
  /** `onPoke(name, n)` while dragging, `onKeep(knob, n)` when let go. */
  constructor({ onPoke, onKeep, onShow }) {
    this.onPoke = onPoke;
    this.onKeep = onKeep;
    this.onShow = onShow;
    this.knobs = [];
    this.rows = new Map(); // name -> { input, out }
    this.list = h('div.studio-tuning-list');
    this.el = h('details.studio-tuning', { open: true, hidden: true },
      h('summary', {}, 'Tuning', h('span.studio-small', {}, ' · the game\'s settings: drag while it plays')), this.list);
  }

  /** Show the knobs of `files` (after a build). */
  set(files) {
    const knobs = tunables(files);
    const same = knobs.length === this.knobs.length && knobs.every((k, i) => k.name === this.knobs[i].name && k.value === this.knobs[i].value);
    this.knobs = knobs;
    this.el.hidden = !knobs.length;
    if (same) return;
    this.rows.clear();
    this.list.replaceChildren(...knobs.map((k) => {
      const [min, max] = sliderRange(k);
      const out = h('button.studio-tuning-value', { title: `${k.file}:${k.line}`, onclick: () => this.onShow?.(k) }, String(k.value));
      const input = h('input', { type: 'range', min, max, step: 1, value: k.value, 'aria-label': k.name });
      input.addEventListener('input', () => {
        out.textContent = input.value;
        this.dragging = k.name;
        this.onPoke(k.name, Number(input.value));
      });
      input.addEventListener('change', () => {
        this.dragging = null;
        if (Number(input.value) !== k.value) this.onKeep(k, Number(input.value));
      });
      this.rows.set(k.name, { input, out });
      return h('label.studio-tuning-row', {}, h('span.studio-tuning-name', {}, k.name), input, out);
    }));
  }

  /** The values the running game has (name -> number), shown on the sliders. */
  update(values) {
    for (const [name, { input, out }] of this.rows) {
      if (name === this.dragging || !values.has(name)) continue;
      const n = values.get(name);
      if (String(n) === out.textContent) continue;
      out.textContent = String(n);
      input.value = n;
    }
  }
}
