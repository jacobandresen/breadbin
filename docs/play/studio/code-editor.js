// code-editor - the CODE mode: CodeMirror 6 with the retrobox theme, C and
// 6502 assembly, completion and hover help for the runtime (bb.h) and the
// project's assets (gen/assets.h), and the compiler's errors in the margin
// and under the code.
//
//   const ed = new CodeEditor(parent, { onChange, onRun, onSave });
//   ed.open('src/main.c', text);
//   ed.setDiagnostics([{ line, severity, message }]);

import {
  EditorState, EditorView, Compartment, keymap, lineNumbers, highlightActiveLine, highlightActiveLineGutter,
  drawSelection, dropCursor, rectangularSelection, crosshairCursor, highlightSpecialChars, indentOnInput,
  bracketMatching, foldGutter, foldKeymap, indentUnit, defaultKeymap, history, historyKeymap, indentWithTab,
  autocompletion, completionKeymap, closeBrackets, closeBracketsKeymap, snippetCompletion, lintGutter,
  setDiagnostics, lintKeymap, searchKeymap, highlightSelectionMatches, cpp, hoverTooltip, Prec,
  StateField, StateEffect, Decoration, gutter, GutterMarker, ViewPlugin, WidgetType, syntaxTree,
} from './vendor/codemirror.js';
import { retrobox } from './theme.js';
import { asm6502 } from './asm-lang.js';
import { explain } from './diagnostics.js';
import { C_KEYWORDS } from './api.js';
import { h } from '../ui.js';

const isC = (path) => /\.(c|h)$/i.test(path);

// ── the debugger's marks: breakpoints in the margin, the line the CPU stopped on ──

/** Redraw the breakpoint margin (the Studio changed the breakpoints, or where the CPU is). */
const refreshMarks = StateEffect.define();
/** The line the CPU stopped on (a number), or 0. */
const setExec = StateEffect.define();
const execLine = StateField.define({
  create: () => Decoration.none,
  update(deco, tr) {
    deco = deco.map(tr.changes);
    for (const e of tr.effects) {
      if (!e.is(setExec)) continue;
      const n = e.value;
      deco = n > 0 && n <= tr.state.doc.lines ? Decoration.set([Decoration.line({ class: 'cm-exec-line' }).range(tr.state.doc.line(n).from)]) : Decoration.none;
    }
    return deco;
  },
  provide: (f) => EditorView.decorations.from(f),
});
class Mark extends GutterMarker {
  constructor(kind) {
    super();
    this.kind = kind;
  }
  eq(o) {
    return o.kind === this.kind;
  }
  toDOM() {
    return h(`span.cm-bp.cm-bp-${this.kind}`);
  }
}
const MARKS = { bp: new Mark('bp'), exec: new Mark('exec'), both: new Mark('both'), none: new Mark('none') };
// ── live values: the running game's globals, at the end of the lines that use them ──

/** The live values changed (or went away): redraw them. */
const liveTick = StateEffect.define();
const MAX_LIVE = 4;

