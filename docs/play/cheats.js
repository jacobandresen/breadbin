// cheats - the cheat finder's search, as Action Replay cartridges did it:
// copy all of RAM, play a little, and keep only the bytes that changed the
// way the thing on screen did (a life lost: "went down"). A few rounds leave
// a handful of addresses, one of them the lives counter. Freezing it there
// (tools.js) is a trainer.
//
//   const s = new CheatSearch(ram);     // a Uint8Array(65536): every byte a candidate
//   s.filter('less', ram2);             // the ones that went down since
//   s.filter('equal', ram3, 2);         // the ones that are 2 now
//   s.addrs                             // what is left (Uint32Array)
//
// The I/O area ($D000-$DFFF) is left out: the chips' registers are not the
// game's state, and RAM there can't be read or frozen while I/O is in. So
// are $00/$01, the CPU's port.

import { disasm, writes } from './disasm.js';

export const KINDS = {
  less: { label: 'Went down', test: (now, was) => now < was },
  more: { label: 'Went up', test: (now, was) => now > was },
  same: { label: 'Unchanged', test: (now, was) => now === was },
  changed: { label: 'Changed', test: (now, was) => now !== was },
  equal: { label: 'Is', test: (now, was, n) => now === n },
};

const searchable = (a) => a >= 2 && (a < 0xd000 || a > 0xdfff);

export class CheatSearch {
  constructor(ram) {
    const all = [];
    for (let a = 0; a < 0x10000; a++) if (searchable(a)) all.push(a);
    this.addrs = Uint32Array.from(all);
    this.last = ram.slice();
    this.rounds = 0;
  }

  /** Keep the candidates whose byte in `ram` passed `kind` against the last copy (or equals n). */
  filter(kind, ram, n = 0) {
    const test = KINDS[kind].test;
    this.addrs = this.addrs.filter((a) => test(ram[a], this.last[a], n));
    this.last = ram.slice();
    this.rounds++;
    return this.addrs.length;
  }
}

/**
 * The instructions in `ram` that write to `addr`: [{ at, text, size,
 * indexed }]. A store, an INC/DEC or a shift with `addr` as its operand
 * (absolute, or zero page for addresses under $100); `indexed` for STA
 * addr,X and the like, which write there when X is 0 (a table's first entry).
 * Bytes in data can look like such an instruction too: the monitor's
 * breakpoint tells the real one (it stops when the game runs it).
 */
export function findWriters(ram, addr) {
  const out = [];
  for (let at = 0; at < 0xfffd; at++) {
    const d = disasm([ram[at], ram[at + 1], ram[at + 2]], at);
    if (!writes(d) || d.target !== addr) continue;
    if (/^(zp|zpx|zpy)$/.test(d.mode) && addr > 0xff) continue;
    if (/^(izx|izy)$/.test(d.mode)) continue; // (zp),Y writes where the pointer points, not to zp
    out.push({ at, text: d.text, size: d.size, indexed: /x|y/.test(d.mode) });
  }
  return out;
}

/** The bytes that make the instruction at `at` (`size` long) do nothing: NOPs. */
export const nops = (size) => Array(size).fill(0xea);
