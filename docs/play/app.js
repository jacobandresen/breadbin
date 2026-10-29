// app - breadbin in the browser. Three tabs like the native app: Games and Demos
// are shelves of keycap cards that each load one small Internet Archive search
// when scrolled into view; Tunes is a CSDb-ranked SID jukebox with the demo
// visualisers. Nothing is bundled: every list and file is fetched on demand
// and cached by the browser (store.js).

import * as ia from './ia.js';
import * as csdb from './csdb.js';
import * as hvsc from './hvsc.js';
import * as store from './store.js';
import * as emu from './emu.js';
import { boxartIndex, canon } from './covers.js';
import { pick, prepare } from './engine.js';
import { createPad } from './touch.js';
import { SidPlayer, sidHeader } from './sid.js';
import { Viz, NAMES } from './viz.js';

const $ = (sel, el = document) => el.querySelector(sel);
const CHIPS = ['var(--red)', 'var(--orange)', 'var(--yellow)', 'var(--green)', 'var(--blue)'];
const DEFAULTS = { hq: true, width: 0.35, volume: 0.9, relay: '', viz: 0, pad: matchMedia('(pointer: coarse)').matches };
let settings = { ...DEFAULTS, ...store.load('settings', {}) };

/** Tiny element builder: h('div.card', { onclick }, child, ...). */
function h(tag, attrs = {}, ...children) {
  const [name, ...classes] = tag.split('.');
  const el = document.createElement(name || 'div');
  if (classes.length) el.className = classes.join(' ');
  for (const [k, v] of Object.entries(attrs || {})) {
    if (v == null || v === false) continue;
    if (k.startsWith('on')) el.addEventListener(k.slice(2), v);
    else if (k === 'style' && typeof v === 'object') Object.assign(el.style, v);
    else if (k in el && k !== 'list') el[k] = v;
    else el.setAttribute(k, v);
  }
  el.append(...children.flat().filter((c) => c != null && c !== false));
  return el;
}

function toast(msg, ms = 3500) {
  const t = $('#toast');
  t.textContent = msg;
  t.classList.add('show');
  clearTimeout(toast.timer);
  toast.timer = setTimeout(() => t.classList.remove('show'), ms);
}

const fmt = new Intl.NumberFormat('en');

// ── boot screen ──────────────────────────────────────────────────────────────

let booted = false;
function bootDone() {
  if (booted) return;
  booted = true;
  $('#boot').classList.add('done');
}
setTimeout(bootDone, 3500);

// ── tabs ─────────────────────────────────────────────────────────────────────

const views = { games: initGames, demos: initDemos, tunes: initTunes };
const started = new Set();
let current = '';

function route() {
  const [tab, item] = (location.hash.slice(1) || 'games').split('/');
  const name = tab in views ? tab : 'games';
  current = name;
  for (const v of Object.keys(views)) $(`#${v}`).hidden = v !== name;
  for (const a of document.querySelectorAll('.tab')) a.setAttribute('aria-selected', String(a.dataset.tab === name));
  $('#search').placeholder = { games: 'Search games…', demos: 'Search demos…', tunes: 'Filter tunes…' }[name];
  $('#search').value = searchTerms[name] || '';
  if (!started.has(name)) {
    started.add(name);
    views[name]();
  }
  if (name !== 'games') bootDone();
  if (item && name !== 'tunes') openItem(name, decodeURIComponent(item));
}
addEventListener('hashchange', route);

/** Deep link (#games/<id>, #demos/<id>): open that item's sheet. Ids not in
 *  the verified list still open, which is how the playtests reach new titles. */
async function openItem(tab, id) {
  try {
    const v = await verified();
    if (tab === 'games') {
      await coversReady;
      const known = v.games.find((g) => g.id === id);
      if (known) return showGame(known);
      if (id.startsWith('gb64:')) {
        const gb = id.slice(5);
        return showGame({ id, gb, title: gb.split('/').pop().replace(/_\d+_\d+\.zip$/i, ''), run: new URLSearchParams(location.search).get('run') || '' });
      }
    } else {
      const known = v.demos.find((d) => d.id === id);
      if (known) return showDemo({ ...known, csdbId: known.csdb });
    }
    const m = (await ia.metadata(id)).metadata;
    if (!m) throw new Error('no such item');
    const raw = String(first(m.title) || id).trim();
    const item = {
      id,
      ia: id,
      title: ia.displayTitle(raw),
      group: String(first(m.csdb_group) || first(m.creator) || '').replace(/,\s*$/, ''),
      publisher: String(first(m.creator) || ''),
      year: Number(/\d{4}/.exec(first(m.date) || first(m.csdb_release_date) || '')?.[0]) || 0,
    };
    if (tab === 'demos') showDemo(item);
    else showGame(item);
  } catch (e) {
    toast(`Couldn't open ${id}: ${e.message}`);
  }
}

/** Keep the address bar pointing at the open item, so it can be shared. */
function linkTo(tab, id) {
  history.replaceState(null, '', id ? `#${tab}/${encodeURIComponent(id)}` : `#${tab}`);
}
$('#detail').addEventListener('close', () => linkTo(current));

// ── shelves ──────────────────────────────────────────────────────────────────

let chipIndex = 0;

