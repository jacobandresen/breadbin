// demos - the Demos tab: playtested CSDb favourites from the Internet Archive,
// by rating, by decade and by group.

import * as ia from './ia.js';
import * as csdb from './csdb.js';
import { verified, recent } from './catalogue.js';
import { launch } from './emulator.js';
import { $, h, art, shelf, facts, closeButton } from './ui.js';

// IA's own thumbnail is often the loading screen; a later capture shows the demo
const cover = (d, size = '_thumb') => [ia.fileUrl(d.id, `screenshot_06${size}.jpg`), ia.thumb(d.id)];
const groupsOf = (d) => (d.group || '').split(/,\s*/).filter(Boolean);

export function demoCard(d) {
  return h('button.card', { onclick: () => showDemo(d), title: d.title },
    art(cover(d), d.title, { badge: d.rating ? `${d.rating.toFixed(1)}★` : null }),
    h('div.card-title', {}, d.title),
    h('div.card-sub', {}, [d.group, d.year || ''].filter(Boolean).join(' · ')));
}

export async function initDemos() {
  const shelves = h('div.shelves.wide');
  $('#demos').append(shelves);
  const { demos } = await verified();
  if (!demos.length) return shelves.append(h('p.banner', {}, 'No playtested demos yet: run tools/playtest to build verified.json.'));
  const opts = { wide: true };
  shelf(shelves, 'Latest watched', await recent('demo'), demoCard, opts);
  shelf(shelves, 'CSDb top rated', demos, demoCard, opts);
  shelf(shelves, 'One-file demos', demos.filter((d) => d.onefile), demoCard, opts);
  for (const decade of [2020, 2010, 2000, 1990, 1980]) {
    shelf(shelves, `The ${decade}s`, demos.filter((d) => d.year >= decade && d.year < decade + 10), demoCard, opts);
  }
  for (const g of new Set(demos.flatMap(groupsOf))) {
    const theirs = demos.filter((d) => groupsOf(d).includes(g));
    if (theirs.length >= 3) shelf(shelves, g, theirs, demoCard, opts);
  }
}

export async function showDemo(d) {
  history.replaceState(null, '', `#demos/${encodeURIComponent(d.id)}`); // shareable
  const dlg = $('#detail');
  const info = facts();
  const shots = h('div.shots');
  let hero = art(cover(d, ''), d.title);
  dlg.replaceChildren(closeButton(dlg), h('div.sheet-body', {},
    h('div.sheet-art.wide', {}, hero),
    h('div', {},
      h('h2', {}, d.title),
      h('p.by', {}, [d.group, d.year || ''].filter(Boolean).join(' · ')),
      info,
      h('div.actions', {}, h('button.key.pill.primary', { onclick: () => launch('demo', d) }, '▶ Run')),
      h('div.links', {},
        h('a', { href: ia.itemUrl(d.id), target: '_blank', rel: 'noopener' }, 'Internet Archive ↗'),
        d.csdb && h('a', { href: csdb.releaseUrl(d.csdb), target: '_blank', rel: 'noopener' }, 'CSDb ↗')),
      shots)));
  dlg.showModal();
  info.add('CSDb rating', d.rating ? `${d.rating.toFixed(2)} ★` : '');
  info.add('Party', d.party);
  try {
    const meta = await ia.metadata(d.id);
    // the archive's first captures are usually the C64 loading the demo, and
    // a still picture yields the same capture over and over (same file size)
    const all = (meta.files || []).filter((f) => /^screenshot_\d+\.jpg$/.test(f.name));
    const sizes = new Set();
    const pics = all
      .slice(all.length > 8 ? 3 : 0)
      .filter((f) => !sizes.has(f.size) && sizes.add(f.size))
      .map((f) => f.name)
      .slice(0, 12);
    for (const p of pics) {
      shots.append(h('img', { src: ia.fileUrl(d.id, p.replace('.jpg', '_thumb.jpg')), alt: '', loading: 'lazy', onclick: () => hero.replaceWith((hero = art([ia.fileUrl(d.id, p)], d.title))) }));
    }
  } catch {}
  if (d.csdb) {
    try {
      const r = await csdb.release(d.csdb);
      if (r.compo?.place) info.add('Compo', `#${r.compo.place} in ${r.compo.name}`);
      info.add('Where', r.eventPlace);
    } catch {}
  }
}
