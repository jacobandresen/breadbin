// studio - the Hack tab: a small game studio for the C64.
// One project at a time, in five modes - CODE (C or assembly), SPRITES,
// TILES, MAP and MUSIC - and a C64 beside them that runs the game on ▶ Run.
//
// The project is a folder of plain files (project.js). It lives in this
// browser, or in a folder on your disk (storage.js), where git, your own
// editor and a native cc65 see the same files; edits made there show up here.
// Builds run in a worker (builder.js, toolchain.js); the C64 is vc64web
// (../emu.js) on the free Open ROMs.

import * as emu from '../emu.js';
import * as store from '../store.js';
import { prepare, patchedKernal, patchedBasic } from '../engine.js';
import { h, toast, share } from '../ui.js';
import { packCart } from '../cart.js';
import { createPad } from '../touch.js';
import { SidPlayer } from '../sid.js';
import { CodeEditor } from './code-editor.js';
import { build, warmUp, psid, headers } from './builder.js';
import { BrowserStore, FolderStore, canUseFolders, browserProjects, folderIsEmpty, rememberedFolders } from './storage.js';
import { meta, normalise, sortPaths, isText, isGenerated, slug, pretty, hasAssets, hasSources } from './project.js';
import { zip, unzip } from './zip.js';
import { summary } from './diagnostics.js';
import { parseApi, parseAssetNames, parseHeader, systemIncludes } from './api.js';
import { resourceText } from './resource.js';
import { EXAMPLES, exampleFiles } from './examples.js';
import { SpriteEditor } from './sprite-editor.js';
import { TileEditor } from './tile-editor.js';
import { MapEditor } from './map-editor.js';
import { MusicEditor } from './music-editor.js';
import { helpSheet } from './help.js';
import { globals, format, ranges, cTypes } from './variables.js';
import { blankSprite } from './assets/sprites.js';
import { newTileset } from './assets/tiles.js';
import { blankRoom } from './assets/map.js';
import { newMusic } from './assets/music.js';
import { findDefinition } from './definitions.js';
import { DebugInfo } from './debuginfo.js';
import { Debugger, formatRegs } from './debugger.js';

const MODES = [
  { id: 'code', label: 'Code', key: '1', icon: '{ }' },
  { id: 'sprites', label: 'Sprites', key: '2', icon: '☺' },
  { id: 'tiles', label: 'Tiles', key: '3', icon: '▦' },
  { id: 'map', label: 'Map', key: '4', icon: '▤' },
  { id: 'music', label: 'Music', key: '5', icon: '♫' },
];
const ASSET_MODES = { sprites: 'assets/sprites.json', tiles: 'assets/tiles.json', map: 'assets/map.json', music: 'assets/music.json' };
const SAVE_DELAY = 400;
/** A phone or tablet: the control deck is on until switched off. */
const TOUCH = matchMedia('(pointer: coarse)').matches;
const POLL = 1500;

const prefs = {
  get: (k, d) => store.load(`studio.${k}`, d),
  set: (k, v) => store.save(`studio.${k}`, v),
};

export class Studio {
  constructor(root) {
    this.root = root;
    this.files = {};
    this.store = null;
    this.key = '';
    this.mode = 'code';
    this.path = '';
    this.pending = {}; // path -> data, not yet written
    this.saveTimer = 0;
    this.saving = Promise.resolve();
    this.diagnostics = [];
    this.session = null;
    this.lastBuild = null;
    this.sid = new SidPlayer();
    this.assetCache = {};
    this.runOnSave = prefs.get('runOnSave', true);
    this.breaks = new Map(); // file -> Set of lines: the breakpoints (kept per project)
    this.debugInfo = null; // the last build's lines and addresses (debuginfo.js)
    this.stopAt = null; // where the CPU stopped: { pc, regs, where, func }
    this.dbg = new Debugger({
      onStop: (s) => this.debugStopped(s),
      onRun: () => this.debugRunning(),
      onError: (m) => toast(`The debugger: ${m}`),
    });
  }

  // ── layout ──────────────────────────────────────────────────────────────