/**
 * A titled row of cards that loads when scrolled into view and can expand
 * into a paged grid ("Show All").
 *   load(page, size) -> { items, total }
 *   card(item) -> element
 */
function shelf(parent, { title, load, card, size = 24, wide = false, hideEmpty = true }) {
  const chip = h('span.shelf-chip', { style: { background: CHIPS[chipIndex++ % CHIPS.length] } });
  const count = h('span.shelf-count');
  const toggle = h('button.key.pill', { hidden: true }, 'Show All');
  const body = h('div.row');
  const more = h('div.more', { hidden: true }, h('button.key.pill', { onclick: () => fetchPage() }, 'More'));
  const el = h(`section.shelf${wide ? '.wide' : ''}`, {},
    h('header.shelf-head', {}, chip, h('h2.shelf-title', {}, title), h('span.shelf-rule'), count, toggle),
    body, more);
  parent.append(el);

  for (let i = 0; i < 8; i++) body.append(h('div.card.skeleton', {}, h('div.art'), h('div.card-title')));

  let page = 0;
  let total = 0;
  let shown = 0;
  let expanded = false;
  let busy = false;

  async function fetchPage() {
    if (busy) return;
    busy = true;
    more.querySelector('button').disabled = true;
    try {
      const res = await load(++page, size);
      total = res.total;
      if (page === 1) body.replaceChildren();
      for (const item of res.items) body.append(card(item));
      shown += res.items.length;
      if (!shown && hideEmpty) el.remove();
      else if (!shown) body.replaceChildren(h('p.empty', {}, 'Nothing here yet.'));
      count.textContent = total > shown ? `${fmt.format(total)} titles` : '';
      toggle.hidden = total <= Math.min(shown, 8) && !expanded;
      toggle.innerHTML = expanded ? 'Show Less' : `Show All<span class="n"> (${fmt.format(total)})</span>`;
      more.hidden = !expanded || shown >= total;
    } catch (e) {
      body.replaceChildren(h('p.empty', {}, `Couldn't load this shelf (${e.message}).`, ' ',
        h('button.key.pill', { onclick: () => { page = 0; shown = 0; fetchPage(); } }, 'Retry')));
    } finally {
      busy = false;
      more.querySelector('button').disabled = false;
    }
    return el;
  }

  toggle.addEventListener('click', () => {
    expanded = !expanded;
    body.className = expanded ? 'grid' : 'row';
    toggle.innerHTML = expanded ? 'Show Less' : `Show All<span class="n"> (${fmt.format(total)})</span>`;
    more.hidden = !expanded || shown >= total;
    if (expanded && shown < Math.min(total, size * 2)) fetchPage();
    if (!expanded) el.scrollIntoView({ block: 'nearest' });
  });

  const ready = new Promise((resolve) => {
    const io = new IntersectionObserver((entries) => {
      if (entries.some((e) => e.isIntersecting)) {
        io.disconnect();
        fetchPage().then(resolve);
      }
    }, { rootMargin: '400px 0px' });
    io.observe(el);
  });
  return { el, ready };
}

/** Cover art that fades in and falls back down a list of URLs, then text. */
function art(urls, label, { boxart = false, badge } = {}) {
  const wrap = h(`div.art${boxart ? '.boxart' : ''}`);
  const img = h('img', { alt: '', loading: 'lazy', decoding: 'async', referrerPolicy: 'no-referrer' });
  const list = urls.filter(Boolean);
  let i = 0;
  const next = () => {
    if (i < list.length) img.src = list[i++];
    else {
      img.remove();
      wrap.prepend(h('div.placeholder', {}, label));
    }
  };
  img.addEventListener('load', () => {
    // archive.org answers missing thumbnails with a tiny generic icon
    if (img.naturalWidth < 40) next();
    else img.classList.add('loaded');
  });
  img.addEventListener('error', next);
  next();
  wrap.append(img);
  if (badge) wrap.append(h('span.badge', {}, badge));
  return wrap;
}

// ── verified catalogue ───────────────────────────────────────────────────────

// verified.json lists what the playtests (tools/playtest) showed working, best
// rated first, so every card offered here starts and plays.
let verifiedList = null;
function verified() {
  verifiedList ??= fetch(new URL('./verified.json', import.meta.url), { cache: 'no-cache' })
    .then((r) => (r.ok ? r.json() : { games: [], demos: [], tunes: [] }))
    .catch(() => ({ games: [], demos: [], tunes: [] }))
    .then((v) => ({
      games: v.games.map((g) => ({ ...g, id: g.gb ? `gb64:${g.gb}` : g.ia })),
      demos: v.demos,
      tunes: v.tunes,
    }));
  return verifiedList;
}

/** A shelf over a fixed list of items. */
function listShelf(parent, title, items, card, opts = {}) {
  if (!items.length) return null;
  return shelf(parent, {
    title, card, size: 24, ...opts,
    load: async (page, size) => ({ total: items.length, items: items.slice((page - 1) * size, page * size) }),
  });
}

// ── games ────────────────────────────────────────────────────────────────────

let covers = new Map();
const coversReady = boxartIndex().then((m) => (covers = m));

const gameArt = (g) => [covers.get(canon(g.title)), g.ia && ia.thumb(g.ia), g.shot && ia.gb64Shot(g.shot)];

