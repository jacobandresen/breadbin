// The browser tests' harness: serves docs/ on a free port, starts headless
// Chromium (CHROME, else /usr/bin/chromium) and opens the Studio in a fresh
// profile. A folder on disk is played by the browser's origin-private file
// system, which hands out the same FileSystemDirectoryHandle a real folder
// picker does.

import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import puppeteer from 'puppeteer-core';

const DOCS = fileURLToPath(new URL('../../../../docs/', import.meta.url));
const TYPES = { '.html': 'text/html', '.js': 'text/javascript', '.mjs': 'text/javascript', '.css': 'text/css', '.json': 'application/json', '.wasm': 'application/wasm', '.svg': 'image/svg+xml', '.png': 'image/png', '.gz': 'application/gzip' };

export const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/** Serve docs/ on a free port; resolves to { url, close }. */
export function serve() {
  const server = http.createServer((req, res) => {
    let p = path.join(DOCS, decodeURIComponent(new URL(req.url, 'http://x').pathname));
    if (!p.startsWith(DOCS)) return (res.writeHead(403), res.end());
    if (p.endsWith('/')) p = path.join(p, 'index.html');
    fs.readFile(p, (err, data) => {
      if (err) return (res.writeHead(404), res.end());
      res.writeHead(200, { 'content-type': TYPES[path.extname(p)] || 'application/octet-stream', 'cache-control': 'no-store' });
      res.end(data);
    });
  });
  return new Promise((resolve) => server.listen(0, '127.0.0.1', () => resolve({ url: `http://127.0.0.1:${server.address().port}/play/`, close: () => server.close() })));
}

/** Is vc64web (the emulator, on the internet) reachable? Tests that run a game skip without it. */
export async function online() {
  try {
    const r = await fetch('https://vc64web.github.io/', { method: 'HEAD', signal: AbortSignal.timeout(5000) });
    return r.ok;
  } catch {
    return false;
  }
}

export async function launch() {
  return puppeteer.launch({
    executablePath: process.env.CHROME || '/usr/bin/chromium',
    headless: 'new',
    protocolTimeout: 120000,
    args: ['--no-sandbox', '--autoplay-policy=no-user-gesture-required', '--mute-audio'],
  });
}

/**
 * Open the Studio in a new page (a new, empty profile each browser). Errors
 * on the page are collected in `page.errors`. With `folder`, the folder
 * picker returns an OPFS directory instead of asking.
 */
export async function openStudio(browser, base, { folder = true, width = 1500, height = 920 } = {}) {
  const ctx = await browser.createBrowserContext();
  const page = await ctx.newPage();
  page.errors = [];
  page.on('pageerror', (e) => page.errors.push(e.message));
  await page.setViewport({ width, height });
  if (folder) {
    await page.evaluateOnNewDocument(() => {
      window.showDirectoryPicker = async () => {
        const root = await navigator.storage.getDirectory();
        return root.getDirectoryHandle(window.__pickName || 'picked', { create: true });
      };
    });
  } else {
    // a browser without folder access (Firefox, Safari)
    await page.evaluateOnNewDocument(() => delete window.showDirectoryPicker);
  }
  await page.goto(`${base}#hack`, { waitUntil: 'domcontentloaded' });
  await page.waitForFunction(() => window.breadbinStudio?.files?.['project.json'], { timeout: 30000 });
  await page.waitForSelector('.cm-content');
  return page;
}

/** Press ▶ Run and wait for its build to finish; resolves to the build's info line. */
export async function run(page) {
  await page.$eval('.studio-buildinfo', (e) => (e.textContent = ''));
  await page.click('.studio-run');
  return built(page);
}

/** Wait until the Studio's last build finished; resolves to its info line. */
export async function built(page) {
  await page.waitForFunction(() => /^(Built|Didn't build|The build failed)/.test(document.querySelector('.studio-buildinfo')?.textContent || ''), { timeout: 60000 });
  return page.$eval('.studio-buildinfo', (e) => e.textContent);
}

/** The vc64web frame in the Studio's C64 panel (or, with `where`, another stage), once the emulator runs. */
export async function c64(page, where = '.studio-stage') {
  await page.waitForFunction((w) => document.querySelector(`${w} iframe`), { timeout: 30000 }, where);
  let frame;
  for (let i = 0; i < 120 && !frame; i++) {
    frame = page.frames().find((f) => f.url().includes('vc64web'));
    if (!frame) await sleep(250);
  }
  const api = {
    frame,
    peek: (a, n = 1) => frame.evaluate((a, n) => Array.from({ length: n }, (_, i) => wasm_peek(a + i)), a, n),
    /** Row `y` of the Studio runtime's screen ($C000), as text. */
    row: async (y) => (await api.peek(0xc000 + y * 40, 40)).map((c) => String.fromCharCode(c < 32 ? c + 64 : c < 128 ? c : 46)).join(''),
    joy: (cmd) => frame.evaluate((c) => emit_joystick_cmd(c), `2${cmd}`),
    /**
     * Write memory: [[address, [bytes...]], ...], all in one go. The emulator
     * runs between calls, not during one, so the game sees all of it or none
     * (separate pokes can be half-seen: the game may rewrite one from another).
     */
    poke: (writes) => frame.evaluate((w) => w.forEach(([a, b]) => b.forEach((v, i) => wasm_poke(a + i, v))), writes),
    /**
     * poke(), again until the memory reads back as written: the C64 may be in
     * the middle of code that copied a variable and writes the copy back
     * (a game's "take, change, put back"), undoing a single poke.
     */
    async pokeHeld(writes, tries = 10) {
      for (let i = 0; i < tries; i++) {
        await api.poke(writes);
        await sleep(60);
        const ok = await frame.evaluate((w) => w.every(([a, b]) => b.every((v, j) => wasm_peek(a + j) === v)), writes);
        if (ok) return;
      }
      throw new Error('the C64 kept changing what was poked');
    },
    async press(cmd, ms = 300) {
      await api.joy(`PRESS_${cmd}`);
      await sleep(ms);
      await api.joy(`RELEASE_${cmd}`);
    },
    /** Sprite n's VIC position (x, y), or null if it is off. */
    async sprite(n) {
      const [regs] = [await api.peek(0xd000, 0x16)];
      if (!(regs[0x15] & (1 << n))) return null;
      return [regs[n * 2] + ((regs[0x10] >> n) & 1) * 256, regs[n * 2 + 1]];
    },
    /** Wait until `fn()` (async) is truthy, polling. */
    async until(fn, ms = 20000) {
      const t = Date.now();
      for (;;) {
        try {
          const v = await fn();
          if (v) return v;
        } catch {}
        if (Date.now() - t > ms) throw new Error('timed out waiting on the C64');
        await sleep(200);
      }
    },
  };
  return api;
}

/** Type into the code editor, replacing its whole text. */
export async function setCode(page, text) {
  await page.evaluate((t) => {
    const v = window.breadbinStudio.code.view;
    v.dispatch({ changes: { from: 0, to: v.state.doc.length, insert: t } });
  }, text);
}
