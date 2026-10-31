// tunes - the Tunes tab: playtested CSDb favourites grouped by the party they
// were released at, the now-playing card and the visualisers. Each row knows
// its HVSC path, so playing needs no CSDb request; the CSDb details (screenshot,
// real names, compo) are fetched afterwards through the relay.

import * as csdb from './csdb.js';
import { sid } from './hvsc.js';
import { verified } from './catalogue.js';
import { settings, save } from './settings.js';
import { SidPlayer, sidHeader } from './sid.js';
import { Viz, NAMES } from './viz.js';
import { $, h, toast, fmt, nextChip, pixel } from './ui.js';

const RADIO_MAX_MS = 4 * 60e3;
let player;
let viz;
let rows = [];
let current = null;
let radio = false;
let radioTimer = 0;
let playToken = 0;

/** Group tunes by party (year dropped); lone ones go last. */
function byParty(tunes) {
  const groups = new Map();
  for (const t of tunes) {
    const party = (t.party || '').replace(/\s*\d{4}$/, '');
    groups.set(party, [...(groups.get(party) || []), t]);
  }
  const sections = [];
  const loners = [];
  for (const [party, list] of groups) (party && list.length > 1 ? sections.push([party, list]) : loners.push(...list));
  sections.sort((a, b) => b[1][0].rating - a[1][0].rating || b[1].length - a[1].length);
  // the handpicked list keeps its order (sort is stable: unrated tunes stay put)
  if (loners.length) sections.push([sections.length ? 'Released outside parties' : 'All-time greats', loners.sort((a, b) => b.rating - a.rating)]);
  return sections;
}

function tuneRow(t) {
  const row = h('button.tune', { onclick: () => playRow(row) },
    h('span.play', {}, '▶'),
    h('div', {}, h('div.tune-name', {}, t.name), h('div.tune-sub', {}, [t.composer, t.group, t.year || ''].filter(Boolean).join(' · '))),
    h('span.tune-rating', {}, t.rating ? `${t.rating.toFixed(1)}★` : ''));
  row.tune = t;
  return row;
}

export async function initTunes() {
  player = new SidPlayer();
  viz = new Viz($('#viz'));
  await viz.ready;
  viz.kind = settings.viz;
  const sel = $('#viz-kind');
  sel.append(...NAMES.map((n, i) => h('option', { value: i }, n)));
  sel.value = settings.viz;
  sel.addEventListener('change', () => save({ viz: (viz.kind = Number(sel.value)) }));
  $('#btn-pause').addEventListener('click', () => player.setPaused(!player.paused));
  $('#btn-stop').addEventListener('click', stop);
  $('#btn-skip').addEventListener('click', next);
  $('#btn-radio').addEventListener('click', () => {
    radio = !radio;
    $('#btn-radio').setAttribute('aria-pressed', String(radio));
    toast(radio ? 'Radio on: shuffling through the list.' : 'Radio off.');
    if (radio && !player.playing) next();
  });
  player.addEventListener('ended', next);
  player.addEventListener('state', updateButtons);
  addEventListener('breadbin:launch', () => player.setPaused(true));
  addEventListener('breadbin:sound', () => {
    player.setWidth(settings.width);
    player.setVolume(settings.volume);
  });
  updateButtons();
  renderNowPlaying(null);
  animate();

  const { tunes } = await verified();
  const list = $('#tune-list');
  for (const [party, items] of byParty(tunes)) {
    const section = items.map(tuneRow);
    rows.push(...section);
    list.append(h('h3.tune-section', { style: { borderLeftColor: nextChip() } }, pixel(party), h('span.count', {}, `${items.length} tunes`)), ...section);
  }
  if (!rows.length) list.append(h('p.banner', {}, 'No playtested tunes yet: run tools/playtest to build verified.json.'));
}

/** Show only rows matching `term`. */
export function filterTunes(term) {
  const needle = term.toLowerCase();
  for (const r of rows) r.hidden = needle && !r.textContent.toLowerCase().includes(needle);
}

async function playRow(row, song = 0) {
  const token = ++playToken;
  current?.classList.remove('current');
  current = row;
  row.classList.add('current', 'loading');
  row.scrollIntoView({ block: 'nearest' });
  clearTimeout(radioTimer);
  try {
    await player.init();
    const bytes = await sid(row.tune.hvsc);
    if (token !== playToken) return;
    await player.play(bytes, song, { hq: settings.hq });
    player.setWidth(settings.width);
    player.setVolume(settings.volume);
    const header = sidHeader(bytes);
    row.song = song || header.startSong;
    viz.title = row.tune.name;
    renderNowPlaying(row, null, header);
    // CSDb details exist for scene releases (numeric ids), not for game tunes (HVSC paths)
    if (typeof row.tune.id === 'number') csdb.release(row.tune.id).then((info) => token === playToken && renderNowPlaying(row, info, header)).catch(() => {});
    if (radio) radioTimer = setTimeout(next, RADIO_MAX_MS);
    const following = nextRow();
    if (following && !radio) sid(following.tune.hvsc).catch(() => {}); // warm the cache
  } catch (e) {
    if (token !== playToken) return;
    toast(`${row.tune.name}: ${e.message}`);
  } finally {
    row.classList.remove('loading');
  }
}