/** Where `name` is given a number in declaration `text` (`int a, speed = -3;`): { at, value }, or null. */
function declared(text, name) {
  const m = new RegExp(`\\b${name}\\s*(?:\\[[^\\]]*\\])?\\s*=\\s*(-?(?:0x[0-9a-fA-F]+|\\d+))\\b`).exec(text);
  return m && !/[({]/.test(text.slice(0, m.index)) ? { at: m.index + m[0].length - m[1].length, value: m[1] } : null;
}

/** The same in assembly: `speed !byte 3`, `_speed: .byte $03` (one value, the line's label). */
function declaredAsm(text, name) {
  const m = new RegExp(`^(_?${name}:?\\s+(?:!(?:by|byte|8|08|wo|word|16)|\\.(?:byte|word))\\s+)(-?(?:\\$[0-9a-fA-F]+|\\d+))\\s*(?:;.*)?$`).exec(text);
  return m ? { at: m[1].length, value: m[2] } : null;
}

/**
 * A C line's names outside comments and strings ([name, index]), and where
 * its live values go (`at`): the line's end, or before a comment that ends it.
 */
function cWords(line, tree) {
  const words = [];
  let at = line.to;
  for (const m of line.text.matchAll(/\/[/*]|[A-Za-z_]\w*/g)) {
    const node = tree.resolveInner(line.from + m.index + 1, -1).name;
    if (m[0][0] !== '/') {
      if (!/Comment|String|Char/.test(node)) words.push([m[0], m.index]);
    } else if (/Comment/.test(node) && line.from + m.index < at && line.text.slice(0, m.index).trim()) at = line.from + line.text.slice(0, m.index).trimEnd().length;
  }
  return { words, at };
}

/** The same for an assembly line: the names before its ; comment, outside quotes. */
function asmWords(line) {
  let end = line.text.length;
  let quote = '';
  for (let i = 0; i < line.text.length && end === line.text.length; i++) {
    const ch = line.text[i];
    if (quote) quote = ch === quote ? '' : quote;
    else if (ch === '"' || ch === "'") quote = ch;
    else if (ch === ';') end = i;
  }
  const code = line.text.slice(0, end).replace(/"[^"]*"|'[^']*'/g, (q) => ' '.repeat(q.length));
  return { words: [...code.matchAll(/[A-Za-z_]\w*/g)].map((m) => [m[0], m.index]), at: line.from + code.trimEnd().length };
}

class LiveWidget extends WidgetType {
  constructor(items, ed) {
    super();
    this.items = items;
    this.ed = ed;
    this.key = items.map((i) => `${i.name}=${i.text}${i.changed ? '*' : ''}${i.keep ? '@' : ''}`).join(' ');
  }
  eq(o) {
    return o.key === this.key;
  }
  ignoreEvent() {
    return true;
  }
  toDOM() {
    return h('span.cm-live', {}, ...this.items.map((it) => {
      const graph = this.ed.onGraph && !/^\[/.test(it.text);
      const name = h(`span.cm-live-name${graph ? '.graphable' : ''}`, graph ? {
        title: `Graph ${it.name}: its last seconds, frame by frame`,
        onpointerdown: (e) => (e.preventDefault(), e.stopPropagation(), this.ed.onGraph(it.name)),
      } : {}, it.name);
      const val = h(`span.cm-live-value${it.changed ? '.changed' : ''}${it.editable ? '.editable' : ''}${it.keep ? '.keep' : ''}`, {
        title: !it.editable ? it.title
          : `${it.title}\nDrag sideways to change it in the running game (Shift: ×10), or click to type a value${it.keep ? '.\nHere, on its declaration, the new value is written into the code too: it stays after the next Run.' : '.\nOn the line that declares it (with = a number), a change goes into the code too.'}`,
      }, it.text);
      if (it.editable) this.ed.scrubbable(val, it);
      return h('span.cm-live-item', {}, name, val);
    }));
  }
}

const isAsm = (path) => /\.(s|asm|a|inc)$/i.test(path);

/** Snippets for the shapes a game's code takes. */
const SNIPPETS = [
  snippetCompletion('for (${i} = 0; ${i} < ${n}; ++${i}) {\n\t${}\n}', { label: 'for', detail: 'loop', type: 'keyword' }),
  snippetCompletion('if (${cond}) {\n\t${}\n}', { label: 'if', detail: 'block', type: 'keyword' }),
  snippetCompletion('for (;;) {\n\tbb_frame();\n\t${}\n}', { label: 'gameloop', detail: 'once a frame, forever', type: 'keyword' }),
  snippetCompletion('switch (${x}) {\ncase ${0}:\n\t${}\n\tbreak;\n}', { label: 'switch', detail: 'block', type: 'keyword' }),
];

export class CodeEditor {
  constructor(parent, { onChange, onRun, onSave, onFocus, onDefinition, onBreakpoint, onBreakpointsMoved } = {}) {
    this.onBreakpointsMoved = onBreakpointsMoved;
    this.breaks = new Set(); // the open file's breakpoint lines
    this.exec = 0; // the line the CPU stopped on in the open file (0: none)
    this.onBreakpoint = onBreakpoint;
    this.states = new Map(); // path -> EditorState, so each file keeps its undo history
    this.path = null;
    this.api = []; // bb.h entries (api.js)
    this.assets = []; // gen/assets.h names
    this.library = []; // cc65's headers the open file includes
    this.onChange = onChange;
    this.lang = new Compartment();
    this.readOnly = new Compartment();
    this.onDefinition = onDefinition;
    this.keys = Prec.high(keymap.of([
      { key: 'Mod-Enter', run: () => (onRun?.(), true) },
      { key: 'Mod-r', run: () => (onRun?.(), true) },
      { key: 'Mod-s', run: () => (onSave?.(), true) },
      { key: 'F12', run: (view) => this.goToDefinition(view.state.selection.main.head) },
      { key: 'F9', run: (view) => (this.onBreakpoint?.(view.state.doc.lineAt(view.state.selection.main.head).number), true) },
    ]));
    this.live = null; // name -> { text, title, editable, changed, num, min, max }: the running game's values
    this.liveHold = false; // a value is being dragged or typed: don't redraw under it
    this.onPoke = null; // (name, value): change a variable in the running game
    this.onKept = null; // (name, text, line): a tweak was written into the code
    this.onGraph = null; // (name): graph this variable over time
    const ed = this;
    this.livePlugin = ViewPlugin.fromClass(class {
      constructor(view) {
        this.decorations = ed.liveDecos(view);
      }
      update(u) {
        if (ed.liveHold) this.decorations = this.decorations.map(u.changes);
        else if (u.docChanged || u.viewportChanged || u.transactions.some((t) => t.effects.some((e) => e.is(liveTick)))) this.decorations = ed.liveDecos(u.view);
      }
    }, { decorations: (p) => p.decorations });
    this.view = new EditorView({ parent, state: this.fresh('', '') });
    this.view.contentDOM.addEventListener('focus', () => onFocus?.());
    // another look (themes/themes.js) brings other type: the editor measures its lines again, once the fonts are in
    addEventListener('breadbin:theme', () => document.fonts.ready.then(() => this.view.requestMeasure()));
    // Ctrl+click (Cmd+click on a Mac) on a name: to its definition
    this.view.contentDOM.addEventListener('mousedown', (e) => {
      if (!(e.ctrlKey || e.metaKey) || e.button !== 0) return;
      const pos = this.view.posAtCoords({ x: e.clientX, y: e.clientY });
      if (pos != null && this.goToDefinition(pos)) e.preventDefault();
    });
  }

  /** The word (an identifier) at `pos`, or ''. */
  wordAt(pos) {
    const line = this.view.state.doc.lineAt(pos);
    let a = pos - line.from;
    let b = a;
    while (a > 0 && /\w/.test(line.text[a - 1])) a--;
    while (b < line.text.length && /\w/.test(line.text[b])) b++;
    return line.text.slice(a, b);
  }

  /** Ask the Studio to show where the name at `pos` is defined; true if there was a name. */
  goToDefinition(pos) {
    const name = this.wordAt(pos);
    if (!/^[A-Za-z_]\w*$/.test(name) || !this.onDefinition) return false;
    this.onDefinition(name);
    return true;
  }

  /** A new editor state for `text` in the language of `path`. */
  fresh(path, text, readOnly = false) {
    return EditorState.create({
      doc: text,
      extensions: [
        // the breakpoint margin: click to set or clear one (F9 at the cursor)
        gutter({
          class: 'cm-bp-gutter',
          lineMarker: (view, block) => {
            const n = view.state.doc.lineAt(block.from).number;
            const bp = this.breaks.has(n);
            const ex = this.exec === n;
            return bp && ex ? MARKS.both : bp ? MARKS.bp : ex ? MARKS.exec : null;
          },
          lineMarkerChange: (u) => u.docChanged || u.transactions.some((t) => t.effects.some((e) => e.is(refreshMarks) || e.is(setExec))),
          initialSpacer: () => MARKS.none,
          domEventHandlers: {
            mousedown: (view, block) => {
              this.onBreakpoint?.(view.state.doc.lineAt(block.from).number);
              return true;
            },
          },
        }),
        execLine,
        this.livePlugin,
        lineNumbers(),
        foldGutter(),
        lintGutter(),
        highlightActiveLineGutter(),
        highlightSpecialChars(),
        // on a touch screen long lines wrap: sideways scrolling by touch loses the place
        matchMedia('(pointer: coarse)').matches ? EditorView.lineWrapping : [],
        history(),
        drawSelection(),
        dropCursor(),
        EditorState.allowMultipleSelections.of(true),
        indentOnInput(),
        bracketMatching(),
        closeBrackets(),
        rectangularSelection(),
        crosshairCursor(),
        highlightActiveLine(),
        highlightSelectionMatches(),
        indentUnit.of(isAsm(path) ? '        ' : '  '),
        EditorState.tabSize.of(8),
        this.lang.of(isC(path) ? cpp() : isAsm(path) ? asm6502 : []),
        this.readOnly.of(EditorState.readOnly.of(readOnly)),
        autocompletion({ override: [(ctx) => this.complete(ctx)], activateOnTyping: true, icons: true }),
        hoverTooltip((view, pos) => this.hover(view, pos)),
        this.keys,
        keymap.of([...closeBracketsKeymap, ...defaultKeymap, ...searchKeymap, ...historyKeymap, ...foldKeymap, ...completionKeymap, ...lintKeymap, indentWithTab]),
        retrobox,
        EditorView.updateListener.of((u) => {
          if (!u.docChanged || this.path !== path) return;
          this.onChange?.(path, u.state.doc.toString());
          // the breakpoints stay with their lines when lines come or go above them
          if (!this.breaks.size) return;
          const old = u.startState.doc;
          const moved = new Set([...this.breaks].map((n) => (n <= old.lines ? u.state.doc.lineAt(u.changes.mapPos(old.line(n).from, 1)).number : n)));
          if ([...moved].some((n) => !this.breaks.has(n)) || moved.size !== this.breaks.size) {
            this.breaks = moved;
            this.onBreakpointsMoved?.(path, [...moved]);
          }
        }),
      ],
    });
  }

  /** Show `path` with `text`; an open file keeps its cursor and history unless its text changed elsewhere. */
  open(path, text, { readOnly = false } = {}) {
    if (this.path) this.states.set(this.path, this.view.state);
    let state = this.states.get(path);
    if (!state || state.doc.toString() !== text || state.facet(EditorState.readOnly) !== readOnly) {
      state = state && state.doc.toString() !== text && !readOnly ? this.replaced(state, text) : this.fresh(path, text, readOnly);
    }
    this.path = path;
    this.view.setState(state);
  }

  /** `state` with its whole text replaced (as one undoable change): an edit from outside. */
  replaced(state, text) {
    return state.update({ changes: { from: 0, to: state.doc.length, insert: text } }).state;
  }

  /** Text changed outside the editor (an asset editor, another program): update quietly. */
  update(path, text) {
    if (path === this.path) {
      if (this.view.state.doc.toString() !== text) this.view.dispatch({ changes: { from: 0, to: this.view.state.doc.length, insert: text } });
    } else this.states.delete(path);
  }

  /** The open file's breakpoints (line numbers). */
  setBreakpoints(lines) {
    this.breaks = new Set(lines);
    this.view.dispatch({ effects: refreshMarks.of(null) });
  }

  /**
   * The running game's values (a Map: name -> { text, title, editable,
   * changed, num, min, max }), shown at the end of each line that uses them;
   * null hides them.
   */
  setLive(values) {
    if (!values && !this.live) return;
    this.live = values;
    if (!this.liveHold) this.view.dispatch({ effects: liveTick.of(null) });
  }

  /** The widgets for the visible lines: each line's known names, in the order they come. */
  liveDecos(view) {
    const asm = isAsm(this.path || '');
    if (!this.live?.size || !(asm || isC(this.path || ''))) return Decoration.none;
    const tree = !asm && syntaxTree(view.state);
    const out = [];
    for (const { from, to } of view.visibleRanges) {
      for (let pos = from; pos <= to;) {
        const line = view.state.doc.lineAt(pos);
        pos = line.to + 1;
        const { words, at } = asm ? asmWords(line) : cWords(line, tree);
        const items = [];
        for (const [word, index] of words) {
          // (a C global used from assembly is `_name`)
          const name = this.live.has(word) ? word : asm && word[0] === '_' && this.live.has(word.slice(1)) ? word.slice(1) : null;
          if (!name || items.some((i) => i.name === name)) continue;
          const v = this.live.get(name);
          // on the line that declares it with a number (`int speed = 3;`, `speed !byte 3`), a tweak is written into the code too
          const declares = asm ? index === 0 && declaredAsm(line.text, name) : /^[a-z]/.test(line.text) && declared(line.text, name);
          items.push({ name, ...v, keep: v.editable && declares ? line.number : 0 });
          if (items.length === MAX_LIVE) break;
        }
        if (items.length) out.push(Decoration.widget({ widget: new LiveWidget(items, this), side: 1 }).range(at));
      }
    }
    return Decoration.set(out);
  }

  /** Make a live value's element change the variable: dragged sideways, or clicked and typed. */
  scrubbable(el, it) {
    el.addEventListener('pointerdown', (e) => {
      if (e.button !== 0) return;
      e.preventDefault();
      e.stopPropagation();
      const x0 = e.clientX;
      let moved = false;
      let last = it.num;
      this.liveHold = true;
      el.setPointerCapture(e.pointerId);
      el.classList.add('scrubbing');
      const move = (ev) => {
        const dx = ev.clientX - x0;
        if (Math.abs(dx) > 3) moved = true;
        if (!moved) return;
        const n = Math.max(it.min, Math.min(it.max, it.num + Math.round(dx / 5) * (ev.shiftKey ? 10 : 1)));
        if (n === last) return;
        last = n;
        el.textContent = String(n);
        this.onPoke?.(it.name, n);
      };
      const up = () => {
        el.removeEventListener('pointermove', move);
        el.removeEventListener('pointerup', up);
        el.removeEventListener('pointercancel', up);
        el.classList.remove('scrubbing');
        if (moved) {
          this.liveHold = false;
          if (last !== it.num) this.keepValue(it, last);
          this.view.dispatch({ effects: liveTick.of(null) });
        } else this.typeValue(el, it);
      };
      el.addEventListener('pointermove', move);
      el.addEventListener('pointerup', up);
      el.addEventListener('pointercancel', up);
    });
  }

  /** A tweak made on a variable's declaring line (it.keep): the new number goes into the code, as one undoable edit. */
  keepValue(it, n) {
    if (!it.keep || it.keep > this.view.state.doc.lines) return;
    const line = this.view.state.doc.line(it.keep);
    const m = isAsm(this.path || '') ? declaredAsm(line.text, it.name) : declared(line.text, it.name);
    if (!m) return;
    // in the number's own form: 0x.. or $.. stays hexadecimal, as wide as it was
    const hex = /^(0x|\$)/i.exec(m.value)?.[0];
    const text = hex && n >= 0 ? `${hex}${n.toString(16).toUpperCase().padStart(m.value.length - hex.length, '0')}` : String(n);
    this.view.dispatch({ changes: { from: line.from + m.at, to: line.from + m.at + m.value.length, insert: text } });
    this.onKept?.(it.name, text, it.keep);
  }

  /** A box in place of a live value: Enter writes what was typed into the game, Esc leaves it. */
  typeValue(el, it) {
    const input = h('input.cm-live-input', { value: String(it.num), size: Math.max(3, String(it.num).length + 1) });
    let done = false;
    const finish = (write) => {
      if (done) return;
      done = true;
      const n = Number(input.value.trim().replace(/^\$/, '0x'));
      const v = Math.max(it.min, Math.min(it.max, Math.round(n)));
      if (write && Number.isFinite(n)) this.onPoke?.(it.name, v);
      this.liveHold = false;
      if (write && Number.isFinite(n) && v !== it.num) this.keepValue(it, v);
      input.replaceWith(el);
      this.view.dispatch({ effects: liveTick.of(null) });
      this.view.focus();
    };
    input.addEventListener('keydown', (e) => {
      e.stopPropagation();
      if (e.key === 'Enter') finish(true);
      else if (e.key === 'Escape') finish(false);
    });
    input.addEventListener('blur', () => finish(true));
    input.addEventListener('pointerdown', (e) => e.stopPropagation());
    el.replaceWith(input);
    input.focus();
    input.select();
  }

  /** The line the CPU stopped on in the open file (0: not in this file), scrolled to. */
  setExecLine(line) {
    this.exec = line;
    const effects = [setExec.of(line)];
    if (line > 0 && line <= this.view.state.doc.lines) effects.push(EditorView.scrollIntoView(this.view.state.doc.line(line).from, { y: 'center' }));
    this.view.dispatch({ effects });
  }

  forget(path) {
    this.states.delete(path);
  }

  get text() {
    return this.view.state.doc.toString();
  }

  focus() {
    this.view.focus();
  }

  /** Put the cursor on `line` (1-based) and scroll there. */
  goto(line, col = 1) {
    const doc = this.view.state.doc;
    const l = doc.line(Math.max(1, Math.min(doc.lines, line)));
    const at = Math.min(l.to, l.from + col - 1);
    this.view.dispatch({ selection: { anchor: at }, scrollIntoView: true, effects: EditorView.scrollIntoView(at, { y: 'center' }) });
    this.view.focus();
  }

  /** Markers for the open file: [{ line, severity, message }]. */
  setDiagnostics(list) {
    const doc = this.view.state.doc;
    const diags = list.filter((d) => d.line > 0).map((d) => {
      const l = doc.line(Math.max(1, Math.min(doc.lines, d.line)));
      // mark the line's text, without its indentation
      const indent = /^\s*/.exec(l.text)[0].length;
      return { from: Math.min(l.to, l.from + indent), to: l.to, severity: d.severity === 'error' ? 'error' : 'warning', message: explain(d.message) ? `${d.message}\n${explain(d.message)}` : d.message, source: d.tool };
    });
    this.view.dispatch(setDiagnostics(this.view.state, diags));
  }

  /**
   * The runtime's API (bb.h), the asset names (gen/assets.h) and cc65's library
   * (the headers the file includes: api.js parseHeader), for completion and hover.
   */
  setNames({ api, assets, library }) {
    if (api) this.api = api;
    if (assets) this.assets = assets;
    if (library) this.library = library;
  }

  complete(ctx) {
    const word = ctx.matchBefore(/[\w]+/);
    if (!word || (word.from === word.to && !ctx.explicit)) return null;
    if (!isC(this.path)) return this.completeWords(ctx, word);
    const options = [
      ...this.api.map((e) => ({
        label: e.name,
        type: e.kind === 'function' ? 'function' : e.kind === 'variable' ? 'variable' : 'constant',
        detail: e.kind === 'function' ? e.signature.replace(/^.*?\b(bb_\w+)/, '$1').replace(/^bb_\w+/, '') : '',
        info: () => info(e.signature, e.doc),
        // a function without arguments completes with its brackets
        apply: e.kind === 'function' && /\(void\)$/.test(e.signature) ? `${e.name}()` : undefined,
        boost: 2,
      })),
      ...this.assets.map((a) => ({ label: a.name, type: 'constant', detail: `${a.value}`, info: a.group, boost: 3 })),
      ...(this.library || []).map((e) => ({
        label: e.name,
        type: e.kind === 'function' ? 'function' : 'constant',
        detail: `<${e.from}>`,
        info: () => info(e.signature, e.doc),
        boost: 1,
      })),
      ...C_KEYWORDS.map((k) => ({ label: k, type: /^(int|char|void|unsigned|signed|long|short|const|static|volatile)$/.test(k) ? 'type' : 'keyword' })),
      ...SNIPPETS,
      ...this.wordsIn(ctx.state, word).map((w) => ({ label: w, type: 'text', boost: -1 })),
    ];
    return { from: word.from, options, validFor: /^\w*$/ };
  }

  completeWords(ctx, word) {
    return { from: word.from, options: this.wordsIn(ctx.state, word).map((w) => ({ label: w, type: 'text' })), validFor: /^\w*$/ };
  }

  /** Identifiers used elsewhere in the file (labels, variables), for completion. */
  wordsIn(state, word) {
    const seen = new Set();
    const typed = state.sliceDoc(word.from, word.to);
    for (const m of state.doc.toString().matchAll(/[A-Za-z_]\w{2,}/g)) if (m[0] !== typed) seen.add(m[0]);
    return [...seen].slice(0, 400);
  }

  hover(view, pos) {
    const line = view.state.doc.lineAt(pos);
    const text = line.text;
    let a = pos - line.from;
    let b = a;
    while (a > 0 && /\w/.test(text[a - 1])) a--;
    while (b < text.length && /\w/.test(text[b])) b++;
    const name = text.slice(a, b);
    const e = this.api.find((x) => x.name === name) || (this.library || []).find((x) => x.name === name);
    const asset = !e && this.assets.find((x) => x.name === name);
    if (!e && !asset) return null;
    return {
      pos: line.from + a,
      end: line.from + b,
      above: true,
      create: () => ({ dom: e ? info(e.signature, e.doc) : info(`${asset.name} = ${asset.value}`, asset.group ? `One of the project's ${asset.group}.` : '') }),
    };
  }
}

/** The help shown for an API entry: its signature in code type, then its comment. */
function info(signature, doc) {
  // (`code` in a comment shows as code)
  return h('div.studio-info', {}, h('code', {}, signature), doc && h('p', {}, ...doc.split('`').map((part, i) => (i % 2 ? h('code', {}, part) : part))));
}
