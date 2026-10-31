// playtest - find out which games, demos and tunes actually work in breadbin's
// browser version, so the web UI only offers those (docs/play/verified.json).
//
//   node playtest.mjs games [--workers 2] [--retest] [--ids a,b]   # the titles in games.txt
//   node playtest.mjs demos [--workers 2] [--retest]      # the demos in demos.txt
//   node playtest.mjs demos --scout [--top 60]            # CSDb's best that the Archive has, to pick from
//   node playtest.mjs tunes [--retest]                    # the tunes in tunes.txt
//   node playtest.mjs write            # results.json -> docs/play/verified.json (+ online copies)
//   node playtest.mjs candidates       # show the games.txt matches
//
// Games and demos are played in headless Chromium against a local copy of the
// site (`make serve-web`, or --base URL): the harness presses Play, waits for the
// boot, then sends FIRE, SPACE, F1 and stick moves, and classifies screenshots:
// a BASIC screen or an error means failure. A game passes only when holding the
// joystick moves the player (its port is recorded); a demo, when it moves.
// Tunes run in the engine's SID player (breadbin.wasm) and must produce sound. Candidates: games from the GameBase64
// database (rating, classic flag, Internet Archive popularity), demos and tunes
// from CSDb's toplists. Downloads are cached in cache/; results are kept in
// results.json, so runs resume and only new titles are tested.

import fs from 'fs';
import os from 'os';
import zlib from 'zlib';
import path from 'path';
import { fileURLToPath } from 'url';
import { DatabaseSync } from 'node:sqlite';
import puppeteer from 'puppeteer-core';
import { audioTap, loudness } from './c64.mjs';
import { PNG } from 'pngjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const { canon } = await import('../../docs/play/covers.js');
const { sources } = await import('../../docs/play/hvsc.js');
const REPO = path.resolve(HERE, '../..');
const RESULTS = path.join(HERE, 'results.json');
const VERIFIED = path.join(REPO, 'docs/play/verified.json');
const CACHE = path.join(HERE, 'cache');
const UA = { 'user-agent': 'breadbin-playtest (+https://github.com/jacobandresen/breadbin)' };

const [cmd = 'help', ...rest] = process.argv.slice(2);
const flag = (name, fallback) => {
  const i = rest.indexOf(`--${name}`);
  return i < 0 ? fallback : rest[i + 1] ?? true;
};
/** games.txt: the titles breadbin offers, in order. "[1541]" at the end of a line marks a game that needs a real drive. */
const RAW_LINES = fs.readFileSync(path.join(HERE, 'games.txt'), 'utf8').split('\n').filter((l) => l.trim() && !l.startsWith('#'));
const PICK_LINES = RAW_LINES.map((l) => l.replace(/\s*\[1541\]\s*$/, '').replace(/\s*\{[^}]*\}/, ''));
/** "{addr:old=new; ...}" on a line: live patches for the game's loader (drive.rs), in hex. */
const PATCHES = new Map(RAW_LINES.filter((l) => /\{[^}]*\}/.test(l)).map((l) => [canon(l.split(' = ')[0]), /\{([^}]*)\}/.exec(l)[1].split(';').map((p) => p.replace(/\s+/g, '')).filter(Boolean)]));
/** Games whose loader runs code on the drive: shelved as "needs a 1541 ROM", tested with one. */
const NEEDS_1541 = new Set(RAW_LINES.filter((l) => /\[1541\]\s*$/.test(l)).map((l) => canon(l.split(' = ')[0].replace(/\s*\[1541\]\s*$/, ''))));
/** A 1541 ROM the playtest installs for those (ROM1541=<file>) instead of breadbin's free one (tools/drive). */
const ROM1541 = process.env.ROM1541 || '';
const PICKS = PICK_LINES.map((l) => l.split(' = ')[0].trim());
/** "Title = source > file": a GameBase64 path or an Internet Archive id, and the disk file to boot. */
const PINS = new Map(
  PICK_LINES.filter((l) => l.includes(' = ')).map((l) => {
    const [source, boot = ''] = l.split(' = ')[1].split(' > ').map((x) => x.trim());
    return [canon(l.split(' = ')[0]), { source, boot }];
  }),
);
const TOP = Number(flag('top', cmd === 'tunes' ? 200 : 150));
/** demos.txt and tunes.txt: the handpicked demos (CSDb release ids) and tunes (HVSC paths), in shelf order. */
const listed = (file) => fs.readFileSync(path.join(HERE, file), 'utf8').split('\n').filter((l) => l.includes(' = ') && !l.startsWith('#'))
  .map((l) => l.split(' = ').map((x) => x.trim()));
// "[1541]" after a demo's id: a trackmo, run on the emulated drive (tools/drive)
const DEMO_PICKS = listed('demos.txt').map(([title, id]) => ({ title, csdb: Number(id.replace(/\s*\[1541\]$/, '')), ...(/\[1541\]$/.test(id) && { rom1541: 1 }) }));
const TUNE_PICKS = listed('tunes.txt').map(([name, hvsc]) => ({ name, hvsc }));
const WORKERS = Number(flag('workers', 2));
const RETEST = rest.includes('--retest');
const IDS = flag('ids', '') ? String(flag('ids')).split(',') : null;
const BASE = flag('base', 'http://localhost:8064/play/');
const SHOTS = flag('shots', null);
const CHROME = process.env.CHROME || '/usr/bin/chromium';

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const results = fs.existsSync(RESULTS) ? JSON.parse(fs.readFileSync(RESULTS, 'utf8')) : {};
for (const k of ['games', 'demos', 'tunes']) results[k] ??= {};
/** Write back `kind`'s results only: other runs may be saving theirs. */
const saveResults = (kind) => {
  const onDisk = fs.existsSync(RESULTS) ? JSON.parse(fs.readFileSync(RESULTS, 'utf8')) : {};
  fs.writeFileSync(RESULTS, JSON.stringify({ ...onDisk, [kind]: results[kind] }, null, 1));
};

