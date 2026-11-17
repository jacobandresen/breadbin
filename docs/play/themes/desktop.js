// desktop - the parts of a desktop look a stylesheet can't make. Loaded by
// themes.js for the looks that ask for it:
//  - 'chicago': a Start menu that opens (the toast at the left of the taskbar;
//    Ctrl+Esc too), icons on the teal desk, a Run box, Help, a Notepad, the
//    Shut Down box (and the screen that says it's safe to turn the computer
//    off), and Dial-Up Networking (themes/dialup.js), which never gets online.
//    A right click on the bare desk has its menu too (Arrange Icons, Refresh,
//    Properties), the buttons on a shelf window's title bar work, the tray's
//    speaker sets the volume, and a game playing has its button on the taskbar.
//  - 'cupertino': the toast at the left of the menu bar opens its menu, as
//    the first desktops' did: About breadbin…, the desk accessories (a
//    Control Panel, Find, a Note Pad, a Puzzle of sliding tiles) and Shut
//    Down; the disk at the top right of the desk, the Trash at the foot.
//  - 'geos': the first menu of the menu bar opens, with the info box, the
//    desk accessories and BASIC, which leaves for the C64's own screen.

import { $, h } from '../ui.js';
import { settings, save } from '../settings.js';
import * as store from '../store.js';
import { applyTheme } from './themes.js';
import { icon, win } from './windows.js';
import { dialUp, hush } from './dialup.js';

const go = (tab) => () => { location.hash = '#' + tab; };

// ── the Start menu ──────────────────────────────────────────────────────────

const MENU = [
  { label: 'Programs', icon: 'folder', items: [
    { label: 'Games', icon: 'computer', run: go('games') },
    { label: 'Tunes', icon: 'note', run: go('tunes') },
    { label: 'Demos', icon: 'demo', run: go('demos') },
    { label: 'Studio', icon: 'hack', run: go('hack') },
    { label: 'Dial-Up Networking', icon: 'phone', run: () => dialUp() },
    { label: 'Notepad', icon: 'text', run: () => notePad() },
  ] },
  { label: 'Documents', icon: 'folder', items: [], recent: true }, // (filled as the menu opens: what was played last)
  { label: 'Settings', icon: 'settings', run: () => $('#open-settings').click() },
  { label: 'Find', icon: 'find', run: () => find('') },
  { label: 'Help', icon: 'help', run: () => help() },
  { label: 'Run…', icon: 'run', run: () => runBox() },
  { sep: true },
  { label: 'Shut Down…', icon: 'off', run: () => shutDown() },
];

const MAC_MENU = [
  { label: 'About breadbin\u2026', icon: 'toast', run: () => about() },
  { sep: true },
  { label: 'Control Panel', icon: 'settings', run: () => $('#open-settings').click() },
  { label: 'Find\u2026', icon: 'find', run: () => find('') },
  { label: 'Note Pad', icon: 'text', run: () => notePad() },
  { label: 'Puzzle', icon: 'puzzle', run: () => puzzle() },
  { sep: true },
  { label: 'Shut Down', icon: 'off', run: () => safeToTurnOff() },
];

/** GEOS's first menu, as its desktop had it: the info box, the desk accessories, and BASIC - out to the C64's own screen. */
const GEOS_MENU = [
  { label: 'breadbin info', icon: 'toast', run: () => about() },
  { sep: true },
  { label: 'preferences', icon: 'settings', run: () => $('#open-settings').click() },
  { label: 'find', icon: 'find', run: () => find('') },
  { label: 'note pad', icon: 'text', run: () => notePad() },
  { label: 'puzzle', icon: 'puzzle', run: () => puzzle() },
  { sep: true },
  { label: 'BASIC', icon: 'off', run: () => save({ theme: applyTheme('classic') }) },
];
const DROPDOWNS = { cupertino: MAC_MENU, geos: GEOS_MENU };

let menu = null;
let desk = null;
let kind = '';

function item(entry) {
  if (entry.sep) return h('li.start-sep', { role: 'separator' });
  const btn = h('button.start-item', { type: 'button', role: 'menuitem', 'aria-haspopup': entry.items ? 'menu' : null }, icon(entry.icon, entry.small ? 16 : 24), h('span', {}, entry.label));
  const li = h('li', { role: 'none' }, btn);
  btn.addEventListener('pointerenter', (e) => e.pointerType === 'mouse' && btn.focus({ preventScroll: true })); // (the highlight follows the mouse, as in any menu)
  if (entry.items) {
    const sub = h('ul.start-sub', { role: 'menu' }, entry.items.map((e) => item({ ...e, small: true })));
    li.append(sub);
    li.classList.add('has-sub');
    if (entry.recent) li.classList.add('recent');
    // a mouse opens it by pointing (a click keeps it open); a finger or a key, by a tap or Enter, and again to close
    btn.addEventListener('click', (e) => (e.pointerType === 'mouse' ? li.classList.add('open') : li.classList.toggle('open')));
    li.addEventListener('pointerenter', (e) => e.pointerType === 'mouse' && li.classList.add('open'));
    li.addEventListener('pointerleave', (e) => e.pointerType === 'mouse' && li.classList.remove('open'));
  } else {
    btn.addEventListener('click', () => { closeMenu(); entry.run(); });
  }
  return li;
}