function gameCard(g) {
  const box = covers.get(canon(g.title));
  const sub = [g.year || '', g.publisher].filter(Boolean).join(' · ');
  return h('button.card', { onclick: () => showGame(g), title: g.title },
    art(gameArt(g), g.title, { boxart: !!box, badge: g.rating >= 4 ? '★' : null }),
    h('div.card-title', {}, g.title),
    sub && h('div.card-sub', {}, sub));
}

/** Recently played items that are still on the verified list. */
async function recentShelf(parent, kind, title, card) {
  const v = await verified();
  const ok = new Set((kind === 'game' ? v.games : v.demos).map((x) => x.id));
  const played = store.recentPlays(kind).filter((p) => ok.has(p.item.id));
  listShelf(parent, title, played.map((p) => p.item), card);
}

async function initGames() {
  const root = $('#games');
  const shelves = h('div.shelves');
  root.append(shelves);
  const [{ games }] = await Promise.all([verified(), Promise.race([coversReady, new Promise((r) => setTimeout(r, 2500))])]);
  if (!games.length) {
    shelves.append(h('p.banner', {}, 'No playtested games yet: run tools/playtest to build verified.json.'));
    return bootDone();
  }
  await recentShelf(shelves, 'game', 'Latest played', gameCard);
  const top = listShelf(shelves, 'Top rated', games, gameCard);
  listShelf(shelves, 'Classics', games.filter((g) => g.classic), gameCard);
  // genres in the order their best game ranks
  const genres = [];
  for (const g of games) if (g.genre && !genres.includes(g.genre)) genres.push(g.genre);
  for (const genre of genres) {
    const items = games.filter((g) => g.genre === genre);
    if (items.length >= 4) listShelf(shelves, genre, items, gameCard);
  }
  top.ready.then(bootDone);
}

async function showGame(g) {
  linkTo('games', g.id);
  const dlg = $('#detail');
  const facts = h('dl.facts');
  const links = h('div.links');
  if (g.ia) links.append(h('a', { href: ia.itemUrl(g.ia), target: '_blank', rel: 'noopener' }, 'Internet Archive ↗'));
  if (g.gb) links.append(h('a', { href: 'https://www.gb64.com/', target: '_blank', rel: 'noopener' }, 'GameBase64 ↗'));
  const shots = h('div.shots');
  if (g.shot) shots.append(h('img', { src: ia.gb64Shot(g.shot), alt: '', loading: 'lazy' }));
  dlg.replaceChildren(closeButton(dlg), h('div.sheet-body', {},
    h('div.sheet-art', {}, art(gameArt(g), g.title, { boxart: true })),
    h('div', {}, h('h2', {}, g.title), h('p.by', {}, [g.publisher, g.year || ''].filter(Boolean).join(' · ')), facts,
      h('div.actions', {}, h('button.key.pill.primary', { onclick: () => launch({ kind: 'game', item: g }) }, '▶ Play')),
      links, shots,
      h('p.hint', {}, 'Joystick in port 2: cursor keys + space, a gamepad, or the on-screen pad. Runs on the free MEGA65 Open ROMs.'))));
  dlg.showModal();
  const add = (k, v) => v && facts.append(h('dt', {}, k), h('dd', {}, v));
  add('Genre', [g.genre, g.sub].filter(Boolean).join(' · '));
  add('Rating', g.rating ? `${'★'.repeat(g.rating)} (GameBase64)` : '');
  add('Classic', g.classic ? 'A GameBase64 classic' : '');
  add('Players', g.players || '');
  if (g.ia && !g.year) {
    try {
      const meta = (await ia.metadata(g.ia)).metadata || {};
      add('Year', first(meta.date));
      add('Publisher', first(meta.creator));
    } catch {}
  }
}

const first = (v) => (Array.isArray(v) ? v[0] : v);

function closeButton(dlg) {
  return h('button.key.icon.small.close', { onclick: () => dlg.close(), 'aria-label': 'Close' }, '✕');
}

// ── demos ────────────────────────────────────────────────────────────────────

function demoCard(d) {
  const shots = d.id ? [ia.thumb(d.id)] : csdb.shotUrls(d.csdbId);
  return h('button.card', { onclick: () => showDemo(d), title: d.title },
    art(shots, d.title, { badge: d.rating ? `${d.rating.toFixed(1)}★` : null }),
    h('div.card-title', {}, d.title),
    h('div.card-sub', {}, [d.group, d.year || ''].filter(Boolean).join(' · ')));
}

async function initDemos() {
  const root = $('#demos');
  const shelves = h('div.shelves.wide');
  root.append(shelves);
  const { demos } = await verified();
  const items = demos.map((d) => ({ ...d, csdbId: d.csdb }));
  await recentShelf(shelves, 'demo', 'Latest watched', demoCard);
  listShelf(shelves, 'CSDb top rated', items, demoCard, { wide: true });
  for (const decade of [2020, 2010, 2000, 1990, 1980]) {
    listShelf(shelves, `The ${decade}s`, items.filter((d) => d.year >= decade && d.year < decade + 10), demoCard, { wide: true });
  }
  const groups = [];
  for (const d of items) for (const g of d.group.split(/,\s*/)) if (g && !groups.includes(g)) groups.push(g);
  for (const g of groups) {
    const theirs = items.filter((d) => d.group.split(/,\s*/).includes(g));
    if (theirs.length >= 3) listShelf(shelves, g, theirs, demoCard, { wide: true });
  }
  if (!items.length) shelves.append(h('p.banner', {}, 'No playtested demos yet: run tools/playtest to build verified.json.'));
}

