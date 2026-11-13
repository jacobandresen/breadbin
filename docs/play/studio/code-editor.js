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
} from './vendor/codemirror.js';
import { retrobox } from './theme.js';
import { asm6502 } from './asm-lang.js';
import { C_KEYWORDS } from './api.js';

const isC = (path) => /\.(c|h)$/i.test(path);
const isAsm = (path) => /\.(s|asm|a|inc)$/i.test(path);

/** Snippets for the shapes a game's code takes. */
const SNIPPETS = [
  snippetCompletion('for (${i} = 0; ${i} < ${n}; ++${i}) {\n\t${}\n}', { label: 'for', detail: 'loop', type: 'keyword' }),
  snippetCompletion('if (${cond}) {\n\t${}\n}', { label: 'if', detail: 'block', type: 'keyword' }),
  snippetCompletion('for (;;) {\n\tbb_frame();\n\t${}\n}', { label: 'gameloop', detail: 'once a frame, forever', type: 'keyword' }),
  snippetCompletion('switch (${x}) {\ncase ${0}:\n\t${}\n\tbreak;\n}', { label: 'switch', detail: 'block', type: 'keyword' }),
];

export class CodeEditor {
  constructor(parent, { onChange, onRun, onSave, onFocus, onDefinition } = {}) {
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
    ]));
    this.view = new EditorView({ parent, state: this.fresh('', '') });
    this.view.contentDOM.addEventListener('focus', () => onFocus?.());
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
        lineNumbers(),
        foldGutter(),
        lintGutter(),
        highlightActiveLineGutter(),
        highlightSpecialChars(),
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
          if (u.docChanged && this.path === path) this.onChange?.(path, u.state.doc.toString());
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
      return { from: Math.min(l.to, l.from + indent), to: l.to, severity: d.severity === 'error' ? 'error' : 'warning', message: d.message, source: d.tool };
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
  const el = document.createElement('div');
  el.className = 'studio-info';
  const sig = document.createElement('code');
  sig.textContent = signature;
  el.append(sig);
  if (doc) {
    const p = document.createElement('p');
    // `code` in a comment shows as code
    for (const [i, part] of doc.split('`').entries()) {
      if (i % 2) {
        const c = document.createElement('code');
        c.textContent = part;
        p.append(c);
      } else p.append(part);
    }
    el.append(p);
  }
  return el;
}
