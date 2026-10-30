// playtest - find out which games, demos and tunes actually work in breadbin's
// browser version, so the web UI only offers those (docs/play/verified.json).
//
//   node playtest.mjs games [--top 300] [--workers 2] [--retest] [--ids a,b]
//   node playtest.mjs demos [--top 150] [--workers 2] [--retest]
//   node playtest.mjs tunes [--top 200] [--retest]
//   node playtest.mjs write            # results.json -> docs/play/verified.json
//   node playtest.mjs candidates       # show the game ranking
//
// Games and demos are played in headless Chromium against a local copy of the
// site (`make serve-web`, or --base URL): the harness presses Play, waits for the
// boot, then sends FIRE, SPACE, F1 and stick moves, and classifies screenshots:
// a BASIC screen or an error means failure, a picture that moves on its own or
// reacts to input means it works. Tunes run in the engine's SID player
// (breadbin.wasm) and must produce sound. Candidates: games from the GameBase64
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
import { PNG } from 'pngjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const { canon } = await import('../../docs/play/covers.js');
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
const TOP = Number(flag('top', cmd === 'games' ? 300 : cmd === 'tunes' ? 200 : 150));
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
 * GameBase64 games that don't need true drive emulation, best first: rating,
 * GB64's "classic" flag, then how often the Internet Archive's copy is
 * downloaded. Each keeps its Internet Archive item (when there is one) as a
 * fallback source and for box art.
 */
