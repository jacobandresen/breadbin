// sid - pure-Rust SID music playback.
//
// A SID tune is a tiny C64 program: a header (PSID/RSID) wrapping 6502 machine
// code with an `init` and a `play` routine. `init` selects a sub-song; `play` is
// called once per video frame (50Hz PAL) and pokes the SID sound chip's 29
// registers. To play one we therefore need three things, all in this file:
//
//   1. a PSID/RSID header parser            (SidHeader / parse_header)
//   2. a 6502 CPU + 64K RAM with the SID    (Cpu) mapped at $D400-$D7FF
//   3. the real chip emulation              (resid-rs) turning register pokes
//                                            into audio samples
//
// `Player` ties them together: it runs `play` every frame, clocks reSID for a
// frame's worth of cycles, and hands back PCM. After each frame it also snapshots
// the SID register state (`Vis`) so a visualiser can draw the three voices in
// lock-step with what is being heard.
//
// Scope: PSID tunes with an explicit play address, or with the play routine
// installed in the IRQ vector ($0314) by `init` - which covers the large majority
// of the High Voltage SID Collection. Multi-speed/CIA-timed exotica is played at
// a flat 50Hz, which is correct for almost everything and close enough otherwise.

use resid::{ChipModel, SamplingMethod, Sid};

/// PAL system clock; one video frame is this many cycles at 50.04Hz.
const PAL_CLOCK: u32 = 985_248;
/// Cycles between successive `play` calls (one PAL video frame).
const FRAME_CYCLES: u32 = PAL_CLOCK / 50;
/// Number of SID registers we shadow for the visualiser ($D400..$D41C + a bit).
pub const NUM_REGS: usize = 0x20;

// ---- PSID / RSID header ----------------------------------------------------

/// The parsed header of a .sid file plus the raw C64 program image.
pub struct SidFile {
    pub name: String,
    pub author: String,
    pub songs: u16,
    pub start_song: u16,
    init_addr: u16,
    play_addr: u16,
    load_addr: u16,
    is_8580: bool,
    /// C64 memory image bytes and the address they load at.
    body: Vec<u8>,
}

fn be16(b: &[u8], o: usize) -> u16 {
    ((b[o] as u16) << 8) | b[o + 1] as u16
}

fn cstr(b: &[u8], o: usize, n: usize) -> String {
    let end = (o..o + n).find(|&i| b[i] == 0).unwrap_or(o + n);
    String::from_utf8_lossy(&b[o..end]).trim().to_string()
}

/// Parse a PSID/RSID v1-v4 header. Returns the metadata and the C64 image.
pub fn parse(data: &[u8]) -> Result<SidFile, String> {
    if data.len() < 0x7c {
        return Err("file too small to be a SID".into());
    }
    let magic = &data[0..4];
    if magic != b"PSID" && magic != b"RSID" {
        return Err("not a PSID/RSID file".into());
    }
    let version = be16(data, 0x04);
    let data_offset = be16(data, 0x06) as usize;
    let mut load_addr = be16(data, 0x08);
    let init_addr = be16(data, 0x0a);
    let play_addr = be16(data, 0x0c);
    let songs = be16(data, 0x0e).max(1);
    let start_song = be16(data, 0x10).max(1);
    let name = cstr(data, 0x16, 32);
    let author = cstr(data, 0x36, 32);
    // v2+ has a flags word at 0x76; bit 4-5 select the SID model.
    let is_8580 = version >= 2 && data.len() >= 0x78 && (be16(data, 0x76) & 0x30) == 0x20;

    if data_offset > data.len() {
        return Err("bad SID data offset".into());
    }
    let mut body = data[data_offset..].to_vec();
    // A zero load address means the real one is the first two bytes of the body
    // (little-endian), C64 PRG style.
    if load_addr == 0 {
        if body.len() < 2 {
            return Err("empty SID body".into());
        }
        load_addr = (body[0] as u16) | ((body[1] as u16) << 8);
        body.drain(0..2);
    }
    Ok(SidFile {
        name,
        author,
        songs,
        start_song,
        init_addr,
        play_addr,
        load_addr,
        is_8580,
        body,
    })
}