function buildMenu() {
  const el = DROPDOWNS[kind]
    ? h('div.start-menu', { hidden: true }, h('ul', { role: 'menu', 'aria-label': 'breadbin' }, DROPDOWNS[kind].map((e) => item({ ...e, small: true }))))
    : h('div.start-menu', { hidden: true },
      h('div.start-banner', { 'aria-hidden': 'true' }, h('b', {}, 'breadbin'), '95'),
      h('ul', { role: 'menu', 'aria-label': 'Start' }, MENU.map(item)));
  // the focus gone elsewhere (Tab), the menu goes too
  el.addEventListener('focusout', (e) => e.relatedTarget && !el.contains(e.relatedTarget) && !e.relatedTarget.closest('.brand') && closeMenu());
  return el;
}

/** "Click here to begin": once a visit, over Start, until it's used (or a while has passed). */
function hint() {
  if (kind !== 'chicago' || sessionStorage.getItem('breadbin.begun')) return; // (gone to another look meanwhile, or Start already found)
  const brand = $('.brand').getBoundingClientRect();
  const tip = h('div.start-hint', { role: 'note', style: { left: brand.left + 'px', bottom: innerHeight - brand.top + 6 + 'px' } }, 'Click here to begin.');
  document.body.append(tip);
  setTimeout(() => tip.remove(), 12e3);
}

/** Documents: the games and demos played last (store.js keeps them), each opening again; "(Empty)" before any. */
function recentDocuments() {
  const sub = menu.querySelector('.recent .start-sub');
  if (!sub) return;
  const played = store.load('played', []).slice(0, 8);
  sub.replaceChildren(...(played.length
    ? played.map((p) => item({ label: p.item?.title || p.id, icon: p.kind === 'demo' ? 'demo' : 'computer', small: true, run: go(`${p.kind}s/${encodeURIComponent(p.id)}`) }))
    : [h('li', { role: 'none' }, h('button.start-item', { type: 'button', role: 'menuitem', disabled: true }, h('span', {}, '(Empty)')))]));
}

function openMenu() {
  sessionStorage.setItem('breadbin.begun', '1');
  recentDocuments();
  document.querySelector('.start-hint')?.remove();
  const bar = $('.case').getBoundingClientRect();
  const brand = $('.brand').getBoundingClientRect();
  if (DROPDOWNS[kind]) Object.assign(menu.style, { top: brand.bottom - 1 + 'px', left: brand.left + 'px' }); // (down from its title in the menu bar)
  else menu.style.bottom = innerHeight - bar.top + 'px'; // (just above the taskbar, one row or two)
  menu.hidden = false;
  $('.brand').setAttribute('aria-expanded', 'true');
  menu.querySelector('.start-item').focus();
}
function closeMenu() {
  if (!menu || menu.hidden) return;
  menu.hidden = true;
  for (const li of menu.querySelectorAll('.open')) li.classList.remove('open');
  $('.brand')?.setAttribute('aria-expanded', 'false');
}
const toggleMenu = () => (menu.hidden ? openMenu() : closeMenu());

/** The arrows move through the menu, Right and Left in and out of a submenu, Esc closes. */
const MENU_KEYS = {
  Escape: () => { closeMenu(); $('.brand').focus(); },
  ArrowDown: (li) => step(li, 1),
  ArrowUp: (li) => step(li, -1),
  ArrowRight: (li) => {
    if (!li?.classList.contains('has-sub')) return;
    li.classList.add('open');
    li.querySelector('.start-sub .start-item').focus();
  },
  ArrowLeft: (li) => {
    const parent = li?.parentElement.closest('.has-sub');
    if (!parent) return;
    parent.classList.remove('open');
    parent.querySelector('.start-item').focus();
  },
};
/** The focus to the item `by` places on from `li`'s, round its list. */
function step(li, by) {
  const list = [...(li?.parentElement.children || [])].map((x) => x.querySelector(':scope > .start-item')).filter(Boolean);
  const i = list.indexOf(document.activeElement);
  list[(i + by + list.length) % list.length]?.focus();
}
function menuKeys(e) {
  if (menu.hidden || !MENU_KEYS[e.key]) return;
  MENU_KEYS[e.key](document.activeElement?.closest('li'));
  e.preventDefault();
}

/** The shelf windows' title-bar buttons, drawn by the looks' sheets, do what they show. Chicago's, at the bar's right
 *  end: _ rolls the window up to its bar (and down again), □ shows all of it or less, × rolls it up too. */
// ── closing windows: a shelf (or the Studio) goes away, its taskbar button (or desk icon) brings it back ──

