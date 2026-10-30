// games - the Games tab: one grid of box art for the games whose controls
// passed a playtest (tools/playtest/games.txt, in that order).

import * as ia from './ia.js';
import { boxartIndex, canon } from './covers.js';
import { verified } from './catalogue.js';
import { launch } from './emulator.js';
import { $, h, art, shelf, facts, closeButton } from './ui.js';

let covers = new Map();
export const coversReady = boxartIndex().then((m) => (covers = m));

const artFor = (g) => [covers.get(canon(g.title)), g.ia && ia.thumb(g.ia), g.shot && ia.gb64Shot(g.shot)];

export function gameCard(g) {
  const sub = [g.year || '', g.publisher].filter(Boolean).join(' · ');
  return h('button.card', { onclick: () => showGame(g), title: g.title },
    art(artFor(g), g.title, { boxart: covers.has(canon(g.title)), badge: g.rating >= 4 ? '★' : null, tag: ia.onlineAt(g) && '🌐 archive.org' }),
    h('div.card-title', {}, g.title),
    sub && h('div.card-sub', {}, sub));
}

/** Build the shelves; resolves once the first one is on screen. */
export async function initGames() {
  const shelves = h('div.shelves');
  $('#games').append(shelves);
  const [{ games }] = await Promise.all([verified(), Promise.race([coversReady, new Promise((r) => setTimeout(r, 2500))])]);
  if (!games.length) return shelves.append(h('p.banner', {}, 'No playtested games yet: run tools/playtest to build verified.json.'));
  const online = games.filter(ia.onlineAt).length;
  shelves.before(h('p.banner.online', {}, '🌐', h('span', {},
    h('b', {}, `${online} of these ${games.length} games are already free to play online`),
    ' at the Internet Archive, in its own browser emulator. breadbin gathers the ones that run well here; each game links to its archive.org copy.')));
  const top = shelf(shelves, 'Games', games, gameCard, { grid: true, size: games.length });
  return top.ready;
}

export async function showGame(g) {
  history.replaceState(null, '', `#games/${encodeURIComponent(g.id)}`); // shareable
  const dlg = $('#detail');
  const info = facts();
  const links = h('div.links');
  if (g.gb) links.append(h('a', { href: 'https://www.gb64.com/', target: '_blank', rel: 'noopener' }, 'GameBase64 ↗'));
  dlg.replaceChildren(closeButton(dlg), h('div.sheet-body', {},
    h('div.sheet-art', {}, art(artFor(g), g.title, { boxart: true })),
    h('div', {},
      h('h2', {}, g.title),
      h('p.by', {}, [g.publisher, g.year || ''].filter(Boolean).join(' · ')),
      info,
      h('div.actions', {}, h('button.key.pill.primary', { onclick: () => launch('game', g) }, '▶ Play')),
      ia.onlineAt(g)
        ? h('a.online', { href: ia.onlineAt(g), target: '_blank', rel: 'noopener' },
          h('b', {}, '🌐 Already free to play online'), h('span', {}, 'Internet Archive (archive.org) · runs in your browser ↗'))
        : h('p.online.none', {}, 'Not found online elsewhere: this version is only on GameBase64.'),
      links,
      g.shot && h('div.shots.solo', {}, h('img', { src: ia.gb64Shot(g.shot), alt: '', loading: 'lazy' })),
      h('p.hint', {}, `Joystick in port ${g.port || 2}: cursor keys + space, a gamepad, or the on-screen pad. Runs on the free MEGA65 Open ROMs.`))));
  dlg.showModal();
  info.add('Genre', [g.genre, g.sub].filter(Boolean).join(' · '));
  info.add('Rating', g.rating ? `${'★'.repeat(g.rating)} (GameBase64)` : '');
  info.add('Classic', g.classic ? 'A GameBase64 classic' : '');
}
