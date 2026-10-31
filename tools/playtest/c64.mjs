// c64 - drive breadbin's player from Node: open a title in the real site (served
// by `make serve-web`), then press keys, move the joystick, read memory, the
// screen and the sprites, and ask vc64web's debugger where the CPU is. The
// playtest (playtest.mjs) and one-off investigations (probe.mjs) are both built
// on it; see README.md.
//
//   const browser = await launch();
//   const c64 = await open(browser, 'gb64:b3/BRUCELEE_01135_02.zip');
//   await c64.joy(2, ['PULL_RIGHT'], 1000);
//   console.log(await c64.screenText(), await c64.cpu());
//   await browser.close();

import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import puppeteer from 'puppeteer-core';

export const BASE = process.env.BASE || 'http://localhost:8064/play/';
export const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/** Keys by name, as (row, column) of the C64 keyboard matrix. */
export const KEYS = {
  RETURN: [0, 1], CRSR_RIGHT: [0, 2], F7: [0, 3], F1: [0, 4], F3: [0, 5], F5: [0, 6], CRSR_DOWN: [0, 7],
  SPACE: [7, 4], RUNSTOP: [7, 7], 1: [7, 0], 2: [7, 3], Y: [3, 1], N: [4, 7],
};

/**
 * Injected into every frame before its own scripts: whatever gets connected to
 * an AudioContext's speakers is also fed to an AnalyserNode (window.__bbTap),
 * so a test can hear what the emulator plays.
 */
export function audioTap() {
  const connect = AudioNode.prototype.connect;
  AudioNode.prototype.connect = function (target, ...rest) {
    if (target instanceof AudioDestinationNode) {
      const ctx = target.context;
      ctx.__bbTap ??= Object.assign(ctx.createAnalyser(), { fftSize: 2048 });
      window.__bbTap = ctx.__bbTap;
      connect.call(this, ctx.__bbTap);
    }
    return connect.call(this, target, ...rest);
  };
}

/** Runs in the frame: how loud the tapped output is, sampled `n` times `ms` apart (RMS, 0..1). */
export function loudness(n = 20, ms = 150) {
  return new Promise((done) => {
    const out = [];
    const buf = new Float32Array(2048);
    const take = () => {
      const tap = window.__bbTap;
      if (tap) {
        tap.getFloatTimeDomainData(buf);
        out.push(Math.sqrt(buf.reduce((s, v) => s + v * v, 0) / buf.length));
      } else out.push(0);
      out.length < n ? setTimeout(take, ms) : done(out);
    };
    take();
  });
}

/** A headless Chromium. `profile` keeps its storage between launches (the playtest's workers do). */
export function launch({ profile } = {}) {
  return puppeteer.launch({
    executablePath: process.env.CHROME || '/usr/bin/chromium',
    headless: 'new',
    protocolTimeout: 180000,
    ...(profile && { userDataDir: path.join(os.tmpdir(), profile) }),
    args: ['--no-sandbox', '--autoplay-policy=no-user-gesture-required', '--mute-audio'],
  });
}

/**
 * Open a title (an id from the shelves: `gb64:<path>` or an Internet Archive
 * id) and press Play. Resolves once the emulator runs, `wait` ms later, or
 * with `error` set if the page showed one instead.
 * - `kind`: 'games' or 'demos', the page's section
 * - `boot` / `run`: the disk file or zip member to start (games.txt's "> file")
 * - `parts`: Internet Archive ids that together hold the release (games.txt's
 *   "a + b"): an EasySplit cartridge or the sides of a disk game
 * - `patches`: live patches for the game's loader ("addr:old=new" in hex)
 * - `roms`: { kernal, basic, charset, floppy } installed as the user's own ROMs
 *   first, as in Settings. KERNAL, BASIC and charset count only together; with
 *   `floppy` (16 KB, see signDriveRom) disks go to vc64web's emulated 1541
 *   instead of breadbin's virtual drive
 */