  async start() {
    const r = this.root;
    r.classList.add('studio');
    r.replaceChildren();

    this.ui = {};
    const u = this.ui;
    u.project = h('button.studio-project', { onclick: () => this.openPicker(), title: 'Projects: open, new, save to a folder' },
      h('span.studio-cart'), h('span.studio-project-name', {}, '…'), h('span.studio-caret', {}, '▾'));
    u.saved = h('span.studio-saved', { title: 'Where this project is saved' });
    u.modes = h('nav.studio-modes', { role: 'tablist' }, ...MODES.map((m) =>
      h('button.studio-mode', { role: 'tab', 'data-mode': m.id, title: `${m.label} (Alt+${m.key})`, onclick: () => this.setMode(m.id) },
        h('span.studio-mode-icon', {}, m.icon), h('span.studio-mode-label', {}, m.label))));
    u.run = h('button.key.pill.primary.studio-run', { onclick: () => this.run(), title: 'Build and run (Ctrl+Enter)' }, '▶ Run');
    u.stop = h('button.key.pill.studio-stop', { onclick: () => this.stop(), title: 'Stop the C64', disabled: true }, '■');
    u.help = h('button.key.pill.studio-helpbtn', { onclick: () => this.openHelp(), title: 'The bb.h cheat sheet (F1)' }, '?');
    const bar = h('header.studio-bar', {}, u.project, u.saved, u.modes, h('div.studio-actions', {}, u.run, u.stop, u.help));

    // CODE: files + editor
    u.files = h('div.studio-filelist');
    u.fileTools = h('div.studio-filetools', {},
      h('button.studio-tool', { onclick: () => this.newFile(), title: 'New file' }, '+ file'),
      h('button.studio-tool', { onclick: () => this.renameFile(), title: 'Rename the open file' }, 'rename'),
      h('button.studio-tool', { onclick: () => this.deleteFile(), title: 'Delete the open file' }, 'delete'));
    u.codeHost = h('div.studio-code');
    u.codeHead = h('div.studio-codehead');
    u.splitFiles = h('div.studio-split', { title: 'Drag to widen the file list (double-click: as it was)', role: 'separator', 'aria-orientation': 'vertical' });
    const code = h('section.studio-pane.studio-pane-code', { 'data-mode': 'code' },
      h('aside.studio-files', {}, h('div.studio-files-head', {}, 'Files'), u.files, u.fileTools),
      u.splitFiles,
      h('div.studio-codewrap', {}, u.codeHead, u.codeHost));
    u.code = code;
    this.code = new CodeEditor(u.codeHost, {
      onChange: (path, text) => this.edited(path, text),
      onRun: () => this.run(),
      onSave: () => this.saveNow(true),
      onDefinition: (name) => this.goToDefinition(name),
      onBreakpoint: (line) => this.toggleBreakpoint(this.path, line),
      onBreakpointsMoved: (path, lines) => {
        // kept for the next build; the running program is the old code, with the old lines
        this.breaks.set(path, new Set(lines));
        this.saveBreakpoints({ send: false });
      },
    });
    this.code.view.dom.addEventListener('keyup', () => this.showCursor());
    this.code.view.dom.addEventListener('click', () => this.showCursor());

    // the asset editors
    this.editors = {
      sprites: new SpriteEditor(this),
      tiles: new TileEditor(this),
      map: new MapEditor(this),
      music: new MusicEditor(this),
    };
    const panes = Object.entries(this.editors).map(([id, ed]) => h('section.studio-pane', { 'data-mode': id }, ed.el));

    // the C64 and what the build said
    u.stage = h('div.studio-stage', {}, h('div.studio-idle', {}, h('b', {}, 'Press ▶ Run'), h('span', {}, 'to build the game and start it here')));
    u.meter = h('div.studio-meter', { title: 'Memory the program uses, of the 46 KB from $0801 to $BFFF' });
    u.buildInfo = h('div.studio-buildinfo');
    u.problems = h('div.studio-problems');
    u.share = h('button.key.pill.studio-share', { onclick: () => this.shareGame(), disabled: true, title: 'A link that plays this game: the whole program is in it' }, '🔗 Share');
    u.live = h('label.studio-live', { title: 'Build and run again whenever a file is saved, here or in your own editor' },
      h('input', { type: 'checkbox', checked: this.runOnSave, onchange: (e) => prefs.set('runOnSave', (this.runOnSave = e.target.checked)) }), ' Run on save');
    u.vars = h('div.studio-vars-list');
    u.varsBox = h('details.studio-vars', { open: prefs.get('vars', false), ontoggle: (e) => prefs.set('vars', e.target.open) },
      h('summary', {}, 'Variables', h('span.studio-small', {}, ' · the running game\'s globals, live')), u.vars);
    // the control deck: an arcade stick, FIRE and the keys games ask for, as the player has
    u.deck = h('div.studio-deck', { hidden: true });
    u.padBtn = h('button.key.pill.studio-padbtn', { onclick: () => this.setPad(!prefs.get('pad', TOUCH)), title: 'A control deck under the C64: a stick, FIRE and keys (for a phone or tablet, or a mouse)', 'aria-pressed': 'false' }, '🕹 Pad');
    // the debugger's bar: pause and the steps, where the CPU stopped, its registers
    const dbtn = (cls, label, title, fn) => h(`button.studio-dbtn.${cls}`, { onclick: fn, title }, label);
    u.dbgPause = dbtn('pause', '⏸', 'Pause at the next line of your code (F6)', () => this.debugToggle());
    u.dbgOver = dbtn('over', '↷', 'Step over: the next line here (F10)', () => this.dbg.over());
    u.dbgInto = dbtn('into', '↓', 'Step into: into the call on this line (F11)', () => this.dbg.into());
    u.dbgOut = dbtn('out', '↑', 'Step out: back to the line that called this function (Shift+F11)', () => this.dbg.out());
    u.dbgWhere = h('button.studio-dbg-where', { onclick: () => this.showStop(), title: 'Show this line' });
    u.dbgRegs = h('div.studio-dbg-regs');
    u.dbgClear = h('button.studio-tool.studio-dbg-clear', { onclick: () => this.clearBreakpoints(), title: 'Clear every breakpoint in the project' }, '');
    u.debug = h('div.studio-debug', { hidden: true },
      h('div.studio-dbg-bar', {}, u.dbgPause, u.dbgOver, u.dbgInto, u.dbgOut, u.dbgWhere),
      h('div.studio-dbg-foot', {}, u.dbgRegs, u.dbgClear));
    u.side = h('aside.studio-side', {},
      (u.screen = h('div.studio-screen', {}, u.stage,
        h('button.studio-fullbtn', { onclick: () => this.fullscreen(), title: 'The C64 full screen (Alt+Enter; Esc to come back)', 'aria-label': 'Full screen' }, '⛶'))),
      u.debug,
      u.deck,
      h('div.studio-sidebar', {}, u.buildInfo, u.share, u.padBtn, u.live),
      u.meter, u.problems, u.varsBox);

    u.status = h('footer.studio-status', {}, h('span.studio-pos'), h('span.studio-lang'), h('span.studio-hint', {}, 'Ctrl+Enter run · Ctrl+S save · F9 breakpoint · F10/F11 step · F1 help'));
    u.banner = h('div.studio-banner', { hidden: true });
    u.splitSide = h('div.studio-split', { title: 'Drag to give the code or the C64 more room (double-click: as it was)', role: 'separator', 'aria-orientation': 'vertical' });
    u.main = h('div.studio-main', {}, h('div.studio-work', {}, code, ...panes), u.splitSide, u.side);
    r.append(bar, u.banner, u.main, u.status);
    this.layout();

    this.bindKeys();
    this.loadApi();
    warmUp();
    setInterval(() => this.poll(), POLL);
    setInterval(() => this.watch(), 250);
    addEventListener('beforeunload', (e) => {
      if (Object.keys(this.pending).length) {
        this.saveNow();
        e.preventDefault();
      }
    });
    addEventListener('breadbin:launch', () => this.stop());
    await this.restore();
    if (this.ui.banner.hidden) this.welcome();
  }

  // ── the layout: sizes you drag, the C64 full screen, the code's size ──────

  /** Put back the sizes of the last visit and make the handles drag. */
  layout() {
    const u = this.ui;
    const set = () => {
      const side = prefs.get('sideWidth', 0);
      const files = prefs.get('filesWidth', 0);
      this.root.style.setProperty('--studio-side', side ? `${side}px` : '');
      this.root.style.setProperty('--studio-files', files ? `${files}px` : '');
      this.root.style.setProperty('--studio-code-size', `${prefs.get('codeSize', 13.5)}px`);
      u.code.classList.toggle('nofiles', prefs.get('hideFiles', false));
      this.code.view.requestMeasure();
    };
    set();
    u.padBtn.setAttribute('aria-pressed', String(prefs.get('pad', TOUCH)));
    // the C64's panel: its left edge follows the pointer
    // (x is where the handle's left edge goes: the panel starts past its 7 pixels)
    this.dragHandle(u.splitSide, (x) => {
      const r = u.main.getBoundingClientRect();
      prefs.set('sideWidth', Math.round(Math.max(300, Math.min(r.width - 360, r.right - x - 7))));
    }, () => prefs.set('sideWidth', 0), set);
    // the file list: its right edge follows the pointer
    this.dragHandle(u.splitFiles, (x) => {
      const r = u.code.getBoundingClientRect();
      prefs.set('filesWidth', Math.round(Math.max(120, Math.min(480, x - r.left))));
    }, () => prefs.set('filesWidth', 0), set);
  }

  /**
   * Make `el` a handle: `move(x)` as it is dragged (x: where its left edge
   * goes, so it moves with the pointer from wherever it was grabbed),
   * `reset()` on a double-click, `apply()` after either.
   */
  dragHandle(el, move, reset, apply) {
    let grab = 0;
    el.addEventListener('pointerdown', (e) => {
      if (e.button !== 0) return;
      e.preventDefault();
      grab = e.clientX - el.getBoundingClientRect().left;
      el.setPointerCapture(e.pointerId);
      el.classList.add('dragging');
      this.root.classList.add('dragging');
    });
    el.addEventListener('pointermove', (e) => {
      if (!el.classList.contains('dragging')) return;
      move(e.clientX - grab);
      apply();
    });
    const stop = () => {
      el.classList.remove('dragging');
      this.root.classList.remove('dragging');
    };
    el.addEventListener('pointerup', stop);
    el.addEventListener('pointercancel', stop);
    el.addEventListener('dblclick', () => (reset(), apply()));
  }

  /** The control deck under the C64 on (`on`) or off; it drives the running C64's joystick 2. */
  setPad(on) {
    prefs.set('pad', on);
    this.ui.padBtn.setAttribute('aria-pressed', String(on));
    const show = on && !!this.session;
    this.ui.deck.hidden = !show;
    if (show && !this.pad && window.BlipDeck) {
      this.pad = createPad(this.ui.deck, (m) => this.session?.send(m), { port: 2 });
    } else if (!show && this.pad) {
      this.pad.destroy();
      this.pad = null;
    }
  }