/** GET `url`, keeping a copy in cache/ for `days`. */
async function cached(name, url, days, transform = (b) => b) {
  const file = path.join(CACHE, name);
  if (fs.existsSync(file) && Date.now() - fs.statSync(file).mtimeMs < days * 864e5) return fs.readFileSync(file);
  const res = await fetch(url, { headers: UA });
  if (!res.ok) throw new Error(`${url}: ${res.status}`);
  const data = transform(Buffer.from(await res.arrayBuffer()));
  fs.mkdirSync(CACHE, { recursive: true });
  fs.writeFileSync(file, data);
  await sleep(1000); // go easy on csdb.dk, hvsc.de and the archive
  return data;
}

const utf8 = new TextDecoder('utf-8', { fatal: true });
const cp1252 = new TextDecoder('windows-1252');
/** GameBase64 text is UTF-8 or Windows-1252. */
const text = (b) => {
  if (b == null) return '';
  try {
    return utf8.decode(b).trim();
  } catch {
    return cp1252.decode(b).trim();
  }
};

// ── candidates ───────────────────────────────────────────────────────────────

/** Internet Archive C64 games: canonical title -> { id, downloads } (most downloaded). */
async function archivePopularity() {
  const file = path.join(CACHE, 'ia-games.json');
  if (fs.existsSync(file) && Date.now() - fs.statSync(file).mtimeMs < 7 * 864e5) return new Map(JSON.parse(fs.readFileSync(file)));
  const best = new Map();
  let cursor = '';
  do {
    const params = new URLSearchParams({ q: 'collection:softwarelibrary_c64_games', fields: 'identifier,title,downloads', count: '10000' });
    if (cursor) params.set('cursor', cursor);
    const page = await (await fetch(`https://archive.org/services/search/v1/scrape?${params}`, { headers: UA })).json();
    for (const it of page.items || []) {
      const key = canon(String(it.title || it.identifier));
      if (!best.has(key) || (it.downloads || 0) > best.get(key).downloads) best.set(key, { id: it.identifier, downloads: it.downloads || 0 });
    }
    cursor = page.cursor || '';
  } while (cursor);
  fs.mkdirSync(CACHE, { recursive: true });
  fs.writeFileSync(file, JSON.stringify([...best]));
  return best;
}

/**
 * The games picked in games.txt, from GameBase64 (versions that don't need true
 * drive emulation, unless the game is marked [1541]). Each keeps its Internet Archive item (when there is one)
 * as a fallback source and for box art.
 */
