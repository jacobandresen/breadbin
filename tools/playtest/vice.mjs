// vice - a VICE workbench for drive and KERNAL work: x64sc with true drive
// emulation, headless under Xvfb, driven through its remote monitor (a text
// protocol on TCP). Unlike vc64web, VICE's monitor reaches the 1541's own CPU,
// so a fast loader's drive code can be watched. See README.md.
//
//   const vice = await start({ disk: 'game.d64', type: 'load"*",8,1\nrun\n' });
//   await vice.run(20);                    // let it run 20 s
//   console.log(await vice.mon('r'));      // C64 registers
//   console.log(await vice.drive('r'));    // the drive's
//   await vice.shot('now.png');
//   await vice.quit();

import { spawn, execFileSync } from 'node:child_process';
import net from 'node:net';
import fs from 'node:fs';
import { sleep } from './c64.mjs';

let nextPort = 6510 + Math.floor(Math.random() * 400);

/**
 * Start x64sc. ROM options take files, and all are needed: `kernal`, `basic`
 * and `chargen` (C64), so VICE never falls back on the Commodore ROMs it ships
 * (commercially licensed; breadbin uses free ones only). Drive 8 is a true 1541
 * only with `dos`, a free 1541 ROM; without it there is no drive. `tape` goes in
 * the datasette (`mon('tapectrl 1')` presses PLAY). `disk` goes in drive 8
 * write-protected (a misbehaving KERNAL can send the drive garbage commands, and
 * a corrupted image fails every later run), `type` is typed after boot. The
 * emulator runs in warp (`warp: false` for real speed); `joystick: false` leaves
 * port 2 unplugged.
 */
export async function start({ kernal, basic, chargen, dos, disk, tape, type, warp = true, joystick = true } = {}) {
  if (!kernal || !basic || !chargen) throw new Error('vice.mjs: pass free kernal, basic and chargen ROMs (the Open ROMs)');
  const port = nextPort++;
  // an X server of our own, so xdotool can press keys in VICE's window
  const display = `:${port}`;
  const xvfb = spawn('Xvfb', [display, '-screen', '0', '1024x768x24'], { stdio: 'ignore', detached: true });
  // joystick port 2 on the numeric keypad: KP_8/2/4/6 and KP_0 for fire
  const args = ['-default', '-silent', '-sounddev', 'dummy', ...(dos ? ['-drive8type', '1541', '-drive8truedrive'] : ['-drive8type', '0']), '-jamaction', '2', ...(joystick ? ['-joydev2', '1'] : []),
    '-remotemonitor', '-remotemonitoraddress', `ip4://127.0.0.1:${port}`];
  if (warp) args.push('-warp');
  for (const [opt, file] of [['-kernal', kernal], ['-basic', basic], ['-chargen', chargen], ['-dos1541', dos], ['-8', disk], ['-1', tape]]) if (file) args.push(opt, file);
  if (disk) args.push('-attach8ro');
  if (type) args.push('-keybuf', type);
  await sleep(500);
  const proc = spawn('x64sc', args, { stdio: 'ignore', detached: true, env: { ...process.env, DISPLAY: display } });
  let sock;
  for (let i = 0; i < 100 && !sock; i++) {
    await sleep(200);
    sock = await new Promise((res) => {
      const s = net.connect(port, '127.0.0.1', () => res(s)).on('error', () => res(null));
    });
  }
  if (!sock) throw new Error('VICE did not start');
  return new Vice(proc, sock, xvfb, display);
}

class Vice {
  constructor(proc, sock, xvfb, display) {
    this.proc = proc;
    this.xvfb = xvfb;
    this.display = display;
    this.sock = sock;
    this.buf = '';
    sock.on('data', (d) => (this.buf += d.toString('latin1')));
  }

  /** Send monitor commands (this pauses the emulator); returns their output. */
  async mon(...cmds) {
    let out = '';
    for (const c of cmds) {
      this.buf = '';
      this.sock.write(c + '\n');
      for (let i = 0; i < 100 && !/\(C:\$[0-9a-f]{4}\) $/.test(this.buf); i++) await sleep(50);
      out += this.buf.replace(/\(C:\$[0-9a-f]{4}\) $/, '');
    }
    return out;
  }

