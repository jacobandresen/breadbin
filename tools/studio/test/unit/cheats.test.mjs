// The player's Tools: the 6502 disassembler (disasm.js) and the cheat
// finder's search (cheats.js), on made-up memory.
import test from 'node:test';
import assert from 'node:assert/strict';
import { disasm, listing, writes } from '../../../../docs/play/disasm.js';
import { CheatSearch, findWriters, nops } from '../../../../docs/play/cheats.js';

test('disasm: each addressing mode, branches to their targets, unknown bytes as .BYTE', () => {
  const code = [0xa9, 0x00, 0x8d, 0x20, 0xd0, 0xd0, 0xf9, 0xb1, 0xfb, 0x9d, 0x00, 0x04, 0x6c, 0x14, 0x03, 0x0a, 0x02, 0x60];
  const at = 0xc000;
  const got = listing((a) => code[a - at] ?? 0, at, 8).map((d) => `${d.addr.toString(16)} ${d.text}`);
  assert.deepEqual(got, ['c000 LDA #$00', 'c002 STA $D020', 'c005 BNE $C000', 'c007 LDA ($FB),Y', 'c009 STA $0400,X', 'c00c JMP ($0314)', 'c00f ASL A', 'c010 .BYTE $02']);
  assert.ok(writes(disasm([0xce, 0x3c, 0x0a])));
  assert.ok(!writes(disasm([0x0a])), 'ASL A writes no memory');
  assert.ok(!writes(disasm([0xad, 0x3c, 0x0a])));
});

test('the search: start with every RAM byte, keep what went down, then what is 1', () => {
  const ram = new Uint8Array(65536);
  ram[0x0a3c] = 3; // the lives
  ram[0x2000] = 9; // a timer, also going down
  ram[0xd020] = 3; // a chip's register: never a candidate
  const s = new CheatSearch(ram);
  assert.equal(s.addrs.length, 65536 - 2 - 4096);
  const next = ram.slice();
  next[0x0a3c] = 2;
  next[0x2000] = 8;
  assert.equal(s.filter('less', next), 2);
  const last = next.slice();
  last[0x0a3c] = 1;
  last[0x2000] = 7;
  s.filter('equal', last, 1);
  assert.deepEqual([...s.addrs], [0x0a3c]);
});

test('who writes it: stores, INC/DEC and indexed stores to the address, not reads or pointers', () => {
  const ram = new Uint8Array(65536);
  ram.set([0xce, 0x3c, 0x0a, 0xad, 0x3c, 0x0a, 0x8d, 0x3c, 0x0a, 0x9d, 0x3c, 0x0a], 0x1000);
  ram.set([0xc6, 0xfb, 0x91, 0xfb], 0x2000);
  assert.deepEqual(findWriters(ram, 0x0a3c).map((w) => [w.at, w.text, w.indexed]), [[0x1000, 'DEC $0A3C', false], [0x1006, 'STA $0A3C', false], [0x1009, 'STA $0A3C,X', true]]);
  assert.deepEqual(findWriters(ram, 0xfb).map((w) => w.text), ['DEC $FB'], 'STA ($FB),Y writes where $FB points');
  assert.deepEqual(nops(3), [0xea, 0xea, 0xea]);
});
