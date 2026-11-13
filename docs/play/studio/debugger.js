// debugger - breakpoints and stepping, line by line through a game's C (and
// its own assembly) while it runs in the C64.
//
// The CPU is VirtualC64's, in vc64web: its RetroShell has a debugger with
// breakpoints ("break at $092c") that stop the whole machine and print the
// registers, and a one-instruction step ("s"). That is all this uses. A C
// step is breakpoints at the right addresses (debuginfo.js says which):
//   step over   every line start of the function we're in, and its return
//   step into   every line start of the program, and the return
//   step out    the return address only (then on to a line start)
//   pause       every line start: the next line the program reaches
// and then running until one is hit. Where the CPU stands on a breakpoint
// already, it first steps one instruction off it.
//
// Two halves: FRAME (a script, installed in vc64web's page, that types into
// RetroShell and watches the CPU's cycle count to see it stop) and Debugger
// (the Studio's side: lines and functions to addresses, the steps, the
// polling while the game runs). The Studio's session.call() joins them.

/** The emulator page's half: window.bbdbg. */
export const FRAME = String.raw`
if (!window.bbdbg) window.bbdbg = (() => {
  const text = () => wasm_retro_shell_get_text();
  const type = (cmd) => {
    for (const ch of cmd) wasm_retro_shell_press_key(ch.charCodeAt(0));
    wasm_retro_shell_press_special(10, 0); // RETURN
  };
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  const cycles = () => Number(wasm_get_cpu_cycles());
  const hex = (a) => '$' + a.toString(16);
  let on = false;      // RetroShell is in its debugger
  let set = [];        // the breakpoints, in RetroShell's order (it numbers them)
  let users = [];      // the Studio's breakpoints
  let temps = [];      // a step's
  let stop = null;     // where the CPU stopped: { pc, regs } (null: it runs)

  const enter = () => {
    if (on) return;
    type('clear');
    type('.');
    on = true;
  };
  // RetroShell's prompt shows the program counter: "(44,50) $ 92c: "
  const pc = () => {
    type('clear');
    const m = /\$ *([0-9a-f]{1,4}):\s*$/i.exec(text());
    return m ? parseInt(m[1], 16) : null;
  };
  // The registers RetroShell printed when a breakpoint stopped the CPU:
  //   " PC  SR AC XR YR SP  NV-BDIZC" then "92C0 30 00 00 40 F3  00110000"
  const dumped = () => {
    const all = [...text().matchAll(/PC\s+SR\s+AC\s+XR\s+YR\s+SP[^\n]*\n\s*\S+\s+([0-9A-F]{2})\s+([0-9A-F]{2})\s+([0-9A-F]{2})\s+([0-9A-F]{2})\s+([0-9A-F]{2})/gi)];
    const m = all[all.length - 1];
    if (!m) return null;
    const [sr, a, x, y, sp] = m.slice(1).map((v) => parseInt(v, 16));
    return { a, x, y, sp, sr };
  };
  // Make RetroShell's breakpoints exactly 'want'.
  const breakpoints = (want) => {
    enter();
    const keep = new Set(want);
    for (let i = set.length - 1; i >= 0; i--) {
      if (!keep.has(set[i])) {
        type('break delete ' + i);
        set.splice(i, 1);
      }
    }
    const have = new Set(set);
    for (const a of want) {
      if (have.has(a)) continue;
      type('break at ' + hex(a));
      set.push(a);
      have.add(a);
    }
    type('clear');
  };
  const running = async () => {
    const a = cycles();
    await sleep(50);
    return cycles() !== a;
  };
  // The stack pointer after one instruction (the step off a breakpoint), when it was known before.
  const spAfter = (op, regs) => {
    if (!regs || regs.sp == null) return null;
    const d = { 0x20: -2, 0x60: 2, 0x48: -1, 0x08: -1, 0x68: 1, 0x28: 1, 0x40: 3, 0x00: -3 }[op] || 0;
    if (op === 0x9a) return regs.x == null ? null : regs.x; // TXS
    return (regs.sp + d) & 0xff;
  };
  // One instruction (the CPU stopped): RetroShell's step, then let the emulator take it.
  const step1 = async () => {
    const before = cycles();
    const op = wasm_peek(stop.pc);
    type('s');
    try { wasm_run(); } catch (e) {} // emscripten's main loop throws to unwind; harmless
    for (let i = 0; i < 50 && cycles() === before; i++) await sleep(10);
    stop = { pc: pc(), regs: { a: null, x: null, y: null, sr: null, sp: spAfter(op, stop.regs) } };
  };
  const go = async (want) => {
    enter();
    temps = want;
    if (stop) {
      // a breakpoint where the CPU stands would stop it again at once
      if (users.includes(stop.pc) || want.includes(stop.pc)) await step1();
      if (users.includes(stop.pc) || want.includes(stop.pc)) return stop; // that one instruction was the step
    }
    breakpoints([...new Set([...users, ...want])]);
    type('clear');
    stop = null;
    try { wasm_run(); } catch (e) {}
    return null;
  };
  return {
    /** The Studio's breakpoints (addresses); kept until changed. */
    async users(addrs) {
      users = addrs;
      if (addrs.length || on) breakpoints([...new Set([...users, ...temps])]);
      return true;
    },
    /** Run until one of 'want' (or a breakpoint) is reached; resolves to the stop if it is already there, else null. */
    go,
    /** Has the CPU stopped? { pc, regs } if so (regs' values null where not known), null while it runs. */
    async status() {
      if (stop) return stop;
      if (!on || await running()) return null;
      const regs = dumped();
      stop = { pc: pc(), regs: regs || { a: null, x: null, y: null, sp: null, sr: null } };
      return stop;
    },
    /** Stop at the next of 'want'; if none comes within a second and a half, stop wherever the CPU is. */
    async pause(want) {
      const at = await go(want);
      if (at) return at;
      for (let i = 0; i < 15; i++) {
        await sleep(100);
        const s = await this.status();
        if (s) return s;
      }
      wasm_halt();
      await sleep(60);
      stop = { pc: pc(), regs: { a: null, x: null, y: null, sp: null, sr: null } };
      return stop;
    },
    /** Let go: no breakpoints, the CPU running, RetroShell out of its debugger. */
    async detach() {
      temps = [];
      users = [];
      if (on) {
        breakpoints([]);
        type('.');
        on = false;
      }
      if (stop) {
        stop = null;
        try { wasm_run(); } catch (e) {}
      }
      return true;
    },
  };
})();`;

