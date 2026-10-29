// csdb - the C64 Scene Database. csdb.dk only sends CORS headers to an allowlist
// of sites, so the page first checks whether it may fetch directly. If not,
// requests go through a relay: r.jina.ai's reader by default, which returns the
// page untouched (X-Return-Format: html) with CORS headers, or your own relay
// set in Settings (see relay/worker.js). The free relay handles text only and
// allows ~20 requests a minute, so requests are queued and every answer is
// cached: toplists for a day, release details for a month.

import { cachedBytes, cachedText, isCached, load } from './store.js';

const SITE = 'https://csdb.dk';
const DAY = 24 * 3600e3;
const JINA = 'https://r.jina.ai/';

export const TOP_DEMOS = `${SITE}/toplist.php?type=release&subtype=(1)`;
export const TOP_MUSIC = `${SITE}/toplist.php?type=release&subtype=(7)`;
export const releaseUrl = (id) => `${SITE}/release/?id=${id}`;
const releaseWs = (id, depth) => `${SITE}/webservice/?type=release&id=${id}&depth=${depth}`;

/** The relay as a URL template ("{url}" is replaced), or '' for r.jina.ai. */
function relay() {
  return load('settings', {}).relay || '';
}

// One request at a time, spaced to stay under the free relay's rate limit.
let chain = Promise.resolve();
let last = 0;
const GAP_MS = 3200;

function relayed(url) {
  const custom = relay();
  const run = async () => {
    if (!custom) {
      const wait = last + GAP_MS - Date.now();
      if (wait > 0) await new Promise((r) => setTimeout(r, wait));
      last = Date.now();
    }
    const target = custom ? custom.replace('{url}', encodeURIComponent(url)) : JINA + url;
    const init = custom ? {} : { headers: { 'X-Return-Format': 'html' } };
    let res = await fetch(target, init);
    if (res.status === 429 && !custom) {
      await new Promise((r) => setTimeout(r, 20000));
      res = await fetch(target, init);
    }
    return res;
  };
  const p = chain.then(run, run);
  chain = p.catch(() => {});
  return p;
}

/** Whether csdb.dk lets this page fetch it directly (checked once). */
let direct = null;
function canFetchDirect() {
  direct ??= fetch(`${SITE}/webservice/?type=release&id=72550&depth=1`).then((r) => r.ok, () => false);
  return direct;
}

const get = (url, maxAgeMs) =>
  cachedText(url, maxAgeMs, async () => ((await canFetchDirect()) ? fetch(url) : relayed(url)));

/** True when CSDb's own download links can be fetched (binary files need
 *  direct access or your own relay; the free reader only does text). */
export async function canFetchFiles() {
  return (await canFetchDirect()) || !!relay();
}

/** Download a file linked from CSDb, cached for good. */
export async function fetchFile(url, onProgress) {
  url = url.replace(/^http:/, 'https:');
  if (await canFetchDirect()) return cachedBytes(url, { onProgress });
  const custom = relay();
  if (!custom) throw new Error('CSDb downloads need direct access or your own relay (see Settings)');
  return cachedBytes(custom.replace('{url}', encodeURIComponent(url)), { onProgress });
}

// ── toplists ─────────────────────────────────────────────────────────────────

/** Ranked [{ id, name, group, rating }] from a CSDb toplist page. */
export async function toplist(url) {
  const doc = new DOMParser().parseFromString(await get(url, DAY), 'text/html');
  const out = [];
  for (const tr of doc.querySelectorAll('tr')) {
    if (tr.querySelector('tr')) continue; // layout rows wrapping the list
    const link = tr.querySelector('a[href*="/release/?id="]');
    const id = link && Number(/id=(\d+)/.exec(link.getAttribute('href'))?.[1]);
    if (!id || out.some((r) => r.id === id)) continue;
    const groups = [...tr.querySelectorAll('a[href*="/group/?id="], a[href*="/scener/?id="]')].map((a) => a.textContent.trim());
    const rating = Number(tr.querySelector('font[size="1"]')?.textContent.trim());
    const name = link.textContent.trim();
    if (name) out.push({ id, name, group: groups.join(', '), rating: Number.isFinite(rating) ? rating : 0 });
  }
  return out;
}

