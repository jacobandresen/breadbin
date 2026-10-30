// app - breadbin in the browser: the boot screen, tabs, deep links and search.
// The tabs (games.js, demos.js, tunes.js) offer only what the playtests showed
// working (catalogue.js); everything they show is fetched on demand and cached
// by the browser (store.js).

import * as ia from './ia.js';
import { verified, search } from './catalogue.js';
import { initGames, showGame, gameCard, coversReady } from './games.js';
import { initDemos, showDemo, demoCard } from './demos.js';
import { initTunes, filterTunes } from './tunes.js';
import { initHack } from './hack.js';
import { openSettings } from './settings.js';
import { $, h, toast, shelf } from './ui.js';

const views = { games: initGames, demos: initDemos, tunes: initTunes, hack: initHack };
const started = new Map();
const terms = {};
let current = '';

// the tape-loader boot screen stays until the first shelf is up
const boot = () => $('#boot').classList.add('done');
setTimeout(boot, 3500);

function route() {
  const [tab, item] = (location.hash.slice(1) || 'games').split('/');
  current = tab in views ? tab : 'games';
  for (const v of Object.keys(views)) $(`#${v}`).hidden = v !== current;
  for (const a of document.querySelectorAll('.tab')) a.setAttribute('aria-selected', String(a.dataset.tab === current));
  $('#search').placeholder = { games: 'Search games…', demos: 'Search demos…', tunes: 'Filter tunes…' }[current] || '';
  $('#search').closest('label').hidden = current === 'hack';
  $('#search').value = terms[current] || '';
  if (!started.has(current)) started.set(current, Promise.resolve(views[current]()).then(boot, boot));
  if (item && (current === 'games' || current === 'demos')) openItem(current, decodeURIComponent(item));
}

/** Deep link (#games/<id>, #demos/<id>). Ids not on the verified list open too,
 *  which is how the playtests reach new titles. */
async function openItem(tab, id) {
  try {
    const v = await verified();
    const known = (tab === 'games' ? v.games : v.demos).find((x) => x.id === id);
    if (known) return tab === 'games' ? showGame(known) : showDemo(known);
    const boot = new URLSearchParams(location.search).get('boot') || '';
    if (id.startsWith('gb64:')) {
      await coversReady;
      const gb = id.slice(5);
      return showGame({ id, gb, title: gb.split('/').pop().replace(/_\d+_\d+\.zip$/i, ''), run: new URLSearchParams(location.search).get('run') || '', boot });
    }
    const m = (await ia.metadata(id)).metadata;
    if (!m) throw new Error('no such item');
    const first = (x) => (Array.isArray(x) ? x[0] : x);
    const title = ia.displayTitle(String(first(m.title) || id));
    const item = { id, ia: id, boot, title, group: String(first(m.csdb_group) || '').replace(/,\s*$/, ''), publisher: String(first(m.creator) || '') };
    return tab === 'games' ? showGame(item) : showDemo(item);
  } catch (e) {
    toast(`Couldn't open ${id}: ${e.message}`);
  }
}

$('#detail').addEventListener('close', () => history.replaceState(null, '', `#${current}`));

// ── search ───────────────────────────────────────────────────────────────────

let searchTimer = 0;
$('#search').addEventListener('input', (e) => {
  clearTimeout(searchTimer);
  const term = (terms[current] = e.target.value.trim());
  searchTimer = setTimeout(() => runSearch(current, term), current === 'tunes' ? 100 : 250);
});

async function runSearch(tab, term) {
  if (tab === 'tunes') return filterTunes(term);
  const root = $(`#${tab}`);
  root.querySelector('.results')?.remove();
  root.querySelector('.shelves').hidden = !!term;
  if (!term) return;
  const v = await verified();
  const hits = search(tab === 'games' ? v.games : v.demos, term);
  const results = h(`div.results${tab === 'demos' ? '.wide' : ''}`);
  root.prepend(results);
  if (!hits.length) return results.append(h('p.banner', {}, `Nothing playtested matches “${term}”.`));
  shelf(results, `Results for “${term}”`, hits, tab === 'games' ? gameCard : demoCard, { wide: tab === 'demos' })?.expand();
}

// ── the rest of the page ─────────────────────────────────────────────────────

$('#open-settings').addEventListener('click', openSettings);
for (const dlg of document.querySelectorAll('dialog')) dlg.addEventListener('click', (e) => e.target === dlg && dlg.close());
addEventListener('hashchange', route);
route();