// ---- a live snapshot of the chip for the visualiser ------------------------

/// SID register shadow captured after each `play`, plus a slice of recent audio
/// for an oscilloscope. Cheap to clone; the player publishes one per frame.
#[derive(Clone)]
pub struct Vis {
    pub regs: [u8; NUM_REGS],
    pub frame: u64,
}

impl Vis {
    /// Voice `v` (0..3) frequency register as a 0.0..1.0 fraction of full scale.
    pub fn voice_freq(&self, v: usize) -> f32 {
        let base = v * 7;
        let f = (self.regs[base] as u16) | ((self.regs[base + 1] as u16) << 8);
        f as f32 / 65535.0
    }
    /// Voice `v` gate bit (note on).
    pub fn voice_gate(&self, v: usize) -> bool {
        self.regs[v * 7 + 4] & 0x01 != 0
    }
    /// Voice `v` waveform nibble (triangle/saw/pulse/noise bits).
    pub fn voice_wave(&self, v: usize) -> u8 {
        self.regs[v * 7 + 4] >> 4
    }
    /// Voice `v` sustain level (0..15), a decent stand-in for current loudness.
    pub fn voice_sustain(&self, v: usize) -> u8 {
        self.regs[v * 7 + 6] >> 4
    }
    /// Master volume (0..15).
    pub fn volume(&self) -> u8 {
        self.regs[0x18] & 0x0f
    }
}

// ---- 6502 CPU + memory + SID -----------------------------------------------

struct Cpu {
    a: u8,
    x: u8,
    y: u8,
    sp: u8,
    pc: u16,
    /// status: N V - B D I Z C (bit5 kept set)
    p: u8,
    ram: Box<[u8; 0x10000]>,
    sid: Sid,
    regs: [u8; NUM_REGS],
    /// set when `step` hits an unemulated opcode, to stop the current routine.
    halt: bool,
}

const C: u8 = 1 << 0;
const Z: u8 = 1 << 1;
const I: u8 = 1 << 2;
const D: u8 = 1 << 3;
const V: u8 = 1 << 6;
const N: u8 = 1 << 7;

impl Cpu {
    fn new(sid: Sid) -> Self {
        Cpu {
            a: 0,
            x: 0,
            y: 0,
            sp: 0xff,
            pc: 0,
            p: 0x24,
            ram: Box::new([0u8; 0x10000]),
            sid,
            regs: [0u8; NUM_REGS],
            halt: false,
        }
    }

    fn rd(&mut self, a: u16) -> u8 {
        if (0xd400..=0xd7ff).contains(&a) {
            return self.sid.read((a & 0x1f) as u8);
        }
        self.ram[a as usize]
    }
    fn wr(&mut self, a: u16, v: u8) {
        if (0xd400..=0xd7ff).contains(&a) {
            let reg = (a & 0x1f) as u8;
            self.sid.write(reg, v);
            self.regs[reg as usize] = v;
            return;
        }
        self.ram[a as usize] = v;
    }
    fn rd16(&mut self, a: u16) -> u16 {
        (self.rd(a) as u16) | ((self.rd(a.wrapping_add(1)) as u16) << 8)
    }

    fn set_zn(&mut self, v: u8) {
        self.p = (self.p & !(Z | N)) | if v == 0 { Z } else { 0 } | (v & 0x80);
    }
    fn flag(&self, f: u8) -> bool {
        self.p & f != 0
    }
    fn setf(&mut self, f: u8, on: bool) {
        if on {
            self.p |= f
        } else {
            self.p &= !f
        }
    }

    fn push(&mut self, v: u8) {
        self.ram[0x100 + self.sp as usize] = v;
        self.sp = self.sp.wrapping_sub(1);
    }
    fn pop(&mut self) -> u8 {
        self.sp = self.sp.wrapping_add(1);
        self.ram[0x100 + self.sp as usize]
    }
    fn push16(&mut self, v: u16) {
        self.push((v >> 8) as u8);
        self.push(v as u8);
    }
    fn pop16(&mut self) -> u16 {
        let lo = self.pop() as u16;
        let hi = self.pop() as u16;
        lo | (hi << 8)
    }