const hex2 = (v) => (v == null ? '··' : v.toString(16).toUpperCase().padStart(2, '0'));
const hex4 = (v) => (v == null ? '····' : v.toString(16).toUpperCase().padStart(4, '0'));

/** The registers as a line: "PC 092C  A 00  X 00  Y 40  SP F3" (·· where not known). */
export function formatRegs(pc, regs = {}) {
  return `PC ${hex4(pc)}  A ${hex2(regs.a)}  X ${hex2(regs.x)}  Y ${hex2(regs.y)}  SP ${hex2(regs.sp)}`;
}

/**
 * The Studio's half. `onStop({ pc, regs, where, func })` when the CPU stops
 * (where: { file, line } or null; func: a function's name or null),
 * `onRun()` when it runs again, `onError(message)`.
 *
 *   const dbg = new Debugger({ onStop, onRun });
 *   dbg.attach(session, new DebugInfo(build.debug, { work: '/p' }));
 *   dbg.setBreakpoints([0x92c]);
 *   await dbg.pause(); await dbg.over(); await dbg.cont();
 */
export class Debugger {
  constructor({ onStop, onRun, onError } = {}) {
    this.onStop = onStop;
    this.onRun = onRun;
    this.onError = onError;
    this.session = null;
    this.info = null;
    this.users = [];
    this.stop = null; // { pc, regs } while stopped
    this.queue = Promise.resolve();
    this.polling = false;
    this.installed = null; // the session the frame half went into
  }

  /** A (re)started program: its session and its debug info (DebugInfo). */
  attach(session, info) {
    this.session = session;
    this.info = info;
    this.stop = null;
    this.ret = null;
    this.users = []; // the Studio sets this program's (setBreakpoints)
    this.onRun?.();
  }

  /** Nothing to debug (the C64 stopped). */
  detach() {
    this.session = null;
    this.stop = null;
    this.polling = false;
  }

  get stopped() {
    return !!this.stop;
  }

  /** One call at a time to the frame, in order. */
  call(fn, arg) {
    const s = this.session;
    if (!s) return Promise.resolve(null);
    if (this.installed !== s) {
      s.script(FRAME);
      this.installed = s;
    }
    const p = this.queue.then(() => s.call(fn, arg));
    this.queue = p.catch(() => {});
    return p;
  }