  /** The C64 full screen (Esc, or the button again, comes back). */
  async fullscreen() {
    const s = this.ui.screen;
    try {
      if (document.fullscreenElement) await document.exitFullscreen();
      else await s.requestFullscreen();
    } catch (e) {
      toast(`No full screen here: ${e.message}`);
    }
    this.session?.frame?.focus();
  }

  /** The code a step bigger (+1) or smaller (-1). */
  codeSize(d) {
    const size = Math.max(10, Math.min(24, prefs.get('codeSize', 13.5) + d));
    prefs.set('codeSize', size);
    this.root.style.setProperty('--studio-code-size', `${size}px`);
    this.code.view.requestMeasure();
  }

  toggleFiles() {
    const hide = !prefs.get('hideFiles', false);
    prefs.set('hideFiles', hide);
    this.ui.code.classList.toggle('nofiles', hide);
    this.code.view.requestMeasure();
  }

  bindKeys() {
    // on the document, so the keys work wherever the focus is while the tab shows
    document.addEventListener('keydown', (e) => {
      if (this.root.hidden || document.querySelector('dialog[open]')) return;
      if (e.altKey && /^[1-5]$/.test(e.key)) {
        e.preventDefault();
        this.setMode(MODES[Number(e.key) - 1].id);
      } else if ((e.ctrlKey || e.metaKey) && (e.key === 'Enter' || e.key === 'r')) {
        e.preventDefault();
        this.run();
      } else if ((e.ctrlKey || e.metaKey) && e.key === 's') {
        e.preventDefault();
        this.saveNow(true);
      } else if (e.key === 'F1') {
        e.preventDefault();
        this.openHelp();
      } else if (e.altKey && e.key === 'Enter') {
        e.preventDefault();
        this.fullscreen();
      } else if ((e.ctrlKey || e.metaKey) && (e.key === '=' || e.key === '+')) {
        e.preventDefault();
        this.codeSize(+1);
      } else if ((e.ctrlKey || e.metaKey) && e.key === '-') {
        e.preventDefault();
        this.codeSize(-1);
      } else if (e.key === 'F6' || (e.key === 'F5' && this.dbg.stopped)) {
        // F6 pauses or runs on; F5 runs on (only while stopped: elsewhere it is the browser's reload)
        e.preventDefault();
        this.debugToggle();
      } else if (e.key === 'F10' || e.key === 'F11') {
        e.preventDefault();
        this.debugKey(e.key === 'F10' ? 'over' : e.shiftKey ? 'out' : 'into');
      } else if ((e.ctrlKey || e.metaKey) && e.key === 'b' && this.mode === 'code') {
        e.preventDefault();
        this.toggleFiles();
      }
    });
  }

  async loadApi() {
    try {
      this.api = parseApi(await resourceText('runtime/bb.h'));
      this.code.setNames({ api: this.api });
    } catch {}
  }

  setMode(mode) {
    if (this.meta?.lang !== 'c' && mode !== 'code') {
      toast('Assembly projects have code only: the editors are for C projects.');
      return;
    }
    this.mode = mode;
    for (const b of this.ui.modes.children) b.setAttribute('aria-selected', String(b.dataset.mode === mode));
    for (const p of this.root.querySelectorAll('.studio-pane')) p.hidden = p.dataset.mode !== mode;
    this.root.dataset.mode = mode;
    if (mode === 'code') {
      this.code.view.requestMeasure();
      this.showCursor();
    } else {
      const pane = this.root.querySelector(`.studio-pane[data-mode=${mode}]`);
      const missing = !hasAssets(this.files);
      // a plain cc65 project has no assets: offer to start them
      pane.querySelector('.studio-noassets')?.remove();
      this.editors[mode].el.hidden = missing;
      if (missing) {
        pane.append(h('div.studio-noassets', {},
          h('b', {}, 'This project has no sprites, tiles, rooms or music yet'),
          h('p', {}, 'It builds as plain cc65. Starting the assets adds the files the editors work on (assets/*.json) and, with them, the Studio\'s runtime: #include "bb.h" and "assets.h" to use them. The program then has RAM up to $C000.'),
          h('button.key.pill.primary', { onclick: () => this.startAssets() }, '✦ Start the assets')));
      } else {
        this.editors[mode].load(this.asset(mode));
        this.editors[mode].show?.();
      }
      this.ui.status.querySelector('.studio-pos').textContent = ASSET_MODES[mode];
    }
    prefs.set('mode', mode);
  }

  /** Give a plain cc65 project the asset files: a font, a sprite, a room, a few instruments. */
  async startAssets() {
    const map = { legend: {}, rooms: [] };
    map.rooms.push(blankRoom(map, 'start'));
    const assets = {
      'assets/sprites.json': { mc1: 0, mc2: 1, sprites: [blankSprite('hero')] },
      'assets/tiles.json': newTileset(),
      'assets/map.json': map,
      'assets/music.json': newMusic(),
    };
    const files = Object.fromEntries(Object.entries(assets).filter(([p]) => !(p in this.files)).map(([p, v]) => [p, `${pretty(v)}\n`]));
    Object.assign(this.files, files);
    this.assetCache = {};
    this.queueSave(files);
    this.renderFiles();
    this.setMode(this.mode);
    toast('The assets are started: #include "bb.h" and "assets.h" to use them.');
  }

  // ── projects ──────────────────────────────────────────────────────────────

  /** Reopen the project of the last visit, or the first example. */
  async restore() {
    const last = prefs.get('current', '');
    try {
      if (last.startsWith('folder:')) {
        const folder = await FolderStore.recall(last);
        if (folder) {
          if (await folder.allow(false)) return await this.openStore(folder, last);
          // reading a folder again needs a click: offer one
          await this.openExample(EXAMPLES[0].id, { quiet: true });
          return this.showBanner(`Your project folder ${folder.label} is waiting: the browser asks once more before the Studio reads it again.`, [
            { label: `📂 Reopen ${folder.label}`, onclick: async () => (await folder.allow(true)) && (await this.openStore(folder, last)) },
          ]);
        }
      } else if (last.startsWith('browser:')) {
        const s = new BrowserStore(last.slice(8));
        if (await s.read()) return await this.openStore(s, last);
      }
    } catch (e) {
      console.warn('studio: could not reopen the last project', e);
    }
    await this.openExample(EXAMPLES[0].id, { quiet: true });
  }

  /** Open the project in `s` (a BrowserStore or FolderStore) under `key`. */
  async openStore(s, key) {
    await this.saveNow();
    const files = await s.read();
    if (!files || !Object.keys(files).length) throw new Error('there is no project there');
    this.store = s;
    this.key = key;
    prefs.set('current', key);
    this.load(files);
    this.hideBanner();
  }

