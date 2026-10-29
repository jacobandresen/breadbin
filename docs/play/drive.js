// drive - a virtual 1541 at the KERNAL level, for running disk images on the
// free Open ROMs (which come without a drive ROM). The KERNAL's LOAD vector
// ($0330) is pointed at a 32-byte stub that raises a flag and waits; a script
// running inside vc64web (its "script" message) sees the flag, reads the file
// name the program asked for, copies that file from the D64 straight into C64
// memory and lets the stub return, exactly as a finished LOAD would. The boot is
// a real LOAD"*",8,1, so autostarting loaders work. Fast loaders that program
// the drive itself still need a real 1541 ROM.

/**
 * Runs inside the vc64web frame. Kept self-contained: it is sent as source.
 * `b64` is the D64 image.
 */
function virtualDrive(b64) {
  const img = Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));
  const peek = (a) => wasm_peek(a);
  const poke = (a, v) => wasm_poke(a & 0xffff, v & 0xff);
  const say = (m) => window.parent.postMessage({ msg: 'breadbin_drive', ...m }, '*');

  // ── D64 ──
  const spt = (t) => (t <= 17 ? 21 : t <= 24 ? 19 : t <= 30 ? 18 : 17);
  const start = [0, 0];
  for (let t = 2; t <= 42; t++) start[t] = start[t - 1] + spt(t - 1);
  const sector = (t, s) => {
    const o = (start[t] + s) * 256;
    return t >= 1 && t <= 42 && s < spt(t) && o + 256 <= img.length ? img.subarray(o, o + 256) : null;
  };
  const chain = (t, s) => {
    const out = [];
    const seen = new Set();
    while (t && !seen.has(t * 64 + s)) {
      seen.add(t * 64 + s);
      const sec = sector(t, s);
      if (!sec) break;
      if (sec[0] === 0) {
        out.push(...sec.subarray(2, Math.max(2, sec[1] + 1)));
        break;
      }
      out.push(...sec.subarray(2));
      [t, s] = [sec[0], sec[1]];
    }
    return out;
  };
  const files = [];
  {
    let [t, s] = [18, 1];
    const seen = new Set();
    while (t && !seen.has(t * 64 + s)) {
      seen.add(t * 64 + s);
      const sec = sector(t, s);
      if (!sec) break;
      for (let e = 0; e < 8; e++) {
        const ent = sec.subarray(e * 32, e * 32 + 32);
        if (!(ent[2] & 0x80) || (ent[2] & 7) === 0) continue;
        const name = [...ent.subarray(5, 21)];
        while (name.length && name[name.length - 1] === 0xa0) name.pop();
        files.push({ type: ent[2] & 7, name, t: ent[3], s: ent[4], blocks: ent[30] | (ent[31] << 8) });
      }
      [t, s] = [sec[0], sec[1]];
    }
  }
  const bam = sector(18, 0);

  // "*" and "?" wildcards; a leading "0:" or "@0:" drive prefix is ignored
  const matches = (pat, name) => {
    for (let i = 0; i < pat.length; i++) {
      if (pat[i] === 0x2a) return true;
      if (i >= name.length || (pat[i] !== 0x3f && pat[i] !== name[i])) return false;
    }
    return pat.length === name.length;
  };

  /** LOAD"$": the directory as a BASIC program, like the drive sends it. */
  const directory = () => {
    const out = [0x01, 0x08];
    let addr = 0x0801;
    const line = (num, bytes) => {
      addr += 4 + bytes.length + 1;
      out.push(addr & 0xff, addr >> 8, num & 0xff, num >> 8, ...bytes, 0);
    };
    const title = bam ? [...bam.subarray(0x90, 0xa0)].map((b) => (b === 0xa0 ? 0x20 : b)) : [];
    line(0, [0x12, 0x22, ...title, 0x22, 0x20, ...(bam ? [...bam.subarray(0xa2, 0xa7)].map((b) => (b === 0xa0 ? 0x20 : b)) : [])]);
    for (const f of files) {
      const pad = Array(Math.max(0, 3 - String(f.blocks).length)).fill(0x20);
      const name = [0x22, ...f.name, 0x22, ...Array(Math.max(0, 17 - f.name.length)).fill(0x20)];
      const type = [...['DEL', 'SEQ', 'PRG', 'USR', 'REL'][f.type] || 'PRG'].map((c) => c.charCodeAt(0));
      line(f.blocks, [...pad, ...name, ...type]);
    }
    line(0, [...'BLOCKS FREE.'].map((c) => c.charCodeAt(0)));
    out.push(0, 0);
    return out;
  };

  // ── the 6502 side ──
  // The entry stub (on the LOAD vector) records the request and spins on FLAG.
  // The file may overwrite the stub itself (autostart loaders often cover page
  // 2), so before copying we park the CPU in a second loop ("park") placed
  // outside the file's range: patch the spin into a JMP there, wait until the
  // park code reports in, then copy and release it.
  const STUB = 0x02a7;
  const FLAG = 0x02ff;
  // STX $C3 / STY $C4 / STA $93 / LDA #$80 / STA FLAG / spin: BIT FLAG / BMI spin / SEC / RTS
  const ENTRY = [0x86, 0xc3, 0x84, 0xc4, 0x85, 0x93, 0xa9, 0x80, 0x8d, 0xff, 0x02, 0x2c, 0xff, 0x02, 0x30, 0xfb, 0x38, 0x60];
  const SPIN = STUB + 11;
  // LDA #$40 / STA FLAG / wait: BIT DONE / BMI wait / BVS err / LDX $AE / LDY $AF / CLC / RTS / NOP /
  // err: LDA #4 / SEC / RTS / DONE
  const park = (at) => [
    0xa9, 0x40, 0x8d, 0xff, 0x02, 0x2c, (at + 23) & 0xff, (at + 23) >> 8, 0x30, 0xfb, 0x70, 0x07,
    0xa6, 0xae, 0xa4, 0xaf, 0x18, 0x60, 0xea, 0xa9, 0x04, 0x38, 0x60, 0x80,
  ];
  // vectors autostarting programs hook, in the order the stock ROMs call them
  const AUTOSTART = [0x326, 0x302, 0x328, 0x300, 0x324, 0x318];
  const PARK_SPOTS = [0x02c8, 0x033c, 0x0110, 0xcfe0, 0x9fe0];
  const overlaps = (a0, a1, b0, b1) => a0 < b1 && b0 < a1;

  const romLoad = peek(0x330) | (peek(0x331) << 8);
  // The stock KERNAL/BASIC vector table ($0300-$0333). Programs often load a copy
  // of it; those addresses mean nothing in the Open ROMs, so they are swapped
  // for the Open ROMs' own handlers (as found at start-up).
  const STOCK = [
    0xe38b, 0xa483, 0xa57c, 0xa71a, 0xa7e4, 0xae86, 0, 0, 0, 0, 0xea31, 0xfe66, 0xfe47, 0xf34a,
    0xf291, 0xf20e, 0xf250, 0xf333, 0xf157, 0xf1ca, 0xf6ed, 0xf13e, 0xf32f, 0xfe66, 0xf4a5, 0xf5ed,
  ];
  const openDefaults = STOCK.map((_, i) => peek(0x300 + i * 2) | (peek(0x301 + i * 2) << 8));
  const translateVectors = (addr, end) => {
    STOCK.forEach((stock, i) => {
      const v = 0x300 + i * 2;
      if (!stock || !overlaps(v, v + 2, addr, end)) return;
      if ((peek(v) | (peek(v + 1) << 8)) !== stock) return;
      const own = v === 0x330 ? STUB : openDefaults[i];
      poke(v, own);
      poke(v + 1, own >> 8);
    });
  };
  const install = () => {
    ENTRY.forEach((b, i) => poke(STUB + i, b));
    poke(FLAG, 0);
    poke(0x330, STUB & 0xff);
    poke(0x331, STUB >> 8);
  };

  /** Look up the requested file: { data, name } or null. */
  const lookup = () => {
    const len = peek(0xb7);
    const ptr = peek(0xbb) | (peek(0xbc) << 8);
    let pat = [];
    for (let i = 0; i < len; i++) pat.push(peek(ptr + i));
    const colon = pat.indexOf(0x3a);
    if (colon >= 0 && colon <= 2) pat = pat.slice(colon + 1);
    if (peek(0xba) !== 8 || !pat.length) return null;
    if (pat.length === 1 && pat[0] === 0x24) return { data: directory(), name: '$' };
    const f = files.find((x) => x.type === 2 && matches(pat, x.name)) || files.find((x) => matches(pat, x.name));
    if (!f) {
      say({ event: 'missing', name: String.fromCharCode(...pat) });
      return null;
    }
    const name = String.fromCharCode(...f.name.map((c) => (c >= 0xc1 && c <= 0xda ? c - 0x80 : c)));
    return { data: chain(f.t, f.s), name };
  };

  let loads = 0;
  let job = null; // { spot, data, name, addr, end } while the CPU is being parked

  const tick = () => {
    if (!job) {
      if (peek(FLAG) !== 0x80 || (peek(0x330) | (peek(0x331) << 8)) !== STUB) return;
      const file = lookup();
      let addr = 0;
      let end = 0;
      if (file && file.data.length >= 2) {
        addr = peek(0xb9) === 0 ? peek(0xc3) | (peek(0xc4) << 8) : file.data[0] | (file.data[1] << 8);
        end = addr + file.data.length - 2;
      }
      const spot = PARK_SPOTS.find((p) => !overlaps(p, p + 24, addr, end) && !overlaps(p, p + 24, STUB, STUB + ENTRY.length) && !overlaps(p, p + 24, FLAG, FLAG + 1));
      job = { ...file, ok: !!(file && end > addr), addr, end, spot };
      park(spot).forEach((b, i) => poke(spot + i, b));
      poke(SPIN, 0x4c); // JMP park
      poke(SPIN + 1, spot & 0xff);
      poke(SPIN + 2, spot >> 8);
      return;
    }
    if (peek(FLAG) !== 0x40) return; // not parked yet
    const { spot, data, addr, end, ok, name } = job;
    job = null;
    const vectors = AUTOSTART.map((v) => peek(v) | (peek(v + 1) << 8));
    if (ok && peek(0x93) === 0) {
      for (let i = 2; i < data.length; i++) poke(addr + i - 2, data[i]);
      translateVectors(addr, end);
    }
    if (ok) {
      poke(0xae, end);
      poke(0xaf, end >> 8);
      poke(0x90, 0x40); // status: end of file
    }
    // Autostart. Under the stock ROMs a LOAD typed at READY returns with the
    // stack pointer at $F7, so files covering $01F8 plant their own return
    // address there; after that, BASIC prints READY (through CHROUT) and re-enters
    // its loop (through IMAIN), so other cracks hook those vectors. The Open
    // ROMs differ in both, so the parked CPU leaves the way the stock ROMs would.
    if (ok && loads === 0) {
      let target = -1;
      if (overlaps(0x1f8, 0x1fa, addr, end)) target = ((peek(0x1f8) | (peek(0x1f9) << 8)) + 1) & 0xffff;
      const hooked = AUTOSTART.findIndex((v, i) => overlaps(v, v + 2, addr, end) && (peek(v) | (peek(v + 1) << 8)) !== vectors[i]);
      if (target >= 0) {
        poke(spot + 16, 0x4c); // CLC / RTS -> JMP target
        poke(spot + 17, target & 0xff);
        poke(spot + 18, target >> 8);
      } else if (hooked >= 0) {
        poke(spot + 16, 0x6c); // CLC / RTS -> JMP (vector)
        poke(spot + 17, AUTOSTART[hooked] & 0xff);
        poke(spot + 18, AUTOSTART[hooked] >> 8);
      }
    }
    // put the entry stub back unless the program just loaded over it
    if (!ok || !overlaps(STUB, FLAG + 1, addr, end)) {
      ENTRY.forEach((b, i) => poke(STUB + i, b));
      poke(FLAG, 0);
      if (!ok || !overlaps(0x330, 0x332, addr, end)) install();
    }
    poke(spot + 23, ok ? 0x00 : 0x40); // release the parked CPU
    if (ok) {
      window.breadbinLoads = ++loads;
      say({ event: 'load', name, addr, end, first: loads === 1 });
    }
  };

  install();
  setInterval(() => {
    tick();
    // a program that resets the KERNAL vectors (RESTOR) gets the drive back
    if (!job && (peek(0x330) | (peek(0x331) << 8)) === romLoad) install();
  }, 4);
  say({ event: 'ready', files: files.length });
}