    fn fetch(&mut self) -> u8 {
        let v = self.rd(self.pc);
        self.pc = self.pc.wrapping_add(1);
        v
    }
    fn fetch16(&mut self) -> u16 {
        let lo = self.fetch() as u16;
        let hi = self.fetch() as u16;
        lo | (hi << 8)
    }

    // ---- addressing modes: each returns the effective operand address -------
    fn am_imm(&mut self) -> u16 {
        let a = self.pc;
        self.pc = self.pc.wrapping_add(1);
        a
    }
    fn am_zp(&mut self) -> u16 {
        self.fetch() as u16
    }
    fn am_zpx(&mut self) -> u16 {
        (self.fetch().wrapping_add(self.x)) as u16
    }
    fn am_zpy(&mut self) -> u16 {
        (self.fetch().wrapping_add(self.y)) as u16
    }
    fn am_abs(&mut self) -> u16 {
        self.fetch16()
    }
    fn am_absx(&mut self) -> u16 {
        self.fetch16().wrapping_add(self.x as u16)
    }
    fn am_absy(&mut self) -> u16 {
        self.fetch16().wrapping_add(self.y as u16)
    }
    fn am_izx(&mut self) -> u16 {
        let z = self.fetch().wrapping_add(self.x);
        (self.rd(z as u16) as u16) | ((self.rd(z.wrapping_add(1) as u16) as u16) << 8)
    }
    fn am_izy(&mut self) -> u16 {
        let z = self.fetch();
        let base = (self.rd(z as u16) as u16) | ((self.rd(z.wrapping_add(1) as u16) as u16) << 8);
        base.wrapping_add(self.y as u16)
    }

    fn branch(&mut self, cond: bool) {
        let off = self.fetch() as i8;
        if cond {
            self.pc = self.pc.wrapping_add(off as u16);
        }
    }

    fn cmp_with(&mut self, reg: u8, addr: u16) {
        let m = self.rd(addr);
        let r = reg.wrapping_sub(m);
        self.setf(C, reg >= m);
        self.set_zn(r);
    }

    fn adc(&mut self, addr: u16) {
        let m = self.rd(addr) as u16;
        let a = self.a as u16;
        let carry = (self.p & C) as u16;
        if self.flag(D) {
            // BCD add
            let mut lo = (a & 0x0f) + (m & 0x0f) + carry;
            let mut hi = (a >> 4) + (m >> 4);
            if lo > 9 {
                lo += 6;
                hi += 1;
            }
            let vflag = !(a ^ m) & (a ^ (hi << 4)) & 0x80 != 0;
            if hi > 9 {
                hi += 6;
            }
            self.setf(C, hi > 15);
            let res = ((hi << 4) | (lo & 0x0f)) as u8;
            self.setf(V, vflag);
            self.set_zn(res);
            self.a = res;
        } else {
            let sum = a + m + carry;
            let res = sum as u8;
            self.setf(C, sum > 0xff);
            self.setf(V, !(a ^ m) & (a ^ sum) & 0x80 != 0);
            self.set_zn(res);
            self.a = res;
        }
    }
    fn sbc(&mut self, addr: u16) {
        let m = self.rd(addr) as u16;
        let a = self.a as u16;
        let carry = (self.p & C) as u16;
        if self.flag(D) {
            let mut lo = (a & 0x0f).wrapping_sub(m & 0x0f).wrapping_sub(1 - carry);
            let mut hi = (a >> 4).wrapping_sub(m >> 4);
            if lo & 0x10 != 0 {
                lo = lo.wrapping_sub(6);
                hi = hi.wrapping_sub(1);
            }
            if hi & 0x10 != 0 {
                hi = hi.wrapping_sub(6);
            }
            let diff = a.wrapping_sub(m).wrapping_sub(1 - carry);
            self.setf(C, diff < 0x100);
            self.setf(V, (a ^ m) & (a ^ diff) & 0x80 != 0);
            let res = ((hi << 4) | (lo & 0x0f)) as u8;
            self.set_zn(res);
            self.a = res;
        } else {
            let diff = a.wrapping_sub(m).wrapping_sub(1 - carry);
            let res = diff as u8;
            self.setf(C, diff < 0x100);
            self.setf(V, (a ^ m) & (a ^ diff) & 0x80 != 0);
            self.set_zn(res);
            self.a = res;
        }
    }

