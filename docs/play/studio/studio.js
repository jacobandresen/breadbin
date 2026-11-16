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
import { h, toast, share, showDialog } from '../ui.js';
import { packCart } from '../cart.js';
import { webPage } from './export.js';
import { createPad } from '../touch.js';
import { pollGamepads } from '../gamepad.js';
import { SidPlayer } from '../sid.js';
import { CodeEditor } from './code-editor.js';
import { build, warmUp, psid, headers } from './builder.js';
import { BrowserStore, FolderStore, canUseFolders, browserProjects, folderIsEmpty, rememberedFolders, projectHistory, keepVersion } from './storage.js';
import { meta, normalise, sortPaths, isText, isGenerated, slug, pretty, hasAssets, hasSources } from './project.js';
import { zip, unzip } from './zip.js';
import { summary, explain } from './diagnostics.js';
import { parseApi, parseAssetNames, parseHeader, systemIncludes } from './api.js';
import { resourceText } from './resource.js';
import { EXAMPLES, exampleFiles } from './examples.js';
import { SpriteEditor } from './sprite-editor.js';
import { TileEditor } from './tile-editor.js';
import { MapEditor } from './map-editor.js';
import { MusicEditor } from './music-editor.js';
import { helpSheet } from './help.js';
import { globals, format, ranges, cDecls, number, range, bytesOf, brief } from './variables.js';
import { blankSprite } from './assets/sprites.js';
import { newTileset } from './assets/tiles.js';
import { blankRoom } from './assets/map.js';
import { newMusic } from './assets/music.js';
import { findDefinition } from './definitions.js';
import { DebugInfo } from './debuginfo.js';
import { Graphs, sampler } from './graphs.js';
import { TuningPanel, tunables, setKnob } from './tuning.js';
import { HitboxOverlay } from './hitbox.js';
import { assetBytes } from './codegen.js';
import { GameTools } from '../tools.js';
import { LESSONS, LessonCard } from './lessons.js';
import { keyBar } from './keybar.js';
import { hasTape, tapeRanges, readTape, tapeFrames, cut, playWrites, patchPrg, usedAfter, position, PLAY } from './tape.js';
import { Debugger, formatRegs } from './debugger.js';
import { Console } from './console.js';

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
/** Narrow enough that the code and the C64 take turns on the screen (studio.css's .studio[data-turns]). */
const NARROW = matchMedia('(max-width: 900px)');
const POLL = 1500;
const SPEEDS = [100, 50, 25]; // slow motion's steps, in percent