  /** Show `files` as the current project. */
  load(files) {
    this.files = files;
    this.pending = {};
    this.assetCache = {};
    this.meta = meta(files, { name: this.store?.kind === 'folder' ? this.store.handle.name : 'untitled' });
    this.diagnostics = [];
    this.breaks = new Map(Object.entries(prefs.get(`breaks.${this.key}`, {})).map(([f, l]) => [f, new Set(l)]));
    this.debugInfo = null;
    this.lineInfo = null;
    this.showBreakCount();
    // the last build and its variables were the old project's
    this.lastBuild = null;
    this.ui.share.disabled = true;
    this.vars = [];
    this.ui.vars.replaceChildren();
    this.code.states.clear();
    this.ui.project.querySelector('.studio-project-name').textContent = this.meta.name;
    this.root.dataset.lang = this.meta.lang;
    this.ui.modes.hidden = this.meta.lang !== 'c';
    this.showSaved();
    this.renderFiles();
    const first = prefs.get(`open.${this.key}`, '') in files ? prefs.get(`open.${this.key}`) : this.meta.entry in files ? this.meta.entry : sortPaths(Object.keys(files).filter(isText))[0];
    this.openFile(first, { mode: false });
    this.code.setNames({ assets: parseAssetNames(files['gen/assets.h']) });
    this.setMode(this.meta.lang === 'c' ? prefs.get('mode', 'code') : 'code');
    this.renderProblems();
    this.ui.meter.replaceChildren();
    this.ui.buildInfo.textContent = '';
  }

  /** Open example `id`: its own copy in this browser, made the first time. */
  async openExample(id, { quiet = false, fresh = false } = {}) {
    const ex = EXAMPLES.find((e) => e.id === id);
    const s = new BrowserStore(ex.id);
    let files = fresh ? null : await s.read().catch(() => null);
    if (!files) {
      if (!quiet) toast(`Fetching ${ex.title}…`);
      files = await exampleFiles(ex);
      await s.write(files);
    }
    await this.openStore(s, `browser:${ex.id}`);
  }

  /** A new project from template `id`, in this browser or (folder) a folder. */
  async newProject(id, { folder = null } = {}) {
    const ex = EXAMPLES.find((e) => e.id === id);
    const files = await exampleFiles(ex);
    if (folder) {
      await folder.write(files);
      const key = `folder:${folder.handle.name}:${Date.now()}`;
      await folder.remember(key);
      return this.openStore(folder, key);
    }
    const name = prompt('Name of the new project:', `My ${ex.title.replace(/^My /, '')}`);
    if (!name) return;
    const m = JSON.parse(files['project.json'] || '{}');
    files['project.json'] = `${pretty({ ...m, name })}\n`;
    const s = new BrowserStore(`${slug(name)}-${Date.now().toString(36)}`);
    await s.write(files);
    return this.openStore(s, `browser:${s.id}`);
  }

  /** Open a folder: its project, or (empty) a new project put there. */
  async openFolder() {
    if (!canUseFolders()) return toast('This browser can\'t open folders on disk: use Import .zip, or Chrome or Edge.');
    const folder = await FolderStore.pick();
    if (!folder) return;
    const files = await folder.read();
    if (files['project.json'] || hasSources(files)) {
      // a Studio project, or any folder of C or assembly (a plain cc65 project)
      const key = `folder:${folder.handle.name}:${Date.now()}`;
      await folder.remember(key);
      await this.openStore(folder, key);
      toast(`Opened ${folder.label} from your disk. Saves go straight into it.`);
    } else if (await folderIsEmpty(folder)) {
      this.closePicker();
      this.pickTemplate(folder);
    } else {
      toast(`${folder.label} has no C or assembly in it: pick a project's folder, or an empty one for a new project.`);
    }
  }

  /** Put the current project into a folder on disk, and keep working there. */
  async saveToFolder() {
    if (!canUseFolders()) return this.exportZip();
    const folder = await FolderStore.pick();
    if (!folder) return;
    if (!(await folderIsEmpty(folder)) && !confirm(`${folder.label} isn't empty. Write the project's files into it anyway? Files with the same names are replaced.`)) return;
    await this.saveNow();
    await folder.write(this.files);
    const key = `folder:${folder.handle.name}:${Date.now()}`;
    await folder.remember(key);
    await this.openStore(folder, key);
    toast(`Saved to ${folder.label} on your disk. From now on every save goes there.`);
  }

  exportZip() {
    this.download(`${slug(this.meta.name)}.zip`, zip(this.files));
  }

  async importZip(file) {
    try {
      const files = normalise(await unzip(new Uint8Array(await file.arrayBuffer())), { unwrap: true });
      if (!Object.keys(files).length) throw new Error('the zip is empty');
      const m = meta(files);
      const s = new BrowserStore(`${slug(m.name)}-${Date.now().toString(36)}`);
      await s.write(files);
      await this.openStore(s, `browser:${s.id}`);
      toast(`Imported ${m.name}.`);
    } catch (e) {
      toast(`Couldn't import ${file.name}: ${e.message}`);
    }
  }

  download(name, bytes) {
    const a = h('a', { href: URL.createObjectURL(new Blob([bytes])), download: name });
    document.body.append(a);
    a.click();
    a.remove();
    setTimeout(() => URL.revokeObjectURL(a.href), 1000);
  }

  // ── files ─────────────────────────────────────────────────────────────────

  renderFiles() {
    const list = this.ui.files;
    list.replaceChildren();
    let group = null;
    for (const p of sortPaths(Object.keys(this.files))) {
      const dir = p.includes('/') ? p.slice(0, p.lastIndexOf('/') + 1) : '';
      if (dir !== group) {
        group = dir;
        if (dir) list.append(h('div.studio-dir', {}, dir));
      }
      const asset = Object.entries(ASSET_MODES).find(([, f]) => f === p)?.[0];
      list.append(h('button.studio-file', {
        'data-path': p,
        title: isGenerated(p) ? `${p}: written by every build` : p,
        className: `studio-file${dir ? ' nested' : ''}${isGenerated(p) ? ' generated' : ''}${p === this.path ? ' open' : ''}`,
        onclick: () => this.openFile(p),
      }, h('span.studio-file-icon', {}, icon(p)), p.slice(dir.length), asset ? h('span.studio-file-tag', {}, asset) : null,
      p in this.pending ? h('span.studio-dot', { title: 'not saved yet' }) : null));
    }
  }

  /** Open `path`: in CODE, or (for an asset with `mode`) in its editor. */
  openFile(path, { mode = true, line = 0 } = {}) {
    if (!path) return;
    if (!isText(path)) return toast(`${path} is a binary file (${this.files[path].length} bytes).`);
    this.path = path;
    prefs.set(`open.${this.key}`, path);
    this.code.open(path, this.files[path] ?? '', { readOnly: isGenerated(path) });
    this.code.setBreakpoints(this.breaks.get(path) || []);
    this.code.setExecLine(this.stopAt?.where?.file === path ? this.stopAt.where.line : 0);
    this.ui.codeHead.replaceChildren(...[h('span.studio-crumb', {}, path),
      isGenerated(path) ? h('span.studio-note', {}, 'written by the build: change the assets or the source instead') : null,
      ...Object.entries(ASSET_MODES).filter(([, f]) => f === path).map(([m]) => h('button.studio-tool', { onclick: () => this.setMode(m) }, `open in the ${m} editor →`)),
      h('span.studio-codetools', {},
        h('button.studio-tool', { onclick: () => this.codeSize(-1), title: 'Smaller code (Ctrl+-)' }, 'A−'),
        h('button.studio-tool', { onclick: () => this.codeSize(+1), title: 'Bigger code (Ctrl+=)' }, 'A+'),
        h('button.studio-tool', { onclick: () => this.toggleFiles(), title: 'Hide or show the file list (Ctrl+B)' }, '◧ files'))].filter(Boolean));
    if (mode && this.mode !== 'code') this.setMode('code');
    this.renderFiles();
    this.showDiagnostics();
    if (line) setTimeout(() => (this.code.goto(line), this.showCursor()), 0);
    this.showCursor();
    this.loadLibrary();
    this.ui.status.querySelector('.studio-lang').textContent = /\.(c|h)$/.test(path) ? 'C (cc65)' : /\.(s|inc)$/.test(path) ? 'ca65 assembly' : /\.(asm|a)$/.test(path) ? 'ACME assembly' : /\.json$/.test(path) ? 'JSON' : 'text';
  }