/** Closes a window on the desk: hidden (class win-closed) until its view is opened again. */
function closeWindow(win) {
  win.classList.add('win-closed');
  win.classList.remove('rolled');
  // its view's last window: the view goes too (its notes and credits with it), leaving the bare desk
  const view = win.closest('.view');
  if (view && [...view.querySelectorAll('.shelf')].every((s) => s.classList.contains('win-closed'))) view.classList.add('win-closed');
  syncTabs();
}
/** Opens every closed window again: in the view now showing, or (all) everywhere. */
function reopen(all) {
  const scope = all ? document : document.querySelector('.view:not([hidden])');
  scope?.querySelectorAll('.win-closed').forEach((w) => w.classList.remove('win-closed'));
  if (scope?.classList?.contains('win-closed')) scope.classList.remove('win-closed');
  syncTabs();
}
/** A view whose windows are all closed: its taskbar button stands up again (data-closed), as a closed window's would. */
function syncTabs() {
  for (const tab of document.querySelectorAll('.tab[data-tab]')) {
    const view = document.getElementById(tab.dataset.tab);
    const wins = view ? [...view.querySelectorAll('.shelf')] : [];
    const closed = view && (view.classList.contains('win-closed') || (wins.length > 0 && wins.every((w) => w.classList.contains('win-closed'))));
    tab.toggleAttribute('data-closed', !!closed);
  }
}

/** The Studio's title bar is its toolbar's ::before: the top row of .studio-bar, its buttons at the right end. */
function studioButtons(e) {
  const bar = e.target;
  const box = bar.getBoundingClientRect();
  const x = box.right - 3 - e.clientX;
  if (e.clientY - box.top > 22 || x < 0 || x > 50) return false;
  if (x < 16 || x >= 34) closeWindow(bar.closest('.view')); // × or _: the Studio goes down to its taskbar button
  return true;
}

/** The buttons drawn on a window's title bar (a picture, so the clicks are placed by where they land). */
function shelfButtons(e) {
  if (kind === 'chicago' && e.target.classList?.contains('studio-bar')) return studioButtons(e);
  const head = e.target.closest('.shelf-head');
  if (!head || e.target.closest('button')) return false;
  const box = head.getBoundingClientRect();
  const shelf = head.closest('.shelf');
  if (kind === 'cupertino') {
    // a double click on the bar rolls the window up to it (windowshade); the close box at the left end closes it
    if (e.detail === 2) shelf.classList.toggle('rolled');
    else if (e.clientX - box.left >= 4 && e.clientX - box.left <= 24) closeWindow(shelf);
    else return false;
    return true;
  }
  const x = box.right - 3 - e.clientX; // from the right end of the buttons (the bar's padding is 3px)
  if (x < 0 || x > 50) return false;
  if (x < 16 || x >= 34) closeWindow(shelf); // × (the last 16px) closes, _ (the first) minimises: both leave the desk
  else maxButton(shelf);
  return true;
}
/** □: a moved window back where it was; else Show All / Show Less. */
const maxButton = (shelf) => (shelf.style.translate ? restoreWindow(shelf) : shelf.querySelector('.shelf-head .key.pill')?.click());

// ── windows that move: a shelf dragged by its title bar (Chicago, Cupertino, GEOS) ──
// It moves by translate within its view (whose sideways scrolling is off while one has moved: style.css), comes to the
// front (z-index), and keeps a piece of its bar on the view so it can always be got back. A double click on the bar
// (Chicago, GEOS), or □, puts it back; Cupertino's double click stays its windowshade.

let topZ = 1;
function dragWindow(e) {
  const head = e.target.closest?.('.shelf-head');
  if (!head || e.button !== 0 || e.pointerType === 'touch' || e.target.closest('button, a, input, select')) return;
  const win = head.closest('.shelf');
  const view = win.closest('.view');
  if (!view) return;
  const [x0, y0] = (win.style.translate || '0px 0px').split(' ').map(parseFloat);
  const box = win.getBoundingClientRect();
  const port = view.getBoundingClientRect();
  const start = [e.clientX, e.clientY];
  let moved = false;
  const move = (m) => {
    const dx = m.clientX - start[0];
    const dy = m.clientY - start[1];
    if (!moved && Math.hypot(dx, dy) < 5) return;
    if (!moved) {
      moved = true;
      win.classList.add('moved');
      win.style.zIndex = ++topZ;
    }
    // (80px of its bar stays on the view across, the bar itself below the view's top)
    const x = Math.min(Math.max(dx, port.left + 80 - box.right), port.right - 80 - box.left);
    const y = Math.max(dy, port.top - box.top);
    win.style.translate = `${x0 + x}px ${y0 + y}px`;
  };
  const up = () => {
    head.removeEventListener('pointermove', move);
    head.removeEventListener('pointerup', up);
    head.removeEventListener('pointercancel', up);
    if (moved) addEventListener('click', (c) => { c.stopImmediatePropagation(); c.preventDefault(); }, { capture: true, once: true }); // (let go over ×: nothing closes)
  };
  head.setPointerCapture(e.pointerId);
  head.addEventListener('pointermove', move);
  head.addEventListener('pointerup', up);
  head.addEventListener('pointercancel', up);
}
function restoreWindow(win) {
  win.style.translate = win.style.zIndex = '';
  win.classList.remove('moved');
}
function onDblClick(e) {
  const head = kind !== 'cupertino' && !e.target.closest('button') && e.target.closest('.shelf-head');
  if (head) restoreWindow(head.closest('.shelf'));
}

