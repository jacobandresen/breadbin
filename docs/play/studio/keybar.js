// keybar - writing code on a phone: the keys a phone's keyboard hides, in a
// row above it while the code editor has the focus (touch screens only).
// The symbols C is made of are two keyboard pages away there; moving the
// cursor by touch is fiddly; Ctrl+Space isn't there: so the symbols, Tab,
// the arrows, undo and ⋯ (complete: bb_ calls, the assets, your names).
// The row sits on top of the on-screen keyboard: the visual viewport says
// how much of the screen the keyboard took. A key acts on the touch itself
// and keeps the editor's focus, so the keyboard stays up.

import { undo, indentMore, cursorCharLeft, cursorCharRight, cursorLineUp, cursorLineDown, startCompletion } from './vendor/codemirror.js';
import { h } from '../ui.js';

const SYMBOLS = ['{', '}', '(', ')', ';', '=', '[', ']', '<', '>', '"', "'", '+', '-', '*', '/', '&', '|', '!', '#', ',', '_'];
const COMMANDS = [
  ['⇥', 'Tab: indent', indentMore],
  ['←', 'Left', cursorCharLeft],
  ['→', 'Right', cursorCharRight],
  ['↑', 'Up', cursorLineUp],
  ['↓', 'Down', cursorLineDown],
  ['↶', 'Undo', undo],
  ['⋯', 'Complete: the names that fit here (Ctrl+Space)', startCompletion],
];

/** The key row for editor `view` (an EditorView): appended to the page, shown while `view` has the focus. */
export function keyBar(view) {
  const key = (label, title, act) => h('button.studio-key', {
    title,
    'aria-label': title,
    onpointerdown: (e) => {
      e.preventDefault(); // (the editor keeps the focus: no blur, the keyboard stays)
      act();
      view.focus();
    },
  }, label);
  const type = (s) => view.dispatch({ ...view.state.replaceSelection(s), scrollIntoView: true, userEvent: 'input.type' });
  const bar = h('div.studio-keybar', { hidden: true, role: 'toolbar', 'aria-label': 'Code keys' },
    ...COMMANDS.map(([label, title, cmd]) => key(label, title, () => cmd(view))),
    ...SYMBOLS.map((s) => key(s, `Type ${s}`, () => type(s))));
  // above the on-screen keyboard: the visual viewport ends where the keyboard starts
  const vv = window.visualViewport;
  const place = () => (bar.style.bottom = `${vv ? Math.max(0, innerHeight - vv.height - vv.offsetTop) : 0}px`);
  vv?.addEventListener('resize', place);
  vv?.addEventListener('scroll', place);
  view.contentDOM.addEventListener('focus', () => ((bar.hidden = false), place()));
  view.contentDOM.addEventListener('blur', () => (bar.hidden = true));
  document.body.append(bar);
  return bar;
}