  /** F12 or Ctrl+click on `name`: open where it is defined. */
  goToDefinition(name) {
    const at = findDefinition(this.files, name, this.path);
    if (!at) return toast(`No definition of ${name} in the project${/^(SPR|TILE|FLAG|ROOM|SONG|SFX)_/.test(name) ? ' yet: Run once, and gen/assets.h has it' : ''}.`);
    this.openFile(at.file, { line: at.line });
  }

  showCursor() {
    const s = this.code.view.state;
    const pos = s.selection.main.head;
    const line = s.doc.lineAt(pos);
    this.ui.status.querySelector('.studio-pos').textContent = `${this.path}  ·  Ln ${line.number}, Col ${pos - line.from + 1}`;
  }

  /** cc65's headers the open C file includes (<stdlib.h>...): into completion and hover. */
  async loadLibrary() {
    const path = this.path;
    if (!/\.(c|h)$/.test(path)) return this.code.setNames({ library: [] });
    const names = systemIncludes(this.files[path] || '');
    this.headerCache ??= new Map();
    const missing = names.filter((n) => !this.headerCache.has(n));
    if (missing.length) {
      try {
        const got = await headers(missing);
        for (const n of missing) this.headerCache.set(n, got[n] ? parseHeader(got[n], n) : []);
      } catch {
        return;
      }
    }
    if (path === this.path) this.code.setNames({ library: names.flatMap((n) => this.headerCache.get(n) || []) });
  }

  /** A file's text changed in the CODE editor. */
  edited(path, text) {
    clearTimeout(this.libraryTimer);
    this.libraryTimer = setTimeout(() => this.loadLibrary(), 800);
    this.files[path] = text;
    if (path in ASSET_MODES || Object.values(ASSET_MODES).includes(path)) delete this.assetCache[path];
    this.queueSave({ [path]: text });
  }

  async newFile() {
    const dir = this.meta.lang === 'c' ? 'src/' : '';
    const name = prompt('New file (e.g. src/enemies.c, src/enemies.h):', `${dir}`);
    if (!name || name.endsWith('/')) return;
    if (name in this.files) return this.openFile(name);
    const text = /\.h$/.test(name) ? `#ifndef ${slug(name).toUpperCase().replace(/-/g, '_')}\n#define ${slug(name).toUpperCase().replace(/-/g, '_')}\n\n#endif\n` : '';
    this.files[name] = text;
    this.queueSave({ [name]: text });
    this.openFile(name);
  }

  async renameFile() {
    const from = this.path;
    if (!from || isGenerated(from) || from === 'project.json') return toast('This file can\'t be renamed here.');
    const to = prompt(`Rename ${from} to:`, from);
    if (!to || to === from) return;
    this.files[to] = this.files[from];
    delete this.files[from];
    this.code.forget(from);
    this.queueSave({ [to]: this.files[to], [from]: null });
    this.openFile(to);
  }

  async deleteFile() {
    const p = this.path;
    if (!p || p === 'project.json' || Object.values(ASSET_MODES).includes(p)) return toast('This file is part of every project.');
    if (!confirm(`Delete ${p}?`)) return;
    delete this.files[p];
    this.code.forget(p);
    this.queueSave({ [p]: null });
    this.openFile(this.meta.entry in this.files ? this.meta.entry : sortPaths(Object.keys(this.files))[0]);
  }

  // ── assets: what the editors read and write ───────────────────────────────

  /** The parsed asset of editor `mode` ('sprites', ...). */
  asset(mode) {
    const path = ASSET_MODES[mode];
    if (!this.assetCache[path]) {
      try {
        this.assetCache[path] = JSON.parse(this.files[path] || 'null');
      } catch (e) {
        toast(`${path} isn't valid JSON (${e.message}): fix it in CODE.`);
        this.assetCache[path] = null;
      }
    }
    return this.assetCache[path];
  }

  /** An editor changed its asset. */
  commit(mode, value) {
    const path = ASSET_MODES[mode];
    const text = `${pretty(value)}\n`;
    this.assetCache[path] = value;
    if (this.files[path] === text) return;
    this.files[path] = text;
    this.code.update(path, text);
    this.queueSave({ [path]: text });
  }

  // ── saving ────────────────────────────────────────────────────────────────

  queueSave(changed) {
    Object.assign(this.pending, changed);
    this.showSaved('unsaved');
    this.renderFilesSoon();
    clearTimeout(this.saveTimer);
    this.saveTimer = setTimeout(() => this.saveNow(), SAVE_DELAY);
  }

  renderFilesSoon() {
    cancelAnimationFrame(this.rf);
    this.rf = requestAnimationFrame(() => this.renderFiles());
  }

  /** Write what is pending. `explicit`: the user pressed Ctrl+S (then maybe run). */
  async saveNow(explicit = false) {
    clearTimeout(this.saveTimer);
    const changed = this.pending;
    this.pending = {};
    if (Object.keys(changed).length && this.store) {
      this.showSaved('saving');
      this.saving = this.saving.then(() => this.store.write(changed)).then(
        () => this.showSaved(),
        (e) => {
          Object.assign(this.pending, changed, this.pending);
          this.showSaved('failed');
          toast(`Couldn't save: ${e.message}`);
        },
      );
    }
    await this.saving;
    this.renderFilesSoon();
    if (explicit && this.runOnSave && this.session) this.run();
  }

  showSaved(state = 'saved') {
    const where = this.store?.kind === 'folder' ? `💾 ${this.store.label}` : '🍞 in this browser';
    const text = { saved: where, unsaved: `${where} · editing`, saving: `${where} · saving…`, failed: `${where} · not saved!` }[state];
    this.ui.saved.textContent = text;
    this.ui.saved.className = `studio-saved ${state} ${this.store?.kind || ''}`;
    this.ui.saved.title = this.store?.kind === 'folder'
      ? `Saved into the folder ${this.store.label} on your disk. Edits made there (in your own editor) show up here.`
      : 'Saved in this browser. Projects menu → Save to a folder… puts it on your disk.';
  }

  /** A folder on disk changed outside the Studio: take the changes in. */
  async poll() {
    if (this.store?.kind !== 'folder' || document.hidden || this.polling) return;
    this.polling = true;
    try {
      if (!(await this.store.allow(false))) return;
      const changed = await this.store.changes();
      const paths = Object.keys(changed).filter((p) => !(p in this.pending));
      if (!paths.length) return;
      for (const p of paths) {
        if (changed[p] == null) delete this.files[p];
        else this.files[p] = changed[p];
        delete this.assetCache[p];
        this.code.update(p, changed[p] ?? '');
      }
      if (paths.includes('project.json')) this.meta = meta(this.files);
      this.renderFiles();
      if (this.mode !== 'code' && paths.includes(ASSET_MODES[this.mode])) this.editors[this.mode].load(this.asset(this.mode));
      toast(`${paths.length === 1 ? paths[0] : `${paths.length} files`} changed on disk`);
      if (this.runOnSave && this.session) this.run();
    } catch (e) {
      console.warn('studio: poll', e);
    } finally {
      this.polling = false;
    }
  }