async function showDemo(d) {
  if (d.id) linkTo('demos', d.id);
  const dlg = $('#detail');
  const facts = h('dl.facts');
  const shots = h('div.shots');
  const links = h('div.links');
  if (d.id) links.append(h('a', { href: ia.itemUrl(d.id), target: '_blank', rel: 'noopener' }, 'Internet Archive ↗'));
  if (d.csdbId) links.append(h('a', { href: csdb.releaseUrl(d.csdbId), target: '_blank', rel: 'noopener' }, 'CSDb ↗'));
  const hero = d.id ? h('img', { src: ia.thumb(d.id), alt: '' }) : art(csdb.shotUrls(d.csdbId), d.title);
  dlg.replaceChildren(closeButton(dlg), h('div.sheet-body', {},
    h('div.sheet-art', {}, hero),
    h('div', {}, h('h2', {}, d.title), h('p.by', {}, [d.group, d.year || ''].filter(Boolean).join(' · ')), facts,
      h('div.actions', {}, h('button.key.pill.primary', { onclick: () => launch({ kind: 'demo', item: d }) }, '▶ Run')),
      links, shots)));
  dlg.showModal();
  const add = (k, v) => v && facts.append(h('dt', {}, k), h('dd', {}, v));
  if (d.rating) add('CSDb rating', `${d.rating.toFixed(2)} ★`);
  if (d.id) try {
    const meta = await ia.metadata(d.id);
    const pics = (meta.files || []).map((f) => f.name).filter((n) => /^screenshot_\d+\.jpg$/.test(n)).slice(0, 12);
    if (pics.length) hero.src = ia.fileUrl(d.id, pics[Math.min(1, pics.length - 1)]);
    for (const p of pics) shots.append(h('img', { src: ia.fileUrl(d.id, p.replace('.jpg', '_thumb.jpg')), alt: '', loading: 'lazy', onclick: () => (hero.src = ia.fileUrl(d.id, p)) }));
    add('Released', first(meta.metadata?.csdb_release_date));
  } catch {}
  if (d.csdbId) {
    try {
      const r = await csdb.release(d.csdbId);
      if (r.event) add('Party', [r.event, r.compo?.place ? `#${r.compo.place} in ${r.compo.name}` : ''].filter(Boolean).join(' · '));
      if (r.eventPlace) add('Where', r.eventPlace);
    } catch {}
  }
}

// ── emulator ─────────────────────────────────────────────────────────────────

let session = null;

async function loadRoms() {
  const roms = {};
  for (const k of ['kernal', 'basic', 'charset', 'floppy']) {
    const b = await store.getBlob(`rom-${k}`);
    if (b) roms[k] = b;
  }
  // a partial set of system ROMs would break the boot; use them only together
  if (!(roms.kernal && roms.basic && roms.charset)) delete roms.kernal;
  return roms;
}

async function launch({ kind, item }) {
  $('#detail').close();
  player?.setPaused(true);
  const stage = $('#emu-stage');
  const status = $('#emu-status');
  const note = $('#emu-note');
  $('#emu-title').textContent = item.title;
  note.textContent = '';
  status.textContent = '';
  const bar = h('i');
  const label = h('div', {}, 'SEARCHING FOR ' + item.title.toUpperCase());
  stage.replaceChildren(h('div.emu-loading', {}, label, h('div.bar', {}, bar)));
  $('#emu').hidden = false;
  const progress = (got, total) => {
    label.textContent = 'LOADING';
    bar.style.width = total ? `${Math.round((got / total) * 100)}%` : `${Math.min(95, got / 2000)}%`;
  };
  try {
    const { name, bytes, prefer } = item.gb
      ? { name: item.gb.split('/').pop(), bytes: await ia.gb64Game(item.gb, progress), prefer: item.run }
      : item.ia || (item.id && !item.id.startsWith('gb64:'))
        ? await archiveFile(item.ia || item.id, progress)
        : await csdbFile(item.csdbId, progress);
    bar.style.width = '100%';
    const roms = await loadRoms();
    const file = await prepare(name, bytes, { canMount: !!roms.floppy, prefer });
    note.textContent = file.vdrive
      ? 'Disk served by breadbin\'s virtual drive. Joystick in port 2: cursor keys + space, a gamepad or the pad.'
      : 'Joystick in port 2: cursor keys + space, a gamepad or the pad.';
    store.recordPlay({ kind, id: item.id || `csdb-${item.csdbId}`, item });
    session = emu.run(stage, file, {
      roms,
      onState: (s) => (status.textContent = s.running ? (s.warping ? 'warp' : 'running') : 'paused'),
      onDrive: (d) => {
        if (d.event === 'load') status.textContent = `loaded ${d.name}`;
        if (d.event === 'missing') note.textContent = `The program asked the drive for "${d.name}", which isn't on this disk. It may need the next disk side, or a real 1541 drive ROM (see Settings).`;
      },
    });
    status.textContent = 'starting…';
    setPad(settings.pad);
  } catch (e) {
    stage.replaceChildren(h('div.emu-error', {}, h('b', {}, '?LOAD ERROR'), e.message));
  }
}