  /** Commands on the drive's CPU (device 8), then back to the C64's. */
  async drive(...cmds) {
    const out = await this.mon('dev 8:', ...cmds);
    await this.mon('dev c:');
    return out;
  }

  /** Let the emulator run for `s` seconds of host time (monitor commands pause it). */
  async run(s) {
    this.sock.write('x\n');
    await sleep(s * 1000);
  }

  /**
   * Run until the C64's CPU reaches `addr` (within `s` seconds); true if it did.
   * The emulator stays paused there for monitor commands.
   */
  async runTo(addr, s = 30) {
    await this.mon(`break exec ${addr.toString(16)}`);
    this.buf = '';
    this.sock.write('x\n');
    for (let i = 0; i < s * 20 && !/\(C:\$[0-9a-f]{4}\) $/.test(this.buf); i++) await sleep(50);
    const hit = new RegExp(`\\(C:\\$${addr.toString(16).padStart(4, '0')}\\) $`).test(this.buf);
    if (!hit) await this.mon(''); // pause, so the breakpoint can go
    await this.mon('del');
    return hit;
  }

  /** Registers of the C64's CPU as numbers: { pc, a, x, y, sp, p }. */
  async regs() {
    const m = /\.;([0-9a-f]{4}) ([0-9a-f]{2}) ([0-9a-f]{2}) ([0-9a-f]{2}) ([0-9a-f]{2})/.exec(await this.mon('r'));
    const [pc, a, x, y, sp] = m.slice(1).map((h) => parseInt(h, 16));
    return { pc, a, x, y, sp, carry: /[01]{7}1 /.test((await this.mon('r')).split('\n').pop()) };
  }

  /** Paused at a JSR'd routine's entry: run until it returns to its caller. */
  async finish(s = 30) {
    const { sp } = await this.regs();
    const [lo, hi] = await this.peek(0x101 + sp, 2);
    return this.runTo(lo + 256 * hi + 1, s);
  }

  /** `n` bytes of the C64's memory as numbers (the drive's with `{ drive: true }`). */
  async peek(addr, n = 1, { drive = false } = {}) {
    const out = await (drive ? this.drive : this.mon).call(this, `m ${addr.toString(16)} ${(addr + n - 1).toString(16)}`);
    return [...out.matchAll(/>[C8]:[0-9a-f]{4}((?:\s+[0-9a-f]{2})+)/g)].flatMap((m) => m[1].trim().split(/\s+/).map((x) => parseInt(x, 16))).slice(0, n);
  }

  /**
   * Hold X keys (keysyms: 'space', 'F1', 'Return', 'KP_0' = fire, 'KP_6' = right...)
   * for `ms` in VICE's window, as a hand would; the emulator keeps running.
   */
  async key(keys, ms = 200) {
    const x = this.focus();
    for (const k of [].concat(keys)) x('keydown', k);
    await sleep(ms);
    for (const k of [].concat(keys)) x('keyup', k);
  }

  /** Type text (\n is RETURN) into VICE's window, key by key; the emulator keeps running. */
  async type(text) {
    const x = this.focus();
    for (const line of text.split(/(\n)/)) {
      if (line === '\n') x('key', '--delay', '80', 'Return');
      else if (line) x('type', '--delay', '80', line);
      await sleep(100);
    }
  }

  // resume, put VICE's window in front, and return an xdotool runner
  focus() {
    const x = (...a) => execFileSync('xdotool', a, { env: { ...process.env, DISPLAY: this.display } }).toString().trim();
    this.sock.write('x\n');
    x('windowfocus', '--sync', x('search', '--sync', '--onlyvisible', '--class', 'x64sc').split('\n')[0]);
    return x;
  }

  /** A PNG of the screen. */
  async shot(file) {
    await this.mon(`screenshot "${file}" 2`);
    for (let i = 0; i < 40 && !fs.existsSync(file); i++) await sleep(50);
  }

  async quit() {
    this.sock.write('quit\n');
    await sleep(300);
    this.sock.destroy();
    for (const p of [this.proc, this.xvfb]) {
      try {
        process.kill(-p.pid);
      } catch {}
    }
  }
}
