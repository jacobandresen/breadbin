// demos - the Demos tab: playtested CSDb favourites from the Internet Archive,
// by rating, by decade and by group.

import * as ia from './ia.js';
import { verified, recent } from './catalogue.js';
import { launch } from './emulator.js';
import { $, h, art, shelf, footer } from './ui.js';

// its most telling capture (tools/playtest picks the biggest), else the sixth: the first are the loading
const cover = (d) => [ia.fileUrl(d.id, `${d.shot || 'screenshot_06'}_thumb.jpg`), ia.thumb(d.id)];
const groupsOf = (d) => (d.group || '').split(/,\s*/).filter(Boolean);

export function demoCard(d) {
  return h('button.card', { onclick: () => launch('demo', d), title: d.title },
    art(cover(d), d.title, { badge: d.rating ? `${d.rating.toFixed(1)}★` : null, wip: d.wip, tag: d.rom1541 ? '💾 trackmo' : null }),
    h('div.card-title', {}, d.title),
    h('div.card-sub', {}, [d.group, d.year || ''].filter(Boolean).join(' · ')));
}

export async function initDemos() {
  const shelves = h('div.shelves.wide');
  $('#demos').append(shelves);
  const { demos: all } = await verified();
  // the ones still in the works get a shelf of their own, at the end
  const demos = all.filter((d) => !d.wip);
  if (!demos.length) return shelves.append(h('p.banner', {}, 'No playtested demos yet: run tools/playtest to build verified.json.'));
  const opts = { wide: true };
  shelf(shelves, 'Latest watched', await recent('demo'), demoCard, opts);
  shelf(shelves, 'Handpicked greats', demos.filter((d) => !d.rom1541), demoCard, opts); // demos.txt, oldest first
  shelf(shelves, 'Trackmos on the free 1541', demos.filter((d) => d.rom1541), demoCard, opts);
  shelf(shelves, 'One-file demos', demos.filter((d) => d.onefile), demoCard, opts);
  for (const decade of [2020, 2010, 2000, 1990, 1980]) {
    shelf(shelves, `The ${decade}s`, demos.filter((d) => d.year >= decade && d.year < decade + 10), demoCard, opts);
  }
  for (const g of new Set(demos.flatMap(groupsOf))) {
    const theirs = demos.filter((d) => groupsOf(d).includes(g));
    if (theirs.length >= 3) shelf(shelves, g, theirs, demoCard, opts);
  }
  shelf(shelves, 'Work in progress', all.filter((d) => d.wip), demoCard, opts);
  shelves.after(footer());
}