async function gameCandidates() {
  await cached('gb64.sqlitedb', 'https://www.twinbirds.com/gamebase64browser/GBC_v19.sqlitedb.gz', 365, (b) => (b[0] === 0x1f && b[1] === 0x8b ? zlib.gunzipSync(b) : b));
  const popular = await archivePopularity();
  const db = new DatabaseSync(path.join(CACHE, 'gb64.sqlitedb'), { readOnly: true });
  const rows = db.prepare(`
    SELECT CAST(g.Name AS BLOB) AS name, g.Filename AS file, g.FileToRun AS run, g.ScrnshotFilename AS shot,
           g.Rating AS rating, g.Classic AS classic, y.Year AS year, CAST(p.Publisher AS BLOB) AS publisher,
           pg.ParentGenre AS genre, ge.Genre AS sub, g.V_TrueDriveEmu AS truedrive
    FROM Games g
    LEFT JOIN Years y ON y.YE_Id = g.YE_Id
    LEFT JOIN Publishers p ON p.PU_Id = g.PU_Id
    LEFT JOIN Genres ge ON ge.GE_Id = g.GE_Id
    LEFT JOIN PGenres pg ON pg.PG_Id = ge.PG_Id
    WHERE g.Adult = 0 AND g.Filename <> ''`).all();
  const slash = (p) => (p || '').replace(/\\/g, '/');
  const best = new Map();
  for (const r of rows) {
    const title = text(r.name).replace(/^(.*), (The|A|An)$/, '$2 $1');
    const publisher = text(r.publisher);
    const key = canon(title);
    if (r.truedrive && !NEEDS_1541.has(key)) continue;
    const pop = popular.get(key) || { downloads: 0, id: '' };
    const g = {
      id: `gb64:${slash(r.file)}`, gb: slash(r.file), run: r.run || '', title,
      rating: r.rating || 0, classic: r.classic ? 1 : 0, genre: r.genre || '', sub: r.sub || '',
      year: r.year > 1900 ? r.year : 0, publisher: /^\(/.test(publisher) ? '' : publisher,
      shot: slash(r.shot), ia: pop.id, downloads: pop.downloads,
      ...(NEEDS_1541.has(key) && { rom1541: 1 }),
      ...(PATCHES.has(key) && { patches: PATCHES.get(key) }),
    };
    const old = best.get(key);
    const pin = PINS.get(key)?.source;
    if (pin?.includes('/') ? pin === g.gb : !old || g.rating + g.classic > old.rating + old.classic) best.set(key, g);
  }
  const missing = PICKS.filter((t) => !best.has(canon(t)));
  if (missing.length) console.log(`not in GameBase64 (or needing true drive emulation): ${missing.join(', ')}`);
  // a pinned Internet Archive copy stands in for GameBase64's, keeping its facts
  // "a + b + c": a cartridge EasySplit cut across several Internet Archive items
  for (const [key, { source, boot }] of PINS) {
    const g = best.get(key);
    const parts = source.split(' + ');
    if (g && parts.length > 1) best.set(key, { ...g, id: parts[0], ia: parts[0], gb: '', run: '', boot, parts });
    else if (g && !source.includes('/')) best.set(key, { ...g, id: source, ia: source, gb: '', run: '', boot });
    else if (g && boot) g.boot = boot;
  }
  return PICKS.map((t) => best.get(canon(t))).filter(Boolean);
}

/** A CSDb toplist: [{ id, name, group, rating }] in rank order. */
// CSDb's HTML entities: numeric ones, accented letters (&eacute; -> é) and a few more
const MARKS = { acute: '\u0301', grave: '\u0300', circ: '\u0302', uml: '\u0308', tilde: '\u0303', ring: '\u030a', cedil: '\u0327' };
const NAMED = { amp: '&', quot: '"', apos: "'", lt: '<', gt: '>', nbsp: ' ', szlig: 'ß', aelig: 'æ', AElig: 'Æ', oslash: 'ø', Oslash: 'Ø' };
const unescape = (s) =>
  s
    .replace(/&#x([0-9a-f]+);/gi, (_, h) => String.fromCodePoint(parseInt(h, 16)))
    .replace(/&#(\d+);/g, (_, d) => String.fromCodePoint(Number(d)))
    .replace(/&([a-z])(acute|grave|circ|uml|tilde|ring|cedil);/gi, (_, c, m) => c + MARKS[m])
    .replace(/&(\w+);/g, (e, n) => NAMED[n] ?? e)
    .normalize('NFC')
    .trim();

async function csdbToplist(subtype) {
  const html = (await cached(`csdb-top-${subtype}.html`, `https://csdb.dk/toplist.php?type=release&subtype=(${subtype})`, 1)).toString();
  const out = [];
  for (const tr of html.split('<tr>')) {
    const m = /\/release\/\?id=(\d+)"[^>]*>([^<]+)<\/a>/.exec(tr);
    if (!m || out.some((r) => r.id === Number(m[1]))) continue;
    const by = tr.slice(tr.indexOf('</a>')).split('</td>')[0];
    const group = [...by.matchAll(/>([^<]+)<\/a>/g)].map((g) => unescape(g[1])).join(', ');
    const rating = Number(/<font size="?1"?>\s*([\d.]+)/.exec(tr)?.[1]) || 0;
    out.push({ id: Number(m[1]), name: unescape(m[2]), group, rating });
  }
  return out;
}

/** A CSDb release document (depth 2), cached for a month. */
const csdbRelease = async (id) =>
  (await cached(`csdb-${id}.xml`, `https://csdb.dk/webservice/?type=release&id=${id}&depth=2`, 30)).toString();
const field = (xml, tag) => new RegExp(`<${tag}>([^<]*)</${tag}>`).exec(xml)?.[1]?.trim() || '';
const party = (xml) => (xml.includes('<ReleasedAt>') ? field(xml.slice(xml.indexOf('<ReleasedAt>')), 'Name') : '');

/** CSDb's top demos and one-file demos that the Internet Archive has, with party and year. */
async function demoCandidates() {
  // --scout: CSDb's all-time toplists (demos and one-file demos) instead of
  // demos.txt, to find the ones worth picking
  const picks = rest.includes('--scout')
    ? [...(await csdbToplist(1)), ...(await csdbToplist(2))].sort((a, b) => b.rating - a.rating).map((d) => ({ title: d.name, csdb: d.id }))
    : DEMO_PICKS;
  const out = [];
  for (let i = 0; i < picks.length && out.length < (rest.includes('--scout') ? TOP : picks.length); i += 40) {
    const chunk = picks.slice(i, i + 40);
    const q = `collection:softwarelibrary_c64_demos AND csdb:(${chunk.map((d) => `"https://csdb.dk/release/?id=${d.csdb}"`).join(' OR ')})`;
    const params = new URLSearchParams({ q, rows: '200', output: 'json' });
    for (const f of ['identifier', 'csdb', 'downloads']) params.append('fl[]', f);
    const docs = (await (await fetch(`https://archive.org/advancedsearch.php?${params}`, { headers: UA })).json()).response.docs;
    for (const d of chunk) {
      const items = docs.filter((x) => x.csdb === `https://csdb.dk/release/?id=${d.csdb}`);
      // multi-disk demos are several items; side A boots
      items.sort((a, b) => Number(/side_?b|_b$/i.test(a.identifier)) - Number(/side_?b|_b$/i.test(b.identifier)) || b.downloads - a.downloads);
      if (!items[0]) {
        if (!rest.includes('--scout')) console.log(`not on the Internet Archive: ${d.title}`);
        continue;
      }
      const xml = await csdbRelease(d.csdb);
      const group = [...xml.matchAll(/<ReleasedBy>[\s\S]*?<\/ReleasedBy>/g)].flatMap((b) => [...b[0].matchAll(/<Group>[\s\S]*?<Name>([^<]+)<\/Name>/g)].map((g) => g[1]));
      // a trackmo gets every side, for the Disk button
      const parts = d.rom1541 && items.length > 1 ? { parts: items.map((x) => x.identifier) } : {};
      out.push({ ...d, ...parts, id: items[0].identifier, group: [...new Set(group)].join(', '), rating: Number(field(xml, 'Rating')) || 0,
        onefile: /One-File Demo/i.test(field(xml, 'Type')) ? 1 : 0, party: party(xml), year: Number(field(xml, 'ReleaseYear')) || 0 });
    }
  }
  return out;
}

// ── screenshot analysis ─────────────────────────────────────────────────────

const decode = (buf) => PNG.sync.read(Buffer.from(buf));

/** Fraction of pixels close to an RGB colour. */
function share(img, [r, g, b], tol = 10) {
  let n = 0;
  const d = img.data;
  for (let i = 0; i < d.length; i += 16) {
    if (Math.abs(d[i] - r) <= tol && Math.abs(d[i + 1] - g) <= tol && Math.abs(d[i + 2] - b) <= tol) n++;
  }
  return n / (d.length / 16);
}

/** Fraction of pixels that changed between two frames. */
function change(a, b) {
  let n = 0;
  for (let i = 0; i < a.data.length; i += 16) {
    if (Math.abs(a.data[i] - b.data[i]) + Math.abs(a.data[i + 1] - b.data[i + 1]) + Math.abs(a.data[i + 2] - b.data[i + 2]) > 40) n++;
  }
  return n / (a.data.length / 16);
}

const BASIC_BLUE = [54, 51, 165];

// ── games and demos in the browser ──────────────────────────────────────────

/** Play one item; returns { ok, reason, ... }. */
/**
 * Which joystick port (1 or 2) controls the player, or 0. Either the player is
 * a sprite that moves right while the stick is held right and left while it is
 * held left (or down and up), which other sprites rarely do, or the screen
 * scrolls while the stick is held and not while it is left alone. Prompts on
 * screen (SPACE, F1, a trainer's Y/N...) are answered first; a title screen
 * gets FIRE, then the usual start keys, between tries.
 */
async function controlledPort({ inFrame, joy, key, screenText, proof, settle, picture, swap, title }) {
  const look = () =>
    inFrame('look', () => {
      // from the VIC itself: peeking $D000 reads RAM when a game banks the I/O out;
      // without $D015 every sprite counts, but one that never moves proves nothing
      const xy = wasm_sprite_info().split(',').map(Number);
      const sprites = Array.from({ length: 8 }, (_, i) => [xy[2 * i], xy[2 * i + 1]]);
      // only sprites inside the visible screen count
      const shown = sprites.map((p) => (p && p[0] > 0 && p[0] < 344 && p[1] > 29 && p[1] < 250 ? p : null));
      return { sprites: shown, scroll: (wasm_peek(0xd016) & 7) * 8 + (wasm_peek(0xd011) & 7) };
    });
  // hold the stick (`dirs`, none for idle) for 1.2 s: how far each sprite moved, how often the screen scrolled
  const hold = async (port, dirs, axis) => {
    const first = await look();
    for (const d of dirs) await joy(port + d);
    let last = first, scrolls = 0;
    for (let t = 0; t < 12; t++) {
      await sleep(100);
      const now = await look();
      if (now.scroll !== last.scroll) scrolls++;
      last = now;
    }
    if (dirs.length) await joy(port + 'RELEASE_XY');
    return { moved: first.sprites.map((p, i) => (p && last.sprites[i] ? last.sprites[i][axis] - p[axis] : 0)), scrolls };
  };
  // each way twice, so a sprite that happens to drift with the stick doesn't count;
  // diagonals for games like Q*bert that only move that way
  const WAYS = [
    [['PULL_RIGHT'], ['PULL_LEFT'], 0],
    [['PULL_DOWN'], ['PULL_UP'], 1],
    [['PULL_RIGHT', 'PULL_DOWN'], ['PULL_LEFT', 'PULL_UP'], 0],
    [['PULL_RIGHT', 'PULL_UP'], ['PULL_LEFT', 'PULL_DOWN'], 0],
  ];
  const responds = async (port) => {
    const idle = await hold(port, [], 0);
    for (const [plus, minus, axis] of WAYS) {
      const [a, b, c, d] = [await hold(port, plus, axis), await hold(port, minus, axis), await hold(port, plus, axis), await hold(port, minus, axis)];
      if (process.env.DEBUG) console.log(`  port ${port} ${plus}: ${a.moved}|${b.moved}|${c.moved}|${d.moved} scrolls ${a.scrolls},${b.scrolls},${c.scrolls},${d.scrolls} idle ${idle.moved} (${idle.scrolls})`);
      const sprite = a.moved.some((m, i) => m >= 6 && c.moved[i] >= 6 && b.moved[i] <= -6 && d.moved[i] <= -6 && Math.abs(idle.moved[i]) < 6);
      const scroll = idle.scrolls <= 1 && ((a.scrolls >= 4 && c.scrolls >= 4) || (b.scrolls >= 4 && d.scrolls >= 4));
      if (sprite || scroll) return true;
    }
    return false;
  };
  // the picture: when the player is no sprite the harness can read (multiplexed,
  // or drawn in characters, like a racing game's road), holding the stick must
  // change much more of the screen than leaving it alone does, both times
  const shows = async (port) => {
    const quiet = change(await picture(), (await sleep(1200), await picture()));
    for (const [plus, minus] of WAYS.slice(0, 2)) {
      const d = [];
      for (const dirs of [plus, minus, plus, minus]) {
        const a = await picture();
        for (const x of dirs) await joy(port + x);
        await sleep(1200);
        await joy(port + 'RELEASE_XY');
        d.push(change(a, await picture()));
      }
      if (process.env.DEBUG) console.log(`  port ${port} ${plus} picture: ${d.map((x) => x.toFixed(3))} quiet ${quiet.toFixed(3)}`);
      // a racing game's road is a small part of the screen: a low floor, but
      // four times the idle change, and twice
      const big = (x) => x > Math.max(0.008, 4 * quiet);
      if (quiet < 0.03 && ((big(d[0]) && big(d[2])) || (big(d[1]) && big(d[3])))) return true;
    }
    return false;
  };
  const type = (c) => inFrame('type', (c) => wasm_auto_type(c), c);
  const fire = async (p) => (await joy(p + 'PRESS_FIRE'), await sleep(200), await joy(p + 'RELEASE_FIRE'));
  // a trainer menu: cursor from the highlighted (reversed) line down to START GAME, RETURN
  const startGame = async (text) => {
    const rows = text.match(/.{40}/g) || [];
    const target = rows.findIndex((r) => /START GAME/.test(r));
    const reversed = await inFrame('menu', () => {
      const base = (3 - (wasm_peek(0xdd00) & 3)) * 0x4000 + (wasm_peek(0xd018) >> 4) * 0x400;
      return Array.from({ length: 25 }, (_, r) => Array.from({ length: 40 }, (_, c) => wasm_peek(base + r * 40 + c) >= 0x80).filter(Boolean).length);
    });
    const marked = reversed.findIndex((n) => n >= 4);
    const from = marked >= 0 && marked <= target ? marked : rows.findIndex((r) => /\?|YES|NO\b|ON\b|OFF\b/.test(r));
    const steps = target - Math.max(from, 0);
    for (let i = 0; i < steps; i++) await key(0, 7), await sleep(250);
    await key(0, 1);
    await sleep(1500);
    // menus driven by the joystick instead
    for (const port of [2, 1]) {
      if (!/START GAME/.test(await screenText())) return;
      for (let i = 0; i < steps; i++) await joy(port + 'PULL_DOWN'), await sleep(200), await joy(port + 'RELEASE_Y'), await sleep(200);
      await fire(port);
      await sleep(1500);
    }
  };
  let named = false;
  let swapped = false;
  // 1 then RETURN, as held keys, a few times: the shop takes a while to appear
  const buyCar = async () => {
    for (let i = 0; i < 4; i++) await sleep(4000), await key(7, 0), await sleep(600), await key(0, 1);
  };
  const prompts = [
    // a two-sided game: the next disk once, then on as the screen says
    [/INSERT SIDE 2|INSERT DISK 2|TURN (THE )?DISK|FLIP (THE )?DISK/, async (t) => {
      if (!swapped) (swapped = true), await swap(), await sleep(2000);
      if (/START GAME/.test(t)) await startGame(t);
      else if (/SPACE/.test(t)) await key(7, 4);
      else await fire(2);
      // the game loads from the new side: a key pressed meanwhile can restart it
      await sleep(25000);
    }],
    // a cartridge holding several games: the F-key whose line names this one
    [/F[1357]\)? [^@]{0,36}START/, async (t) => {
      const roman = { II: '2', III: '3', IV: '4' };
      const words = title.toUpperCase().split(/[^A-Z0-9]+/).map((w) => roman[w] || w).filter((w) => w.length > 1 || /\d/.test(w));
      const lines = (t.match(/.{40}/g) || []).filter((l) => /F[1357]\)? .*START/.test(l));
      const best = lines.map((l) => [l, words.filter((w) => new RegExp(`\\b${w}\\b`).test(l)).length]).sort((a, b) => b[1] - a[1])[0];
      const n = best && /F([1357])/.exec(best[0])[1];
      if (n) await key(0, { 1: 4, 3: 5, 5: 6, 7: 3 }[n]);
    }],
    [/START GAME/, startGame],
    [/RUN.?STOP\]? TO (START|PLAY)/, () => key(7, 7)],
    // Ghostbusters asks for a name, then for an account, in text drawn outside screen memory
    // (once: the title stays in screen memory); then it buys the first car
    [/^@*(GHOSTBUSTERS|CAZAFANTASMAS)@/, async () => (named = true, await type('breadbin\nn'), await buyCar()), true],
    [/HAVE AN ACCOUNT/, () => type('n')],
    [/YOUR NAME|ENTER NAME|NAME[^?:]{0,16}[?:]\s*$/, () => type('breadbin\n')],
    [/\[L\]OAD OR \[R\]ESTORE/, () => type('r')],
    [/CHALLENGE LEVEL|SELECT (A |THE )?(SKILL )?LEVEL/, () => key(7, 0), true], // 1: the easiest (WarGames, in a box of fill characters)
    [/H\/T|\[H\]IGH ?SCORE/, () => type('h')],
    [/Y\/N|\(Y\)|YES OR NO|TRAINER|CHEAT|INFINITE|UNLIMITED/, () => type('n')],
    [/^[^?]{0,160}\?{1,2}$/, () => type('n')], // one short question on the screen
    [/SPACE/, () => key(7, 4)],
    [/F7/, () => key(0, 3)],
    [/F1/, () => key(0, 4)],
    [/RETURN/, () => key(0, 1)],
  ];
  const starts = [() => {}, () => fire(2), () => fire(1), () => key(7, 4), () => key(0, 4), () => key(0, 3), () => key(0, 1)];
  // words, not just letters: a game drawn in character graphics reads as letters too
  const COMMON = /\b(THE|AND|PRESS|FIRE|TO|OF|YOU|YOUR|GAME|START|BY|FOR|IN|ON|PLAY|SPACE|KEY|IS|WITH|ALL|THIS|NOT|ARE|OR)\b/g;
  const isText = (t) => (t.match(/[A-Z]/g) || []).length >= 40 && (t.match(/[A-Z ]/g) || []).length > t.length * 0.7 && (t.match(COMMON) || []).length >= 3;
  // after each answered prompt the start keys begin again
  // each screen's prompt is answered at most twice (its text may stay on screen)
  const answered = new Map();
  for (let next = 0; next < starts.length; ) {
    await settle();
    const text = await screenText();
    if (/READY\.|BYTES FREE/.test(text)) return 0; // back at BASIC: nothing to control
    // prompts are only read off screens of text, not graphics shown as characters
    const wordy = (text.match(/[A-Z ]/g) || []).length > text.length * 0.7;
    const screen = text.replace(/\s+/g, ' ').trim();
    const prompt = (answered.get(screen) || 0) < 2 && prompts.find(([re, , any]) => (wordy || (any && !named)) && re.test(screen));
    if (process.env.DEBUG) console.log(`  ${prompt ? 'answer ' + prompt[0] : 'start ' + next} | ${text.replace(/\s+/g, ' ').trim().slice(0, 100)}`);
    if (prompt) {
      answered.set(screen, (answered.get(screen) || 0) + 1);
      next = 0;
      await prompt[1](text);
      await sleep(2500);
      continue;
    }
    await starts[next++]();
    await sleep(2500);
    // a screen of text (docs, menus) isn't the game, even if the stick scrolls it
    if (isText(await screenText())) continue;
    for (const port of [2, 1]) if (await responds(port)) return (await proof(), port);
    for (const port of [2, 1]) if (await shows(port)) return (await proof(), port);
  }
  return 0;
}