  /** The Studio's breakpoints: addresses where the program stops. */
  async setBreakpoints(addrs) {
    this.users = [...new Set(addrs)];
    if (!this.session || (!this.users.length && this.installed !== this.session)) return;
    await this.call('bbdbg.users', this.users).catch((e) => this.onError?.(e.message));
    this.poll();
  }

  /** Watch for a breakpoint stopping the CPU, while it runs. */
  async poll() {
    if (this.polling) return;
    this.polling = true;
    const session = this.session;
    while (this.polling && this.session === session && !this.stop) {
      await new Promise((r) => setTimeout(r, 150));
      if (this.session !== session || this.stop) break;
      let s = null;
      try {
        s = await this.call('bbdbg.status');
      } catch {
        break;
      }
      if (s) {
        this.polling = false;
        await this.halted(s);
        return;
      }
    }
    this.polling = false;
  }

  /** The CPU stopped at `s`: on to a line start if a step ended inside one, then tell the Studio. */
  async halted(s) {
    this.stop = s;
    if (this.ret != null && s.pc === this.ret && !this.info.isStart(s.pc) && this.hops++ < 4) {
      // back in the caller, after its call: the rest of its line, to the next line start
      return this.stepOver();
    }
    this.ret = null;
    this.onStop?.({ ...s, where: this.info.lineAt(s.pc), func: this.info.funcAt(s.pc)?.name || null });
  }

  /** Where the function we're in returns to (null where that isn't known). */
  async returnAddress() {
    const sp = this.stop?.regs?.sp;
    if (sp == null) return null;
    const bytes = await this.session.peek([[0x100 + ((sp + 1) & 0xff), 1], [0x100 + ((sp + 2) & 0xff), 1]]);
    if (!bytes) return null;
    const ret = ((bytes[0][0] | (bytes[1][0] << 8)) + 1) & 0xffff;
    // a C function's caller is C too (main's isn't: then there is nowhere to go back to)
    return this.info.funcAt(ret) ? ret : null;
  }

  /** Run until one of `want` (addresses), or a breakpoint. */
  async run(want, { ret = null } = {}) {
    if (!this.session) return;
    this.ret = ret;
    const at = await this.call('bbdbg.go', [...new Set(want)]);
    if (at) return this.halted(at);
    this.stop = null;
    this.onRun?.();
    this.poll();
  }

  /** Stop at the next line the program reaches (or wherever it is). */
  async pause() {
    if (!this.session || this.stop) return;
    this.ret = null;
    // your own code's lines (not the runtime's, gen/), unless there are none
    const mine = this.info.lineStarts().filter((a) => !/^gen\//.test(this.info.starts.get(a).file));
    const s = await this.call('bbdbg.pause', mine.length ? mine : this.info.lineStarts());
    if (s) await this.halted(s);
  }

  /** Run on, to the next breakpoint. */
  async cont() {
    if (!this.stop) return;
    await this.run([]);
  }

  /** The next line here (calls run through), or the caller's after a return. */
  async over() {
    if (!this.stop) return this.pause();
    this.hops = 0;
    return this.stepOver();
  }

  async stepOver() {
    const fn = this.info.funcAt(this.stop.pc);
    const ret = await this.returnAddress();
    // outside any function (the runtime's assembly, an interrupt): the next line anywhere
    const want = fn ? this.info.lineStarts(fn) : this.info.lineStarts();
    await this.run(ret != null ? [...want, ret] : want, { ret });
  }

  /** The next line anywhere: into a call. */
  async into() {
    if (!this.stop) return this.pause();
    this.hops = 0;
    const ret = await this.returnAddress();
    const want = this.info.lineStarts();
    await this.run(ret != null ? [...want, ret] : want, { ret });
  }

  /** On to the line after this function's call. */
  async out() {
    if (!this.stop) return this.pause();
    this.hops = 0;
    const ret = await this.returnAddress();
    if (ret == null) return this.stepOver();
    await this.run([ret], { ret });
  }

  /** Let the program go (no breakpoints) before it is replaced or stopped. */
  async release() {
    this.stop = null;
    this.polling = false;
    if (this.session && this.installed === this.session) await this.call('bbdbg.detach').catch(() => {});
  }
}