function onClick(e) {
  if (!e.target.closest('.desk-menu')) closeDeskMenu();
  if (!e.target.closest('.volume-pop, .tray-volume')) closeVolume();
  // another task on the taskbar, or the desk: the player goes down to its button
  if (kind === 'chicago' && !$('#emu').hidden && e.target.closest('.tab:not(.task-emu), .desk-icon')) minimize(true);
  if (shelfButtons(e)) return;
  // a view's taskbar button, its desk icon or the menu's way to it: its closed windows come back
  if (e.target.closest('.tab, .desk-icon, .start-item')) setTimeout(reopen);
  if (e.target.closest('.brand')) {
    e.preventDefault(); // (Start is a menu here, not the way to Games)
    return toggleMenu();
  }
  if (!e.target.closest('.start-menu')) closeMenu();
}
function onKey(e) {
  if (e.key === 'Escape') closeDeskMenu(), closeVolume();
  if (e.key === 'Escape' && e.ctrlKey && kind === 'chicago') { e.preventDefault(); return toggleMenu(); } // the keyboard's way to Start
  menuKeys(e);
}

// ── the desk: icons to open things with ─────────────────────────────────────

const DESK = [
  { label: 'My Breadbin', icon: 'computer', run: go('games') },
  { label: 'Tunes', icon: 'note', run: go('tunes') },
  { label: 'Demos', icon: 'demo', run: go('demos') },
  { label: 'Studio', icon: 'hack', run: go('hack') },
  { label: 'Dial-Up Networking', icon: 'phone', run: () => dialUp() },
  { label: 'readme.txt', icon: 'text', run: () => readMe() },
  { label: 'Recycle Bin', icon: 'bin', run: () => recycleBin() },
];

const MAC_DESK = [
  { label: 'breadbin', icon: 'disk', run: go('games') },
  { label: 'Read Me', icon: 'text', run: () => readMe() },
  { label: 'Trash', icon: 'trash', run: () => trash() },
];

/** A right click on the bare desk: its menu where the pointer is (Arrange Icons, Refresh, Properties). */
function deskMenu(e) {
  if (kind !== 'chicago' || !(e.target.matches('.desk-icons, .view') || e.target.closest('.desk-icons') === e.target)) return;
  e.preventDefault();
  closeDeskMenu();
  const entries = [
    { label: 'Arrange Icons', run: arrangeIcons },
    { label: 'Refresh', run: () => location.reload() },
    { sep: true },
    { label: 'Properties', run: () => $('#open-settings').click() },
  ];
  const ctx = h('div.start-menu.desk-menu', { style: { left: e.clientX + 'px', top: e.clientY + 'px', bottom: 'auto' } },
    h('ul', { role: 'menu', 'aria-label': 'Desktop' }, entries.map((x) => (x.sep ? h('li.start-sep', { role: 'separator' })
      : h('li', { role: 'none' }, h('button.start-item', { type: 'button', role: 'menuitem', onclick: () => { closeDeskMenu(); x.run(); } }, h('span', {}, x.label)))))));
  document.body.append(ctx);
  // kept on the screen
  const r = ctx.getBoundingClientRect();
  if (r.right > innerWidth) ctx.style.left = innerWidth - r.width - 2 + 'px';
  if (r.bottom > innerHeight) ctx.style.top = innerHeight - r.height - 2 + 'px';
  ctx.querySelector('.start-item').focus();
}
const closeDeskMenu = () => document.querySelector('.desk-menu')?.remove();

function buildDesk() {
  // one click chooses an icon, two open it (one tap on a touch screen); Enter opens the chosen one; in Chicago, a mouse drags it
  return h('nav.desk-icons', { 'aria-label': 'Desktop' }, (kind === 'cupertino' ? MAC_DESK : DESK).map((d) => h('button.desk-icon', {
    type: 'button',
    'data-label': d.label,
    onclick: (e) => { e.currentTarget.focus(); if (matchMedia('(pointer: coarse)').matches) d.run(); },
    ondblclick: d.run,
    onkeydown: (e) => e.key === 'Enter' && (e.preventDefault(), d.run()),
    onpointerdown: kind === 'chicago' ? dragIcon : null,
  }, icon(d.icon), h('span', {}, d.label))));
}

// ── Chicago's icons where you put them: a grid of cells down the left of the desk ──
// Each icon keeps a cell (column, row), saved by its label in store 'desk'. The windows keep clear of the columns in use,
// so an icon is never hidden behind one: dragged onto a window's edge, an icon starts a new column and the windows move over.

const CELL_W = 84; // (an icon's width: its label wraps inside it)
const CELL_H = 70; // (a picture and two lines of label)
const DESK_PAD = 8;
const MAX_COLS = () => Math.max(1, Math.min(4, Math.floor((innerWidth - 700) / CELL_W))); // (the windows keep 700px or so)
const deskRows = () => Math.max(1, Math.floor((desk.clientHeight - DESK_PAD) / CELL_H));

