// covers - libretro's C64 box art, matched to archive titles the same way the
// native app does (core::norm + disk::game_title). The file listing is one
// GitHub API call, fetched after the first shelves are up and cached for a week.

import { cachedJSON } from './store.js';

const REPO = 'https://api.github.com/repos/libretro-thumbnails/Commodore_-_64';
const RAW = 'https://raw.githubusercontent.com/libretro-thumbnails/Commodore_-_64/master/Named_Boxarts/';
const WEEK = 7 * 24 * 3600e3;

const ROMAN = { i: '1', ii: '2', iii: '3', iv: '4', v: '5', vi: '6', vii: '7', viii: '8', ix: '9', x: '10' };
const ABBREV = { jr: 'junior', bros: 'brothers', intl: 'international', vs: 'versus', n: 'and' };

/** Port of core::norm: a comparison key for titles. */
export function norm(s) {
  return s
    .toLowerCase()
    .replace(/&/g, ' and ')
    .replace(/\b(jr|bros|intl|vs|n)\b\.?/g, (_, w) => ABBREV[w])
    .replace(/\b(i{1,3}|iv|v|vi{0,3}|ix|x)\b/g, (_, r) => ROMAN[r])
    .replace(/\b(the|a)\b/g, ' ')
    .replace(/[^a-z0-9]/g, '');
}

/** Port of disk::game_title: drop release tags, subtitles and versions. */
export function canon(title) {
  let t = title.split(/\s*[([]/)[0];
  t = t.split(/:| - /)[0];
  return norm(t.replace(/\bv\d+(\.\d+)*\b/g, ''));
}

function regionRank(name) {
  const n = name.toLowerCase();
  if (n.includes('(world')) return 0;
  if (n.includes('(europe')) return 1;
  if (n.includes('(usa')) return 2;
  return 3;
}

let index = null;

/** canon -> boxart URL, or an empty map when GitHub is unreachable. */
export function boxartIndex() {
  index ??= (async () => {
    try {
      const root = await cachedJSON(`${REPO}/contents/`, WEEK);
      const dir = root.find((e) => e.name === 'Named_Boxarts');
      const tree = await cachedJSON(`${REPO}/git/trees/${dir.sha}`, WEEK);
      const best = new Map();
      for (const { path } of tree.tree) {
        if (!path.endsWith('.png')) continue;
        const key = norm(path.slice(0, -4).split(/\s*\(/)[0]);
        const rank = regionRank(path);
        if (key && (!best.has(key) || rank < best.get(key).rank)) best.set(key, { rank, path });
      }
      return new Map([...best].map(([k, v]) => [k, RAW + encodeURIComponent(v.path)]));
    } catch {
      return new Map();
    }
  })();
  return index;
}
