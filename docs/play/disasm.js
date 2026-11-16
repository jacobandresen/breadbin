// disasm - a 6502 disassembler, for the game tools' monitor (tools.js): the
// instructions at an address, as the C64's CPU will read them.
//
//   disasm(bytes, at)  -> { text: 'LDA $D012', size: 3, op: 'LDA', mode: 'abs', target: 0xd012 }
//   listing(peek, at, n) -> n of them from `at` (peek(addr) -> byte)
//
// The documented opcodes only: any other byte is shown as `.BYTE $xx`, one
// byte long (games do use the undocumented ones, but rarely in what a cheat
// finder lands on: the code that counts lives down).

// mode: [operand bytes, format]
const MODES = {
  imp: [0, () => ''],
  acc: [0, () => 'A'],
  imm: [1, (v) => `#$${hex2(v)}`],
  zp: [1, (v) => `$${hex2(v)}`],
  zpx: [1, (v) => `$${hex2(v)},X`],
  zpy: [1, (v) => `$${hex2(v)},Y`],
  izx: [1, (v) => `($${hex2(v)},X)`],
  izy: [1, (v) => `($${hex2(v)}),Y`],
  abs: [2, (v) => `$${hex4(v)}`],
  abx: [2, (v) => `$${hex4(v)},X`],
  aby: [2, (v) => `$${hex4(v)},Y`],
  ind: [2, (v) => `($${hex4(v)})`],
  rel: [1, (v, at) => `$${hex4((at + 2 + ((v << 24) >> 24)) & 0xffff)}`],
};

// opcode -> [mnemonic, mode]; written as "mnemonic mode:opcode,..." to stay short
const TABLE = `
ADC imm:69 zp:65 zpx:75 abs:6D abx:7D aby:79 izx:61 izy:71
AND imm:29 zp:25 zpx:35 abs:2D abx:3D aby:39 izx:21 izy:31
ASL acc:0A zp:06 zpx:16 abs:0E abx:1E
BCC rel:90
BCS rel:B0
BEQ rel:F0
BIT zp:24 abs:2C
BMI rel:30
BNE rel:D0
BPL rel:10
BRK imp:00
BVC rel:50
BVS rel:70
CLC imp:18
CLD imp:D8
CLI imp:58
CLV imp:B8
CMP imm:C9 zp:C5 zpx:D5 abs:CD abx:DD aby:D9 izx:C1 izy:D1
CPX imm:E0 zp:E4 abs:EC
CPY imm:C0 zp:C4 abs:CC
DEC zp:C6 zpx:D6 abs:CE abx:DE
DEX imp:CA
DEY imp:88
EOR imm:49 zp:45 zpx:55 abs:4D abx:5D aby:59 izx:41 izy:51
INC zp:E6 zpx:F6 abs:EE abx:FE
INX imp:E8
INY imp:C8
JMP abs:4C ind:6C
JSR abs:20
LDA imm:A9 zp:A5 zpx:B5 abs:AD abx:BD aby:B9 izx:A1 izy:B1
LDX imm:A2 zp:A6 zpy:B6 abs:AE aby:BE
LDY imm:A0 zp:A4 zpx:B4 abs:AC abx:BC
LSR acc:4A zp:46 zpx:56 abs:4E abx:5E
NOP imp:EA
ORA imm:09 zp:05 zpx:15 abs:0D abx:1D aby:19 izx:01 izy:11
PHA imp:48
PHP imp:08
PLA imp:68
PLP imp:28
ROL acc:2A zp:26 zpx:36 abs:2E abx:3E
ROR acc:6A zp:66 zpx:76 abs:6E abx:7E
RTI imp:40
RTS imp:60
SBC imm:E9 zp:E5 zpx:F5 abs:ED abx:FD aby:F9 izx:E1 izy:F1
SEC imp:38
SED imp:F8
SEI imp:78
STA zp:85 zpx:95 abs:8D abx:9D aby:99 izx:81 izy:91
STX zp:86 zpy:96 abs:8E
STY zp:84 zpx:94 abs:8C
TAX imp:AA
TAY imp:A8
TSX imp:BA
TXA imp:8A
TXS imp:9A
TYA imp:98
`;
const OPS = new Map();
for (const row of TABLE.trim().split('\n')) {
  const [op, ...modes] = row.trim().split(/\s+/);
  for (const m of modes) {
    const [mode, code] = m.split(':');
    OPS.set(parseInt(code, 16), [op, mode]);
  }
}

const hex2 = (v) => v.toString(16).toUpperCase().padStart(2, '0');
const hex4 = (v) => v.toString(16).toUpperCase().padStart(4, '0');

/** The instruction whose first byte is bytes[0], at address `at`. */
export function disasm(bytes, at = 0) {
  const known = OPS.get(bytes[0]);
  if (!known) return { text: `.BYTE $${hex2(bytes[0] ?? 0)}`, size: 1, op: null, mode: null, target: null };
  const [op, mode] = known;
  const [n, fmt] = MODES[mode];
  const v = n === 2 ? (bytes[1] | (bytes[2] << 8)) : n === 1 ? bytes[1] : 0;
  const operand = fmt(v, at);
  const target = mode === 'rel' ? (at + 2 + ((v << 24) >> 24)) & 0xffff : n === 2 || /^(zp|izx|izy)/.test(mode) ? v : null;
  return { text: operand ? `${op} ${operand}` : op, size: n + 1, op, mode, target };
}

/** `n` instructions from `at`: [{ addr, bytes, text, size, op, mode, target }]. */
export function listing(peek, at, n) {
  const out = [];
  for (let i = 0, a = at; i < n; i++) {
    const bytes = [peek(a), peek((a + 1) & 0xffff), peek((a + 2) & 0xffff)];
    const d = disasm(bytes, a);
    out.push({ addr: a, bytes: bytes.slice(0, d.size), ...d });
    a = (a + d.size) & 0xffff;
  }
  return out;
}

/** Does instruction `d` (from disasm) write to memory? (What a watchpoint on a store finds.) */
export const writes = (d) => /^(STA|STX|STY|INC|DEC|ASL|LSR|ROL|ROR)$/.test(d.op || '') && d.mode !== 'acc';

export { hex2, hex4 };