async function gameCandidates() {
  await cached('gb64.sqlitedb', 'https://www.twinbirds.com/gamebase64browser/GBC_v19.sqlitedb.gz', 365, (b) => (b[0] === 0x1f && b[1] === 0x8b ? zlib.gunzipSync(b) : b));
  const popular = await archivePopularity();
  const db = new DatabaseSync(path.join(CACHE, 'gb64.sqlitedb'), { readOnly: true });
  const rows = db.prepare(`
    SELECT CAST(g.Name AS BLOB) AS name, g.Filename AS file, g.FileToRun AS run, g.ScrnshotFilename AS shot,
           g.Rating AS rating, g.Classic AS classic, y.Year AS year, CAST(p.Publisher AS BLOB) AS publisher,
           pg.ParentGenre AS genre, ge.Genre AS sub
    FROM Games g
    LEFT JOIN Years y ON y.YE_Id = g.YE_Id
    LEFT JOIN Publishers p ON p.PU_Id = g.PU_Id
    LEFT JOIN Genres ge ON ge.GE_Id = g.GE_Id
    LEFT JOIN PGenres pg ON pg.PG_Id = ge.PG_Id
    WHERE g.V_TrueDriveEmu = 0 AND g.Adult = 0 AND g.Filename <> ''`).all();
  const slash = (p) => (p || '').replace(/\\/g, '/');
  const best = new Map();
  for (const r of rows) {
    const title = text(r.name).replace(/^(.*), (The|A|An)$/, '$2 $1');
    const publisher = text(r.publisher);
    const key = canon(title);
    const pop = popular.get(key) || { downloads: 0, id: '' };
    const g = {
      id: `gb64:${slash(r.file)}`, gb: slash(r.file), run: r.run || '', title,
      rating: r.rating || 0, classic: r.classic ? 1 : 0, genre: r.genre || '', sub: r.sub || '',
      year: r.year > 1900 ? r.year : 0, publisher: /^\(/.test(publisher) ? '' : publisher,
      shot: slash(r.shot), ia: pop.id, downloads: pop.downloads,
    };
    const old = best.get(key);
    if (!old || g.rating + g.classic > old.rating + old.classic) best.set(key, g);
  }
  return [...best.values()]
    .sort((a, b) => b.rating - a.rating || b.classic - a.classic || b.downloads - a.downloads)
    .slice(0, TOP);
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
  const [demos, onefile] = await Promise.all([csdbToplist(1), csdbToplist(2)]);
  const top = [...demos.map((d) => ({ ...d, onefile: 0 })), ...onefile.map((d) => ({ ...d, onefile: 1 }))]
    .sort((a, b) => b.rating - a.rating)
    .map((d) => ({ csdb: d.id, title: d.name, group: d.group, rating: d.rating, onefile: d.onefile }));
  const out = [];
  for (let i = 0; i < top.length && out.length < TOP; i += 40) {
    const chunk = top.slice(i, i + 40);
    const q = `collection:softwarelibrary_c64_demos AND csdb:(${chunk.map((d) => `"https://csdb.dk/release/?id=${d.csdb}"`).join(' OR ')})`;
    const params = new URLSearchParams({ q, rows: '200', output: 'json' });
    for (const f of ['identifier', 'csdb', 'downloads']) params.append('fl[]', f);
    const docs = (await (await fetch(`https://archive.org/advancedsearch.php?${params}`, { headers: UA })).json()).response.docs;
    for (const d of chunk) {
      const items = docs.filter((x) => x.csdb === `https://csdb.dk/release/?id=${d.csdb}`);
      // multi-disk demos are several items; side A boots
      items.sort((a, b) => Number(/side_?b|_b$/i.test(a.identifier)) - Number(/side_?b|_b$/i.test(b.identifier)) || b.downloads - a.downloads);
      if (!items[0]) continue;
      const xml = await csdbRelease(d.csdb);
      out.push({ ...d, id: items[0].identifier, party: party(xml), year: Number(field(xml, 'ReleaseYear')) || 0 });
    }
  }
  return out.slice(0, TOP);
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
async function playtest(browser, kind, item) {
  const page = await browser.newPage();
  await page.setViewport({ width: 1100, height: 860 });
  await page.setCacheEnabled(false); // always the current page code
  const drive = [];
  await page.evaluateOnNewDocument(() =>
    addEventListener('message', (e) => e.data?.msg === 'breadbin_drive' && console.log('DRIVE ' + JSON.stringify(e.data))),
  );
  page.on('console', (m) => m.text().startsWith('DRIVE ') && drive.push(JSON.parse(m.text().slice(6))));
  if (process.env.DEBUG) page.on('pageerror', (e) => console.log('pageerror:', e.message));
  if (process.env.DEBUG) page.on('console', (m) => m.type() === 'error' && console.log('console:', m.text().slice(0, 200)));
  const shots = {};
  const r = { drive: [] };
  // vc64web reports a jammed CPU with alert(), which would block its frame
  let jammed = false;
  page.on('dialog', (d) => ((jammed ||= /jammed/.test(d.message())), d.dismiss().catch(() => {})));
  try {
    const run = item.run ? `?run=${encodeURIComponent(item.run)}` : '';
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
        let s = '';
        for (let a = 0x400; a < 0x7e8; a++) {
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
      await key(7, 4); // SPACE (past a crack intro)
      await sleep(1500);
      // then often a trainer menu: say no to each question
      for (const c of 'nnnnn0\n') {
        await inFrame('type', (c) => wasm_auto_type(c), c);
        await sleep(400);
      }
      await key(0, 4); // F1
      await sleep(4000);
      await shot('fire');
      for (const p of ['2', '1']) {
        for (const d of ['PULL_RIGHT', 'PULL_LEFT']) {
          await joy(p + d);
          await sleep(600);
          await joy(p + 'RELEASE_X');
        }
        for (const d of ['PULL_UP', 'PULL_DOWN']) {
          await joy(p + d);
          await sleep(450);
          await joy(p + 'RELEASE_Y');
        }
        await joy(p + 'PRESS_FIRE');
        await sleep(200);
        await joy(p + 'RELEASE_FIRE');
      }
      await sleep(1500);
    } else {
      await sleep(8000);
      await shot('fire');
      await sleep(4000);
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
    if (dark && moving < 0.001) return { ...r, ok: false, reason: 'blank screen (likely a fast loader)' };
    if (moving < 0.0005) return { ...r, ok: false, reason: 'picture never changed' };
    return { ...r, ok: true };
  } catch (e) {
    return { ...r, ok: false, ...(jammed ? { reason: 'CPU jammed' } : { harness: true, reason: `harness: ${e.message.split('\n')[0]}` }) };
  } finally {
    await page.close().catch(() => {});
  }
}

async function runBrowserTests(kind, candidates) {
  const todo = candidates.filter((c) => (IDS ? IDS.some((i) => c.id.includes(i)) : RETEST || !results[kind][c.id] || results[kind][c.id].harness));
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
          // GameBase64's version failed: try the Internet Archive's copy
          if (!res.ok && !res.harness && item.gb && item.ia) {
            const alt = await playtest(browser, kind, { ...item, id: item.ia, gb: '', run: '' });
            if (alt.ok) res = { ...alt, gb: '', run: '' };
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


async function testTunes() {
  const wasmFile = path.join(REPO, 'docs/play/breadbin.wasm');
  if (!fs.existsSync(wasmFile)) throw new Error('docs/play/breadbin.wasm is missing: run `make web`');
  const env = { host_peek: () => 0, host_poke() {}, host_event() {} };
  const { instance } = await WebAssembly.instantiate(fs.readFileSync(wasmFile), { env });
  const w = instance.exports;
  const todo = (await csdbToplist(7)).slice(0, TOP).filter((t) => RETEST || !results.tunes[t.id]);
  console.log(`tunes: ${todo.length} to test`);
  for (const t of todo) {
    let res;
    try {
      const xml = await csdbRelease(t.id);
      const used = xml.slice(xml.indexOf('<UsedSIDs>'));
      const hvsc = xml.includes('<UsedSIDs>') ? field(used, 'HVSCPath') : '';
      if (!hvsc) throw new Error('not in the HVSC');
      const sid = await cached(`sid-${t.id}.sid`, 'https://www.hvsc.de/download/C64Music' + hvsc.split('/').map(encodeURIComponent).join('/'), 365);
      if (!/^[PR]SID/.test(sid.subarray(0, 4).toString('latin1'))) throw new Error('hvsc.de did not return a SID');
      const ptr = w.bb_alloc(sid.length);
      new Uint8Array(w.memory.buffer, ptr, sid.length).set(sid);
      const p = w.bb_player_new(ptr, sid.length, 0, 44100, 0);
      w.bb_free(ptr, sid.length);
      if (!p) throw new Error('the player rejected it');
      // 20 seconds: some tunes open with a long quiet intro
      const n = 44100;
      const out = w.bb_alloc(n * 2);
      const regs = w.bb_alloc(32);
      let loud = 0;
      const states = new Set(); // SID register snapshots: a crashed tune leaves a frozen drone
      for (let s = 0; s < 20; s++) {
        w.bb_player_render(p, out, n);
        const pcm = new Int16Array(w.memory.buffer, out, n);
        let sum = 0;
        for (const v of pcm) sum += v * v;
        if (Math.sqrt(sum / n) > 250) loud++;
        w.bb_player_regs(p, regs);
        if (s >= 10) states.add(new Uint8Array(w.memory.buffer, regs, 25).join());
      }
      w.bb_free(out, n * 2);
      w.bb_free(regs, 32);
      w.bb_player_free(p);
      if (loud < 3) throw new Error('silent in our player');
      if (states.size < 5) throw new Error('stopped playing in our player');
      const credits = [...xml.matchAll(/<Credit>[\s\S]*?<\/Credit>/g)].map((c) => c[0]).filter((c) => /<CreditType>Music/.test(c));
      const composer = [...new Set(credits.map((c) => /<Handle>([^<]+)<\/Handle>/.exec(c)?.[1]).filter(Boolean))].join(' & ');
      res = { ok: true, hvsc, composer, party: party(xml), year: Number(field(xml, 'ReleaseYear')) || 0 };
    } catch (e) {
      res = { ok: false, reason: e.message };
    }
    results.tunes[t.id] = { ...t, ...res, testedAt: new Date().toISOString() };
    saveResults('tunes');
    console.log(`${res.ok ? 'PASS' : 'FAIL'}  ${t.name.slice(0, 40).padEnd(40)} ${res.reason || res.hvsc}`);
  }
}

// ── verified.json ───────────────────────────────────────────────────────────

/** Drop empty fields to keep verified.json small. */
const compact = (o) => Object.fromEntries(Object.entries(o).filter(([, v]) => v !== '' && v !== 0 && v != null));

/** Write the allowlist the web UI reads: working items only, best first. */
function writeVerified() {
  const ok = (k) => Object.values(results[k]).filter((r) => r.ok);
  const out = {
    note: 'Generated by tools/playtest from playtests of the browser version. Only these are offered in docs/play.',
    games: ok('games')
      .sort((a, b) => b.rating - a.rating || b.classic - a.classic || b.downloads - a.downloads)
      .map(({ gb, run, ia, title, rating, classic, genre, sub, year, publisher, shot }) =>
        compact({ gb, run, ia, title, rating, classic, genre, sub, year, publisher, shot })),
    demos: ok('demos')
      .sort((a, b) => b.rating - a.rating)
      .map(({ id, title, group, rating, party, year, csdb, onefile }) => compact({ id, title, group, rating, party, year, csdb, onefile })),
    tunes: ok('tunes')
      .sort((a, b) => b.rating - a.rating)
      .map(({ id, name, composer, group, rating, year, party, hvsc }) => ({ id, name, composer, group, rating, year, party, hvsc })),
  };
  fs.writeFileSync(VERIFIED, JSON.stringify(out, (_, v) => (typeof v === 'string' ? unescape(v) : v)));
  console.log(`wrote ${path.relative(REPO, VERIFIED)}: ${out.games.length} games, ${out.demos.length} demos, ${out.tunes.length} tunes`);
}

if (cmd === 'games') await runBrowserTests('games', await gameCandidates());
else if (cmd === 'demos') await runBrowserTests('demos', await demoCandidates());
else if (cmd === 'tunes') await testTunes();
else if (cmd === 'write') writeVerified();
else if (cmd === 'candidates') console.table((await gameCandidates()).map(({ title, rating, classic, genre, year, publisher, ia, run }) => ({ title, rating, classic, genre, year, publisher, ia: !!ia, run })));
else console.log(fs.readFileSync(fileURLToPath(import.meta.url), 'utf8').split('\n').slice(0, 18).join('\n'));