async function archiveFile(id, progress) {
  const meta = await ia.metadata(id);
  const name = await pick((meta.files || []).map((f) => f.name), meta.metadata?.emulator_start);
  if (!name) throw new Error('this item has no C64 disk, tape or program file');
  return { name, bytes: await ia.download(id, name, progress) };
}

/** The best runnable download of a CSDb release. */
async function csdbFile(id, progress) {
  const { links } = await csdb.release(id);
  const runnable = links.filter((l) => /\.(d64|prg|zip|crt|t64|g64)$/i.test(l));
  runnable.sort((a, b) => Number(!/csdb\.dk/.test(a)) - Number(!/csdb\.dk/.test(b)));
  if (!runnable.length) throw new Error('CSDb lists no runnable download for this release');
  const url = runnable[0];
  return { name: decodeURIComponent(url.split('/').pop()), bytes: await csdb.fetchFile(url, progress) };
}

let pad = null;

/** Show or hide the on-screen joystick for the running session. */
function setPad(on) {
  pad?.destroy();
  pad = null;
  $('#emu').classList.toggle('touch', on);
  $('#emu-pad').setAttribute('aria-pressed', String(on));
  if (on && session) pad = createPad($('#emu'), (m) => session.send(m));
}
$('#emu-pad').addEventListener('click', () => {
  saveSettings({ pad: !settings.pad });
  setPad(settings.pad);
});

function closeEmu() {
  setPad(false);
  session?.stop();
  session = null;
  $('#emu').hidden = true;
  if (document.fullscreenElement) document.exitFullscreen();
}
$('#emu-close').addEventListener('click', closeEmu);
$('#emu-full').addEventListener('click', () => {
  const el = $('#emu');
  if (document.fullscreenElement) document.exitFullscreen();
  else el.requestFullscreen?.().then(() => session?.frame.focus());
});

// ── tunes ────────────────────────────────────────────────────────────────────

let player = null;
let viz = null;
let queue = []; // rows in list order
let currentRow = null;
let radio = false;
let radioTimer = 0;
const RADIO_MAX_MS = 4 * 60e3;

function tuneRow(entry) {
  const sub = h('div.tune-sub', {}, entry.sub || '');
  const row = h('button.tune', {},
    h('span.play', {}, '▶'),
    h('div', {}, h('div.tune-name', {}, entry.rank ? h('span.rank', {}, `${entry.rank}.`) : null, entry.name), sub),
    h('span.tune-rating', {}, entry.rating ? `${entry.rating.toFixed(1)}★` : ''));
  row.entry = entry;
  row.subEl = sub;
  row.addEventListener('click', () => !row.classList.contains('dead') && playRow(row));
  queue.push(row);
  return row;
}

async function initTunes() {
  player = new SidPlayer();
  viz = new Viz($('#viz'));
  await viz.ready;
  viz.kind = settings.viz;
  const sel = $('#viz-kind');
  NAMES.forEach((n, i) => sel.append(h('option', { value: i }, n)));
  sel.value = settings.viz;
  sel.addEventListener('change', () => {
    viz.kind = Number(sel.value);
    saveSettings({ viz: viz.kind });
  });
  $('#btn-pause').addEventListener('click', () => player.setPaused(!player.paused));
  $('#btn-stop').addEventListener('click', stopTune);
  $('#btn-skip').addEventListener('click', () => nextTune());
  $('#btn-radio').addEventListener('click', () => {
    radio = !radio;
    $('#btn-radio').setAttribute('aria-pressed', String(radio));
    toast(radio ? 'Radio on: shuffling through the list.' : 'Radio off.');
    if (radio && !player.playing) nextTune();
  });
  player.addEventListener('ended', () => nextTune());
  player.addEventListener('state', updateButtons);
  updateButtons();
  renderNowPlaying(null);
  animate();

  const list = $('#tune-list');
  const top = h('div');
  list.append(top);
  for (const [name, dir] of hvsc.LEGENDS) list.append(legendSection(name, dir));
  // Playtested CSDb favourites, grouped by the party they were released at
  // (like the desktop jukebox); each row knows its HVSC path, so playing needs
  // no CSDb request at all.
  const { tunes } = await verified();
  const byParty = new Map();
  for (const t of tunes) {
    const party = (t.party || '').replace(/\s*\d{4}$/, '');
    if (!byParty.has(party)) byParty.set(party, []);
    byParty.get(party).push(t);
  }
  const loners = [...(byParty.get('') || [])];
  const sections = [];
  for (const [party, list] of byParty) {
    if (!party) continue;
    if (list.length < 2) loners.push(...list);
    else sections.push([party, list]);
  }
  sections.sort((a, b) => b[1][0].rating - a[1][0].rating || b[1].length - a[1].length);
  if (loners.length) sections.push(['Released outside parties', loners.sort((a, b) => b.rating - a.rating)]);
  const rows = [];
  for (const [party, list] of sections) {
    top.append(h('h3.tune-section', { style: { borderLeftColor: CHIPS[chipIndex++ % CHIPS.length] } }, party, h('span.count', {}, `${list.length} tunes`)));
    for (const t of list) {
      const row = tuneRow({ kind: 'csdb', id: t.id, hvsc: t.hvsc, name: t.name, sub: [t.composer, t.group, t.year || ''].filter(Boolean).join(' · '), rating: t.rating });
      rows.push(row);
      top.append(row);
    }
  }
  // queue currently holds legend rows first; CSDb rows belong at the front
  queue = [...rows, ...queue.filter((r) => !rows.includes(r))];
  if (!rows.length) {
    top.append(h('p.banner', {}, 'No playtested tunes yet: run tools/playtest. The HVSC composer shelves below still play.'));
    list.querySelector('.tune-section button')?.click();
  }
}