  // ── build and run ─────────────────────────────────────────────────────────

  async build() {
    await this.saveNow();
    this.ui.run.disabled = true;
    this.ui.buildInfo.className = 'studio-buildinfo busy';
    this.ui.buildInfo.textContent = 'Building…';
    try {
      const r = await build(this.files);
      this.lastBuild = r;
      // which lines made code: where a breakpoint can go (the running program keeps its own, debugInfo)
      if (r.ok) this.lineInfo = new DebugInfo(r.debug, { work: '/p' });
      this.diagnostics = r.diagnostics;
      if (r.ok) {
        this.vars = globals(r.labels, r.memory, r.sizes, { hidden: r.hidden, types: cTypes(this.files) });
        this.ui.vars.replaceChildren();
      }
      // gen/ goes into the project, so the folder builds with a native cc65 too
      const gen = Object.fromEntries(Object.entries(r.generated || {}).filter(([p, t]) => this.files[p] !== t));
      if (Object.keys(gen).length) {
        Object.assign(this.files, gen);
        for (const [p, t] of Object.entries(gen)) this.code.update(p, t);
        this.queueSave(gen);
        this.code.setNames({ assets: parseAssetNames(this.files['gen/assets.h']) });
      }
      // in a folder on disk, the program goes next to the sources: run it in VICE, copy it to a real C64
      if (r.ok && this.store?.kind === 'folder') this.store.write({ 'game.prg': r.prg }).catch(() => {});
      this.ui.share.disabled = !r.ok;
      this.ui.buildInfo.className = `studio-buildinfo ${r.ok ? 'ok' : 'bad'}`;
      this.ui.buildInfo.textContent = r.ok ? `Built in ${r.ms} ms · ${summary(r.diagnostics)}` : `Didn't build: ${summary(r.diagnostics)}`;
      this.renderMeter(r.memory);
      this.renderProblems();
      this.showDiagnostics();
      return r;
    } catch (e) {
      this.ui.buildInfo.className = 'studio-buildinfo bad';
      this.ui.buildInfo.textContent = `The build failed: ${e.message}`;
      return { ok: false };
    } finally {
      this.ui.run.disabled = false;
    }
  }

  async run() {
    const r = await this.build();
    if (!r.ok) {
      const first = this.diagnostics.find((d) => d.severity === 'error' && d.file in this.files);
      if (first) this.openFile(first.file, { line: first.line });
      return;
    }
    this.sid.stop();
    this.debugInfo = new DebugInfo(r.debug, { work: '/p' });
    this.stopAt = null;
    this.code.setExecLine(0);
    if (this.session?.started) {
      this.session.reload(r.prg);
      this.dbg.attach(this.session, this.debugInfo);
      this.dbg.setBreakpoints(this.breakAddrs());
    } else {
      this.session?.stop();
      const prg = await prepare('game.prg', r.prg, { canMount: false });
      this.session = emu.run(this.ui.stage, prg, {
        border: 0.1,
        kernal: await patchedKernal(store.cachedBytes),
        basic: await patchedBasic(store.cachedBytes),
        // the Studio's keys still work while the C64 has the keyboard
        onHotkey: (key) => {
          if (key === 'run') this.run();
          else if (key === 'save') this.saveNow(true);
          else if (key === 'help') this.openHelp();
          else if (key === 'fullscreen') this.fullscreen();
          else if (key === 'pause') this.debugToggle();
          else if (key === 'over' || key === 'into' || key === 'out') this.debugKey(key);
          else if (key === 'escape') this.mode === 'code' ? this.code.focus() : this.editors[this.mode].el.focus();
        },
      });
    }
    this.ui.stop.disabled = false;
    this.setPad(prefs.get('pad', TOUCH));
    this.ui.debug.hidden = false;
    if (!this.dbg.session || this.dbg.session !== this.session) this.debugFresh(this.session, r.prg);
    if (matchMedia('(max-width: 900px)').matches) this.ui.stage.scrollIntoView({ behavior: 'smooth', block: 'center' });
  }

  // ── the debugger: breakpoints, pause, steps (debugger.js does the work) ───

  /**
   * A new C64 (the first Run): once its program is in, and if there are
   * breakpoints, start the program again with them set, so that they catch
   * its start-up code too.
   */
  async debugFresh(session, prg) {
    for (let i = 0; i < 300 && !session.started; i++) await new Promise((r) => setTimeout(r, 100));
    if (this.session !== session || !session.started) return;
    this.dbg.attach(session, this.debugInfo);
    if (this.breakAddrs().length) {
      session.reload(prg);
      this.dbg.setBreakpoints(this.breakAddrs());
    }
    this.debugRunning();
  }

  /** The breakpoints' addresses in the last build (a line without code: the next line that has some). */
  breakAddrs() {
    const info = this.debugInfo;
    if (!info) return [];
    const out = [];
    for (const [file, lines] of this.breaks) {
      for (const l of lines) {
        const at = info.snap(file, l);
        if (at) out.push(...info.addrs(file, at));
      }
    }
    return out;
  }

  /** A breakpoint on (or off) `line` of `path`: on the line itself, or the next one with code. */
  toggleBreakpoint(path, line) {
    if (!path || !/\.(c|h|s|asm)$/i.test(path)) return;
    const set = this.breaks.get(path) || new Set();
    if (set.has(line)) set.delete(line);
    else {
      const info = this.lineInfo || this.debugInfo;
      const at = info?.files.has(path) ? info.snap(path, line) : line;
      if (at == null) return toast(`Line ${line} made no code: a breakpoint needs a line that does something.`);
      if (set.has(at)) set.delete(at);
      else set.add(at);
    }
    if (set.size) this.breaks.set(path, set);
    else this.breaks.delete(path);
    this.saveBreakpoints();
  }

  clearBreakpoints() {
    this.breaks.clear();
    this.saveBreakpoints();
  }

  saveBreakpoints({ send = true } = {}) {
    prefs.set(`breaks.${this.key}`, Object.fromEntries([...this.breaks].map(([f, l]) => [f, [...l].sort((a, b) => a - b)])));
    if (send) this.code.setBreakpoints(this.breaks.get(this.path) || []);
    this.showBreakCount();
    if (send && this.session && this.dbg.session === this.session) this.dbg.setBreakpoints(this.breakAddrs());
  }

  showBreakCount() {
    const n = [...this.breaks.values()].reduce((a, s) => a + s.size, 0);
    this.ui.dbgClear.textContent = n ? `${n} breakpoint${n > 1 ? 's' : ''} ✕` : '';
  }

  /** F6, ⏸/▶: pause the game, or run it on. */
  debugToggle() {
    if (!this.session || !this.debugInfo) return;
    if (this.dbg.stopped) this.dbg.cont();
    else {
      this.ui.dbgWhere.textContent = 'Pausing…';
      this.dbg.pause();
    }
  }

  /** F10, F11, Shift+F11. */
  debugKey(step) {
    if (!this.session || !this.debugInfo) return;
    if (!this.dbg.stopped) return this.debugToggle();
    this.ui.dbgWhere.textContent = 'Stepping…';
    this.dbg[step]();
  }

