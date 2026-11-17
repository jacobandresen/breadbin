// console - a drop-down console over the Studio, a tribute to the one a 1996
// shooter slid down from the top of the screen at the press of the key under
// Esc. Half the Studio high, it logs each build (its result and its
// problems, file:line) and takes a few commands: help, run, build, stop,
// open, mode, ls, clear, theme, quit.
//
// It is a look's choice: on only where the theme's CSS says --studio-console: 1
// (Chicago's). The key is the backquote (`), as it was.

import { h } from '../ui.js';

const VERSION = 'breadbin 1.06';

export class Console {
  constructor(studio) {
    this.studio = studio;
    this.lines = h('div.studio-console-lines', { role: 'log', 'aria-live': 'polite' });
    this.input = h('input.studio-console-input', { 'aria-label': 'Console command', autocomplete: 'off', spellcheck: false });
    this.el = h('section.studio-console', { hidden: true, 'aria-label': 'Console' },
      this.lines,
      h('label.studio-console-prompt', {}, h('span', { 'aria-hidden': 'true' }, ']'), this.input),
      h('span.studio-console-version', { 'aria-hidden': 'true' }, VERSION));
    this.history = [];
    this.at = 0;
    this.input.addEventListener('keydown', (e) => this.key(e));
    this.print(`${VERSION} console. Type help for the commands, \` to close.`);
    this.watch();
  }

  /** On in this look? (themes say so with --studio-console: 1) */
  get enabled() {
    return getComputedStyle(this.studio.root).getPropertyValue('--studio-console').trim() === '1';
  }

  toggle(open = this.el.hidden) {
    if (open && !this.enabled) return;
    this.el.hidden = !open;
    if (open) this.input.focus();
    else this.studio.code.focus?.();
  }

  print(text, kind = '') {
    this.lines.append(h(`div${kind ? `.${kind}` : ''}`, {}, text));
    while (this.lines.childElementCount > 300) this.lines.firstChild.remove();
    this.lines.scrollTop = this.lines.scrollHeight;
  }

  /** Each build into the log: the line the Studio shows, then its problems. */
  watch() {
    const info = this.studio.ui.buildInfo;
    new MutationObserver(() => {
      const text = info.textContent;
      if (!text || text === 'Building…') return;
      this.print(text, info.classList.contains('bad') ? 'bad' : 'ok');
      for (const d of this.studio.diagnostics || []) this.print(`${d.file}:${d.line}: ${d.severity}: ${d.message}`, d.severity === 'error' ? 'bad' : '');
    }).observe(info, { childList: true, characterData: true, subtree: true });
  }

  key(e) {
    if (e.key === '`') return e.preventDefault(), this.toggle(false);
    if (e.key === 'Escape') return this.toggle(false);
    if (e.key === 'ArrowUp' || e.key === 'ArrowDown') {
      e.preventDefault();
      this.at = Math.max(0, Math.min(this.history.length, this.at + (e.key === 'ArrowUp' ? -1 : 1)));
      this.input.value = this.history[this.at] || '';
      return;
    }
    if (e.key !== 'Enter') return;
    const line = this.input.value.trim();
    this.input.value = '';
    if (!line) return;
    this.history.push(line);
    this.at = this.history.length;
    this.print(`]${line}`, 'echo');
    this.run(line);
  }

  run(line) {
    const [cmd, ...args] = line.split(/\s+/);
    const arg = args.join(' ');
    const s = this.studio;
    const files = () => Object.keys(s.files).sort();
    const commands = {
      help: () => Object.entries(HELP).forEach(([c, what]) => this.print(`  ${c.padEnd(12)} ${what}`)),
      run: () => s.run(),
      build: () => s.build(),
      stop: () => (s.stop(), this.print('stopped')),
      ls: () => this.print(files().join('  ')),
      dir: () => commands.ls(),
      open: () => {
        const path = files().find((p) => p === arg) || files().find((p) => p.endsWith(`/${arg}`) || p.endsWith(arg));
        if (!path) return this.print(`no file "${arg}"`, 'bad');
        s.openFile(path);
        this.toggle(false);
      },
      mode: () => {
        if (!['code', 'sprites', 'tiles', 'map', 'music'].includes(arg)) return this.print('mode code | sprites | tiles | map | music', 'bad');
        s.setMode(arg);
      },
      clear: () => this.lines.replaceChildren(),
      theme: async () => {
        const [{ THEMES, applyTheme }, { save }] = await Promise.all([import('../themes/themes.js'), import('../settings.js')]);
        if (!THEMES.some((t) => t.id === arg)) return this.print(`theme ${THEMES.map((t) => t.id).join(' | ')}`, 'bad');
        save({ theme: applyTheme(arg) });
      },
      quit: () => this.toggle(false),
      exit: () => this.toggle(false),
    };
    (commands[cmd] || (() => this.print(`Unknown command "${cmd}"`, 'bad')))();
  }
}

const HELP = {
  run: 'build the game and run it on the C64',
  build: 'build it, without running',
  stop: 'stop the C64',
  'open <file>': 'open a file (main.c, sprites.json...)',
  'mode <m>': 'code, sprites, tiles, map or music',
  ls: 'the project\'s files',
  clear: 'clear the console',
  'theme <id>': 'another look (bakery, classic, geos, chicago, cupertino)',
  quit: 'close the console (or `)',
};
