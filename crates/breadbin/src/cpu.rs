//! A cycle-counting NMOS 6502, including the stable undocumented opcodes that
//! demoscene music routines use (LAX, SAX, DCP, ISC, SLO, RLA, SRE, RRA, ANC,
//! ALR, ARR, SBX and the multi-byte NOPs).

/// What the CPU is attached to.
pub trait Bus {
    fn read(&mut self, addr: u16) -> u8;
    fn write(&mut self, addr: u16, value: u8);
}

const C: u8 = 0x01;
const Z: u8 = 0x02;
const I: u8 = 0x04;
const D: u8 = 0x08;
const B: u8 = 0x10;
const U: u8 = 0x20;
const V: u8 = 0x40;
const N: u8 = 0x80;

/// Base cycles per opcode (page-crossing and branch penalties are added).
#[rustfmt::skip]
const CYCLES: [u8; 256] = [
    7,6,2,8,3,3,5,5,3,2,2,2,4,4,6,6, 2,5,2,8,4,4,6,6,2,4,2,7,4,4,7,7,
    6,6,2,8,3,3,5,5,4,2,2,2,4,4,6,6, 2,5,2,8,4,4,6,6,2,4,2,7,4,4,7,7,
    6,6,2,8,3,3,5,5,3,2,2,2,3,4,6,6, 2,5,2,8,4,4,6,6,2,4,2,7,4,4,7,7,
    6,6,2,8,3,3,5,5,4,2,2,2,5,4,6,6, 2,5,2,8,4,4,6,6,2,4,2,7,4,4,7,7,
    2,6,2,6,3,3,3,3,2,2,2,2,4,4,4,4, 2,6,2,6,4,4,4,4,2,5,2,5,5,5,5,5,
    2,6,2,6,3,3,3,3,2,2,2,2,4,4,4,4, 2,5,2,5,4,4,4,4,2,4,2,4,4,4,4,4,
    2,6,2,8,3,3,5,5,2,2,2,2,4,4,6,6, 2,5,2,8,4,4,6,6,2,4,2,7,4,4,7,7,
    2,6,2,8,3,3,5,5,2,2,2,2,4,4,6,6, 2,5,2,8,4,4,6,6,2,4,2,7,4,4,7,7,
];

#[derive(Default)]
pub struct Cpu {
    pub a: u8,
    pub x: u8,
    pub y: u8,
    pub s: u8,
    pub p: u8,
    pub pc: u16,
    /// Set by a JAM opcode; the CPU stays put until reset.
    pub jammed: bool,
    extra: u8,
}

enum Mode {
    Imm,
    Zp,
    ZpX,
    ZpY,
    Abs,
    AbsX,
    AbsY,
    IndX,
    IndY,
}

impl Cpu {
    pub fn new() -> Cpu {
        Cpu { s: 0xff, p: U | I, ..Default::default() }
    }

    fn fetch(&mut self, b: &mut impl Bus) -> u8 {
        let v = b.read(self.pc);
        self.pc = self.pc.wrapping_add(1);
        v
    }
    fn fetch16(&mut self, b: &mut impl Bus) -> u16 {
        let lo = self.fetch(b) as u16;
        lo | (self.fetch(b) as u16) << 8
    }
    fn flag(&mut self, f: u8, on: bool) {
        if on { self.p |= f } else { self.p &= !f }
    }
    fn nz(&mut self, v: u8) -> u8 {
        self.flag(Z, v == 0);
        self.flag(N, v & 0x80 != 0);
        v
    }
    pub fn push(&mut self, b: &mut impl Bus, v: u8) {
        b.write(0x100 | self.s as u16, v);
        self.s = self.s.wrapping_sub(1);
    }
    fn pull(&mut self, b: &mut impl Bus) -> u8 {
        self.s = self.s.wrapping_add(1);
        b.read(0x100 | self.s as u16)
    }
    fn read16_wrap(b: &mut impl Bus, a: u16) -> u16 {
        // the 6502's JMP ($xxFF) and zero-page pointer wrap-around
        let hi = (a & 0xff00) | (a.wrapping_add(1) & 0x00ff);
        b.read(a) as u16 | (b.read(hi) as u16) << 8
    }