// The marked moment, in the emulator's page: VirtualC64's snapshot of the
// whole machine (vc64web gives its place in the wasm heap; a copy is kept),
// and loading it back as a file, which restores it.
const SNAP = `
window.bbsnapMark = (frames) => {
  const j = JSON.parse(wasm_take_user_snapshot());
  window.bbsnapBytes = Module.HEAPU8.slice(j.address, j.address + j.size);
  // the tape's frame count at the same moment (a Studio runtime game), for Keep my place and Replay
  return { frames: frames ? wasm_peek(frames) | (wasm_peek(frames + 1) << 8) : null };
};
window.bbsnapAgain = async ({ writes, tape }) => {
  const b = window.bbsnapBytes;
  if (!b) return false;
  // The emulator takes the snapshot in on one of its next turns. A game with
  // a tape says so by its frame count going back (to the mark's); any other
  // by the CPU's cycle count going back. Writes made before that would be
  // overwritten by the snapshot's memory.
  const frames = () => (tape ? wasm_peek(tape.frames) | (wasm_peek(tape.frames + 1) << 8) : 0);
  const cycles = () => Number(wasm_get_cpu_cycles());
  const f0 = frames();
  const c0 = cycles();
  wasm_loadfile('mark.vc64', b, b.length);
  for (let i = 0; i < 200 && (tape ? frames() >= f0 : cycles() >= c0); i++) await new Promise((r) => setTimeout(r, 5));
  for (const [a, bytes] of writes) bytes.forEach((v, i) => wasm_poke(a + i, v));
  if (tape && tape.bytes) {
    // Replay: the take played from the frame the game is at now (the mark's, or a frame or two on)
    const position = ${position}; // (tape.js's, as source)
    const { pos, left } = position(tape.bytes, frames());
    const put = (a, bytes) => bytes.forEach((v, i) => wasm_poke(a + i, v));
    put(tape.tape, tape.bytes);
    put(tape.len, [tape.bytes.length & 255, tape.bytes.length >> 8]);
    put(tape.pos, [pos & 255, pos >> 8]);
    put(tape.left, [left]);
    put(tape.mode, [2]);
  }
  return true;
};`;

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
    this.speed = 100;
    this.hitbox = new HitboxOverlay();
    // the player's 🔧 Tools (the monitor, the cheat finder) under the C64 here too
    this.tools = new GameTools();
    this.lessonCard = new LessonCard({
      onShow: (st) => this.applyFiles(st.show(this.files), st),
      onStep: (st) => this.lessonStep(st),
      onExit: () => {},
    });
    this.markFrame = null; // the tape's frame at the marked moment (null: no mark, or no tape)
    this.lastBuild = null;
    this.sid = new SidPlayer();
    this.assetCache = {};
    this.runOnSave = prefs.get('runOnSave', true);
    this.breaks = new Map(); // file -> Set of lines: the breakpoints (kept per project)
    this.debugInfo = null; // the last build's lines and addresses (debuginfo.js)
    this.stopAt = null; // where the CPU stopped: { pc, regs, where, func }
    this.graphs = new Graphs(); // variables over time, under the C64
    this.tuning = new TuningPanel({
      onPoke: (name, n) => this.pokeVar(name, n),
      onKeep: (knob, n) => this.keepKnob(knob, n),
      onShow: (knob) => this.openFile(knob.file, { line: knob.line }),
    });
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

    const code = this.buildCode();

    // the asset editors
    this.editors = {
      sprites: new SpriteEditor(this),
      tiles: new TileEditor(this),
      map: new MapEditor(this),
      music: new MusicEditor(this),
    };
    const panes = Object.entries(this.editors).map(([id, ed]) => h('section.studio-pane', { 'data-mode': id }, ed.el));

    this.buildSide();

    u.status = h('footer.studio-status', {}, h('span.studio-pos'), h('span.studio-lang'), h('span.studio-hint', {}, 'Ctrl+Enter run · drag a value to tune it · F9 breakpoint · F10/F11 step · F1 help'));
    u.banner = h('div.studio-banner', { hidden: true });
    u.splitSide = h('div.studio-split', { title: 'Drag to give the code or the C64 more room (double-click: as it was)', role: 'separator', 'aria-orientation': 'vertical' });
    u.main = h('div.studio-main', {}, h('div.studio-work', {}, code, ...panes), u.splitSide, u.side);
    // on a phone (or in a look that asks for it) the code and the C64 take turns on the screen (studio.css): this bar switches
    const view = (v, label, title) => h('button.studio-phoneview', { 'data-view': v, onclick: () => this.setView(v), title }, label);
    u.phoneBar = h('nav.studio-phonebar', {}, view('work', '{ } Code', 'The code and the editors'),
      h('button.key.pill.primary', { onclick: () => this.run(), title: 'Build and run' }, '▶ Run'), view('game', '🕹 Game', 'The C64'));
    r.append(bar, u.banner, u.main, u.status, u.phoneBar);
    this.setView('work');
    // taking turns: on a narrow screen, or when the look says so (its --studio-turns: 1, as Classic's: a C64 shows one thing at a time)
    const turns = () => this.root.toggleAttribute('data-turns', NARROW.matches || getComputedStyle(this.root).getPropertyValue('--studio-turns').trim() === '1');
    NARROW.addEventListener('change', turns);
    addEventListener('breadbin:theme', turns);
    turns();
    // writing code on a phone: the keys its keyboard hides, above it (keybar.js)
    if (TOUCH) keyBar(this.code.view);
    this.layout();

    // the drop-down console, in the looks that have one (console.js)
    this.console = new Console(this);
    r.append(this.console.el);
    this.bindKeys();
    this.loadApi();
    warmUp();
    setInterval(() => this.poll(), POLL);
    setInterval(() => this.watch(), 250);
    setInterval(() => this.frameMeter(), 200);
    setInterval(() => this.lessonCard.lesson && this.lessonCard.check({ files: this.files, vars: this.vars || [], running: !!this.session?.started, value: (n) => this.lastValues?.get(n) }), 500);
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

  /** CODE: the file list and the editor; returns its pane. */
  buildCode() {
    const u = this.ui;
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
    this.code.onPoke = (name, n) => this.pokeVar(name, n);
    this.code.onGraph = (name) => {
      const v = this.vars?.find((x) => x.name === name && x.size <= 2);
      if (v) this.graphs.toggle(v);
    };
    this.code.onKept = (name, text, line) => toast(`${name} = ${text}: in the game now, and kept in ${this.path.split('/').pop()}:${line}`);
    this.code.view.dom.addEventListener('keyup', () => this.showCursor());
    this.code.view.dom.addEventListener('click', () => this.showCursor());
    return code;
  }

  /** The C64's column: the C64, the debugger's bar, what the build said, the settings and panels. */
  buildSide() {
    const u = this.ui;
    // the C64 and what the build said
    u.stage = h('div.studio-stage', {}, h('div.studio-idle', {}, h('b', {}, 'Press ▶ Run'), h('span', {}, 'to build the game and start it here')));
    u.meter = h('div.studio-meter', { title: 'Memory the program uses, of the 46 KB from $0801 to $BFFF' });
    // the frame meter: how much of each frame the game's code takes (bb_frame notes the raster line)
    u.cpuBar = h('i');
    u.cpuText = h('div.studio-meter-text');
    u.cpu = h('div.studio-cpu', { hidden: true, title: 'How much of each frame (1/50 s) the game\'s code takes before it waits in bb_frame(): over 100%, the game slows down' },
      h('div.studio-meter-bar', {}, u.cpuBar), u.cpuText);
    u.buildInfo = h('div.studio-buildinfo');
    u.problems = h('div.studio-problems');
    u.share = h('button.key.pill.studio-share', { onclick: () => this.openExport(), title: 'Share the game as a link, or export it: a web page that plays offline, a .prg' }, '⇪ Share');
    // a setting kept in the prefs, as a checkbox: `then(on)` after a change
    const checkbox = (pref, label, title, then = () => {}) => h('label.studio-live', { title },
      h('input', { type: 'checkbox', checked: prefs.get(pref, true), onchange: (e) => (prefs.set(pref, e.target.checked), then(e.target.checked)) }), ` ${label}`);
    u.live = checkbox('runOnSave', 'Run on save', 'Build and run again whenever a file is saved, here or in your own editor', (on) => (this.runOnSave = on));
    u.keep = checkbox('keepPlace', 'Keep my place', 'Run plays what you did since the game started again, at warp speed, with the new code: you are back where you were (or at the marked moment)');
    u.liveToggle = checkbox('liveValues', 'Values in code', 'While the game runs, show its variables beside the code that uses them; drag one to change it', (on) => on || this.code.setLive(null));
    u.vars = h('div.studio-vars-list');
    u.varsBox = h('details.studio-vars', { open: prefs.get('vars', false), ontoggle: (e) => prefs.set('vars', e.target.open) },
      h('summary', {}, 'All variables', h('span.studio-small', {}, ' · the running game\'s globals, live')), u.vars);
    // the control deck: an arcade stick, FIRE and the keys games ask for, as the player has
    u.deck = h('div.studio-deck', { hidden: true });
    u.toolsBtn = h('button.key.pill.studio-toolsbtn', { onclick: () => this.tools.toggle(), title: 'Tools: a monitor (memory as hex or code, Pause, Step, breakpoints at addresses) and a cheat finder', 'aria-pressed': 'false' }, '🔧 Tools');
    this.tools.onToggle = (on) => u.toolsBtn.setAttribute('aria-pressed', String(on));
    u.padBtn = h('button.key.pill.studio-padbtn', { onclick: () => this.setPad(!prefs.get('pad', TOUCH)), title: 'A control deck under the C64: a stick, FIRE and keys (for a phone or tablet, or a mouse)', 'aria-pressed': 'false' }, '🕹 Pad');
    // the debugger's bar: pause and the steps, where the CPU stopped, its registers
    const dbtn = (cls, label, title, fn) => h(`button.studio-dbtn.${cls}`, { onclick: fn, title }, label);
    u.dbgPause = dbtn('pause', '⏸', 'Pause at the next line of your code (F6)', () => this.debugToggle());
    u.dbgOver = dbtn('over', '↷', 'Step over: the next line here (F10)', () => this.dbg.over());
    u.dbgInto = dbtn('into', '↓', 'Step into: into the call on this line (F11)', () => this.dbg.into());
    u.dbgOut = dbtn('out', '↑', 'Step out: back to the line that called this function (Shift+F11)', () => this.dbg.out());
    // slow motion: the whole C64 at a half or a quarter of its speed, to watch a jump or a hit
    u.speed = h('button.studio-dbtn.speed', { onclick: () => this.slower(), title: 'Slow motion: the C64 at full, half or quarter speed (Shift+F6)' }, '1×');
    // a marked moment to go back to: tune, then try the same jump again
    u.mark = h('button.studio-dbtn.mark', { onclick: () => this.markMoment(), title: 'Mark this moment, to come back to it (Shift+F8)' }, '◉');
    u.replay = h('button.studio-dbtn.replay', { onclick: () => this.tryAgain(true), title: 'Replay: back to the marked moment, and what you did from there played again, with the code as it is now (Alt+F8)', disabled: true }, '▷');
    u.again = h('button.studio-dbtn.again', { onclick: () => this.tryAgain(), title: 'Back to the marked moment, with the Tuning settings as they are now (F8)', disabled: true }, '↺');
    u.dbgWhere = h('button.studio-dbg-where', { onclick: () => this.showStop(), title: 'Show this line' });
    u.dbgRegs = h('div.studio-dbg-regs');
    u.dbgClear = h('button.studio-tool.studio-dbg-clear', { onclick: () => this.clearBreakpoints(), title: 'Clear every breakpoint in the project' }, '');
    u.debug = h('div.studio-debug', { hidden: true },
      h('div.studio-dbg-bar', {}, u.dbgPause, u.dbgOver, u.dbgInto, u.dbgOut, u.dbgWhere, u.mark, u.again, u.replay, u.speed),
      h('div.studio-dbg-foot', {}, u.dbgRegs, u.dbgClear));
    u.side = h('aside.studio-side', {},
      this.lessonCard.el,
      // Safari starts the C64's sound only after a tap inside it: the note says so, and lets the tap through
      (u.screen = h('div.studio-screen', {}, u.stage, (u.sound = h('p.emu-sound', { hidden: true }, '🔇 Tap the C64 for sound')),
        h('button.studio-fullbtn', { onclick: () => this.fullscreen(), title: 'The C64 full screen (Alt+Enter; Esc to come back)', 'aria-label': 'Full screen' }, '⛶'),
        (u.boxes = h('button.studio-fullbtn.studio-boxbtn', { onclick: () => this.setBoxes(!this.hitbox.on), title: 'Hitboxes: each sprite\'s box, and the boxes the game tests with bb_overlap (red where they overlap)', 'aria-label': 'Hitboxes', 'aria-pressed': 'false' }, '▢')),
        this.hitbox.canvas)),
      u.debug,
      this.graphs.el,
      u.deck,
      h('div.studio-sidebar', {}, u.buildInfo, u.share, u.padBtn, u.toolsBtn, u.live, u.keep, u.liveToggle),
      this.tuning.el,
      u.cpu, u.meter, u.problems, u.varsBox, this.tools.el);

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
      this.root.style.setProperty('--studio-code-size', `${prefs.get('codeSize', TOUCH ? 16 : 13.5)}px`);
      u.code.classList.toggle('nofiles', prefs.get('hideFiles', TOUCH));
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
    if (show && !this.pad) {
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
    const size = Math.max(10, Math.min(24, prefs.get('codeSize', TOUCH ? 16 : 13.5) + d));
    prefs.set('codeSize', size);
    this.root.style.setProperty('--studio-code-size', `${size}px`);
    this.code.view.requestMeasure();
  }

  toggleFiles() {
    const hide = !prefs.get('hideFiles', TOUCH);
    prefs.set('hideFiles', hide);
    this.ui.code.classList.toggle('nofiles', hide);
    this.code.view.requestMeasure();
  }

  bindKeys() {
    // on the document, so the keys work wherever the focus is while the tab shows
    document.addEventListener('keydown', (e) => {
      if (this.root.hidden || document.querySelector('dialog[open]')) return;
      // the key under Esc drops the console down (in a look that has one), but not while a field takes text
      if (e.key === '`' && this.console.enabled && !e.target.closest?.('input, textarea, .studio-console')) {
        e.preventDefault();
        return this.console.toggle();
      }
      const name = emu.studioKey(e, this.dbg.stopped);
      if (!name || (name === 'files' && this.mode !== 'code')) return;
      e.preventDefault();
      this.act(name);
    });
  }

  /** Do what key `name` (emu.js studioKey: a key here, or the C64's hotkeys) stands for. */
  act(name) {
    if (/^mode[1-5]$/.test(name)) return this.setMode(MODES[name.at(-1) - 1].id);
    const debug = () => this.debugKey(name);
    ({
      run: () => this.run(),
      save: () => this.saveNow(true),
      help: () => this.openHelp(),
      fullscreen: () => this.fullscreen(),
      bigger: () => this.codeSize(+1),
      smaller: () => this.codeSize(-1),
      files: () => this.toggleFiles(),
      pause: () => this.debugToggle(),
      over: debug,
      into: debug,
      out: debug,
      slower: () => this.slower(),
      mark: () => this.markMoment(),
      again: () => this.tryAgain(),
      replay: () => this.tryAgain(true),
      escape: () => (this.setView('work'), this.mode === 'code' ? this.code.focus() : this.editors[this.mode].el.focus()),
    })[name]?.();
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
    this.lessonCard.stop(); // (another project: the lesson was about the last one)
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
    this.vars = [];
    this.graphs.rebind([]);
    this.tuning.set(files);
    this.lastValues = null;
    this.code.setLive(null);
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
      return this.openFolderStore(folder);
    }
    const name = prompt('Name of the new project:', `My ${ex.title.replace(/^My /, '')}`);
    if (name) return this.openNew(files, name);
  }

  /** `files` as a new project in this browser (renamed `name`, if given), opened. */
  async openNew(files, name = null) {
    if (name) files['project.json'] = `${pretty({ ...JSON.parse(files['project.json'] || '{}'), name })}\n`;
    const s = new BrowserStore(`${slug(name || meta(files).name)}-${Date.now().toString(36)}`);
    await s.write(files);
    return this.openStore(s, `browser:${s.id}`);
  }

  /** Open the project in `folder`, remembered for the next visit. */
  async openFolderStore(folder) {
    const key = `folder:${folder.handle.name}:${Date.now()}`;
    await folder.remember(key);
    return this.openStore(folder, key);
  }

  /** Open a folder: its project, or (empty) a new project put there. */
  async openFolder() {
    if (!canUseFolders()) return toast('This browser can\'t open folders on disk: use Import .zip, or Chrome or Edge.');
    const folder = await FolderStore.pick();
    if (!folder) return;
    const files = await folder.read();
    if (files['project.json'] || hasSources(files)) {
      // a Studio project, or any folder of C or assembly (a plain cc65 project)
      await this.openFolderStore(folder);
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
    await this.openFolderStore(folder);
    toast(`Saved to ${folder.label} on your disk. From now on every save goes there.`);
  }

  exportZip() {
    this.download(`${slug(this.meta.name)}.zip`, zip(this.files));
  }

  async importZip(file) {
    try {
      const files = normalise(await unzip(new Uint8Array(await file.arrayBuffer())), { unwrap: true });
      if (!Object.keys(files).length) throw new Error('the zip is empty');
      await this.openNew(files);
      toast(`Imported ${this.meta.name}.`);
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
    // gen/, what the build writes, folded away unless opened (or one of its files is the open one)
    const genOpen = prefs.get('genOpen', false);
    const gens = Object.keys(this.files).filter(isGenerated).length;
    for (const p of sortPaths(Object.keys(this.files))) {
      const dir = p.includes('/') ? p.slice(0, p.lastIndexOf('/') + 1) : '';
      if (dir !== group) {
        group = dir;
        if (dir === 'gen/' && gens) {
          list.append(h('button.studio-dir.studio-dir-fold', { onclick: () => (prefs.set('genOpen', !genOpen), this.renderFiles()), title: 'The files every build writes (the runtime, the assets as C): read them, change the assets or the source instead', 'aria-expanded': String(genOpen) },
            `${genOpen ? '▾' : '▸'} gen/`, h('span.studio-dir-note', {}, ` written by the build · ${gens}`)));
        } else if (dir) list.append(h('div.studio-dir', {}, dir));
      }
      if (isGenerated(p) && !genOpen && p !== this.path) continue;
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
    if (mode === 'sprites' || mode === 'tiles') {
      clearTimeout(this.liveTimer);
      this.liveTimer = setTimeout(() => this.liveAssets(), 60);
    }
  }

  /**
   * Live editing: the sprites and tiles as the editors have them now, written
   * into the running game (no Run): the sprite images at $D000 (the RAM under
   * the I/O chips: the CPU port switched for the copy), their colours, the
   * character set at $C800 and the tiles' colours, and the screen's cells
   * whose tile changed colour. The game's next bb_spr()/bb_set_tile() uses
   * them; what is on the screen changes at once. A new sprite or a removed one
   * needs a Run (the images' numbers move).
   */
  liveAssets() {
    const s = this.session;
    const l = this.runLabels;
    if (!s?.started || s.loading || this.meta?.lang !== 'c' || l?._bb_sprite_color === undefined) return;
    let b;
    try {
      b = assetBytes({ sprites: this.asset('sprites'), tiles: this.asset('tiles') });
    } catch {
      return;
    }
    if (this.liveCount !== undefined && b.count !== this.liveCount) {
      if (!this.liveWarned) toast('A new or removed sprite: ▶ Run to use it (the others change live)');
      this.liveWarned = true;
      return;
    }
    const old = this.liveTileColor || null;
    this.liveTileColor = b.tileColor;
    s.script(`(() => {
      const put = (a, bytes) => bytes.forEach((v, i) => wasm_poke(a + i, v));
      const port = wasm_peek(1);
      wasm_poke(1, (port & 0xf8) | 4); // RAM at $D000 for the sprite images
      put(0xd000, ${JSON.stringify([...b.spriteData])});
      wasm_poke(1, port);
      put(${l._bb_sprite_color}, ${JSON.stringify(b.spriteColor)});
      put(${l._bb_sprite_multi}, ${JSON.stringify(b.spriteMulti)});
      put(0xd025, ${JSON.stringify(b.mc)});
      put(0xc800, ${JSON.stringify([...b.charset])});
      const tc = ${JSON.stringify(b.tileColor)};
      put(${l._bb_tile_color}, tc);
      const was = ${JSON.stringify(old)};
      // the cells on the screen whose tile has a new colour (text, coloured by the game, left as it is)
      if (was) for (let i = 0; i < 1000; i++) { const t = wasm_peek(0xc000 + i); if (was[t] !== tc[t]) wasm_poke(0xd800 + i, tc[t]); }
      // sprites on the screen showing an image: its colour now
      for (let n = 0; n < 8; n++) {
        const img = wasm_peek(0xc3f8 + n) - 64;
        if (img >= 0 && (wasm_peek(0xd015) >> n) & 1) {
          const c = ${JSON.stringify(b.spriteColor)}[img];
          if (c !== undefined) wasm_poke(0xd027 + n, c);
        }
      }
    })();`);
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
        // (an assembly project's come from ACME's listing: asmdebug.js)
        this.vars = r.vars || globals(r.labels, r.memory, r.sizes, { hidden: r.hidden, types: cDecls(this.files) });
        this.graphs.rebind(this.vars);
        this.tuning.set(this.files);
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
    // History: this version kept (if it changed since the last Run), to come back to from Projects
    keepVersion(this.key, this.sourceFiles(), 'Run').catch(() => {});
    this.debugInfo = new DebugInfo(r.debug, { work: '/p' });
    this.stopAt = null;
    this.code.setExecLine(0);
    this.forgetMoment();
    const place = this.session?.started && prefs.get('keepPlace', true) ? await this.placeFor(r) : null;
    this.markFrame = null; // (the new build gets its own mark, at the same moment, when it is back there)
    this.runLabels = r.labels;
    this.runTapeSize = r.sizes?.bb_tape?.size || 768;
    if (this.session?.started) {
      this.session.reload(place ? place.prg : r.prg);
      if (place) this.backToPlace(this.session, place);
      this.dbg.attach(this.session, this.debugInfo);
      this.dbg.setBreakpoints(this.breakAddrs());
    } else {
      this.session?.stop();
      const prg = await prepare('game.prg', r.prg, { canMount: false });
      this.session = emu.run(this.ui.stage, prg, {
        border: 0.1,
        onAudio: (state) => (this.ui.sound.hidden = state !== 'suspended'),
        kernal: await patchedKernal(store.cachedBytes),
        basic: await patchedBasic(store.cachedBytes),
        // the Studio's keys still work while the C64 has the keyboard
        onHotkey: (key) => this.act(key),
      });
    }
    this.ui.stop.disabled = false;
    this.hitbox.attach(this.session, r.labels);
    this.tools.attach(this.session); // (a new program: the last one's search, freezes and breakpoints go)
    // live editing starts from this build's sprites and tiles
    try {
      const b = assetBytes({ sprites: this.asset('sprites'), tiles: this.asset('tiles') });
      [this.liveCount, this.liveTileColor, this.liveWarned] = [b.count, b.tileColor, false];
    } catch {
      this.liveCount = undefined;
    }
    // a gamepad is joystick 2, as the deck is
    this.stopGamepads ??= pollGamepads((m) => this.session?.send(m), () => 2);
    this.setPad(prefs.get('pad', TOUCH));
    this.ui.debug.hidden = false;
    if (!this.dbg.session || this.dbg.session !== this.session) this.debugFresh(this.session, r.prg);
    this.setView('game');
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
    if (this.speed !== 100) this.setSpeed(this.speed); // a new C64 starts at full speed
  }

  /** Mark this moment of the game (all of the C64: a VirtualC64 snapshot, kept in the emulator's page). */
  async markMoment(quiet = false) {
    const s = this.session;
    if (!s?.started) return;
    if (this.snapSession !== s) {
      s.script(SNAP);
      this.snapSession = s;
    }
    const tape = hasTape(this.runLabels) ? this.runLabels._bb_tape_frames : 0;
    const m = await s.call('bbsnapMark', tape, 5000).catch(() => null);
    if (!m) return toast('This moment could not be marked');
    this.markFrame = m.frames;
    this.take = null;
    this.ui.again.disabled = false;
    this.ui.replay.disabled = m.frames === null;
    if (!quiet) toast('Marked. Tune the game, then ↺ (F8) plays it again from here, ▷ replays what you did');
  }

  /** The tape of the running game (bb.c), or null (not a Studio runtime game, or no answer). */
  async tape() {
    const l = this.runLabels;
    if (!hasTape(l) || !this.session?.started) return null;
    const got = await this.session.peek(tapeRanges(l, this.runTapeSize));
    return got && readTape(got);
  }

  /** Back to the marked moment, the knobs set to what the code says now (the snapshot has their old values). */
  async tryAgain(replay = false) {
    const s = this.session;
    if (!s?.started || this.snapSession !== s || this.ui.again.disabled) return;
    const writes = tunables(this.files).map((k) => [this.vars?.find((v) => v.name === k.name), k.value]).filter(([v]) => v).map(([v, n]) => [v.addr, bytesOf(n, v)]);
    // The take: what was played since the mark, kept before the snapshot puts the tape back as it was
    // there (so ↺ then ▷ still has it). Replay plays the last take again from the mark.
    let tape = null;
    if (this.markFrame !== null && hasTape(this.runLabels)) {
      const t = await this.tape();
      // (a new take only when the stick was used since the mark: waiting between ↺ and ▷ is no take)
      if (t && tapeFrames(t.bytes) > this.markFrame + 5 && usedAfter(t.bytes, this.markFrame)) this.take = t.bytes;
      const l = this.runLabels;
      tape = { frames: l._bb_tape_frames, tape: l._bb_tape, len: l._bb_tape_len, pos: l._bb_tape_pos, left: l._bb_tape_left, mode: l._bb_tape_mode, bytes: replay && this.take ? [...this.take] : null };
    }
    if (!(await s.call('bbsnapAgain', { writes, tape }, 5000).catch(() => false))) return;
    this.lastValues = null; // no "changed" flashes for the jump back
    s.frame.focus();
  }

  forgetMoment() {
    this.ui.again.disabled = true;
    this.ui.replay.disabled = true;
    if (this.snapSession === this.session) this.session?.script('window.bbsnapBytes = null;');
  }

  /**
   * Keep my place: the new build `r` with the running game's tape in it (up
   * to the marked moment, if there is one), to be played at warp speed; null
   * when there is nothing to keep (no tape, or under a second of play).
   */
  async placeFor(r) {
    if (!hasTape(r.labels)) return null;
    const t = await this.tape();
    if (!t || t.mode === PLAY) return null; // (still replaying the last one: start afresh)
    const frames = this.markFrame ?? tapeFrames(t.bytes);
    if (frames < 50) return null;
    const bytes = cut(t.bytes, frames);
    try {
      return { prg: patchPrg(r.prg, playWrites(r.labels, bytes, { seed: t.seed })), frames, mark: this.markFrame !== null, labels: r.labels };
    } catch {
      return null;
    }
  }

  /** The new build replays its tape at warp speed; at its end, normal speed (and the mark again, if there was one). */
  async backToPlace(session, place) {
    const mode = [[place.labels._bb_tape_mode, 1]];
    session.script('wasm_set_warp(1);');
    toast(`Back to your place: ${(place.frames / 50).toFixed(1)} s of play, at warp speed…`, 30000);
    const t0 = Date.now();
    for (;;) {
      await new Promise((r) => setTimeout(r, 100));
      if (this.session !== session) return;
      if (session.loading) continue;
      const got = await session.peek(mode);
      if ((got && got[0][0] !== PLAY) || Date.now() - t0 > 30000) break;
    }
    session.script('wasm_set_warp(0);');
    if (this.speed !== 100) this.setSpeed(this.speed);
    if (place.mark) await this.markMoment(true);
    toast(place.mark ? 'Back at the marked moment, with the new code' : 'Back where you were, with the new code');
    this.placed = (this.placed || 0) + 1; // (for the tests)
    session.frame.focus();
  }

  /**
   * The frame meter: the busiest of the last second's samples of where the
   * raster was when the game finished its frame (bb_busy_line, from the
   * interrupt at line 250 round the 312 lines of a PAL frame), and the frames
   * it missed (bb_busy_late going up).
   */
  async frameMeter() {
    const s = this.session;
    const l = this.runLabels;
    const u = this.ui;
    const on = !!(s?.started && !s.loading && l?._bb_busy_line !== undefined && !this.dbg.stopped && !document.hidden);
    u.cpu.hidden = !on;
    if (!on) return;
    const got = await s.peek([[l._bb_busy_line, 2], [l._bb_busy_late, 1]]).catch(() => null);
    if (!got) return;
    const line = got[0][0] | (got[0][1] << 8);
    const used = ((line - 251 + 312) % 312) / 312;
    const late = got[1][0];
    const now = Date.now();
    this.busy = (this.busy || []).filter((b) => now - b.t < 1000);
    this.busy.push({ t: now, used, late });
    const missed = (this.busy.at(-1).late - this.busy[0].late + 256) % 256;
    const pct = missed ? 100 : Math.round(Math.max(...this.busy.map((b) => b.used)) * 100);
    u.cpuBar.style.width = `${Math.min(100, pct)}%`;
    u.cpuBar.className = pct >= 90 ? 'full' : '';
    u.cpuText.replaceChildren(missed ? h('b', {}, `Over a frame: ${missed} missed in the last second, the game slows down`) : h('span', {}, h('b', {}, `${pct}%`), ' of a frame used by the game\'s code'));
  }

  /** Files changed for you (a lesson's Show me): into the project, the editors and the save. */
  applyFiles(changed, step = {}) {
    for (const [p, t] of Object.entries(changed)) {
      this.files[p] = t;
      delete this.assetCache[p];
      this.code.update(p, t);
    }
    this.queueSave(changed);
    this.renderFiles();
    if (step.mode) this.setMode(step.mode);
    else if (step.file) {
      this.setMode('code');
      this.openFile(step.file);
    }
  }

  /** A lesson's step begins: the editor it is about. */
  lessonStep(st) {
    if (st.mode) this.setMode(st.mode);
    else if (st.file) {
      this.setMode('code');
      this.openFile(st.file);
    }
  }

  /** Start lesson `id` in a new project made from the Empty game. */
  async startLesson(id) {
    const lesson = LESSONS.find((l) => l.id === id);
    await this.openNew(await exampleFiles(EXAMPLES.find((e) => e.id === 'blank')), lesson.title);
    this.lessonCard.start(lesson);
    this.lessonStep(lesson.steps[0]);
  }

  /** The project's own files (not gen/, which every build writes again). */
  sourceFiles() {
    return Object.fromEntries(Object.entries(this.files).filter(([p]) => !isGenerated(p)));
  }

  /** The History section of Projects: the versions kept at each Run, newest first, each to go back to. */
  async historySection() {
    const versions = await projectHistory(this.key).catch(() => []);
    if (!versions.length) return h('p.studio-small', {}, 'Each Run keeps a version of the project here, to go back to.');
    const ago = (t) => {
      const m = Math.round((Date.now() - t) / 60000);
      return m < 1 ? 'just now' : m < 60 ? `${m} min ago` : m < 1440 ? `${Math.round(m / 60)} h ago` : new Date(t).toLocaleDateString();
    };
    // what each version changed, from the one before it (older)
    const changed = (v, older) => {
      if (!older) return 'the first kept';
      const paths = new Set([...Object.keys(v.files), ...Object.keys(older.files)]);
      const diff = [...paths].filter((p) => JSON.stringify(v.files[p] ?? null) !== JSON.stringify(older.files[p] ?? null)).map((p) => p.split('/').pop());
      return diff.length ? diff.slice(0, 3).join(', ') + (diff.length > 3 ? ` and ${diff.length - 3} more` : '') : 'no change';
    };
    return h('div.studio-history', {}, ...versions.map((v, i) => h('div.studio-history-row', {},
      h('span.studio-history-when', {}, ago(v.at)), h('span.studio-history-what', {}, changed(v, versions[i + 1])),
      h('button.studio-tool', { onclick: () => this.restoreVersion(v), title: 'Put the project back as it was then (the project as it is now is kept first)' }, 'restore'))));
  }

  /** Back to version `v`: the project as it is now kept first, then its files as they were. */
  async restoreVersion(v) {
    if (!confirm('Put the project back as it was then? (How it is now is kept in History first.)')) return;
    await keepVersion(this.key, this.sourceFiles(), 'before a restore');
    const changed = { ...v.files };
    for (const p of Object.keys(this.sourceFiles())) if (!(p in v.files)) changed[p] = null;
    for (const [p, t] of Object.entries(changed)) {
      if (t === null) delete this.files[p];
      else this.files[p] = t;
      delete this.assetCache[p];
      this.code.forget?.(p);
    }
    this.queueSave(changed);
    this.renderFiles();
    this.openFile(this.path in this.files ? this.path : Object.keys(this.files).find((p) => /\.(c|asm)$/.test(p)) || 'project.json');
    this.setMode(this.mode === 'code' ? 'code' : this.mode);
    this.tuning.set(this.files);
    document.querySelector('.studio-picker[open]')?.close();
    toast('The project is back as it was. ▶ Run to play that version.');
  }

  /** Taking turns (data-turns): the code and editors ('work') or the C64 ('game') on the screen. (Otherwise both show.) */
  setView(v) {
    this.root.dataset.view = v;
    for (const b of this.ui.phoneBar.querySelectorAll('[data-view]')) b.setAttribute('aria-pressed', String(b.dataset.view === v));
  }

  /** The hitbox overlay on or off. */
  setBoxes(on) {
    this.hitbox.set(on);
    this.ui.boxes.setAttribute('aria-pressed', String(on));
  }

  /** The next of full, half and quarter speed (the speed button, Shift+F6). */
  slower() {
    if (this.session) this.setSpeed(SPEEDS[(SPEEDS.indexOf(this.speed) + 1) % SPEEDS.length]);
  }

  /** Slow motion: the C64's speed in percent (100, 50, 25). */
  setSpeed(pct) {
    this.speed = pct;
    const u = this.ui;
    u.speed.textContent = { 100: '1×', 50: '0.5×', 25: '0.25×' }[pct];
    u.speed.classList.toggle('slow', pct !== 100);
    u.speed.setAttribute('aria-label', `Speed ${pct}%`);
    // VirtualC64's speed boost: 50 runs it at half its speed (and its sound an octave down)
    this.session?.script(`try { wasm_configure('OPT_C64_SPEED_BOOST', '${pct}'); } catch (e) {}`);
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
    // (an assembly routine is its label, without C's parentheses)
    u.dbgWhere.replaceChildren(h('b', {}, s.func ? (s.where && !/\.(c|h)$/.test(s.where.file) ? s.func : `${s.func}()`) : 'stopped'), ` ${where}`);
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
    u.dbgWhere.replaceChildren(h('span.studio-small', { title: 'Click the margin left of a line number for a breakpoint (F9)' }, 'Running'));
    u.dbgRegs.textContent = '';
    this.showBreakCount();
  }

  /**
   * Read the running game's globals: into the code, at the end of the lines
   * that use them, and into the Variables panel when it is open.
   */
  async watch() {
    if (this.graphs.list.length) this.sampleGraphs();
    const box = this.ui.varsBox;
    const inCode = this.mode === 'code' && prefs.get('liveValues', true);
    const tuning = !this.tuning.el.hidden;
    if (this.root.hidden || document.hidden || !this.session?.started || this.session.loading || this.watching || !(box.open || inCode || tuning)) return;
    const vars = this.vars || [];
    if (!vars.length) {
      if (box.open && !this.ui.vars.childElementCount) this.ui.vars.append(h('p.studio-small', {}, 'Run the game to see its variables here.'));
      return;
    }
    this.watching = true;
    try {
      const values = await this.session?.peek(ranges(vars));
      if (!values) return;
      this.lastValues ??= new Map();
      if (tuning) this.tuning.update(new Map(vars.map((v, i) => [v, values[i]]).filter(([v]) => v.size <= 2).map(([v, b]) => [v.name, number(b, v)])));
      if (inCode) {
        const live = new Map();
        vars.forEach((v, i) => {
          const text = brief(values[i], v);
          const plain = v.size <= 2;
          const [min, max] = plain ? range(v) : [0, 0];
          live.set(v.name, { text, changed: this.lastValues.has(v.name) && this.lastValues.get(v.name) !== text, editable: plain, num: plain ? number(values[i], v) : 0, min, max, title: `${v.name}: ${format(values[i], v.size, v.unit)}\n${place(v)}` });
          this.lastValues.set(v.name, text);
        });
        this.code.setLive(live);
        this.liveTip();
      } else this.code.setLive(null);
      if (box.open) this.showVars(vars, values);
    } finally {
      this.watching = false;
    }
  }

  /** All variables (the panel): each global and its value, a short flash on the ones that changed. */
  showVars(vars, values) {
    const list = this.ui.vars;
    if (list.childElementCount !== vars.length) {
      list.replaceChildren(...vars.map((v) => h(`div.studio-var${v.size > 2 ? '.wide' : ''}`, { title: place(v) }, h('span.studio-var-name', {}, v.name), h('span.studio-var-value'))));
    }
    vars.forEach((v, i) => {
      const el = list.children[i]?.lastChild;
      const text = format(values[i], v.size, v.unit);
      if (!el || el.textContent === text) return;
      el.textContent = text;
      el.classList.remove('changed');
      requestAnimationFrame(() => el.classList.add('changed'));
    });
  }

  /** Take the graphed variables' samples from the emulator (installing its sampler when they changed). */
  async sampleGraphs() {
    const s = this.session;
    if (!s?.started || this.sampling || document.hidden) return;
    this.sampling = true;
    try {
      if (this.graphs.changed || this.graphsSession !== s) {
        s.script(sampler(this.graphs.specs()));
        this.graphs.changed = false;
        this.graphsSession = s;
      }
      const rows = await s.call('bbgraphTake', null, 2000).catch(() => null);
      if (rows?.length) this.graphs.push(rows);
    } finally {
      this.sampling = false;
    }
  }

  /** Once: say what the numbers beside the code are, the first time they show. */
  liveTip() {
    if (prefs.get('liveTipped', false) || !this.ui.banner.hidden) return;
    prefs.set('liveTipped', true);
    const tune = Object.keys(this.files).find((p) => /\.c$/.test(p) && /^signed char jump = /m.test(this.files[p]));
    this.showBanner('The numbers beside the code are the game\'s variables, live. Drag one sideways to change it while the game plays, or click it and type. On the line that declares a variable, the new value stays in the code. The Tuning sliders under the C64 do the same for the game\'s settings.', [
      ...(tune ? [{ label: '🎚 Tune Bubbler\'s jump', onclick: () => {
        this.hideBanner();
        this.openFile(tune, { line: this.files[tune].split('\n').findIndex((l) => l.startsWith('signed char jump')) + 1 });
      } }] : []),
    ], { dismiss: 'got it' });
  }

  /** A Tuning slider let go: its number into the knob's declaration, so the next Run keeps it. */
  keepKnob(knob, n) {
    // the line may have moved since the panel was made
    const k = tunables(this.files).find((x) => x.name === knob.name) || knob;
    const lines = (this.files[k.file] || '').split('\n');
    const changed = setKnob(lines[k.line - 1] || '', k, n);
    if (changed === null) return;
    lines[k.line - 1] = changed;
    const text = lines.join('\n');
    if (this.path === k.file) this.code.update(k.file, text); // the editor reports the edit, which saves it
    else {
      this.files[k.file] = text;
      this.code.forget(k.file);
      this.queueSave({ [k.file]: text });
    }
    knob.value = n;
    toast(`${k.name} = ${n}: kept in ${k.file.split('/').pop()}:${k.line}`);
  }

  /** Change global `name` in the running game to `n`: a tweak, kept until the game changes it. */
  pokeVar(name, n) {
    const v = this.vars?.find((x) => x.name === name);
    if (!v || !this.session?.started) return;
    this.session.poke(v.addr, bytesOf(n, v));
  }

  stop() {
    this.code.setLive(null);
    this.dbg.detach();
    this.stopAt = null;
    this.code.setExecLine(0);
    this.ui.debug.hidden = true;
    this.session?.stop();
    this.session = null;
    this.tools.attach(null);
    this.stopGamepads?.();
    this.stopGamepads = null;
    this.ui.sound.hidden = true;
    this.setPad(prefs.get('pad', TOUCH));
    this.ui.stop.disabled = true;
    this.ui.stage.replaceChildren(h('div.studio-idle', {}, h('b', {}, 'Press ▶ Run'), h('span', {}, 'to build the game and start it here')));
  }

  /**
   * Share & export: the game as a link, a web page, or a PRG. The web page
   * is breadbin's own small C64 with the game inside (export.js); the C64
   * here stays the place to make it.
   */
  async openExport() {
    const dlg = h('dialog.sheet.studio-picker.studio-export');
    const close = () => dlg.close();
    const busy = async (btn, what, fn) => {
      const r = this.lastBuild?.ok ? this.lastBuild : await this.build();
      if (!r.ok) return toast('Fix the build first: the export carries the built program.');
      btn.classList.add('busy');
      try {
        await fn(r);
      } catch (e) {
        toast(`No ${what}: ${e.message}`);
      } finally {
        btn.classList.remove('busy');
      }
    };
    const name = this.meta.name;
    const base = slug(name) || 'game';
    const card = (icon, color, title, about, what, fn) => {
      const btn = h('button.studio-card', { title: about, onclick: () => busy(btn, what, fn) },
        h('span.studio-card-art', { style: { background: color } }, icon),
        h('span.studio-card-body', {}, h('b', {}, title), h('span', {}, about)));
      return btn;
    };
    dlg.append(
      h('header.studio-picker-head', {}, h('h2', {}, `Share & export ${name}`), h('button.key.icon.small.close', { onclick: close, 'aria-label': 'Close' }, '×')),
      h('section', {}, h('span.studio-label', {}, 'Share'), h('div.studio-cards', {},
        card('🔗', '#458588', 'A link', 'Plays in breadbin, on a phone too. The whole game is inside the link: nothing is uploaded.', 'link', async (r) => {
          close();
          await share('play', await packCart(r.prg, name), name);
        }))),
      h('section', {}, h('span.studio-label', {}, 'Export · the game with a small C64 of its own, no emulator to install'), h('div.studio-cards', {},
        card('🌐', '#b16286', 'Web page', `${base}.html: one file that plays in any browser, offline too, on Linux, Windows, Mac and phones. Put it on any web host.`, 'web page', async (r) => this.download(`${base}.html`, new TextEncoder().encode(await webPage(r.prg, name)))))),
      h('section', {}, h('span.studio-label', {}, 'For a C64'), h('div.studio-cards', {},
        card('💾', '#7c6f64', 'C64 program', `${base}.prg: for VICE (x64sc ${base}.prg), or an SD card in a real C64.`, '.prg', async (r) => this.download(`${base}.prg`, r.prg)))),
    );
    showDialog(dlg);
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
      }, h('span.studio-problem-where', {}, d.file ? `${d.file.split('/').pop()}:${d.line}` : d.tool), h('span', {}, d.message, explain(d.message) ? h('span.studio-problem-hint', {}, explain(d.message)) : null)));
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
    let historyBox;
    dlg.append(
      h('header.studio-picker-head', {}, h('h2', {}, 'Projects'), h('button.key.icon.small.close', { onclick: close, 'aria-label': 'Close' }, '×')),
      h('section.studio-picker-now', {},
        h('div', {}, h('span.studio-label', {}, 'Open now'), h('b', {}, this.meta.name), h('span.studio-where', {}, this.store?.kind === 'folder' ? ` in ${this.store.label} on your disk` : ' in this browser')),
        h('div.studio-picker-actions', {},
          folders ? h('button.key.pill.primary', { onclick: () => (close(), this.saveToFolder()), title: 'Write the project into a folder on your disk and keep working there' }, '💾 Save to a folder…') : null,
          h('button.key.pill', { onclick: () => this.exportZip() }, '⇩ Export .zip'),
          h('button.key.pill', { onclick: () => (close(), this.openExport()), title: 'A link, a web page that plays offline, a .prg' }, '⇪ Share & export…'),
          this.key.startsWith('browser:') && EXAMPLES.some((e) => `browser:${e.id}` === this.key)
            ? h('button.key.pill', { onclick: async () => confirm('Throw away your changes to this example?') && (close(), await this.openExample(this.key.slice(8), { fresh: true })) }, 'Reset example') : null)),
      (historyBox = h('details.studio-picker-history', { ontoggle: async (e) => e.target.open && historyBox.querySelector('.studio-history-body').replaceChildren(await this.historySection()) },
        h('summary', {}, h('span.studio-label', {}, '🕘 History'), h('span.studio-small', {}, ' · a version kept at each Run')), h('div.studio-history-body'))),
      h('section', {}, h('span.studio-label', {}, 'Open'),
        h('div.studio-picker-actions', {},
          h('button.key.pill', { onclick: () => (close(), this.openFolder()), disabled: !folders, title: folders ? 'A project folder on your disk, or an empty folder for a new one' : 'Needs Chrome, Edge or Opera' }, '📂 Open a folder…'),
          h('button.key.pill', { onclick: () => fileIn.click() }, '⇧ Import .zip…'), fileIn),
        folders ? null : h('p.studio-small', {}, 'This browser can\'t reach folders on your disk: Export and Import .zip carry a project in and out. In Chrome, Edge or Opera, the Studio saves straight into a folder.'),
        folderCards.length || ownCards.length ? h('div.studio-owns', {}, ...folderCards, ...ownCards) : null),
      h('section', {}, h('span.studio-label', {}, 'Examples · each opens its own copy'), h('div.studio-cards', {}, ...EXAMPLES.map((ex) => card(ex, async () => (close(), await this.openExample(ex.id)))))),
      h('section.studio-picker-lessons', {}, h('span.studio-label', {}, '🎓 Lessons'),
        h('div.studio-lesson-list', {}, ...LESSONS.map((l) => h('button.studio-lesson-pick', { onclick: async () => (close(), await this.startLesson(l.id)) }, h('b', {}, l.title), h('span', {}, `${l.about} ${l.steps.length} steps.`))))),
      h('section', {}, h('span.studio-label', {}, 'New project from…'), h('div.studio-picker-actions', {},
        ...EXAMPLES.filter((e) => e.template).map((ex) => h('button.key.pill', { onclick: () => (close(), this.newProject(ex.id)) }, `${ex.icon} ${ex.title}`)))),
    );
    showDialog(dlg);
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
    showDialog(dlg);
  }

  openHelp() {
    showDialog(helpSheet(this.api || [], (e) => this.insertCall(e)));
  }

  /** A call picked in the cheat sheet, into the code at the cursor: a function as name(), the cursor between the brackets. */
  insertCall(e) {
    this.setView('work');
    this.setMode('code');
    const v = this.code.view;
    const text = e.kind === 'function' ? `${e.name}()` : e.name;
    const { from, to } = v.state.selection.main;
    v.dispatch({ changes: { from, to, insert: text }, selection: { anchor: from + text.length - (e.kind === 'function' ? 1 : 0) }, scrollIntoView: true });
    v.focus();
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
    this.showBanner('Welcome to the Studio! ▶ Run plays Bubbler on the C64 beside the code. Change its code, sprites, rooms or music and run it again: while it plays, the code shows the game\'s numbers, and you can drag them to tune it.',
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

/** Where variable `v` is: "$C03A, 2 bytes". */
const place = (v) => `$${v.addr.toString(16).toUpperCase()}, ${v.size} byte${v.size > 1 ? 's' : ''}`;

const seg = (mem, name) => mem.segments.filter((s) => s.name === name).reduce((n, s) => n + s.size, 0);

/** A file's icon in the list, by its name. */
const ICONS = [[/\.c$/, 'C'], [/\.h$/, 'H'], [/\.(s|asm|a|inc)$/, '◇'], [/sprites\.json$/, '☺'], [/tiles\.json$/, '▦'], [/map\.json$/, '▤'],
  [/music\.json$/, '♫'], [/\.json$/, '{}'], [/Makefile$/, '⚙'], [/\.md$/, '¶']];
const icon = (path) => ICONS.find(([re]) => re.test(path))?.[1] ?? '·';

/** Start the Studio in the Hack tab's section. */
export async function initStudio(root) {
  const s = new Studio(root);
  window.breadbinStudio = s; // for the browser tests (tools/studio/test/browser) and the curious
  await s.start();
  return s;
}