/** Every icon to its cell: the saved one if it's on the screen and free, else the first free cell (down, then across). */
function layoutDesk() {
  if (kind !== 'chicago' || !desk) return;
  const rows = deskRows();
  const cols = MAX_COLS();
  const saved = store.load('desk', {});
  const taken = new Map(); // "c,r" → icon
  const cells = new Map(); // icon → [c, r]
  const icons = [...desk.querySelectorAll('.desk-icon')];
  for (const el of icons) {
    const at = saved[el.dataset.label];
    if (at && at[0] < cols && at[1] < rows && !taken.has(at + '')) taken.set(at + '', el), cells.set(el, at);
  }
  for (const el of icons.filter((x) => !cells.has(x))) {
    for (let i = 0; i < rows * cols; i++) {
      const at = [Math.floor(i / rows), i % rows];
      if (!taken.has(at + '')) { taken.set(at + '', el); cells.set(el, at); break; }
    }
  }
  let used = 1;
  for (const [el, [c, r]] of cells) {
    el.style.left = DESK_PAD / 2 + c * CELL_W + 'px';
    el.style.top = DESK_PAD + r * CELL_H + 'px';
    used = Math.max(used, c + 1);
  }
  // the windows start after the last column in use
  $('main').style.setProperty('--desk-w', used * CELL_W + DESK_PAD + 'px');
  return cells;
}

/** Right click ▸ Arrange Icons: by name, down the first column. */
function arrangeIcons() {
  const names = [...desk.querySelectorAll('.desk-icon')].map((el) => el.dataset.label).sort((a, b) => a.localeCompare(b));
  const rows = deskRows();
  store.save('desk', Object.fromEntries(names.map((n, i) => [n, [Math.floor(i / rows), i % rows]])));
  layoutDesk();
}

/** A mouse's press on an icon: past a few pixels it's a drag, the icon following the pointer; let go, it takes the cell
 *  it's over (its occupant swapping into the cell it left). A press that doesn't move stays a click. */
function dragIcon(e) {
  if (e.button !== 0 || e.pointerType === 'touch' || getComputedStyle(e.currentTarget).position !== 'absolute') return;
  const el = e.currentTarget;
  const box = desk.getBoundingClientRect();
  const from = { x: e.clientX, y: e.clientY, left: el.offsetLeft, top: el.offsetTop };
  let moved = false;
  const move = (m) => {
    const dx = m.clientX - from.x;
    const dy = m.clientY - from.y;
    if (!moved && Math.hypot(dx, dy) < 5) return;
    if (!moved) { moved = true; el.classList.add('dragging'); desk.classList.add('dragging'); }
    // (kept on the desk: not under the taskbar, not off its edges)
    el.style.left = Math.max(0, Math.min(innerWidth - box.left - CELL_W, from.left + dx)) + 'px';
    el.style.top = Math.max(0, Math.min(box.height - CELL_H, from.top + dy)) + 'px';
  };
  const up = () => {
    el.removeEventListener('pointermove', move);
    el.removeEventListener('pointerup', up);
    el.removeEventListener('pointercancel', up);
    if (!moved) return;
    el.classList.remove('dragging');
    desk.classList.remove('dragging');
    // the click that ends a drag opens nothing
    el.addEventListener('click', (c) => { c.stopImmediatePropagation(); c.preventDefault(); }, { capture: true, once: true });
    drop(el);
  };
  el.setPointerCapture(e.pointerId);
  el.addEventListener('pointermove', move);
  el.addEventListener('pointerup', up);
  el.addEventListener('pointercancel', up);
}

/** The dragged icon to the cell nearest where it was let go (no further right than one new column). */
function drop(el) {
  const at = { left: el.offsetLeft, top: el.offsetTop }; // (where it was let go, before the layout puts it back)
  const cells = layoutDesk() || new Map();
  const used = Math.max(...[...cells].filter(([x]) => x !== el).map(([, [c]]) => c + 1), 1);
  const c = Math.max(0, Math.min(MAX_COLS() - 1, used, Math.round((at.left - DESK_PAD / 2) / CELL_W)));
  const r = Math.max(0, Math.min(deskRows() - 1, Math.round((at.top - DESK_PAD) / CELL_H)));
  const saved = Object.fromEntries([...cells].map(([x, at]) => [x.dataset.label, at]));
  const other = [...cells].find(([x, [oc, or]]) => x !== el && oc === c && or === r)?.[0];
  if (other) saved[other.dataset.label] = cells.get(el); // (swapped into the cell the dragged one left)
  saved[el.dataset.label] = [c, r];
  store.save('desk', saved);
  layoutDesk();
  el.focus();
}

function find(term) {
  if (location.hash.startsWith('#hack')) location.hash = '#games'; // (the Studio has no search; Tunes filters its own)
  const s = $('#search');
  s.focus();
  if (term) { s.value = term; s.dispatchEvent(new Event('input')); }
}

function runBox() {
  const input = h('input', { type: 'text', value: '', spellcheck: false, autocomplete: 'off', 'aria-label': 'Open' });
  const look = (id) => () => save({ theme: applyTheme(id) });
  const PROGRAMS = {
    games: go('games'), tunes: go('tunes'), demos: go('demos'), hack: go('hack'), studio: go('hack'),
    rasphone: dialUp, dialup: dialUp, shutdown: shutDown, help: help, winhelp: help,
    bakery: look('bakery'), classic: look('classic'), c64: look('classic'), geos: look('geos'), cupertino: look('cupertino'),
  };
  const run = (dlg) => {
    const what = input.value.trim();
    dlg.close();
    if (!what) return;
    const key = what.toLowerCase().replace(/\.exe$/, '').replace(/[^a-z]/g, '');
    if (PROGRAMS[key]) PROGRAMS[key]();
    else find(what); // anything else: looked for among the games
  };
  const dlg = win('Run', h('div.win-row', {}, icon('run'), h('div', {},
    h('p', {}, 'Type the name of a program, a game or a look, and breadbin will open it for you.'),
    h('label.win-field', {}, h('span', {}, 'Open:'), input))),
  [['OK', run, true], ['Cancel', (d) => d.close()]], '.run-box');
  input.focus();
  return dlg;
}

