// links - checks that every game, demo and tune on the shelves can be shared:
// its Share button copies a link, and that link, opened by someone who has
// never been to the site, starts it.
//
//   node links.mjs                        # all of docs/play/verified.json
//   node links.mjs games demos            # some shelves
//   node links.mjs tunes --ids Hubbard    # titles whose id or name holds a substring
//   node links.mjs --workers 4            # parallel visitors (default 3)
//
// Needs the site on localhost:8064 (make -C ../.. serve-web), or BASE.
//
// Each title gets a fresh browser context: no storage, no earlier tap, as a
// friend arriving from a chat. Unlike c64.mjs's launch(), Chromium keeps its
// real autoplay policy: where it holds sound back, a shared tune has to ask for
// a tap, and this taps when asked.
// - Game or demo: its card's Share button, on a phone, hands `#<tab>/<id>/play`
//   to the share sheet; that link opens the player, which loads the title and
//   runs it (vc64web reports running) without a load error; the player's own
//   Share button, on a computer, copies the same link.
// - Tune: the Share button copies `#tunes/<id>/play`; that link plays the
//   tune, after a tap if the browser asks for one, and it is heard
//   (audioTap/loudness).

import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import puppeteer from 'puppeteer-core';
import { BASE, sleep, audioTap, loudness } from './c64.mjs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const VERIFIED = path.join(HERE, '../../docs/play/verified.json');
const args = process.argv.slice(2);
const flag = (name, dflt) => (args.includes(`--${name}`) ? args[args.indexOf(`--${name}`) + 1] : dflt);
const WORKERS = Number(flag('workers', 3));
const IDS = flag('ids', '');
const shelves = ['games', 'demos', 'tunes'].filter((s) => args.includes(s));
const v = JSON.parse(fs.readFileSync(VERIFIED, 'utf8'));
// the page's ids (catalogue.js): a GameBase64 path or an Internet Archive id for games
const ids = {
  games: v.games.map((g) => ({ id: g.gb ? `gb64:${g.gb}` : g.ia, title: g.title })),
  demos: v.demos.map((d) => ({ id: d.id, title: d.title, wip: d.wip })),
  tunes: v.tunes.map((t) => ({ id: String(t.id), title: t.name })),
};
const todo = (shelves.length ? shelves : Object.keys(ids))
  .flatMap((tab) => ids[tab].map((x) => ({ tab, ...x })))
  .filter((x) => !IDS || `${x.id} ${x.title}`.toLowerCase().includes(IDS.toLowerCase()));

const linkFor = (tab, id) => `${new URL(BASE).origin}${new URL(BASE).pathname}#${tab}/${encodeURIComponent(id)}/play`;

const browser = await puppeteer.launch({
  executablePath: process.env.CHROME || '/usr/bin/chromium',
  headless: 'new',
  protocolTimeout: 180000,
  // a phone's policy: no sound before the visitor taps or presses a key
  args: ['--no-sandbox', '--mute-audio', '--autoplay-policy=document-user-activation-required'],
});

/**
 * A first-time visitor's tab, on a computer or (`phone`) a touch screen.
 * Clipboard writes are recorded, since one browser has one clipboard; a phone
 * gets a share sheet that records what it was handed (Chromium on Linux has none).
 */
async function visitor({ phone = false } = {}) {
  const ctx = await browser.createBrowserContext();
  await ctx.overridePermissions(new URL(BASE).origin, ['clipboard-read', 'clipboard-write', 'clipboard-sanitized-write']);
  const page = await ctx.newPage();
  await page.setViewport(phone ? { width: 390, height: 844, isMobile: true, hasTouch: true } : { width: 1100, height: 860 });
  if (phone) await page.evaluateOnNewDocument(() => (navigator.share = async (d) => window.__copied.push(`share:${d.url}`)));
  await page.setCacheEnabled(false);
  await page.evaluateOnNewDocument(audioTap);
  await page.evaluateOnNewDocument(() => {
    window.__copied = [];
    const write = navigator.clipboard?.writeText?.bind(navigator.clipboard);
    if (write) navigator.clipboard.writeText = (t) => (window.__copied.push(t), write(t));
    const exec = document.execCommand.bind(document);
    document.execCommand = (c, ...r) => (c === 'copy' && window.__copied.push(document.activeElement?.value), exec(c, ...r));
  });
  page.on('dialog', (d) => d.dismiss().catch(() => {}));
  return { ctx, page };
}

/** Click `sel` and return what it copied or shared ("share:<url>"; null if nothing) and the toast. */
async function copyFrom(page, sel) {
  await page.waitForSelector(sel, { visible: true, timeout: 30000 });
  await page.evaluate(() => (window.__copied.length = 0));
  // a real click, again if it missed: the player's bar reflows as its status
  // changes ("loaded <file>" can wrap), which can move the button under the pointer
  for (let i = 0; i < 3; i++) {
    await page.click(sel);
    if (await page.waitForFunction(() => window.__copied.length, { timeout: 3000 }).then(() => true, () => false)) break;
  }
  await sleep(300);
  return page.evaluate(() => ({ copied: window.__copied.at(-1) ?? null, toast: document.querySelector('#toast')?.textContent || '' }));
}

/**
 * `watch(expr, ms)`: evaluate `expr` in the page every 250 ms until it is
 * truthy (its value) or `ms` pass (null). Puppeteer's evaluate and
 * waitForFunction run as a user gesture, which would unlock sound and count as
 * the visitor's tap; this goes through CDP without one.
 */