/** A collapsible composer section listing their HVSC folder. */
function legendSection(name, dir) {
  const wrap = h('div');
  const open = h('button.key.pill', {}, 'Show');
  const head = h('h3.tune-section', { style: { borderLeftColor: CHIPS[chipIndex++ % CHIPS.length] } }, name, open);
  wrap.append(head);
  let rows = null;
  open.addEventListener('click', async () => {
    if (rows) {
      const hide = !rows[0]?.hidden;
      for (const r of rows) r.hidden = hide;
      open.textContent = hide ? 'Show' : 'Hide';
      return;
    }
    open.disabled = true;
    try {
      const files = (await hvsc.list(dir)).filter((f) => f.toLowerCase().endsWith('.sid'));
      rows = files.map((f) => tuneRow({ kind: 'hvsc', path: dir + f, name: hvsc.tuneTitle(f), sub: name, composer: name }));
      wrap.append(...rows);
      open.textContent = 'Hide';
    } catch (e) {
      toast(`Couldn't list ${name}: ${e.message}`);
    } finally {
      open.disabled = false;
    }
  });
  return wrap;
}

function describeRow(row, info) {
  const who = info.musicians.map((m) => m.handle).join(' & ') || row.entry.sub;
  const where = info.event ? `${info.event}` : info.year || '';
  row.subEl.textContent = [who, info.group?.name, where].filter(Boolean).join(' · ');
}

async function resolveTune(entry) {
  if (entry.kind === 'hvsc') return { bytes: await hvsc.sid(entry.path), info: null };
  if (entry.hvsc) return { bytes: await hvsc.sid(entry.hvsc), info: null };
  const info = await csdb.release(entry.id);
  if (info.hvscPath) return { bytes: await hvsc.sid(info.hvscPath), info };
  // Too new for the HVSC: CSDb's own copy, when this page may download from it.
  const link = info.links.find((l) => /\.sid$/i.test(l));
  if (!link || !(await csdb.canFetchFiles())) throw new Error('this tune is not in the HVSC yet');
  return { bytes: await csdb.fetchFile(link), info };
}

let playToken = 0;
async function playRow(row, song = 0) {
  const token = ++playToken;
  currentRow?.classList.remove('current');
  currentRow = row;
  row.classList.add('current', 'loading');
  row.scrollIntoView({ block: 'nearest' });
  clearTimeout(radioTimer);
  try {
    await player.init();
    const { bytes, info } = await resolveTune(row.entry);
    if (token !== playToken) return;
    if (info) describeRow(row, info);
    await player.play(bytes, song, { hq: settings.hq });
    player.setWidth(settings.width);
    player.setVolume(settings.volume);
    row.bytes = bytes;
    row.song = song || sidHeader(bytes).startSong;
    viz.title = row.entry.name;
    renderNowPlaying(row, info, sidHeader(bytes));
    // the CSDb details (screenshot, real names, compo) arrive when the relay answers
    if (!info && row.entry.kind === 'csdb') {
      csdb.release(row.entry.id).then((full) => token === playToken && renderNowPlaying(row, full, sidHeader(bytes))).catch(() => {});
    }
    if (radio) radioTimer = setTimeout(() => nextTune(), RADIO_MAX_MS);
    prefetchNext();
  } catch (e) {
    if (token !== playToken) return;
    row.classList.add('dead');
    row.querySelector('.play').textContent = '✕';
    row.title = e.message;
    toast(`${row.entry.name}: ${e.message}`);
    if (radio || player.playing) nextTune();
  } finally {
    row.classList.remove('loading');
  }
}

function nextRow() {
  const live = queue.filter((r) => r.isConnected && !r.hidden && !r.classList.contains('dead'));
  if (!live.length) return null;
  if (radio) return live[Math.floor(Math.random() * live.length)];
  const i = live.indexOf(currentRow);
  return live[(i + 1) % live.length];
}

function nextTune() {
  const row = nextRow();
  if (row) playRow(row);
}

/** Warm the cache for the following tune so the next track starts at once. */
function prefetchNext() {
  if (radio) return;
  const row = nextRow();
  if (row && row !== currentRow) resolveTune(row.entry).catch(() => {});
}

function stopTune() {
  player.stop();
  clearTimeout(radioTimer);
  currentRow?.classList.remove('current');
  currentRow = null;
  renderNowPlaying(null);
}

function updateButtons() {
  $('#btn-pause').textContent = player.paused ? 'Resume' : 'Pause';
  $('#btn-pause').disabled = !player.playing;
  $('#btn-stop').disabled = !player.playing;
}