export async function open(browser, id, { kind = 'games', boot, run, parts, patches, roms = {}, wait = 0 } = {}) {
  const page = await browser.newPage();
  await page.setViewport({ width: 1100, height: 860 });
  await page.setCacheEnabled(false); // always the current page code
  const c64 = new C64(page);
  await page.evaluateOnNewDocument(() =>
    addEventListener('message', (e) => e.data?.msg === 'breadbin_drive' && console.log('DRIVE ' + JSON.stringify(e.data))),
  );
  await page.evaluateOnNewDocument(audioTap);
  page.on('console', (m) => m.text().startsWith('DRIVE ') && c64.drive.push(JSON.parse(m.text().slice(6))));
  // vc64web reports a jammed CPU with alert(), which would block its frame
  page.on('dialog', (d) => ((c64.jammed ||= /jammed/.test(d.message())), d.dismiss().catch(() => {})));
  if (process.env.DEBUG) page.on('pageerror', (e) => console.log('pageerror:', e.message));

  if (Object.keys(roms).length) {
    await page.goto(BASE, { waitUntil: 'domcontentloaded' });
    for (const [k, rom] of Object.entries(roms)) await page.evaluate(async (k, rom) => (await import('./store.js')).putBlob(`rom-${k}`, new Uint8Array(rom)), k, [...rom]);
  }
  const query = new URLSearchParams(Object.entries({ run, boot, parts: parts?.join(','), patches: patches?.join(',') }).filter(([, v]) => v));
  const q = query.size ? `?${query}` : '';
  await page.goto(`${BASE}${q}#${kind}/${encodeURIComponent(id)}`, { waitUntil: 'domcontentloaded' });
  await page.waitForSelector('#detail[open] .primary', { timeout: 45000 });
  await page.click('#detail[open] .primary');
  await page.waitForFunction(
    () => /running|loaded|warp/.test(document.querySelector('#emu-status')?.textContent || '') || document.querySelector('.emu-error'),
    { timeout: 120000 },
  );
  c64.error = await page.$eval('#emu-stage', (e) => e.querySelector('.emu-error')?.innerText.replace(/\s+/g, ' ') || null);
  c64.frame = page.frames().find((f) => f.url().includes('vc64web'));
  if (wait) await sleep(wait);
  return c64;
}

/** One running C64 in a page. Everything goes through vc64web's own JavaScript API. */
export class C64 {
  constructor(page) {
    this.page = page;
    this.frame = null;
    this.drive = []; // breadbin's virtual drive events: { event: 'load' | 'missing' | 'ready', name, addr, end }
    this.jammed = false;
    this.error = null;
  }

  /** Run `fn(...args)` inside vc64web's frame; a stuck emulator fails after 15 s instead of hanging. */
  ev(label, fn, ...args) {
    return Promise.race([this.frame.evaluate(fn, ...args), sleep(15000).then(() => Promise.reject(new Error(`emulator stopped answering (${label})`)))]);
  }

  /** `n` bytes from the CPU's view of memory (ROM where ROM is banked in). */
  peek(addr, n = 1) {
    return this.ev('peek', (a, n) => Array.from({ length: n }, (_, i) => wasm_peek(a + i)), addr, n).then((b) => (n === 1 ? b[0] : b));
  }

  /** Type through vc64web's auto-typer. Lowercase letters arrive as PETSCII capitals. */
  type(text) {
    return this.ev('type', (t) => wasm_auto_type(t), text);
  }

  /** Hold a key (a KEYS name or [row, col]) for `ms`, as a hand would; some games miss auto-typed keys. */
  key(k, ms = 150) {
    const [row, col] = typeof k === 'string' ? KEYS[k] : k;
    return this.ev(`key ${k}`, async (row, col, ms) => {
      wasm_key(row, col, 1);
      await new Promise((r) => setTimeout(r, ms));
      wasm_key(row, col, 0);
    }, row, col, ms);
  }

  /** Joystick on `port`: vc64web's commands ('PULL_RIGHT', 'PRESS_FIRE', 'RELEASE_XY'...). */
  joyCmd(port, cmd) {
    return this.ev(cmd, (c) => emit_joystick_cmd(c), port + cmd);
  }

  /** Hold `dirs` (e.g. ['PULL_RIGHT', 'PULL_DOWN']) for `ms`, then centre the stick. */
  async joy(port, dirs, ms) {
    for (const d of dirs) await this.joyCmd(port, d);
    await sleep(ms);
    await this.joyCmd(port, 'RELEASE_XY');
  }