function help() {
  const KEYS = [
    ['/', 'Search the games, demos or tunes'],
    ['Ctrl+Esc', 'The Start menu; the arrows and Enter in it'],
    ['Enter', 'Open the chosen icon on the desk'],
    ['Esc', 'Close a window, the menu, the player\u2019s Tools'],
    ['F1', 'The Studio\u2019s cheat sheet: every call, with an example'],
    ['Ctrl+Enter', 'Build the Studio\u2019s game and run it'],
    ['\u0060', 'The Studio\u2019s console, from the top of the screen'],
  ];
  win('breadbin Help', h('div.win-list', {}, h('table.help-keys', {}, KEYS.map(([k, what]) => h('tr', {}, h('td', {}, h('kbd', {}, k)), h('td', {}, what))))),
    [['Close', (d) => d.close(), true]], '.help');
}

/** What breadbin is and how to get about it, as the text file on the desk of the time said it. */
const README = `breadbin
========

A Commodore 64 in your browser, on free ROMs, with the scene's own
archives on its shelves: games, demos and SID music.

  Games, Demos   pick one and it starts. 🕹 Pad gives a phone a stick.
  Tunes          the SID chip plays them; Radio plays one after another.
  Hack           the Studio: make a C64 game in C, with sprites, maps
                 and music, and play it beside the code.

Keys
  /              search
  Esc            close a window
  F1             the Studio's cheat sheet

Looks: Settings (\u2630) has five, this is one of them.
Nothing is kept anywhere but in this browser.`;

function readMe() {
  const title = kind === 'cupertino' ? 'Read Me' : 'readme.txt - Notepad';
  win(title, h('textarea.readme', { readOnly: true, spellcheck: false, rows: 18, 'aria-label': 'Read me' }, README), [['Close', (d) => d.close(), true]], '.readme-win');
}

/** A note pad for what a game wants remembered (level codes, passwords, the way through a maze), kept in this browser as it is typed. */
function notePad() {
  const title = { chicago: 'notes.txt - Notepad', cupertino: 'Note Pad', geos: 'note pad' }[kind];
  let timer = 0;
  const text = h('textarea.readme.notepad', {
    rows: 14, spellcheck: false, 'aria-label': 'Notes', placeholder: 'Level codes, passwords, the way through a maze\u2026',
    oninput: () => { clearTimeout(timer); timer = setTimeout(() => store.save('notes', text.value), 300); },
  }, store.load('notes', ''));
  const dlg = win(title, text, [['Close', (d) => d.close(), true]], '.readme-win');
  dlg.addEventListener('close', () => { clearTimeout(timer); store.save('notes', text.value); });
  text.focus();
}

function recycleBin() {
  win('Recycle Bin', h('div.win-list', {}, h('p', {}, 'This folder is empty.'), h('p.win-hint', {}, '(Nothing on a breadbin is ever thrown away: it is all on the Internet Archive.)')), [['Close', (d) => d.close(), true]]);
}

// ── Shut Down ───────────────────────────────────────────────────────────────

function shutDown() {
  const choice = (value, label, checked) => h('label.check', {}, h('input', { type: 'radio', name: 'shutdown', value, checked }), label);
  const yes = (dlg) => {
    const how = dlg.querySelector('input:checked').value;
    dlg.close();
    if (how === 'restart') location.reload();
    else if (how === 'c64') save({ theme: applyTheme('classic') });
    else safeToTurnOff();
  };
  win('Shut Down breadbin', h('div.win-row', {}, icon('off'), h('div', {},
    h('p', {}, 'Are you sure you want to:'),
    choice('off', 'Shut down the computer?', true),
    choice('restart', 'Restart the computer?'),
    choice('c64', 'Restart the computer in C64 mode?'))),
  [['Yes', yes, true], ['No', (d) => d.close()]], '.shutdown');
}

/** The last screen of the day: orange on black (Chicago), or a box on the grey desk (Cupertino), until a click or a key turns it back on. */
function safeToTurnOff() {
  const words = kind === 'cupertino' ? ['You may now switch off', 'your breadbin safely.'] : ['It\u2019s now safe to turn off', 'your computer.'];
  const p = h('p', {}, words[0], h('br'), words[1]);
  const screen = h('div.safe-off' + (kind === 'cupertino' ? '.mac' : ''), { role: 'dialog', 'aria-label': 'Shut down', tabindex: '-1' },
    kind === 'cupertino' ? h('div', {}, icon('toast'), p) : p,
    h('small', {}, 'Click or press a key to turn it back on.'));
  const back = () => { screen.remove(); removeEventListener('keydown', back, true); };
  screen.addEventListener('click', back);
  setTimeout(() => addEventListener('keydown', back, true), 300);
  document.body.append(screen);
  screen.focus();
}

// ── Cupertino's desk accessories ────────────────────────────────────────────