function watcher(page) {
  const cdp = page.createCDPSession();
  return async (expr, ms) => {
    const session = await cdp;
    for (const end = Date.now() + ms; ; await sleep(250)) {
      const { result } = await session.send('Runtime.evaluate', { expression: expr, returnByValue: true, userGesture: false });
      if (result.value) return result.value;
      if (Date.now() >= end) return null;
    }
  };
}

async function checkRun({ tab, id }) {
  const want = linkFor(tab, id);
  const notes = [];
  // 1. the card's Share button, on a phone: its share sheet
  let { ctx, page } = await visitor({ phone: true });
  try {
    await page.goto(`${BASE}#${tab}/${encodeURIComponent(id)}`, { waitUntil: 'domcontentloaded' });
    const card = await copyFrom(page, '#detail[open] .share');
    if (card.copied !== `share:${want}`) return { ok: false, why: `card Share on a phone gave ${card.copied}` };
  } finally {
    await ctx.close();
  }
  // 2. the link, in a new visitor's tab
  ({ ctx, page } = await visitor());
  try {
    const t0 = Date.now();
    await page.goto(want, { waitUntil: 'domcontentloaded' });
    const watch = watcher(page);
    const state =
      (await watch(
        `(() => {
          const err = document.querySelector('#emu-stage .emu-error');
          if (err) return 'error: ' + err.innerText.replace(/\\s+/g, ' ');
          const s = document.querySelector('#emu-status')?.textContent || '';
          return !document.querySelector('#emu').hidden && /running|loaded|warp/.test(s) && s;
        })()`,
        240000,
      )) || 'timeout';
    const secs = Math.round((Date.now() - t0) / 1000);
    if (!/^(running|loaded|warp)/.test(state)) return { ok: false, why: `link didn't start it: ${state}`, secs };
    const title = await watch(`document.querySelector('#emu-title').textContent`, 1);
    if (await watch(`!document.querySelector('#emu-sound').hidden`, 1)) notes.push('asks for a tap for sound');
    // 3. the player's Share button, on a computer: the clipboard
    const bar = await copyFrom(page, '#emu-share');
    if (bar.copied !== want) return { ok: false, why: `player Share copied ${bar.copied}`, secs };
    if (!/copied/.test(bar.toast)) notes.push('no "copied" toast');
    return { ok: true, why: `${state} as "${title}" in ${secs}s${notes.length ? `; ${notes.join(', ')}` : ''}`, secs };
  } finally {
    await ctx.close();
  }
}

async function checkTune({ id, title }) {
  const want = linkFor('tunes', id);
  const { ctx, page } = await visitor();
  try {
    await page.goto(want, { waitUntil: 'domcontentloaded' });
    const watch = watcher(page);
    const playing = `(() => {
      const row = document.querySelector('.tune.current');
      return row && !row.classList.contains('loading') && !document.querySelector('#btn-pause').disabled && row.tune?.name === ${JSON.stringify(title)} && 'playing';
    })()`;
    const asked = `/Tap anywhere/.test(document.querySelector('#toast')?.textContent || '') && 'asked'`;
    const state = `\`toast "\${document.querySelector('#toast')?.textContent}", now \${document.querySelector('.tune.current')?.className || 'nothing'}\``;
    // the tune plays at once where the browser allows sound, or asks for a tap first
    const first = await watch(`(${asked}) || ${playing}`, 60000);
    if (!first) return { ok: false, why: `neither played nor asked for a tap (${await watch(state, 1)})` };
    if (first === 'asked') {
      await page.click('#viz');
      if (!(await watch(playing, 60000))) return { ok: false, why: `didn't start after the tap (${await watch(state, 1)})` };
    }
    const level = Math.max(...(await page.evaluate(loudness, 40, 250)));
    if (!(level > 0.001)) return { ok: false, why: `silent (peak RMS ${level.toFixed(4)})` };
    const share = await copyFrom(page, '#btn-share');
    if (share.copied !== want) return { ok: false, why: `Share copied ${share.copied}` };
    return { ok: true, why: `${first === 'asked' ? 'plays after a tap' : 'plays at once'}, peak RMS ${level.toFixed(3)}` };
  } finally {
    await ctx.close();
  }
}

console.log(`links: ${todo.length} titles against ${BASE}, ${WORKERS} at a time`);
const results = [];
let next = 0;
await Promise.all(
  Array.from({ length: WORKERS }, async () => {
    while (next < todo.length) {
      const item = todo[next++];
      let r;
      try {
        r = await (item.tab === 'tunes' ? checkTune(item) : checkRun(item));
      } catch (e) {
        r = { ok: false, why: e.message.split('\n')[0] };
      }
      results.push({ ...item, ...r });
      console.log(`${r.ok ? 'ok  ' : 'FAIL'} ${item.tab.padEnd(5)} ${item.title}${item.wip ? ' (wip)' : ''}: ${r.why}`);
    }
  }),
);
await browser.close();

const failed = results.filter((r) => !r.ok);
for (const tab of ['games', 'demos', 'tunes']) {
  const of = results.filter((r) => r.tab === tab);
  if (of.length) console.log(`${tab}: ${of.filter((r) => r.ok).length}/${of.length} start from their shared link`);
}
if (failed.length) {
  console.log('\nFailed:');
  for (const f of failed) console.log(`  ${f.tab}/${f.id} (${f.title}): ${f.why}`);
}
process.exit(failed.length ? 1 : 0);