function nextRow() {
  const live = rows.filter((r) => !r.hidden);
  if (!live.length) return null;
  if (radio) return live[Math.floor(Math.random() * live.length)];
  return live[(live.indexOf(current) + 1) % live.length];
}

function next() {
  const row = nextRow();
  if (row) playRow(row);
}

function stop() {
  player.stop();
  clearTimeout(radioTimer);
  current?.classList.remove('current');
  current = null;
  renderNowPlaying(null);
}

function updateButtons() {
  $('#btn-pause').textContent = player.paused ? 'Resume' : 'Pause';
  $('#btn-pause').disabled = !player.playing;
  $('#btn-stop').disabled = !player.playing;
}

/** The card above the visualiser: CSDb details when known, else the SID header. */
function renderNowPlaying(row, info, header) {
  const np = $('#now-playing');
  const sub = $('#subtune');
  if (!row) {
    np.replaceChildren(h('p.np-idle', {}, h('b', {}, 'Pick a tune'), ' to start the SID chip, or press ', h('b', {}, 'Radio'), ' and let it play.'));
    sub.hidden = true;
    return;
  }
  const t = row.tune;
  const m = info?.musicians?.[0];
  const artist = m
    ? `${info.musicians.map((x) => x.handle).join(' & ')}${m.realName ? ` (${m.realName})` : ''}${m.country ? ` · ${m.country}` : ''}`
    : t.composer || header.author;
  const where = info ? [info.event, info.eventPlace].filter(Boolean).join(' · ') + (info.compo?.place ? ` · #${info.compo.place} in the ${info.compo.name} compo` : '') : t.party;
  const stats = [
    (info?.rating || t.rating) ? `★ ${(info?.rating || t.rating).toFixed(2)}` : '',
    [info?.sidModel || header.model, info?.clock || header.clock].filter(Boolean).join(' '),
    info?.downloads ? `${fmt.format(info.downloads)} downloads` : header.songs > 1 ? `${header.songs} songs` : '',
  ];
  const g = info?.group;
  np.replaceChildren(
    h('div.np-shot', {}, info?.screenshot ? h('img', { src: info.screenshot, alt: '', referrerPolicy: 'no-referrer' }) : sidArt(header)),
    h('div', {},
      h('h2.np-title', {}, pixel(t.name)),
      h('p.np-artist', {}, artist),
      where && h('p.np-meta', {}, where),
      h('p.np-stats', {}, stats.filter(Boolean).join(' · ')),
      g && h('p.np-group', {}, `${g.name}${g.short ? ` (${g.short})` : ''}${g.founded ? ` · since ${g.founded}` : ''}`, g.slogan && h('br'), g.slogan && `“${g.slogan}”`)));

  sub.hidden = header.songs <= 1;
  if (header.songs > 1) {
    $('#sub-label').textContent = `Song ${row.song} of ${header.songs}`;
    $('#sub-prev').onclick = () => playRow(row, row.song > 1 ? row.song - 1 : header.songs);
    $('#sub-next').onclick = () => playRow(row, row.song < header.songs ? row.song + 1 : 1);
  }
}

/** Stand-in "screenshot": the SID header as a card in breadbin's colours. */
function sidArt(header) {
  const c = h('canvas', { width: 320, height: 200, style: { width: '100%', height: '100%', imageRendering: 'pixelated' } });
  const x = c.getContext('2d');
  x.fillStyle = '#1b1729';
  x.fillRect(0, 0, 320, 200);
  x.fillStyle = '#c4622d';
  x.fillRect(0, 186, 320, 14);
  x.font = '8px Silkscreen';
  const lines = [['#ffd36b', header.name], ['#f4ead8', header.author], ['#b2a8c6', header.released], ['', ''], ['#f2a23c', `${header.model || 'SID'} ${header.clock}`.trim()]];
  lines.forEach(([c, l], i) => ((x.fillStyle = c || '#000'), x.fillText(l.slice(0, 38), 12, 40 + i * 18)));
  return c;
}

function animate() {
  const tick = (ms) => {
    if (!$('#tunes').hidden && !document.hidden) {
      viz.update(player.playing && !player.paused ? player.analyser : null, player.regs);
      viz.draw(ms / 1000);
    }
    requestAnimationFrame(tick);
  };
  requestAnimationFrame(tick);
}