  async fire(port, ms = 250) {
    await this.joyCmd(port, 'PRESS_FIRE');
    await sleep(ms);
    await this.joyCmd(port, 'RELEASE_FIRE');
  }

  /** The 1000 characters wherever the VIC shows the text screen; screen codes 0-31 read as @A-Z[\]^_. */
  screenText() {
    return this.ev('screen', () => {
      const base = (3 - (wasm_peek(0xdd00) & 3)) * 0x4000 + (wasm_peek(0xd018) >> 4) * 0x400;
      let s = '';
      for (let a = base; a < base + 1000; a++) {
        const c = wasm_peek(a) & 0x7f;
        s += String.fromCharCode(c < 32 ? c + 64 : c);
      }
      return s;
    });
  }

  /**
   * Sprite positions ([x, y], or null outside the visible screen), read from the
   * VIC itself (peeking $D000 reads RAM while a game banks the I/O out), and the
   * fine-scroll registers. Disabled sprites are included: compare over time.
   */
  sprites() {
    return this.ev('sprites', () => {
      const xy = wasm_sprite_info().split(',').map(Number);
      const sprites = Array.from({ length: 8 }, (_, i) => [xy[2 * i], xy[2 * i + 1]]).map((p) => (p[0] > 0 && p[0] < 344 && p[1] > 29 && p[1] < 250 ? p : null));
      return { sprites, scroll: (wasm_peek(0xd016) & 7) * 8 + (wasm_peek(0xd011) & 7) };
    });
  }

  /** The emulator's sound level over ~3 s: RMS samples, 0 when silent (see audioTap). */
  sound(n = 20, ms = 150) {
    return this.frame.evaluate(loudness, n, ms);
  }

  /** A PNG of the emulator's picture (written to `file` too, if given). */
  async shot(file) {
    const buf = await (await this.page.$('.emu-frame')).screenshot();
    if (file) fs.writeFileSync(file, buf);
    return buf;
  }

  /** Run RetroShell commands; returns what they printed. */
  shell(...cmds) {
    return this.ev('shell', async (cmds) => {
      const before = wasm_retro_shell_get_text().length;
      for (const c of cmds) {
        for (const ch of c) wasm_retro_shell_press_key(ch.charCodeAt(0));
        wasm_retro_shell_press_special(10, 0); // RETURN
      }
      await new Promise((r) => setTimeout(r, 300));
      return wasm_retro_shell_get_text().slice(before);
    }, cmds);
  }

  /** Pause in the debugger and run `cmds` there ('r', 'd $c000', 'm $0800'...), then carry on. */
  async debug(...cmds) {
    const out = await this.shell('.', ...cmds);
    await this.shell('.');
    return out;
  }

  /** The CPU's program counter, sampled `n` times a little apart: where a game loops or waits. */
  async pcs(n = 6) {
    const seen = [];
    for (let i = 0; i < n; i++) {
      // the prompt pads short addresses: "(0,1) $ f4c:"
      seen.push(/\(\d+,\d+\) \$ *([0-9a-f]{1,4}):/i.exec(await this.debug())?.[1]?.padStart(4, '0'));
      await sleep(250);
    }
    return seen;
  }

  /** Disassembly from `addr` (16 instructions). */
  async disasm(addr) {
    return (await this.debug(`d $${addr.toString(16)}`)).split('\n').filter((l) => /^\s*(->)?\s*[0-9A-F]{4} /.test(l)).join('\n');
  }

  /** vc64web's drive 8: connected, powered, which disk, and the 1541 ROM it holds (if any). */
  driveState() {
    return this.ev('drive', () => ({
      connected: String(wasm_get_config('DRIVE_CONNECT', 8)) === '1',
      power: String(wasm_get_config('DRIVE_POWER_SWITCH', 8)) === '1',
      disk: floppy_has_disk,
      file: file_slot_file_name,
      rom: local_storage_get('vc1541_rom.bin') != null,
    }));
  }

  close() {
    return this.page.close().catch(() => {});
  }
}

export { signDriveRom } from '../drive/sign.mjs';
