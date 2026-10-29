// catalogue - what the shelves offer: verified.json, the games, demos and
// tunes the playtests (tools/playtest) showed working, best rated first.

import * as store from './store.js';

let list = null;

export function verified() {
  list ??= fetch(new URL('./verified.json', import.meta.url), { cache: 'no-cache' })
    .then((r) => (r.ok ? r.json() : {}))
    .catch(() => ({}))
    .then(({ games = [], demos = [], tunes = [] }) => ({
      games: games.map((g) => ({ ...g, id: g.gb ? `gb64:${g.gb}` : g.ia })),
      demos,
      tunes,
    }));
  return list;
}

/** Recently played items of `kind` ('game' or 'demo') still on the list. */
export async function recent(kind) {
  const v = await verified();
  const ok = new Set((kind === 'game' ? v.games : v.demos).map((x) => x.id));
  return store.recentPlays(kind).filter((p) => ok.has(p.item.id)).map((p) => p.item);
}

/** Items whose text contains every word of `term`. */
export function search(items, term) {
  const words = term.toLowerCase().split(/\s+/).filter(Boolean);
  return items.filter((x) => {
    const hay = `${x.title} ${x.publisher || ''} ${x.group || ''} ${x.genre || ''} ${x.party || ''}`.toLowerCase();
    return words.every((w) => hay.includes(w));
  });
}