/** Script source for vc64web's {cmd: "script"} that attaches the drive to `d64`. */
export function driveScript(d64) {
  let bin = '';
  for (let i = 0; i < d64.length; i += 0x8000) bin += String.fromCharCode(...d64.subarray(i, i + 0x8000));
  return `(${virtualDrive.toString()})(${JSON.stringify(btoa(bin))});`;
}

/**
 * Script that waits for READY, types LOAD"*",8,1 and then, like a person
 * would, types RUN if the machine comes back to READY after the first load.
 * (Autostarting loaders never return to READY, so they're left alone.)
 */
export function bootScript() {
  return `
    const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
    const readies = () => {
      let n = 0;
      for (let a = 0x400; a < 0x7e3; a += 1) {
        if (wasm_peek(a) === 0x12 && wasm_peek(a + 1) === 0x05 && wasm_peek(a + 2) === 0x01 &&
            wasm_peek(a + 3) === 0x04 && wasm_peek(a + 4) === 0x19 && wasm_peek(a + 5) === 0x2e) n++;
      }
      return n;
    };
    for (let i = 0; i < 100 && readies() === 0; i++) await sleep(100);
    const before = readies();
    wasm_auto_type('load"*",8,1:\\n');
    for (let i = 0; i < 300 && !window.breadbinLoads; i++) await sleep(100);
    for (let i = 0; i < 30; i++) {
      await sleep(100);
      if (readies() > before) {
        wasm_auto_type('run:\\n');
        break;
      }
    }`;
}