function about() {
  const used = 26; // K of the 64: what breadbin's page takes, more or less
  win(kind === 'geos' ? 'breadbin info' : 'About breadbin', h('div.win-row', {}, icon('toast'), h('div', {},
    h('p', {}, h('b', {}, 'breadbin'), ' \u2014 a C64 in your browser'),
    h('p.win-hint', {}, 'Free ROMs, the scene\u2019s archives, and a Studio to make games in.'),
    h('div.about-mem', { 'aria-label': `${used}K of 64K in use` },
      h('span', {}, 'Total memory: 64K'),
      h('span', {}, 'Largest unused block: 38,911 bytes'),
      h('i', { style: `--used: ${used / 64}` })))),
  [['OK', (d) => d.close(), true]], '.about');
}

/** The fifteen tiles in a frame of sixteen: slide one into the gap; in order, it's solved. */
function puzzle() {
  const tiles = [...Array(15).keys()].map((n) => n + 1).concat(0);
  // shuffled by sliding, as a hand would, so it can always be solved
  let gap = 15;
  for (let i = 0; i < 200; i++) {
    const moves = neighbours(gap);
    const to = moves[Math.floor(Math.random() * moves.length)];
    [tiles[gap], tiles[to]] = [tiles[to], tiles[gap]];
    gap = to;
  }
  const grid = h('div.puzzle', { role: 'grid', 'aria-label': 'Puzzle' });
  const draw = () => grid.replaceChildren(...tiles.map((n, i) => h('button.puzzle-tile', {
    type: 'button', disabled: !n, 'aria-label': n ? String(n) : 'gap',
    onclick: () => slide(i),
  }, n ? String(n) : '')));
  const slide = (i) => {
    if (!neighbours(i).includes(gap)) return;
    const was = gap;
    [tiles[gap], tiles[i]] = [tiles[i], tiles[gap]];
    gap = i;
    draw();
    grid.querySelectorAll('.puzzle-tile')[was]?.focus(); // (the tile that moved keeps the focus, for the next key)
    if (tiles.every((n, k) => n === (k + 1) % 16)) grid.classList.add('solved');
  };
  // the arrows slide the tile on that side of the gap into it
  const ARROWS = { ArrowLeft: 1, ArrowRight: -1, ArrowUp: 4, ArrowDown: -4 };
  grid.addEventListener('keydown', (e) => {
    const from = gap + (ARROWS[e.key] ?? NaN);
    if (!neighbours(gap).includes(from)) return;
    e.preventDefault();
    slide(from);
  });
  draw();
  win(kind === 'geos' ? 'puzzle' : 'Puzzle', grid, [], '.puzzle-win');
  grid.querySelector('.puzzle-tile:not(:disabled)')?.focus();
}
const neighbours = (i) => [i - 4, i + 4, i % 4 ? i - 1 : -1, i % 4 < 3 ? i + 1 : -1].filter((j) => j >= 0 && j < 16);

function trash() {
  win('Trash', h('div.win-row', {}, icon('trash'), h('div', {}, h('p', {}, 'The Trash is empty.'), h('p.win-hint', {}, 'Nothing on a breadbin is ever thrown away: it is all on the Internet Archive.'))),
    [['OK', (d) => d.close(), true]]);
}


// ── on and off with the look ────────────────────────────────────────────────

/** The desktop on, as `look` has it ('chicago', 'cupertino' or 'geos'): the menu works, the icons are on the desk. */
export function start(look = 'chicago') {
  if (menu && kind === look) return;
  if (menu) stop();
  kind = look;
  menu = buildMenu();
  document.body.append(menu);
  if (kind !== 'geos') $('main').prepend((desk = buildDesk()));
  if (kind === 'chicago') layoutDesk(), addEventListener('resize', layoutDesk);
  $('.brand').setAttribute('aria-haspopup', 'menu');
  $('.brand').setAttribute('aria-expanded', 'false');
  document.addEventListener('click', onClick, true);
  document.addEventListener('contextmenu', deskMenu);
  document.addEventListener('pointerdown', dragWindow);
  document.addEventListener('dblclick', onDblClick);
  addEventListener('keydown', onKey);
  addEventListener('hashchange', closeMenu);
  addEventListener('hashchange', onRoute);
  if (kind === 'chicago') tabIcons(true), taskbar(true), volumeTray(true);
  if (kind === 'chicago' && !matchMedia('(width < 600px)').matches) setTimeout(hint, 1500); // (a phone's taskbar has no room over Start)
}

/** The taskbar's buttons with the desk's own pictures, 16 pixels square, instead of the glyphs; or back. */
const TAB_ICONS = { games: 'computer', tunes: 'note', demos: 'demo', hack: 'hack' };
function tabIcons(on) {
  for (const el of document.querySelectorAll('.tab .tab-icon')) {
    const tab = el.closest('.tab').dataset.tab;
    if (on && !el.dataset.glyph) {
      el.dataset.glyph = el.textContent;
      el.replaceChildren(icon(TAB_ICONS[tab], 16));
    } else if (!on && el.dataset.glyph) {
      el.textContent = el.dataset.glyph;
      delete el.dataset.glyph;
    }
  }
}

// ── the player as a window on the taskbar (Chicago) ─────────────────────────