  /** The CPU stopped: show the line, its registers, the variables as they are. */
  debugStopped(s) {
    this.stopAt = s;
    const u = this.ui;
    u.debug.classList.add('stopped');
    u.dbgPause.textContent = '▶';
    u.dbgPause.title = 'Run on (F6 or F5)';
    for (const b of [u.dbgOver, u.dbgInto, u.dbgOut]) b.disabled = false;
    const where = s.where ? `${s.where.file}:${s.where.line}` : `$${s.pc.toString(16).toUpperCase().padStart(4, '0')}`;
    u.dbgWhere.replaceChildren(h('b', {}, s.func ? `${s.func}()` : 'stopped'), ` ${where}`);
    u.dbgRegs.textContent = formatRegs(s.pc, s.regs);
    this.showStop();
    this.watch();
  }

  /** Open the file and line the CPU stopped on. */
  showStop() {
    const w = this.stopAt?.where;
    if (!w || !(w.file in this.files)) return;
    if (this.mode !== 'code') this.setMode('code');
    if (this.path !== w.file) this.openFile(w.file, { mode: false });
    else this.code.setExecLine(w.line);
  }

  debugRunning() {
    this.stopAt = null;
    this.code.setExecLine(0);
    const u = this.ui;
    u.debug.classList.remove('stopped');
    u.dbgPause.textContent = '⏸';
    u.dbgPause.title = 'Pause at the next line of your code (F6)';
    for (const b of [u.dbgOver, u.dbgInto, u.dbgOut]) b.disabled = true;
    u.dbgWhere.replaceChildren(h('span.studio-small', {}, 'Running · click the margin left of a line number for a breakpoint (F9)'));
    u.dbgRegs.textContent = '';
    this.showBreakCount();
  }

  /** Read the running game's globals into the Variables panel. */
  async watch() {
    const box = this.ui.varsBox;
    if (!box.open || this.root.hidden || document.hidden || !this.session?.started || this.watching) return;
    const vars = this.vars || [];
    if (!vars.length) {
      if (!this.ui.vars.childElementCount) this.ui.vars.append(h('p.studio-small', {}, 'Run a C game to see its global variables here.'));
      return;
    }
    this.watching = true;
    try {
      const values = await this.session.peek(ranges(vars));
      if (!values) return;
      if (this.ui.vars.childElementCount !== vars.length) {
        this.ui.vars.replaceChildren(...vars.map((v) => h(v.size > 2 ? 'div.studio-var.wide' : 'div.studio-var', { title: `$${v.addr.toString(16).toUpperCase()}, ${v.size} byte${v.size > 1 ? 's' : ''}` },
          h('span.studio-var-name', {}, v.name), h('span.studio-var-value'))));
      }
      vars.forEach((v, i) => {
        const el = this.ui.vars.children[i]?.lastChild;
        const text = format(values[i], v.size, v.unit);
        if (!el || el.textContent === text) return;
        el.textContent = text;
        // a short flash on the value that changed
        el.classList.remove('changed');
        requestAnimationFrame(() => el.classList.add('changed'));
      });
    } finally {
      this.watching = false;
    }
  }

  stop() {
    this.dbg.detach();
    this.stopAt = null;
    this.code.setExecLine(0);
    this.ui.debug.hidden = true;
    this.session?.stop();
    this.session = null;
    this.setPad(prefs.get('pad', TOUCH));
    this.ui.stop.disabled = true;
    this.ui.stage.replaceChildren(h('div.studio-idle', {}, h('b', {}, 'Press ▶ Run'), h('span', {}, 'to build the game and start it here')));
  }

  /**
   * Share the game as a link: the PRG, gzipped, is in the link itself
   * (../cart.js), and opening it starts the game in breadbin's player.
   */
  async shareGame() {
    const r = this.lastBuild?.ok ? this.lastBuild : await this.build();
    if (!r.ok) return toast('Fix the build first: a link carries the built program.');
    const id = await packCart(r.prg, this.meta.name);
    await share('games', id, this.meta.name);
  }

  /** Download the last build as a PRG. */
  async downloadPrg() {
    const r = this.lastBuild?.ok ? this.lastBuild : await this.build();
    if (r.ok) this.download(`${slug(this.meta.name)}.prg`, r.prg);
  }

  renderMeter(mem) {
    const m = this.ui.meter;
    m.replaceChildren();
    if (!mem) return;
    const kb = (n) => `${(n / 1024).toFixed(1)} KB`;
    const pct = Math.min(100, (mem.used / mem.total) * 100);
    m.append(h('div.studio-meter-bar', {}, h('i', { style: { width: `${pct}%` }, className: pct > 90 ? 'full' : '' })),
      h('div.studio-meter-text', {}, h('b', {}, kb(mem.used)), ` of ${kb(mem.total)} used`, mem.segments.length ? h('span', {}, ` · code ${kb(seg(mem, 'CODE'))}, data ${kb(seg(mem, 'RODATA') + seg(mem, 'DATA'))}, vars ${kb(seg(mem, 'BSS'))}`) : null));
  }

  renderProblems() {
    const p = this.ui.problems;
    p.replaceChildren();
    for (const d of this.diagnostics) {
      p.append(h(`button.studio-problem.${d.severity}`, {
        onclick: () => (d.file in this.files ? this.openFile(d.file, { line: d.line }) : null),
        title: d.file ? `${d.file}:${d.line}` : d.tool,
      }, h('span.studio-problem-where', {}, d.file ? `${d.file.split('/').pop()}:${d.line}` : d.tool), h('span', {}, d.message)));
    }
  }

  showDiagnostics() {
    this.code.setDiagnostics(this.diagnostics.filter((d) => d.file === this.path));
  }

  // ── the sound of the MUSIC editor ─────────────────────────────────────────

  /** Play `music` (or part of it, see psid.js previewMusic) through the SID engine. */
  async playMusic(music) {
    const bytes = await psid(music, { name: this.meta.name });
    await this.sid.play(bytes, 1, { hq: true });
  }

  /** The project's music as a PSID file, every song a sub-tune, for any SID player. */
  async downloadSid() {
    const music = this.asset('music');
    if (!music?.songs?.length) return toast('Make a song first: a .sid file plays songs.');
    try {
      const bytes = await psid(music, { name: this.meta.name, author: 'made with breadbin', released: String(new Date().getFullYear()) });
      this.download(`${slug(this.meta.name)}.sid`, bytes);
    } catch (e) {
      toast(`No .sid: ${e.message}`);
    }
  }

  // ── the projects menu ─────────────────────────────────────────────────────