async function playtest(browser, kind, item) {
  const page = await browser.newPage();
  await page.setViewport({ width: 1100, height: 860 });
  await page.setCacheEnabled(false); // always the current page code
  const drive = [];
  await page.evaluateOnNewDocument(() =>
    addEventListener('message', (e) => e.data?.msg === 'breadbin_drive' && console.log('DRIVE ' + JSON.stringify(e.data))),
  );
  await page.evaluateOnNewDocument(audioTap); // what the emulator plays, for the sound check
  page.on('console', (m) => m.text().startsWith('DRIVE ') && drive.push(JSON.parse(m.text().slice(6))));
  if (process.env.DEBUG) page.on('pageerror', (e) => console.log('pageerror:', e.message));
  if (process.env.DEBUG) page.on('console', (m) => m.type() === 'error' && console.log('console:', m.text().slice(0, 200)));
  const shots = {};
  const r = { drive: [] };
  // vc64web reports a jammed CPU with alert(), which would block its frame
  let jammed = false;
  page.on('dialog', (d) => ((jammed ||= /jammed/.test(d.message())), d.dismiss().catch(() => {})));
  try {
    // a [1541] item runs on the emulated drive with breadbin's free DOS, or
    // with ROM1541 as a visitor would install it in Settings
    const rom = item.rom1541 && ROM1541 ? [...fs.readFileSync(ROM1541)] : null;
    await page.goto(BASE, { waitUntil: 'domcontentloaded' });
    await page.evaluate(async (rom) => {
      const store = await import('./store.js');
      await (rom ? store.putBlob('rom-floppy', new Uint8Array(rom)) : store.deleteBlob('rom-floppy'));
    }, rom);
    const query = new URLSearchParams(Object.entries({ run: item.run, boot: item.boot, parts: item.parts?.join(','), patches: item.patches?.join(','), rom1541: item.rom1541 }).filter(([, v]) => v));
    const run = query.size ? `?${query}` : '';
    await page.goto(`${BASE}${run}#${kind}/${encodeURIComponent(item.id)}`, { waitUntil: 'domcontentloaded' });
    await page.waitForSelector('#detail[open] .primary', { timeout: 45000 });
    await page.click('#detail[open] .primary');
    await page.waitForFunction(
      () => /running|loaded|warp/.test(document.querySelector('#emu-status')?.textContent || '') || document.querySelector('.emu-error'),
      { timeout: 120000 },
    );
    const error = await page.$eval('#emu-stage', (e) => e.querySelector('.emu-error')?.innerText || null);
    if (error) return { ok: false, reason: error.replace(/\s+/g, ' ') };
    await sleep(kind === 'demos' ? 30000 : 24000);

    const frame = page.frames().find((f) => f.url().includes('vc64web'));
    // a stuck emulator thread would otherwise hang the test for the full protocol timeout
    const inFrame = (label, fn, ...args) =>
      Promise.race([frame.evaluate(fn, ...args), sleep(15000).then(() => Promise.reject(new Error(`emulator stopped answering (${label})`)))]);
    const el = await page.$('.emu-frame');
    const shot = async (name) => {
      const buf = await el.screenshot();
      shots[name] = decode(buf);
      if (SHOTS) fs.writeFileSync(path.join(SHOTS, `${kind}-${item.id.replace(/[^\w.-]/g, '_')}-${name}.png`), buf);
    };
    const screenText = () =>
      inFrame('screen', () => {
        // wherever the VIC shows the screen
        const base = (3 - (wasm_peek(0xdd00) & 3)) * 0x4000 + (wasm_peek(0xd018) >> 4) * 0x400;
        let s = '';
        for (let a = base; a < base + 1000; a++) {
          const c = wasm_peek(a) & 0x7f;
          s += c < 32 ? String.fromCharCode(c + 64) : String.fromCharCode(c);
        }
        return s;
      });
    const joy = (c) => inFrame(c, (x) => emit_joystick_cmd(x), c);
    const key = (row, col) =>
      inFrame(`key ${row},${col}`, async (row, col) => {
        wasm_key(row, col, 1);
        await new Promise((res) => setTimeout(res, 150));
        wasm_key(row, col, 0);
      }, row, col);

    // a tape keeps loading for minutes, and loads again between levels: wait
    // while its motor runs ($01 bit 5 low), until it has stopped for 10 s; a
    // real drive likewise while vc64web warps for it
    const tape = await inFrame('tape', () => /\.tap$/i.test(file_slot_file_name || ''));
    const busy = () => inFrame('busy', (tape) => Module._wasm_is_warping() || (tape && (wasm_peek(0) & 0x20) !== 0 && (wasm_peek(1) & 0x20) === 0), tape);
    const settle = async (limit = 420) => {
      let t = 0;
      for (let quiet = 0; (tape || item.rom1541) && quiet < 5 && t < limit; t += 2) (await busy()) ? (quiet = 0) : quiet++, await sleep(2000);
      if (process.env.DEBUG && t > 10) console.log(`  waited ${t} s for the ${tape ? 'tape' : 'drive'}`);
    };
    await settle();

    await shot('boot');
    await sleep(2500);
    await shot('idle');
    if (kind === 'games') {
      for (const p of ['2', '1']) {
        await joy(p + 'PRESS_FIRE');
        await sleep(250);
        await joy(p + 'RELEASE_FIRE');
        await sleep(400);
      }
      await shot('fire');
      // a screenshot of the moment the stick was shown to work, for review
      const look = async () => decode(await el.screenshot());
      // the page's Disk button, for "insert side 2"
      const swap = () => page.click('#emu-disk').catch(() => {});
      r.port = await controlledPort({ inFrame, joy, key, screenText, proof: () => shot('control'), settle, picture: look, swap, title: item.title });
      await sleep(1500);
    } else {
      // a still screen asking for SPACE (Lunatico's "designed for the new SID
      // chip... SPACE to run anyway") gets it, as a viewer would press it
      if (change(shots.boot, shots.idle) < 0.001 && /SPACE/.test(await screenText())) await key(7, 4), await settle(), await shot('idle');
      await sleep(8000);
      await shot('fire');
      // a demo must be heard: the RMS of what reaches the speakers, ~3 s
      const levels = await frame.evaluate(loudness, 20, 150);
      r.sound = Number(Math.max(...levels).toFixed(3));
      r.loud = levels.filter((v) => v > 0.004).length;
      await sleep(1000);
    }
    await shot('play');
    const text = (await screenText()).replace(/\s+/g, ' ');

    jammed ||= /jammed/.test(await page.$eval('#emu-note', (e) => e.textContent));
    if (jammed) return { ...r, ok: false, reason: 'CPU jammed' };
    const { boot, idle, fire, play } = shots;
    // a blue screen is only BASIC if it says so (trainer menus are blue too)
    const basic = share(play, BASIC_BLUE) > 0.45 && /READY\.|ERROR|BRK AT/.test(text);
    const dark = share(play, [0, 0, 0], 12) > 0.985;
    const moving = Math.max(change(boot, idle), change(idle, fire), change(fire, play));
    r.drive = drive.filter((d) => d.event !== 'ready').map((d) => (d.event === 'load' ? `${d.name.trim()}@${d.addr.toString(16)}` : `${d.event}:${d.name}`));
    r.moving = Number(moving.toFixed(4));
    if (basic && /ERROR|BRK AT/.test(text)) return { ...r, ok: false, reason: (/\?[A-Z' ]*ERROR( IN \d+)?|BRK AT \$[0-9A-F]+/.exec(text) || ['error'])[0] };
    if (basic) return { ...r, ok: false, reason: drive.some((d) => d.event === 'missing') ? 'asked for a file not on the disk' : 'stayed at BASIC' };
    if (kind === 'games' && !r.port) return { ...r, ok: false, reason: 'the joystick moved nothing' };
    if (dark && moving < 0.001) return { ...r, ok: false, reason: 'blank screen (likely a fast loader)' };
    if (moving < 0.0005) return { ...r, ok: false, reason: 'picture never changed' };
    if (kind === 'demos' && r.loud < 5) return { ...r, ok: false, reason: 'no sound' };
    return { ...r, ok: true };
  } catch (e) {
    return { ...r, ok: false, ...(jammed ? { reason: 'CPU jammed' } : { harness: true, reason: `harness: ${e.message.split('\n')[0]}` }) };
  } finally {
    await page.close().catch(() => {});
  }
}

async function runBrowserTests(kind, candidates) {
  // --fresh <hours>: with --retest, skip what was tested that recently
  const fresh = Number(flag('fresh', 0)) * 3600e3;
  const recent = (c) => fresh && Date.now() - Date.parse(results[kind][c.id]?.testedAt || 0) < fresh;
  const todo = candidates.filter((c) => (IDS ? IDS.some((i) => c.id.includes(i)) : (RETEST && !recent(c)) || !results[kind][c.id] || results[kind][c.id].harness));
  console.log(`${kind}: ${candidates.length} candidates, ${todo.length} to test`);
  if (SHOTS) fs.mkdirSync(SHOTS, { recursive: true });
  let next = 0;
  const launch = (w) =>
    puppeteer.launch({
      executablePath: CHROME,
      headless: 'new',
      protocolTimeout: 180000,
      userDataDir: path.join(os.tmpdir(), `breadbin-playtest-${kind}-${w}`),
      args: ['--no-sandbox', '--autoplay-policy=no-user-gesture-required', '--mute-audio'],
    });
  await Promise.all(
    Array.from({ length: WORKERS }, async (_, w) => {
      let browser = await launch(w);
      while (next < todo.length) {
        const item = todo[next++];
        // a crashed browser is replaced and the title tried once more
        let res;
        for (let attempt = 0; attempt < 2; attempt++) {
          if (!browser.connected) browser = await launch(w);
          res = await playtest(browser, kind, item);
          // GameBase64's version failed: try the Internet Archive's copies
          if (kind === 'games' && !res.ok && !res.harness) {
            const alts = [...new Set([item.ia, ...(await archiveCopies(item.title))])].filter((id) => id && id !== item.id).slice(0, 4);
            for (const id of alts) {
              const alt = await playtest(browser, kind, { ...item, id, ia: id, gb: '', run: '' });
              if (alt.ok) {
                res = { ...alt, ia: id, gb: '', run: '' };
                break;
              }
            }
          }
          if (!res.harness) break;
        }
        results[kind][item.id] = { ...item, ...res, testedAt: new Date().toISOString() };
        saveResults(kind);
        console.log(`${res.ok ? 'PASS' : 'FAIL'}  ${item.title.slice(0, 40).padEnd(40)} ${res.reason || ''} ${res.drive?.join(' ') || ''}`);
      }
      await browser.close().catch(() => {});
    }),
  );
}

// ── tunes in Node ───────────────────────────────────────────────────────────


/** A SID file's header text (name, author, released), PSID/RSID offsets $16, $36, $56. */
const sidText = (sid, at) => sid.subarray(at, at + 32).toString('latin1').replace(/\0.*$/s, '').trim();

async function testTunes() {
  const wasmFile = path.join(REPO, 'docs/play/breadbin.wasm');
  if (!fs.existsSync(wasmFile)) throw new Error('docs/play/breadbin.wasm is missing: run `make web`');
  const env = { host_peek: () => 0, host_poke() {}, host_event() {} };
  const { instance } = await WebAssembly.instantiate(fs.readFileSync(wasmFile), { env });
  const w = instance.exports;
  const todo = TUNE_PICKS.filter((t) => RETEST || !results.tunes[t.hvsc]);
  console.log(`tunes: ${todo.length} to test`);
  for (const t of todo) {
    let res;
    try {
      const hvsc = t.hvsc;
      // from where the page gets it (hvsc.js: the Archive's HVSC, then hvsc.de)
      let sid = null;
      for (const [i, url] of sources(hvsc).entries()) {
        const got = await cached(`sid-${hvsc.replace(/\W+/g, '_')}-${i}.sid`, url, 365).catch(() => null);
        if (got && /^[PR]SID/.test(got.subarray(0, 4).toString('latin1'))) {
          sid = got;
          break;
        }
      }
      if (!sid) throw new Error('no HVSC source the page uses returned it');
      const ptr = w.bb_alloc(sid.length);
      new Uint8Array(w.memory.buffer, ptr, sid.length).set(sid);
      const p = w.bb_player_new(ptr, sid.length, 0, 44100, 0);
      w.bb_free(ptr, sid.length);
      if (!p) throw new Error('the player rejected it');
      // 20 seconds: some tunes open with a long quiet intro
      const n = 44100;
      const out = w.bb_alloc(n * 4); // stereo i16
      const regs = w.bb_alloc(32);
      let loud = 0;
      // a crashed tune leaves a drone: no more interrupts, SID registers frozen
      const states = new Set();
      const frames = [];
      for (let s = 0; s < 20; s++) {
        w.bb_player_render(p, out, n);
        const pcm = new Int16Array(w.memory.buffer, out, n * 2);
        let sum = 0;
        for (const v of pcm) sum += v * v;
        if (Math.sqrt(sum / (n * 2)) > 250) loud++;
        const frame = w.bb_player_regs(p, regs);
        if (s >= 10) {
          states.add(new Uint8Array(w.memory.buffer, regs, 25).join());
          frames.push(frame);
        }
      }
      w.bb_free(out, n * 4);
      w.bb_free(regs, 32);
      w.bb_player_free(p);
      if (loud < 3) throw new Error('silent in our player');
      if (states.size < 3 && frames[0] === frames.at(-1)) throw new Error('stopped playing in our player');
      // "1986 Ocean": the year, then the publisher
      const released = sidText(sid, 0x56);
      res = { ok: true, composer: sidText(sid, 0x36), group: released.replace(/^\d{4}(-\d+)?\s*/, ''), year: Number(/^\d{4}/.exec(released)?.[0]) || 0, party: '' };
    } catch (e) {
      res = { ok: false, reason: e.message };
    }
    results.tunes[t.hvsc] = { id: t.hvsc, name: t.name, hvsc: t.hvsc, rating: 0, ...res, testedAt: new Date().toISOString() };
    saveResults('tunes');
    console.log(`${res.ok ? 'PASS' : 'FAIL'}  ${t.name.slice(0, 40).padEnd(40)} ${res.reason || `${res.composer} · ${res.year}`}`);
  }
}

// ── verified.json ───────────────────────────────────────────────────────────

/** Drop empty fields to keep verified.json small. */
const compact = (o) => Object.fromEntries(Object.entries(o).filter(([, v]) => v !== '' && v !== 0 && v != null));

/**
 * Where a game already plays online: an Internet Archive item that runs in the
 * archive's own browser emulator. The C64 games collection always does; other
 * titles are looked up in the whole C64 software library.
 */
async function onlineCopy(g) {
  return g.ia || (await archiveCopies(g.title))[0] || '';
}

/** Internet Archive C64 items titled like `title` that run in its emulator, most downloaded first. */
async function archiveCopies(title) {
  // the main title only: subtitles and roman numerals are spelled many ways
  const words = title.split(/:| - /)[0].replace(/[^\w ]|\b[IVX]+\b/g, ' ').trim();
  const params = new URLSearchParams([['q', `collection:softwarelibrary_c64 AND emulator:vice* AND title:(${words})`],
    ['fl[]', 'identifier'], ['fl[]', 'title'], ['sort[]', 'downloads desc'], ['rows', '50'], ['output', 'json']]);
  const res = JSON.parse(await cached(`online-${canon(title)}.json`, `https://archive.org/advancedsearch.php?${params}`, 30));
  const keys = [canon(title), canon(title.replace(/:| - /g, ' '))];
  return res.response.docs.filter((d) => keys.includes(canon(String(d.title).replace(/^Commodore 64: /, '')))).map((d) => d.identifier);
}

/** Write the allowlist the web UI reads: working items only, best first. */
async function writeVerified() {
  const current = new Set((await gameCandidates()).map((g) => g.id));
  // what passed, and what games.txt and demos.txt list but didn't (yet): those
  // go on the shelf marked "wip", with the reason
  const ok = (k) => Object.values(results[k]).filter((r) => r.ok || (r.reason && !r.harness)).map((r) => (r.ok ? r : { ...r, wip: r.reason }));
  for (const g of ok('games')) g.online = await onlineCopy(g);
  const out = {
    note: 'Generated by tools/playtest from playtests of the browser version. Only these are offered in docs/play; "wip" marks one that failed its playtest, with the reason.',
    // only each title's current version (games.txt pins), not older results
    games: ok('games')
      .filter((g) => current.has(g.id))
      .sort((a, b) => PICKS.findIndex((t) => canon(t) === canon(a.title)) - PICKS.findIndex((t) => canon(t) === canon(b.title)))
      .map(({ gb, run, boot, ia, online, port, title, rating, classic, genre, sub, year, publisher, shot, rom1541, parts, patches, wip }) =>
        compact({ gb, run, boot, ia, online: online === ia ? '' : online, port: port === 1 ? 1 : 0, title, rating, classic, genre, sub, year, publisher, shot, rom1541: rom1541 || (NEEDS_1541.has(canon(title)) ? 1 : 0), parts, patches, wip })),
    demos: ok('demos')
      .filter((d) => DEMO_PICKS.some((p) => p.csdb === d.csdb))
      // one item per release, a passing one first
      .sort((a, b) => Number(!!a.wip) - Number(!!b.wip))
      .filter((d, i, all) => all.findIndex((x) => x.csdb === d.csdb) === i)
      .sort((a, b) => DEMO_PICKS.findIndex((p) => p.csdb === a.csdb) - DEMO_PICKS.findIndex((p) => p.csdb === b.csdb))
      .map(({ id, title, group, rating, party, year, csdb, onefile, rom1541, parts, wip }) =>
        compact({ id, title, group, rating, party, year, csdb, onefile, rom1541, parts, wip })),
    tunes: ok('tunes')
      .filter((t) => TUNE_PICKS.some((p) => p.hvsc === t.hvsc))
      .sort((a, b) => TUNE_PICKS.findIndex((p) => p.hvsc === a.hvsc) - TUNE_PICKS.findIndex((p) => p.hvsc === b.hvsc))
      .map(({ id, name, composer, group, rating, year, party, hvsc }) => ({ id, name, composer, group, rating, year, party, hvsc })),
  };
  fs.writeFileSync(VERIFIED, JSON.stringify(out, (_, v) => (typeof v === 'string' ? unescape(v) : v)));
  console.log(`wrote ${path.relative(REPO, VERIFIED)}: ${out.games.length} games, ${out.demos.length} demos, ${out.tunes.length} tunes`);
}

if (cmd === 'games') await runBrowserTests('games', await gameCandidates());
else if (cmd === 'demos') await runBrowserTests('demos', await demoCandidates());
else if (cmd === 'tunes') await testTunes();
else if (cmd === 'write') await writeVerified();
else if (cmd === 'candidates') console.table((await gameCandidates()).map(({ id, title, rating, year, run, parts }) => ({ id: id.slice(0, 40), title, rating, year, run, parts: parts?.length || '' })));
else console.log(fs.readFileSync(fileURLToPath(import.meta.url), 'utf8').split('\n').slice(0, 18).join('\n'));
