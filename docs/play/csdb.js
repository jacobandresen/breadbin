// csdb - release details from the C64 Scene Database for the Demos and Tunes
// cards. csdb.dk only sends CORS headers to sites on its allowlist, so the page
// checks once whether it may fetch directly, and otherwise goes through a relay:
// r.jina.ai's reader by default (returns the document untouched with
// X-Return-Format: html, ~20 requests a minute) or your own, set in Settings
// (see relay/worker.js). Requests are queued and answers cached for a month.

import { cachedText } from './store.js';
import { settings } from './settings.js';

const SITE = 'https://csdb.dk';
const JINA = 'https://r.jina.ai/';
const GAP_MS = 3200; // keeps the free relay under its rate limit
const MONTH = 30 * 24 * 3600e3;

export const releaseUrl = (id) => `${SITE}/release/?id=${id}`;

let direct = null;
const canFetchDirect = () =>
  (direct ??= fetch(`${SITE}/webservice/?type=release&id=72550&depth=1`).then((r) => r.ok, () => false));

// one relayed request at a time
let chain = Promise.resolve();
let last = 0;
function relayed(url) {
  const custom = settings.relay;
  const run = async () => {
    if (custom) return fetch(custom.replace('{url}', encodeURIComponent(url)));
    await new Promise((r) => setTimeout(r, Math.max(0, last + GAP_MS - Date.now())));
    last = Date.now();
    return fetch(JINA + url, { headers: { 'X-Return-Format': 'html' } });
  };
  const p = chain.then(run, run);
  chain = p.catch(() => {});
  return p;
}

const get = (url) => cachedText(url, MONTH, async () => ((await canFetchDirect()) ? fetch(url) : relayed(url)));

const text = (el, sel) => el?.querySelector(sel)?.textContent.trim() || '';
const num = (el, sel) => Number(text(el, sel)) || 0;

/** The parts of a release (webservice, depth 2) the cards show. */
export async function release(id) {
  const xml = await get(`${SITE}/webservice/?type=release&id=${id}&depth=2`);
  const rel = new DOMParser().parseFromString(xml.slice(xml.indexOf('<')), 'text/xml').querySelector('Release');
  if (!rel) throw new Error(`CSDb has no release ${id}`);
  const ev = rel.querySelector(':scope > ReleasedAt > Event');
  const groups = [...rel.querySelectorAll(':scope > ReleasedBy > Group')];
  const sid = rel.querySelector('UsedSIDs SID');
  const author = text(sid, 'Author'); // HVSC style: "Real Name (Handle)"
  const musicians = [];
  for (const c of rel.querySelectorAll(':scope > Credits > Credit')) {
    const handle = text(c.querySelector('Handle'), ':scope > Handle');
    if (!text(c, 'CreditType').startsWith('Music') || !handle || musicians.some((m) => m.handle === handle)) continue;
    musicians.push({ handle, realName: author.includes(`(${handle})`) ? author.split(' (')[0] : '', country: text(c, 'Country') });
  }
  const ach = rel.querySelector(':scope > Achievement');
  const g = groups[0];
  return {
    rating: num(rel, ':scope > Rating'),
    screenshot: text(rel, ':scope > ScreenShot'),
    event: text(ev, ':scope > Name'),
    eventPlace: [text(ev, 'City'), text(ev, 'Country')].filter(Boolean).join(', '),
    compo: ach ? { name: text(ach, 'Compo'), place: num(ach, 'Place') } : null,
    group: g && {
      name: groups.map((x) => text(x, ':scope > Name')).join(' & '),
      short: text(g, ':scope > Short'),
      slogan: text(g, ':scope > Slogan'),
      founded: num(g, ':scope > FoundYear'),
    },
    musicians,
    sidModel: text(sid, 'SIDModel'),
    clock: text(sid, 'ClockSpeed'),
    downloads: [...rel.querySelectorAll('DownloadLink Downloads')].reduce((n, d) => n + (Number(d.textContent) || 0), 0),
  };
}