    /// Effective address; read instructions pay a cycle when indexing crosses a page.
    fn addr(&mut self, b: &mut impl Bus, mode: Mode) -> u16 {
        let cross = |base: u16, a: u16| (base & 0xff00) != (a & 0xff00);
        match mode {
            Mode::Imm => {
                let a = self.pc;
                self.pc = self.pc.wrapping_add(1);
                a
            }
            Mode::Zp => self.fetch(b) as u16,
            Mode::ZpX => self.fetch(b).wrapping_add(self.x) as u16,
            Mode::ZpY => self.fetch(b).wrapping_add(self.y) as u16,
            Mode::Abs => self.fetch16(b),
            Mode::AbsX | Mode::AbsY => {
                let base = self.fetch16(b);
                let a = base.wrapping_add(if matches!(mode, Mode::AbsX) { self.x } else { self.y } as u16);
                self.extra = cross(base, a) as u8;
                a
            }
            Mode::IndX => {
                let z = self.fetch(b).wrapping_add(self.x);
                Self::read16_wrap(b, z as u16)
            }
            Mode::IndY => {
                let z = self.fetch(b);
                let base = Self::read16_wrap(b, z as u16);
                let a = base.wrapping_add(self.y as u16);
                self.extra = cross(base, a) as u8;
                a
            }
        }
    }

    fn adc(&mut self, m: u8) {
        let c = (self.p & C) as u16;
        if self.p & D != 0 {
            let mut lo = (self.a & 0x0f) as u16 + (m & 0x0f) as u16 + c;
            let mut hi = (self.a >> 4) as u16 + (m >> 4) as u16;
            if lo > 9 {
                lo += 6;
                hi += 1;
            }
            let bin = self.a as u16 + m as u16 + c;
            self.flag(Z, bin & 0xff == 0);
            self.flag(N, hi & 0x08 != 0);
            self.flag(V, ((self.a ^ m) & 0x80 == 0) && ((self.a as u16 ^ (hi << 4)) & 0x80 != 0));
            if hi > 9 {
                hi += 6;
            }
            self.flag(C, hi > 15);
            self.a = ((hi << 4) | (lo & 0x0f)) as u8;
        } else {
            let sum = self.a as u16 + m as u16 + c;
            self.flag(C, sum > 0xff);
            self.flag(V, (!(self.a ^ m) & (self.a ^ sum as u8) & 0x80) != 0);
            self.a = self.nz(sum as u8);
        }
    }

    fn sbc(&mut self, m: u8) {
        if self.p & D != 0 {
            let c = 1 - (self.p & C) as i16;
            let bin = self.a as i16 - m as i16 - c;
            let mut lo = (self.a & 0x0f) as i16 - (m & 0x0f) as i16 - c;
            let mut hi = (self.a >> 4) as i16 - (m >> 4) as i16;
            if lo < 0 {
                lo -= 6;
                hi -= 1;
            }
            if hi < 0 {
                hi -= 6;
            }
            self.flag(C, bin >= 0);
            self.flag(V, ((self.a ^ m) & (self.a ^ bin as u8) & 0x80) != 0);
            self.nz(bin as u8);
            self.a = ((hi << 4) as u8) | (lo as u8 & 0x0f);
        } else {
            self.adc(!m);
        }
    }

    fn cmp(&mut self, r: u8, m: u8) {
        self.flag(C, r >= m);
        self.nz(r.wrapping_sub(m));
    }

    fn branch(&mut self, b: &mut impl Bus, cond: bool) -> u8 {
        let off = self.fetch(b) as i8;
        if !cond {
            return 0;
        }
        let to = self.pc.wrapping_add(off as u16);
        let cost = 1 + ((to & 0xff00) != (self.pc & 0xff00)) as u8;
        self.pc = to;
        cost
    }

    /// Take an interrupt through `vector` (IRQ $FFFE, NMI $FFFA). Returns cycles.
    pub fn interrupt(&mut self, b: &mut impl Bus, vector: u16) -> u32 {
        self.push(b, (self.pc >> 8) as u8);
        self.push(b, self.pc as u8);
        self.push(b, (self.p & !B) | U);
        self.p |= I;
        self.pc = b.read(vector) as u16 | (b.read(vector + 1) as u16) << 8;
        7
    }

    pub fn irq_enabled(&self) -> bool {
        self.p & I == 0
    }

