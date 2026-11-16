// games - the Games tab: one grid of box art for the games whose controls
// passed a playtest (tools/playtest/games.txt, in that order).

import * as ia from './ia.js';
import { boxartIndex, canon } from './covers.js';
import { verified } from './catalogue.js';
import { launch } from './emulator.js';
import { $, h, art, shelf, footer } from './ui.js';

let covers = new Map();
export const coversReady = boxartIndex().then((m) => (covers = m));

const artFor = (g) => [covers.get(canon(g.title)), g.ia && ia.thumb(g.ia), g.shot && ia.gb64Shot(g.shot)];

export function gameCard(g) {
  const sub = [g.year || '', g.publisher].filter(Boolean).join(' · ');
  return h('button.card', { onclick: () => launch('game', g), title: g.title },
    art(artFor(g), g.title, { boxart: covers.has(canon(g.title)), badge: g.rating >= 4 ? '★' : null, wip: g.wip, tag: g.rom1541 ? '💾 real 1541' : null }),
    h('div.card-title', {}, g.title),
    sub && h('div.card-sub', {}, sub));
}

/** Build the shelves; resolves once the first one is on screen. */
export async function initGames() {
  const shelves = h('div.shelves');
  $('#games').append(shelves);
  const [{ games }] = await Promise.all([verified(), Promise.race([coversReady, new Promise((r) => setTimeout(r, 2500))])]);
  if (!games.length) return shelves.append(h('p.banner', {}, 'No playtested games yet: run tools/playtest to build verified.json.'));
  // the ones still in the works go last
  const ordered = [...games.filter((g) => !g.wip), ...games.filter((g) => g.wip)];
  const top = shelf(shelves, 'Games', ordered, gameCard, { grid: true, size: games.length });
  shelves.after(footer());
  return top.ready;
}