    fn asl(&mut self, addr: u16) {
        let m = self.rd(addr);
        self.setf(C, m & 0x80 != 0);
        let r = m << 1;
        self.wr(addr, r);
        self.set_zn(r);
    }
    fn lsr(&mut self, addr: u16) {
        let m = self.rd(addr);
        self.setf(C, m & 1 != 0);
        let r = m >> 1;
        self.wr(addr, r);
        self.set_zn(r);
    }
    fn rol(&mut self, addr: u16) {
        let m = self.rd(addr);
        let c = self.p & C;
        self.setf(C, m & 0x80 != 0);
        let r = (m << 1) | c;
        self.wr(addr, r);
        self.set_zn(r);
    }
    fn ror(&mut self, addr: u16) {
        let m = self.rd(addr);
        let c = (self.p & C) << 7;
        self.setf(C, m & 1 != 0);
        let r = (m >> 1) | c;
        self.wr(addr, r);
        self.set_zn(r);
    }

    /// Execute one instruction. Returns false on an opcode we don't emulate
    /// (treated as a stop), true otherwise. Cycle timing is not tracked: the
    /// player clocks the SID a whole frame at a time, which is what real-world
    /// simple PSID players do and what tunes are written against.
    fn step(&mut self) {
        let op = self.fetch();
        match op {
            // ADC
            0x69 => { let a = self.am_imm(); self.adc(a); }
            0x65 => { let a = self.am_zp(); self.adc(a); }
            0x75 => { let a = self.am_zpx(); self.adc(a); }
            0x6d => { let a = self.am_abs(); self.adc(a); }
            0x7d => { let a = self.am_absx(); self.adc(a); }
            0x79 => { let a = self.am_absy(); self.adc(a); }
            0x61 => { let a = self.am_izx(); self.adc(a); }
            0x71 => { let a = self.am_izy(); self.adc(a); }
            // SBC
            0xe9 => { let a = self.am_imm(); self.sbc(a); }
            0xe5 => { let a = self.am_zp(); self.sbc(a); }
            0xf5 => { let a = self.am_zpx(); self.sbc(a); }
            0xed => { let a = self.am_abs(); self.sbc(a); }
            0xfd => { let a = self.am_absx(); self.sbc(a); }
            0xf9 => { let a = self.am_absy(); self.sbc(a); }
            0xe1 => { let a = self.am_izx(); self.sbc(a); }
            0xf1 => { let a = self.am_izy(); self.sbc(a); }
            // AND
            0x29 => { let a = self.am_imm(); let m = self.rd(a); self.a &= m; self.set_zn(self.a); }
            0x25 => { let a = self.am_zp(); let m = self.rd(a); self.a &= m; self.set_zn(self.a); }
            0x35 => { let a = self.am_zpx(); let m = self.rd(a); self.a &= m; self.set_zn(self.a); }
            0x2d => { let a = self.am_abs(); let m = self.rd(a); self.a &= m; self.set_zn(self.a); }
            0x3d => { let a = self.am_absx(); let m = self.rd(a); self.a &= m; self.set_zn(self.a); }
            0x39 => { let a = self.am_absy(); let m = self.rd(a); self.a &= m; self.set_zn(self.a); }
            0x21 => { let a = self.am_izx(); let m = self.rd(a); self.a &= m; self.set_zn(self.a); }
            0x31 => { let a = self.am_izy(); let m = self.rd(a); self.a &= m; self.set_zn(self.a); }
            // ORA
            0x09 => { let a = self.am_imm(); let m = self.rd(a); self.a |= m; self.set_zn(self.a); }
            0x05 => { let a = self.am_zp(); let m = self.rd(a); self.a |= m; self.set_zn(self.a); }
            0x15 => { let a = self.am_zpx(); let m = self.rd(a); self.a |= m; self.set_zn(self.a); }
            0x0d => { let a = self.am_abs(); let m = self.rd(a); self.a |= m; self.set_zn(self.a); }
            0x1d => { let a = self.am_absx(); let m = self.rd(a); self.a |= m; self.set_zn(self.a); }
            0x19 => { let a = self.am_absy(); let m = self.rd(a); self.a |= m; self.set_zn(self.a); }
            0x01 => { let a = self.am_izx(); let m = self.rd(a); self.a |= m; self.set_zn(self.a); }
            0x11 => { let a = self.am_izy(); let m = self.rd(a); self.a |= m; self.set_zn(self.a); }
            // EOR
            0x49 => { let a = self.am_imm(); let m = self.rd(a); self.a ^= m; self.set_zn(self.a); }
            0x45 => { let a = self.am_zp(); let m = self.rd(a); self.a ^= m; self.set_zn(self.a); }
            0x55 => { let a = self.am_zpx(); let m = self.rd(a); self.a ^= m; self.set_zn(self.a); }
            0x4d => { let a = self.am_abs(); let m = self.rd(a); self.a ^= m; self.set_zn(self.a); }
            0x5d => { let a = self.am_absx(); let m = self.rd(a); self.a ^= m; self.set_zn(self.a); }
            0x59 => { let a = self.am_absy(); let m = self.rd(a); self.a ^= m; self.set_zn(self.a); }
            0x41 => { let a = self.am_izx(); let m = self.rd(a); self.a ^= m; self.set_zn(self.a); }
            0x51 => { let a = self.am_izy(); let m = self.rd(a); self.a ^= m; self.set_zn(self.a); }
            // ASL
            0x0a => { self.setf(C, self.a & 0x80 != 0); self.a <<= 1; self.set_zn(self.a); }
            0x06 => { let a = self.am_zp(); self.asl(a); }
            0x16 => { let a = self.am_zpx(); self.asl(a); }
            0x0e => { let a = self.am_abs(); self.asl(a); }
            0x1e => { let a = self.am_absx(); self.asl(a); }
            // LSR
            0x4a => { self.setf(C, self.a & 1 != 0); self.a >>= 1; self.set_zn(self.a); }
            0x46 => { let a = self.am_zp(); self.lsr(a); }
            0x56 => { let a = self.am_zpx(); self.lsr(a); }
            0x4e => { let a = self.am_abs(); self.lsr(a); }
            0x5e => { let a = self.am_absx(); self.lsr(a); }
            // ROL
            0x2a => { let c = self.p & C; self.setf(C, self.a & 0x80 != 0); self.a = (self.a << 1) | c; self.set_zn(self.a); }
            0x26 => { let a = self.am_zp(); self.rol(a); }
            0x36 => { let a = self.am_zpx(); self.rol(a); }
            0x2e => { let a = self.am_abs(); self.rol(a); }
            0x3e => { let a = self.am_absx(); self.rol(a); }
            // ROR
            0x6a => { let c = (self.p & C) << 7; self.setf(C, self.a & 1 != 0); self.a = (self.a >> 1) | c; self.set_zn(self.a); }
            0x66 => { let a = self.am_zp(); self.ror(a); }
            0x76 => { let a = self.am_zpx(); self.ror(a); }
            0x6e => { let a = self.am_abs(); self.ror(a); }
            0x7e => { let a = self.am_absx(); self.ror(a); }
            // BIT
            0x24 => { let a = self.am_zp(); let m = self.rd(a); self.setf(Z, self.a & m == 0); self.setf(N, m & 0x80 != 0); self.setf(V, m & 0x40 != 0); }
            0x2c => { let a = self.am_abs(); let m = self.rd(a); self.setf(Z, self.a & m == 0); self.setf(N, m & 0x80 != 0); self.setf(V, m & 0x40 != 0); }
            // branches
            0x10 => { let c = !self.flag(N); self.branch(c); }
            0x30 => { let c = self.flag(N); self.branch(c); }
            0x50 => { let c = !self.flag(V); self.branch(c); }
            0x70 => { let c = self.flag(V); self.branch(c); }
            0x90 => { let c = !self.flag(C); self.branch(c); }
            0xb0 => { let c = self.flag(C); self.branch(c); }
            0xd0 => { let c = !self.flag(Z); self.branch(c); }
            0xf0 => { let c = self.flag(Z); self.branch(c); }
            // CMP/CPX/CPY
            0xc9 => { let a = self.am_imm(); self.cmp_with(self.a, a); }
            0xc5 => { let a = self.am_zp(); self.cmp_with(self.a, a); }
            0xd5 => { let a = self.am_zpx(); self.cmp_with(self.a, a); }
            0xcd => { let a = self.am_abs(); self.cmp_with(self.a, a); }
            0xdd => { let a = self.am_absx(); self.cmp_with(self.a, a); }
            0xd9 => { let a = self.am_absy(); self.cmp_with(self.a, a); }
            0xc1 => { let a = self.am_izx(); self.cmp_with(self.a, a); }
            0xd1 => { let a = self.am_izy(); self.cmp_with(self.a, a); }
            0xe0 => { let a = self.am_imm(); self.cmp_with(self.x, a); }
            0xe4 => { let a = self.am_zp(); self.cmp_with(self.x, a); }
            0xec => { let a = self.am_abs(); self.cmp_with(self.x, a); }
            0xc0 => { let a = self.am_imm(); self.cmp_with(self.y, a); }
            0xc4 => { let a = self.am_zp(); self.cmp_with(self.y, a); }
            0xcc => { let a = self.am_abs(); self.cmp_with(self.y, a); }
            // DEC/INC
            0xc6 => { let a = self.am_zp(); let r = self.rd(a).wrapping_sub(1); self.wr(a, r); self.set_zn(r); }
            0xd6 => { let a = self.am_zpx(); let r = self.rd(a).wrapping_sub(1); self.wr(a, r); self.set_zn(r); }
            0xce => { let a = self.am_abs(); let r = self.rd(a).wrapping_sub(1); self.wr(a, r); self.set_zn(r); }
            0xde => { let a = self.am_absx(); let r = self.rd(a).wrapping_sub(1); self.wr(a, r); self.set_zn(r); }
            0xe6 => { let a = self.am_zp(); let r = self.rd(a).wrapping_add(1); self.wr(a, r); self.set_zn(r); }
            0xf6 => { let a = self.am_zpx(); let r = self.rd(a).wrapping_add(1); self.wr(a, r); self.set_zn(r); }
            0xee => { let a = self.am_abs(); let r = self.rd(a).wrapping_add(1); self.wr(a, r); self.set_zn(r); }
            0xfe => { let a = self.am_absx(); let r = self.rd(a).wrapping_add(1); self.wr(a, r); self.set_zn(r); }
            // register inc/dec
            0xca => { self.x = self.x.wrapping_sub(1); self.set_zn(self.x); }
            0xe8 => { self.x = self.x.wrapping_add(1); self.set_zn(self.x); }
            0x88 => { self.y = self.y.wrapping_sub(1); self.set_zn(self.y); }
            0xc8 => { self.y = self.y.wrapping_add(1); self.set_zn(self.y); }
            // JMP
            0x4c => { self.pc = self.fetch16(); }
            0x6c => {
                let ptr = self.fetch16();
                // 6502 indirect-jump page-wrap bug
                let lo = self.rd(ptr) as u16;
                let hi = self.rd((ptr & 0xff00) | ((ptr + 1) & 0x00ff)) as u16;
                self.pc = lo | (hi << 8);
            }
            // JSR / RTS / RTI
            0x20 => { let target = self.fetch16(); self.push16(self.pc.wrapping_sub(1)); self.pc = target; }
            0x60 => { self.pc = self.pop16().wrapping_add(1); }
            0x40 => { self.p = (self.pop() & !0x10) | 0x20; self.pc = self.pop16(); }
            // LDA
            0xa9 => { let a = self.am_imm(); self.a = self.rd(a); self.set_zn(self.a); }
            0xa5 => { let a = self.am_zp(); self.a = self.rd(a); self.set_zn(self.a); }
            0xb5 => { let a = self.am_zpx(); self.a = self.rd(a); self.set_zn(self.a); }
            0xad => { let a = self.am_abs(); self.a = self.rd(a); self.set_zn(self.a); }
            0xbd => { let a = self.am_absx(); self.a = self.rd(a); self.set_zn(self.a); }
            0xb9 => { let a = self.am_absy(); self.a = self.rd(a); self.set_zn(self.a); }
            0xa1 => { let a = self.am_izx(); self.a = self.rd(a); self.set_zn(self.a); }
            0xb1 => { let a = self.am_izy(); self.a = self.rd(a); self.set_zn(self.a); }
            // LDX
            0xa2 => { let a = self.am_imm(); self.x = self.rd(a); self.set_zn(self.x); }
            0xa6 => { let a = self.am_zp(); self.x = self.rd(a); self.set_zn(self.x); }
            0xb6 => { let a = self.am_zpy(); self.x = self.rd(a); self.set_zn(self.x); }
            0xae => { let a = self.am_abs(); self.x = self.rd(a); self.set_zn(self.x); }
            0xbe => { let a = self.am_absy(); self.x = self.rd(a); self.set_zn(self.x); }
            // LDY
            0xa0 => { let a = self.am_imm(); self.y = self.rd(a); self.set_zn(self.y); }
            0xa4 => { let a = self.am_zp(); self.y = self.rd(a); self.set_zn(self.y); }
            0xb4 => { let a = self.am_zpx(); self.y = self.rd(a); self.set_zn(self.y); }
            0xac => { let a = self.am_abs(); self.y = self.rd(a); self.set_zn(self.y); }
            0xbc => { let a = self.am_absx(); self.y = self.rd(a); self.set_zn(self.y); }
            // STA
            0x85 => { let a = self.am_zp(); self.wr(a, self.a); }
            0x95 => { let a = self.am_zpx(); self.wr(a, self.a); }
            0x8d => { let a = self.am_abs(); self.wr(a, self.a); }
            0x9d => { let a = self.am_absx(); self.wr(a, self.a); }
            0x99 => { let a = self.am_absy(); self.wr(a, self.a); }
            0x81 => { let a = self.am_izx(); self.wr(a, self.a); }
            0x91 => { let a = self.am_izy(); self.wr(a, self.a); }
            // STX / STY
            0x86 => { let a = self.am_zp(); self.wr(a, self.x); }
            0x96 => { let a = self.am_zpy(); self.wr(a, self.x); }
            0x8e => { let a = self.am_abs(); self.wr(a, self.x); }
            0x84 => { let a = self.am_zp(); self.wr(a, self.y); }
            0x94 => { let a = self.am_zpx(); self.wr(a, self.y); }
            0x8c => { let a = self.am_abs(); self.wr(a, self.y); }
            // transfers
            0xaa => { self.x = self.a; self.set_zn(self.x); }
            0xa8 => { self.y = self.a; self.set_zn(self.y); }
            0x8a => { self.a = self.x; self.set_zn(self.a); }
            0x98 => { self.a = self.y; self.set_zn(self.a); }
            0xba => { self.x = self.sp; self.set_zn(self.x); }
            0x9a => { self.sp = self.x; }
            // stack
            0x48 => { self.push(self.a); }
            0x68 => { self.a = self.pop(); self.set_zn(self.a); }
            0x08 => { self.push(self.p | 0x30); }
            0x28 => { self.p = (self.pop() & !0x10) | 0x20; }
            // flags
            0x18 => self.setf(C, false),
            0x38 => self.setf(C, true),
            0x58 => self.setf(I, false),
            0x78 => self.setf(I, true),
            0xb8 => self.setf(V, false),
            0xd8 => self.setf(D, false),
            0xf8 => self.setf(D, true),
            // NOP (official + common undocumented single-byte)
            0xea | 0x1a | 0x3a | 0x5a | 0x7a | 0xda | 0xfa => {}
            // anything else: stop this routine rather than run wild.
            _ => {
                self.pc = self.pc.wrapping_sub(1);
                self.halt = true;
            }
        }
    }