    /// Execute one instruction; returns the cycles it took.
    pub fn step(&mut self, b: &mut impl Bus) -> u32 {
        if self.jammed {
            return 2;
        }
        let op = self.fetch(b);
        self.extra = 0;
        let mut cycles = CYCLES[op as usize];
        // column/row decoding for the regular opcodes; the rest are matched explicitly
        let mode = |op: u8| match op & 0x1f {
            0x00 | 0x02 | 0x09 | 0x0b => Mode::Imm,
            0x01 | 0x03 => Mode::IndX,
            0x04..=0x07 => Mode::Zp,
            0x0c..=0x0f => Mode::Abs,
            0x11 | 0x13 => Mode::IndY,
            0x14 | 0x15 => Mode::ZpX,
            0x16 | 0x17 => if matches!(op, 0x96 | 0x97 | 0xb6 | 0xb7) { Mode::ZpY } else { Mode::ZpX },
            0x19 | 0x1b => Mode::AbsY,
            0x1c | 0x1d => Mode::AbsX,
            0x1e | 0x1f => if matches!(op, 0x9e | 0x9f | 0xbe | 0xbf) { Mode::AbsY } else { Mode::AbsX },
            _ => Mode::Imm,
        };
        // read-modify-write helper
        macro_rules! rmw {
            ($f:expr) => {{
                let a = self.addr(b, mode(op));
                let v = b.read(a);
                b.write(a, v); // the 6502 writes the old value back first
                let r = $f(self, v);
                b.write(a, r);
                self.extra = 0;
                r
            }};
        }
        macro_rules! rd {
            () => {{
                let a = self.addr(b, mode(op));
                b.read(a)
            }};
        }
        let asl = |c: &mut Cpu, v: u8| { c.flag(C, v & 0x80 != 0); c.nz(v << 1) };
        let lsr = |c: &mut Cpu, v: u8| { c.flag(C, v & 1 != 0); c.nz(v >> 1) };
        let rol = |c: &mut Cpu, v: u8| { let r = (v << 1) | (c.p & C); c.flag(C, v & 0x80 != 0); c.nz(r) };
        let ror = |c: &mut Cpu, v: u8| { let r = (v >> 1) | ((c.p & C) << 7); c.flag(C, v & 1 != 0); c.nz(r) };

        match op {
            // ── loads, stores, transfers ──
            0xa9 | 0xa5 | 0xb5 | 0xad | 0xbd | 0xb9 | 0xa1 | 0xb1 => { let v = rd!(); self.a = self.nz(v); }
            0xa2 | 0xa6 | 0xb6 | 0xae | 0xbe => { let v = rd!(); self.x = self.nz(v); }
            0xa0 | 0xa4 | 0xb4 | 0xac | 0xbc => { let v = rd!(); self.y = self.nz(v); }
            0x85 | 0x95 | 0x8d | 0x9d | 0x99 | 0x81 | 0x91 => { let a = self.addr(b, mode(op)); b.write(a, self.a); self.extra = 0; }
            0x86 | 0x96 | 0x8e => { let a = self.addr(b, mode(op)); b.write(a, self.x); }
            0x84 | 0x94 | 0x8c => { let a = self.addr(b, mode(op)); b.write(a, self.y); }
            0xaa => self.x = self.nz(self.a),
            0xa8 => self.y = self.nz(self.a),
            0x8a => self.a = self.nz(self.x),
            0x98 => self.a = self.nz(self.y),
            0xba => self.x = self.nz(self.s),
            0x9a => self.s = self.x,
            // ── arithmetic and logic ──
            0x69 | 0x65 | 0x75 | 0x6d | 0x7d | 0x79 | 0x61 | 0x71 => { let v = rd!(); self.adc(v); }
            0xe9 | 0xeb | 0xe5 | 0xf5 | 0xed | 0xfd | 0xf9 | 0xe1 | 0xf1 => { let v = rd!(); self.sbc(v); }
            0x29 | 0x25 | 0x35 | 0x2d | 0x3d | 0x39 | 0x21 | 0x31 => { let v = rd!(); self.a = self.nz(self.a & v); }
            0x09 | 0x05 | 0x15 | 0x0d | 0x1d | 0x19 | 0x01 | 0x11 => { let v = rd!(); self.a = self.nz(self.a | v); }
            0x49 | 0x45 | 0x55 | 0x4d | 0x5d | 0x59 | 0x41 | 0x51 => { let v = rd!(); self.a = self.nz(self.a ^ v); }
            0xc9 | 0xc5 | 0xd5 | 0xcd | 0xdd | 0xd9 | 0xc1 | 0xd1 => { let v = rd!(); self.cmp(self.a, v); }
            0xe0 | 0xe4 | 0xec => { let v = rd!(); self.cmp(self.x, v); }
            0xc0 | 0xc4 | 0xcc => { let v = rd!(); self.cmp(self.y, v); }
            0x24 | 0x2c => { let v = rd!(); self.flag(Z, self.a & v == 0); self.flag(N, v & 0x80 != 0); self.flag(V, v & 0x40 != 0); }
            // ── shifts, increments ──
            0x0a => self.a = asl(self, self.a),
            0x4a => self.a = lsr(self, self.a),
            0x2a => self.a = rol(self, self.a),
            0x6a => self.a = ror(self, self.a),
            0x06 | 0x16 | 0x0e | 0x1e => { rmw!(asl); }
            0x46 | 0x56 | 0x4e | 0x5e => { rmw!(lsr); }
            0x26 | 0x36 | 0x2e | 0x3e => { rmw!(rol); }
            0x66 | 0x76 | 0x6e | 0x7e => { rmw!(ror); }
            0xe6 | 0xf6 | 0xee | 0xfe => { rmw!(|c: &mut Cpu, v: u8| c.nz(v.wrapping_add(1))); }
            0xc6 | 0xd6 | 0xce | 0xde => { rmw!(|c: &mut Cpu, v: u8| c.nz(v.wrapping_sub(1))); }
            0xe8 => self.x = self.nz(self.x.wrapping_add(1)),
            0xc8 => self.y = self.nz(self.y.wrapping_add(1)),
            0xca => self.x = self.nz(self.x.wrapping_sub(1)),
            0x88 => self.y = self.nz(self.y.wrapping_sub(1)),
            // ── flow ──
            0x10 => cycles += self.branch(b, self.p & N == 0),
            0x30 => cycles += self.branch(b, self.p & N != 0),
            0x50 => cycles += self.branch(b, self.p & V == 0),
            0x70 => cycles += self.branch(b, self.p & V != 0),
            0x90 => cycles += self.branch(b, self.p & C == 0),
            0xb0 => cycles += self.branch(b, self.p & C != 0),
            0xd0 => cycles += self.branch(b, self.p & Z == 0),
            0xf0 => cycles += self.branch(b, self.p & Z != 0),
            0x4c => self.pc = self.fetch16(b),
            0x6c => { let a = self.fetch16(b); self.pc = Self::read16_wrap(b, a); }
            0x20 => {
                let to = self.fetch16(b);
                let ret = self.pc.wrapping_sub(1);
                self.push(b, (ret >> 8) as u8);
                self.push(b, ret as u8);
                self.pc = to;
            }
            0x60 => { let lo = self.pull(b) as u16; let hi = self.pull(b) as u16; self.pc = (lo | hi << 8).wrapping_add(1); }
            0x40 => {
                self.p = (self.pull(b) & !B) | U;
                let lo = self.pull(b) as u16;
                let hi = self.pull(b) as u16;
                self.pc = lo | hi << 8;
            }
            0x00 => {
                self.pc = self.pc.wrapping_add(1);
                self.push(b, (self.pc >> 8) as u8);
                self.push(b, self.pc as u8);
                self.push(b, self.p | B | U);
                self.p |= I;
                self.pc = b.read(0xfffe) as u16 | (b.read(0xffff) as u16) << 8;
            }
            // ── stack and flags ──
            0x48 => self.push(b, self.a),
            0x08 => self.push(b, self.p | B | U),
            0x68 => { let v = self.pull(b); self.a = self.nz(v); }
            0x28 => self.p = (self.pull(b) & !B) | U,
            0x18 => self.p &= !C,
            0x38 => self.p |= C,
            0x58 => self.p &= !I,
            0x78 => self.p |= I,
            0xb8 => self.p &= !V,
            0xd8 => self.p &= !D,
            0xf8 => self.p |= D,
            // ── undocumented ──
            0xa7 | 0xb7 | 0xaf | 0xbf | 0xa3 | 0xb3 => { let v = rd!(); self.a = self.nz(v); self.x = v; } // LAX
            0xab => { let v = rd!(); self.a = self.nz((self.a | 0xee) & v); self.x = self.a; } // LXA
            0x87 | 0x97 | 0x8f | 0x83 => { let a = self.addr(b, mode(op)); b.write(a, self.a & self.x); } // SAX
            0xc7 | 0xd7 | 0xcf | 0xdf | 0xdb | 0xc3 | 0xd3 => { let r = rmw!(|_: &mut Cpu, v: u8| v.wrapping_sub(1)); self.cmp(self.a, r); } // DCP
            0xe7 | 0xf7 | 0xef | 0xff | 0xfb | 0xe3 | 0xf3 => { let r = rmw!(|_: &mut Cpu, v: u8| v.wrapping_add(1)); self.sbc(r); } // ISC
            0x07 | 0x17 | 0x0f | 0x1f | 0x1b | 0x03 | 0x13 => { let r = rmw!(asl); self.a = self.nz(self.a | r); } // SLO
            0x27 | 0x37 | 0x2f | 0x3f | 0x3b | 0x23 | 0x33 => { let r = rmw!(rol); self.a = self.nz(self.a & r); } // RLA
            0x47 | 0x57 | 0x4f | 0x5f | 0x5b | 0x43 | 0x53 => { let r = rmw!(lsr); self.a = self.nz(self.a ^ r); } // SRE
            0x67 | 0x77 | 0x6f | 0x7f | 0x7b | 0x63 | 0x73 => { let r = rmw!(ror); self.adc(r); } // RRA
            0x0b | 0x2b => { let v = rd!(); self.a = self.nz(self.a & v); self.flag(C, self.a & 0x80 != 0); } // ANC
            0x4b => { let v = rd!(); let t = self.a & v; self.a = lsr(self, t); } // ALR
            0x6b => { // ARR
                let v = rd!();
                let t = self.a & v;
                self.a = self.nz((t >> 1) | ((self.p & C) << 7));
                self.flag(C, self.a & 0x40 != 0);
                self.flag(V, ((self.a >> 6) ^ (self.a >> 5)) & 1 != 0);
            }
            0xcb => { let v = rd!(); let t = (self.a & self.x) as i16 - v as i16; self.flag(C, t >= 0); self.x = self.nz(t as u8); } // SBX
            0x8b => { let v = rd!(); self.a = self.nz(self.x & v); } // ANE (approximate)
            0xbb => { let v = rd!() & self.s; self.a = self.nz(v); self.x = v; self.s = v; } // LAS
            0x9b | 0x9c | 0x9e | 0x9f | 0x93 => { let a = self.addr(b, mode(op)); b.write(a, self.a & self.x & ((a >> 8) as u8).wrapping_add(1)); } // SHA/SHX/SHY family (approximate)
            0x1a | 0x3a | 0x5a | 0x7a | 0xda | 0xfa | 0xea => {} // NOP
            0x80 | 0x82 | 0x89 | 0xc2 | 0xe2 | 0x04 | 0x44 | 0x64 | 0x14 | 0x34 | 0x54 | 0x74 | 0xd4 | 0xf4 | 0x0c | 0x1c | 0x3c
            | 0x5c | 0x7c | 0xdc | 0xfc => { let _ = rd!(); } // NOPs with operands
            _ => { // JAM
                self.jammed = true;
                self.pc = self.pc.wrapping_sub(1);
            }
        }
        cycles as u32 + self.extra as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Ram(Vec<u8>);
    impl Bus for Ram {
        fn read(&mut self, a: u16) -> u8 {
            self.0[a as usize]
        }
        fn write(&mut self, a: u16, v: u8) {
            self.0[a as usize] = v;
        }
    }

    fn run(code: &[u8]) -> (Cpu, Ram) {
        let mut ram = Ram(vec![0; 0x10000]);
        ram.0[0x1000..0x1000 + code.len()].copy_from_slice(code);
        let mut cpu = Cpu::new();
        cpu.pc = 0x1000;
        while cpu.pc < 0x1000 + code.len() as u16 && !cpu.jammed {
            cpu.step(&mut ram);
        }
        (cpu, ram)
    }

    #[test]
    fn arithmetic_and_flags() {
        let (c, _) = run(&[0x18, 0xa9, 0x50, 0x69, 0x50]); // CLC; LDA #$50; ADC #$50
        assert_eq!(c.a, 0xa0);
        assert!(c.p & V != 0 && c.p & N != 0 && c.p & C == 0);
        let (c, _) = run(&[0xf8, 0x18, 0xa9, 0x19, 0x69, 0x28]); // SED; CLC; LDA #$19; ADC #$28 (BCD)
        assert_eq!(c.a, 0x47);
    }

    #[test]
    fn undocumented_opcodes() {
        // LDA #$0F; STA $20; LAX $20; DCP $20 (-> $0E, compare)
        let (c, r) = run(&[0xa9, 0x0f, 0x85, 0x20, 0xa7, 0x20, 0xc7, 0x20]);
        assert_eq!((c.a, c.x, r.0[0x20]), (0x0f, 0x0f, 0x0e));
        assert!(c.p & C != 0);
        // LDA #$F0; LDX #$3C; SAX $21
        let (_, r) = run(&[0xa9, 0xf0, 0xa2, 0x3c, 0x87, 0x21]);
        assert_eq!(r.0[0x21], 0x30);
    }

    #[test]
    fn subroutines_and_cycles() {
        let mut ram = Ram(vec![0; 0x10000]);
        ram.0[0x1000..0x1003].copy_from_slice(&[0x20, 0x00, 0x20]); // JSR $2000
        ram.0[0x2000] = 0x60; // RTS
        let mut cpu = Cpu::new();
        cpu.pc = 0x1000;
        assert_eq!(cpu.step(&mut ram) + cpu.step(&mut ram), 12);
        assert_eq!(cpu.pc, 0x1003);
    }
}