  async openPicker() {
    const dlg = h('dialog.sheet.studio-picker');
    const close = () => dlg.close();
    const card = (ex, onclick) => h('button.studio-card', { onclick }, h('span.studio-card-art', { style: { background: ex.color } }, ex.icon),
      h('span.studio-card-body', {}, h('b', {}, ex.title), h('span', {}, ex.about), h('span.studio-card-lang', {}, ex.lang === 'c' ? 'C · cc65' : 'ACME assembly')));
    const own = (await browserProjects().catch(() => [])).filter((id) => !EXAMPLES.some((e) => e.id === id));
    const ownCards = await Promise.all(own.map(async (id) => {
      const files = await new BrowserStore(id).read();
      const m = meta(files || {});
      return h('div.studio-own', {}, h('button.studio-own-open', { onclick: async () => (close(), await this.openStore(new BrowserStore(id), `browser:${id}`)) }, h('b', {}, m.name), h('span', {}, m.lang === 'c' ? 'C' : 'asm')),
        h('button.studio-tool', { title: `Delete ${m.name} from this browser`, onclick: async (e) => {
          if (!confirm(`Delete ${m.name} from this browser? (A copy on your disk is not touched.)`)) return;
          await new BrowserStore(id).remove();
          e.target.closest('.studio-own').remove();
        } }, '×'));
    }));
    const fileIn = h('input', { type: 'file', accept: '.zip,application/zip', hidden: true, onchange: (e) => e.target.files[0] && (close(), this.importZip(e.target.files[0])) });
    const folders = canUseFolders();
    // the folders worked in before (the open one aside)
    const folderCards = (folders ? await rememberedFolders() : []).filter((f) => f.key !== this.key).map((f) => h('div.studio-own', {},
      h('button.studio-own-open', {
        title: `Open the project in ${f.handle.name}/ on your disk`,
        onclick: async () => {
          const store = new FolderStore(f.handle);
          close();
          try {
            if (await store.allow(true)) await this.openStore(store, f.key);
          } catch (e) {
            toast(`Couldn't open ${f.handle.name}/: ${e.message}`);
          }
        },
      }, h('b', {}, `📁 ${f.handle.name}/`), h('span', {}, 'on disk')),
      h('button.studio-tool', { title: `Forget ${f.handle.name}/ (the folder itself is not touched)`, onclick: async (e) => {
        await FolderStore.forget(f.key);
        e.target.closest('.studio-own').remove();
      } }, '×')));
    dlg.append(
      h('header.studio-picker-head', {}, h('h2', {}, 'Projects'), h('button.key.icon.small.close', { onclick: close, 'aria-label': 'Close' }, '×')),
      h('section.studio-picker-now', {},
        h('div', {}, h('span.studio-label', {}, 'Open now'), h('b', {}, this.meta.name), h('span.studio-where', {}, this.store?.kind === 'folder' ? ` in ${this.store.label} on your disk` : ' in this browser')),
        h('div.studio-picker-actions', {},
          folders ? h('button.key.pill.primary', { onclick: () => (close(), this.saveToFolder()), title: 'Write the project into a folder on your disk and keep working there' }, '💾 Save to a folder…') : null,
          h('button.key.pill', { onclick: () => this.exportZip() }, '⇩ Export .zip'),
          h('button.key.pill', { onclick: () => this.downloadPrg() }, '⇩ Download .prg'),
          h('button.key.pill', { onclick: () => (close(), this.shareGame()), title: 'A link that plays this game: the whole program is in it' }, '🔗 Share the game'),
          this.key.startsWith('browser:') && EXAMPLES.some((e) => `browser:${e.id}` === this.key)
            ? h('button.key.pill', { onclick: async () => confirm('Throw away your changes to this example?') && (close(), await this.openExample(this.key.slice(8), { fresh: true })) }, '↺ Reset example') : null)),
      h('section', {}, h('span.studio-label', {}, 'Open'),
        h('div.studio-picker-actions', {},
          h('button.key.pill', { onclick: () => (close(), this.openFolder()), disabled: !folders, title: folders ? 'A project folder on your disk, or an empty folder for a new one' : 'Needs Chrome, Edge or Opera' }, '📂 Open a folder…'),
          h('button.key.pill', { onclick: () => fileIn.click() }, '⇧ Import .zip…'), fileIn),
        folders ? null : h('p.studio-small', {}, 'This browser can\'t reach folders on your disk: Export and Import .zip carry a project in and out. In Chrome, Edge or Opera, the Studio saves straight into a folder.'),
        folderCards.length || ownCards.length ? h('div.studio-owns', {}, ...folderCards, ...ownCards) : null),
      h('section', {}, h('span.studio-label', {}, 'Examples · each opens its own copy'), h('div.studio-cards', {}, ...EXAMPLES.map((ex) => card(ex, async () => (close(), await this.openExample(ex.id)))))),
      h('section', {}, h('span.studio-label', {}, 'New project from…'), h('div.studio-picker-actions', {},
        ...EXAMPLES.filter((e) => e.template).map((ex) => h('button.key.pill', { onclick: () => (close(), this.newProject(ex.id)) }, `${ex.icon} ${ex.title}`)))),
    );
    document.body.append(dlg);
    dlg.addEventListener('close', () => dlg.remove());
    dlg.addEventListener('click', (e) => e.target === dlg && close());
    dlg.showModal();
    this.picker = dlg;
  }

  closePicker() {
    this.picker?.close();
  }

  /** An empty folder was picked: which template goes in it? */
  pickTemplate(folder) {
    const dlg = h('dialog.sheet.studio-picker');
    dlg.append(h('header.studio-picker-head', {}, h('h2', {}, `A new project in ${folder.label}`)),
      h('p', {}, 'Start from:'),
      h('div.studio-cards', {}, ...EXAMPLES.filter((e) => e.lang === 'c').map((ex) => h('button.studio-card', { onclick: async () => (dlg.close(), await this.newProject(ex.id, { folder })) },
        h('span.studio-card-art', { style: { background: ex.color } }, ex.icon), h('span.studio-card-body', {}, h('b', {}, ex.title), h('span', {}, ex.about))))));
    document.body.append(dlg);
    dlg.addEventListener('close', () => dlg.remove());
    dlg.showModal();
  }

  openHelp() {
    const dlg = helpSheet(this.api || []);
    document.body.append(dlg);
    dlg.addEventListener('close', () => dlg.remove());
    dlg.showModal();
  }

  /** A line across the top: `text`, then buttons for `actions` ([{ label, onclick }], the first one primary). */
  showBanner(text, actions, { dismiss = 'not now', onDismiss } = {}) {
    const b = this.ui.banner;
    b.replaceChildren(h('span.studio-banner-text', {}, text),
      ...actions.map((a, i) => h(`button.key.pill${i ? '' : '.primary'}`, { onclick: () => (this.hideBanner(), a.onclick()) }, a.label)),
      h('button.studio-tool', { onclick: () => (this.hideBanner(), onDismiss?.()) }, dismiss));
    b.hidden = false;
  }

  /** The first visit: what to try. */
  welcome() {
    if (prefs.get('welcomed', false)) return;
    const done = () => prefs.set('welcomed', true);
    this.showBanner('Welcome to the Studio! ▶ Run plays Bubbler on the C64 on the right. Change its code, sprites, rooms or music and run it again.',
      [
        { label: '▶ Run Bubbler', onclick: () => (done(), this.run()) },
        ...(canUseFolders() ? [{ label: '💾 Work in a folder on my disk…', onclick: () => (done(), this.saveToFolder()) }] : []),
        { label: '? The cheat sheet', onclick: () => (done(), this.openHelp()) },
      ], { dismiss: 'got it', onDismiss: done });
  }

  hideBanner() {
    this.ui.banner.hidden = true;
  }
}

const seg = (mem, name) => mem.segments.filter((s) => s.name === name).reduce((n, s) => n + s.size, 0);

function icon(path) {
  if (/\.c$/.test(path)) return 'C';
  if (/\.h$/.test(path)) return 'H';
  if (/\.(s|asm|a|inc)$/.test(path)) return '◇';
  if (/sprites\.json$/.test(path)) return '☺';
  if (/tiles\.json$/.test(path)) return '▦';
  if (/map\.json$/.test(path)) return '▤';
  if (/music\.json$/.test(path)) return '♫';
  if (/\.json$/.test(path)) return '{}';
  if (/Makefile$/.test(path)) return '⚙';
  if (/\.md$/.test(path)) return '¶';
  return '·';
}

/** Start the Studio in the Hack tab's section. */
export async function initStudio(root) {
  const s = new Studio(root);
  window.breadbinStudio = s; // for the browser tests (tools/studio/test/browser) and the curious
  await s.start();
  return s;
}