    /// Call a 6502 subroutine and run until it returns to our (empty) frame,
    /// guarded against runaway code. We push no return address, so the routine's
    /// final RTS is recognised by the stack pointer returning to where it began.
    fn call(&mut self, addr: u16, a: u8) {
        self.a = a;
        self.pc = addr;
        self.halt = false;
        let entry_sp = self.sp;
        let mut guard: u32 = 0;
        loop {
            // The top-level routine's own RTS (stack back to where it started)
            // means it is returning to its phantom caller: stop.
            if self.rd(self.pc) == 0x60 && self.sp >= entry_sp {
                break;
            }
            self.step();
            if self.halt {
                break;
            }
            guard += 1;
            if guard > 2_000_000 {
                break;
            }
        }
    }
}

// ---- the player ------------------------------------------------------------

/// Plays one sub-song of a SID file, producing PCM at `sample_rate` and a live
/// register snapshot for the visualiser.
pub struct Player {
    cpu: Cpu,
    play_addr: u16,
    frame_cycles_left: u32,
    frame: u64,
    pub name: String,
    pub author: String,
    pub songs: u16,
}

impl Player {
    /// Build a player for `song` (1-based) of the given .sid bytes, generating
    /// audio at `sample_rate` Hz (mono).
    pub fn new(bytes: &[u8], song: u16, sample_rate: u32) -> Result<Player, String> {
        let f = parse(bytes)?;
        let model = if f.is_8580 {
            ChipModel::Mos8580
        } else {
            ChipModel::Mos6581
        };
        let mut sid = Sid::new(model);
        sid.set_sampling_parameters(SamplingMethod::Interpolate, PAL_CLOCK, sample_rate);
        let mut cpu = Cpu::new(sid);

        // Load the C64 image into RAM at its load address.
        let load = f.load_addr as usize;
        for (i, b) in f.body.iter().enumerate() {
            let a = load + i;
            if a < 0x10000 {
                cpu.ram[a] = *b;
            }
        }
        // A few zero-page/KERNAL bytes player code commonly relies on.
        cpu.ram[0x01] = 0x37; // default 6510 port: BASIC+KERNAL+IO banked in

        // 0 means "the file's default song"; otherwise clamp to a valid index.
        let song = if song == 0 { f.start_song } else { song }.clamp(1, f.songs);
        // PSID convention: the accumulator holds the (0-based) sub-song on init.
        cpu.call(f.init_addr, (song - 1) as u8);

        // No explicit play address -> the init routine installed the player in
        // the IRQ vector ($0314/$0315). Use that.
        let play_addr = if f.play_addr != 0 {
            f.play_addr
        } else {
            cpu.rd16(0x0314)
        };

        Ok(Player {
            cpu,
            play_addr,
            frame_cycles_left: 0,
            frame: 0,
            name: f.name,
            author: f.author,
            songs: f.songs,
        })
    }

    /// Fill `out` (mono i16) with the next samples, running `play` at each 50Hz
    /// frame boundary. Returns the latest visual snapshot.
    pub fn render(&mut self, out: &mut [i16]) -> Vis {
        let mut idx = 0;
        while idx < out.len() {
            if self.frame_cycles_left == 0 {
                if self.play_addr != 0 {
                    self.cpu.call(self.play_addr, self.cpu.a);
                }
                self.frame = self.frame.wrapping_add(1);
                self.frame_cycles_left = FRAME_CYCLES;
            }
            let (n, delta_left) = self
                .cpu
                .sid
                .sample(self.frame_cycles_left, &mut out[idx..], 1);
            idx += n;
            self.frame_cycles_left = delta_left;
            // If reSID consumed the whole frame without filling the buffer, the
            // loop iterates, triggers the next `play`, and continues.
            if n == 0 && delta_left == 0 {
                // Defensive: avoid an infinite loop on a degenerate frame.
                self.frame_cycles_left = 0;
                break;
            }
        }
        self.snapshot()
    }

    pub fn snapshot(&self) -> Vis {
        Vis {
            regs: self.cpu.regs,
            frame: self.frame,
        }
    }
}