// ── release details ─────────────────────────────────────────────────────────

const text = (el, sel) => el?.querySelector(sel)?.textContent.trim() || '';
const num = (el, sel) => Number(text(el, sel)) || 0;

/** Parse a release webservice document (the parts breadbin shows). */
function parseRelease(xml) {
  const doc = new DOMParser().parseFromString(xml.slice(xml.indexOf('<')), 'text/xml');
  const rel = doc.querySelector('Release');
  if (!rel) return null;
  const ev = rel.querySelector(':scope > ReleasedAt > Event');
  const groups = [...rel.querySelectorAll(':scope > ReleasedBy > Group')];
  const g = groups[0];
  const sid = rel.querySelector('UsedSIDs SID');
  const author = text(sid, 'Author');
  const musicians = [];
  for (const c of rel.querySelectorAll(':scope > Credits > Credit')) {
    if (!text(c, 'CreditType').startsWith('Music')) continue;
    const h = c.querySelector('Handle');
    const handle = text(h, ':scope > Handle') || text(h, 'Handle');
    if (!handle || musicians.some((m) => m.handle === handle)) continue;
    const real = author.includes(`(${handle})`) ? author.split(' (')[0] : '';
    musicians.push({ handle, realName: real, country: text(c, 'Country') });
  }
  const ach = rel.querySelector(':scope > Achievement');
  return {
    id: num(rel, ':scope > ID'),
    name: text(rel, ':scope > Name'),
    type: text(rel, ':scope > Type'),
    year: num(rel, ':scope > ReleaseYear'),
    rating: num(rel, ':scope > Rating'),
    screenshot: text(rel, ':scope > ScreenShot'),
    event: text(ev, ':scope > Name'),
    eventPlace: [text(ev, 'City'), text(ev, 'Country')].filter(Boolean).join(', '),
    compo: ach ? { name: text(ach, 'Compo'), place: num(ach, 'Place') } : null,
    group: g
      ? {
          name: groups.map((x) => text(x, ':scope > Name')).join(' & '),
          short: text(g, ':scope > Short'),
          slogan: text(g, ':scope > Slogan'),
          founded: num(g, ':scope > FoundYear'),
          country: text(g, ':scope > BaseCountry'),
        }
      : null,
    musicians,
    hvscPath: text(sid, 'HVSCPath'),
    sidModel: text(sid, 'SIDModel'),
    clock: text(sid, 'ClockSpeed'),
    songs: num(sid, 'NOSongs'),
    downloads: [...rel.querySelectorAll('DownloadLink Downloads')].reduce((n, d) => n + (Number(d.textContent) || 0), 0),
    links: [...rel.querySelectorAll('DownloadLink')].filter((l) => text(l, 'Status') !== 'Dead').map((l) => text(l, 'Link')).filter(Boolean),
  };
}

/** Release details (depth 2: credits, SIDs, group), cached for a month. */
export async function release(id) {
  const info = parseRelease(await get(releaseWs(id, 2), 30 * DAY));
  if (!info) throw new Error(`CSDb has no release ${id}`);
  return info;
}

/** Screenshot URLs to try for a release (CSDb files them by id). */
export function shotUrls(id) {
  const dir = `${SITE}/gfx/releases/${Math.floor(id / 1000) * 1000}/${id}`;
  return [`${dir}.png`, `${dir}.gif`, `${dir}.jpg`];
}

/** Release details only if already cached, without touching the network. */
export async function cachedRelease(id) {
  return (await isCached(releaseWs(id, 2))) ? release(id) : null;
}