function renderNowPlaying(row, info, header) {
  const np = $('#now-playing');
  const sub = $('#subtune');
  if (!row) {
    np.replaceChildren(h('p.np-idle', {}, h('b', {}, 'Pick a tune'), ' to start the SID chip, or press ', h('b', {}, 'Radio'), ' and let it play.'));
    sub.hidden = true;
    return;
  }
  const shot = info?.screenshot
    ? h('div.np-shot', {}, h('img', { src: info.screenshot, alt: '', referrerPolicy: 'no-referrer' }))
    : h('div.np-shot', {}, sidArt(header));
  const lines = [];
  const m = info?.musicians?.[0];
  const artist = m
    ? `${info.musicians.map((x) => x.handle).join(' & ')}${m.realName ? ` (${m.realName})` : ''}${m.country ? ` · ${m.country}` : ''}`
    : header.author;
  lines.push(h('p.np-artist', {}, artist));
  if (info) {
    const where = [info.event, info.eventPlace].filter(Boolean).join(' · ');
    const compo = info.compo?.place ? ` · #${info.compo.place} in the ${info.compo.name} compo` : '';
    if (where) lines.push(h('p.np-meta', {}, where + compo));
    const stats = [info.rating ? `★ ${info.rating.toFixed(2)}` : '', [info.sidModel || header.model, info.clock || header.clock].filter(Boolean).join(' '), info.downloads ? `${fmt.format(info.downloads)} downloads` : ''];
    lines.push(h('p.np-stats', {}, stats.filter(Boolean).join(' · ')));
    if (info.group) {
      const g = info.group;
      lines.push(h('p.np-group', {}, `${g.name}${g.short ? ` (${g.short})` : ''}${g.founded ? ` · since ${g.founded}` : ''}`, g.slogan ? h('br') : null, g.slogan ? `“${g.slogan}”` : null));
    }
  } else {
    if (header.released) lines.push(h('p.np-meta', {}, header.released));
    lines.push(h('p.np-stats', {}, [header.model, header.clock, header.songs > 1 ? `${header.songs} songs` : ''].filter(Boolean).join(' · ')));
  }
  np.replaceChildren(shot, h('div', {}, h('h2.np-title', {}, row.entry.name), lines));

  sub.hidden = header.songs <= 1;
  if (header.songs > 1) {
    $('#sub-label').textContent = `Song ${row.song} of ${header.songs}`;
    $('#sub-prev').onclick = () => playRow(row, row.song > 1 ? row.song - 1 : header.songs);
    $('#sub-next').onclick = () => playRow(row, row.song < header.songs ? row.song + 1 : 1);
  }
}

/** A stand-in "screenshot" for tunes without one: the SID header, C64 style. */
function sidArt(header) {
  const c = h('canvas', { width: 320, height: 200 });
  const x = c.getContext('2d');
  x.fillStyle = '#40318d';
  x.fillRect(0, 0, 320, 200);
  x.fillStyle = '#7869c4';
  x.font = '8px Sixtyfour';
  const lines = ['  **** SID TUNE ****', '', header.name, header.author, header.released, '', `${header.model || 'SID'} ${header.clock}`.trim(), '', 'READY.'];
  lines.forEach((l, i) => x.fillText(l.slice(0, 38).toUpperCase(), 8, 24 + i * 14));
  c.style.width = '100%';
  c.style.height = '100%';
  c.style.imageRendering = 'pixelated';
  return c;
}

function animate() {
  const tick = (ms) => {
    if (current === 'tunes' && !document.hidden) {
      const t = ms / 1000;
      const live = player.playing && !player.paused;
      viz.update(live ? player.analyser : null, player.regs);
      viz.draw(t);
    }
    requestAnimationFrame(tick);
  };
  requestAnimationFrame(tick);
}

// ── search ───────────────────────────────────────────────────────────────────

const searchTerms = {};
let searchTimer = 0;
$('#search').addEventListener('input', (e) => {
  clearTimeout(searchTimer);
  const term = e.target.value.trim();
  searchTerms[current] = term;
  searchTimer = setTimeout(() => runSearch(current, term), current === 'tunes' ? 120 : 400);
});

async function runSearch(tab, term) {
  if (tab === 'tunes') {
    const needle = term.toLowerCase();
    for (const r of queue) r.hidden = needle && !`${r.entry.name} ${r.subEl.textContent}`.toLowerCase().includes(needle);
    return;
  }
  const root = $(`#${tab}`);
  const shelves = root.querySelector('.shelves');
  root.querySelector('.results')?.remove();
  if (shelves) shelves.hidden = !!term;
  if (!term) return;
  const words = term.toLowerCase().split(/\s+/).filter(Boolean);
  const v = await verified();
  const list = tab === 'games' ? v.games : v.demos.map((d) => ({ ...d, csdbId: d.csdb }));
  const hits = list.filter((x) => {
    const hay = `${x.title} ${x.publisher || ''} ${x.group || ''} ${x.genre || ''} ${x.party || ''}`.toLowerCase();
    return words.every((w) => hay.includes(w));
  });
  const results = h(`div.results${tab === 'demos' ? '.wide' : ''}`);
  root.prepend(results);
  if (!hits.length) {
    results.append(h('p.banner', {}, `Nothing playtested matches “${term}”.`));
    return;
  }
  const s = listShelf(results, `Results for “${term}”`, hits, tab === 'games' ? gameCard : demoCard, { wide: tab === 'demos' });
  s.el.querySelector('.shelf-head button').click();
}