let taskWatch = null;
let taskTitle = '';
/** While a game runs, the taskbar keeps a pressed button for it: a click minimises the player (the desk again), another brings it back. */
function taskbar(on) {
  taskWatch?.disconnect();
  taskWatch = null;
  if (!on) {
    document.querySelector('.task-emu')?.remove();
    minimize(false);
    document.documentElement.style.removeProperty('--taskbar');
    removeEventListener('resize', barHeight);
    return;
  }
  taskWatch = new MutationObserver(taskButton);
  taskWatch.observe($('#emu'), { attributes: true, attributeFilter: ['hidden'] });
  taskWatch.observe($('#emu-title'), { childList: true, characterData: true, subtree: true });
  taskButton();
  barHeight();
  addEventListener('resize', barHeight);
}
/** The player's button on the taskbar, as the player is: there or not, its title, pressed unless minimised. */
function taskButton() {
  const emu = $('#emu');
  let btn = document.querySelector('.task-emu');
  if (emu.hidden) {
    btn?.remove();
    minimize(false);
    return;
  }
  const title = $('#emu-title').textContent || 'C64';
  if (title !== taskTitle) minimize(false); // (another game: it comes up)
  taskTitle = title;
  if (!btn) {
    btn = h('button.key.tab.task-emu', { type: 'button', onclick: () => minimize(!emu.classList.contains('minimized')) }, h('span.tab-icon', {}, icon('computer', 16)), h('span.task-label'));
    $('.tabs').append(btn);
  }
  btn.querySelector('.task-label').textContent = title;
  btn.setAttribute('aria-pressed', String(!emu.classList.contains('minimized')));
}
/** The player down to its button (or up again). */
function minimize(min) {
  const emu = $('#emu');
  if (emu.classList.contains('minimized') === min) return;
  emu.classList.toggle('minimized', min);
  document.querySelector('.task-emu')?.setAttribute('aria-pressed', String(!min));
}
// ── the tray's speaker: the volume, as the taskbar of the time let it be set ─

let volumePop = null;
let unmuted = 0.9;
function volumeTray(on) {
  document.querySelector('.tray-volume')?.remove();
  closeVolume();
  if (!on) return;
  const btn = h('button.tray-volume', { type: 'button', title: 'Volume', 'aria-label': 'Volume', onclick: toggleVolume }, icon('speaker', 16));
  document.querySelector('.theme-clock')?.before(btn);
}
/** A slider and Mute in a little window over the tray; they change breadbin's own volume (Settings has the same). */
function toggleVolume() {
  if (volumePop) return closeVolume();
  const setVolume = (v) => {
    save({ volume: v });
    dispatchEvent(new Event('breadbin:sound'));
  };
  const range = h('input', { type: 'range', min: 0, max: 1.4, step: 0.05, value: settings.volume, 'aria-label': 'Volume', oninput: () => { setVolume(Number(range.value)); mute.checked = !Number(range.value); } });
  const mute = h('input', { type: 'checkbox', checked: !settings.volume, onchange: () => {
    if (mute.checked) unmuted = settings.volume || unmuted;
    range.value = mute.checked ? 0 : unmuted;
    setVolume(Number(range.value));
  } });
  volumePop = h('div.volume-pop', { role: 'dialog', 'aria-label': 'Volume' }, h('b', {}, 'Volume'), range, h('label.check', {}, mute, 'Mute'));
  document.body.append(volumePop);
  const btn = document.querySelector('.tray-volume').getBoundingClientRect();
  Object.assign(volumePop.style, { right: Math.max(2, innerWidth - btn.right - 20) + 'px', bottom: innerHeight - $('.case').getBoundingClientRect().top + 2 + 'px' });
  range.focus();
}
function closeVolume() {
  volumePop?.remove();
  volumePop = null;
}

/** The taskbar's height, for the player to stop above it. */
const barHeight = () => document.documentElement.style.setProperty('--taskbar', $('.case').offsetHeight + 'px');

/** And off: Start goes back to being the way to Games. */
export function stop() {
  if (!menu) return;
  menu.remove();
  desk?.remove();
  tabIcons(false);
  taskbar(false);
  volumeTray(false);
  document.querySelector('.start-hint')?.remove();
  menu = desk = null;
  removeEventListener('resize', layoutDesk);
  $('main').style.removeProperty('--desk-w');
  kind = '';
  hush();
  $('.brand').removeAttribute('aria-haspopup');
  $('.brand').removeAttribute('aria-expanded');
  document.removeEventListener('click', onClick, true);
  document.removeEventListener('contextmenu', deskMenu);
  document.removeEventListener('pointerdown', dragWindow);
  document.removeEventListener('dblclick', onDblClick);
  document.querySelectorAll('.shelf.moved').forEach(restoreWindow);
  closeDeskMenu();
  removeEventListener('keydown', onKey);
  removeEventListener('hashchange', closeMenu);
  removeEventListener('hashchange', onRoute);
  reopen(true);
  document.querySelectorAll('.shelf.rolled').forEach((s) => s.classList.remove('rolled'));
  document.querySelectorAll('.tab[data-closed]').forEach((t) => t.removeAttribute('data-closed'));
}
function onRoute() { setTimeout(() => { reopen(); layoutDesk(); }); }