// ── settings ─────────────────────────────────────────────────────────────────

function saveSettings(patch) {
  settings = { ...settings, ...patch };
  store.save('settings', settings);
}

const ROMS = [
  ['kernal', 'KERNAL (8 KB)', 8192],
  ['basic', 'BASIC (8 KB)', 8192],
  ['charset', 'Character ROM (4 KB)', 4096],
  ['floppy', '1541 drive ROM (16 KB)', 16384],
];

async function openSettings() {
  const dlg = $('#settings');
  const body = h('div.sheet-body');
  dlg.replaceChildren(closeButton(dlg), body);
  const relay = h('input', { type: 'url', value: settings.relay, placeholder: 'https://r.jina.ai (default)' });
  relay.addEventListener('change', () => saveSettings({ relay: relay.value.trim() }));
  const hq = h('input', { type: 'checkbox', checked: settings.hq });
  hq.addEventListener('change', () => saveSettings({ hq: hq.checked }));
  const width = h('input', { type: 'range', min: 0, max: 1, step: 0.05, value: settings.width });
  width.addEventListener('input', () => {
    saveSettings({ width: Number(width.value) });
    player?.setWidth(settings.width);
  });
  const volume = h('input', { type: 'range', min: 0, max: 1.4, step: 0.05, value: settings.volume });
  volume.addEventListener('input', () => {
    saveSettings({ volume: Number(volume.value) });
    player?.setVolume(settings.volume);
  });

  const romRows = await Promise.all(ROMS.map(async ([key, label, size]) => {
    const state = h('span.state');
    const input = h('input', { type: 'file', hidden: true });
    const pick = h('button.key.pill', { onclick: () => input.click() }, 'Choose…');
    const clear = h('button.key.pill', {}, 'Remove');
    const refresh = async () => {
      const b = await store.getBlob(`rom-${key}`);
      state.textContent = b ? 'installed' : 'not set';
      state.classList.toggle('ok', !!b);
      clear.hidden = !b;
    };
    input.addEventListener('change', async () => {
      const f = input.files[0];
      if (!f) return;
      const b = new Uint8Array(await f.arrayBuffer());
      if (b.length !== size && b.length !== size + 2) return toast(`${f.name} is ${b.length} bytes; a ${label} is ${size}.`);
      await store.putBlob(`rom-${key}`, b.length === size + 2 ? b.subarray(2) : b);
      refresh();
    });
    clear.addEventListener('click', async () => {
      await store.deleteBlob(`rom-${key}`);
      refresh();
    });
    await refresh();
    return h('div.rom-row', {}, h('span', {}, label), state, h('span', {}, pick, ' ', clear, input));
  }));

  const usage = h('span', {}, '…');
  store.cacheUsage().then((n) => (usage.textContent = `${(n / 1048576).toFixed(1)} MB in this browser`));

  body.append(
    h('h2', {}, 'Settings'),
    h('h3', {}, 'Sound'),
    h('label.check', {}, hq, 'High-quality resampling (reSID band-limited; uses more CPU)'),
    h('label.field', {}, 'Stereo width', width),
    h('label.field', {}, 'Volume', volume),
    h('h3', {}, 'Your own ROMs (optional)'),
    h('p', {}, 'Breadbin runs on the free MEGA65 Open ROMs, so most single-file games and demos start without anything licensed. Multi-load titles need a real 1541 drive. If you own the Commodore ROMs (C64 Forever includes them), add them here: they stay in this browser and go only to the emulator.'),
    ...romRows,
    h('h3', {}, 'CSDb relay'),
    h('p', {}, 'csdb.dk doesn\'t allow web pages to read it, so its toplists and release details go through a relay: r.jina.ai by default (about 20 requests a minute), or your own. Use {url} where the CSDb address goes, for example https://your-worker.workers.dev/?url={url}.'),
    h('label.field', {}, 'Relay URL', relay),
    h('h3', {}, 'Downloads'),
    h('p', {}, 'Games, demos, SIDs and catalogue pages are cached so they work again without a download. ', usage),
    h('div.actions', {}, h('button.key.pill', { onclick: async () => { await store.clearCaches(); toast('Cache cleared.'); dlg.close(); } }, 'Clear cache')),
    h('h3', {}, 'Where everything comes from'),
    h('p', {}, 'Games and demos: the Internet Archive\'s C64 software library. Rankings and scene details: CSDb. SID music: the High Voltage SID Collection (hvsc.de). Box art: libretro-thumbnails. Emulator: vc64web (VirtualC64) with the MEGA65 Open ROMs. SID playback: breadbin\'s own player on reSID, compiled to WebAssembly.'),
  );
  dlg.showModal();
}
$('#open-settings').addEventListener('click', openSettings);

for (const dlg of document.querySelectorAll('dialog')) {
  dlg.addEventListener('click', (e) => e.target === dlg && dlg.close());
}

route();
