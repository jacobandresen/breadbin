//! SID music playback: a small C64 around the reSID chip emulator.
//!
//! A .sid file is a C64 program with an `init` and (for PSID) a `play`
//! routine. PSID tunes get the classic player treatment: `init` once, then
//! `play` every video frame, or at the CIA timer rate when the tune asks for
//! it. RSID tunes are real programs that set up their own interrupts, so the
//! machine around the CPU is real enough for them: CIA timers, the VIC raster
//! interrupt, memory banking and the few KERNAL entry points interrupt code
//! jumps through. SID writes reach reSID at the cycle they happen, so sampled
//! digis play too.

use crate::cpu::{Bus, Cpu};
use resid::{ChipModel, Sid};

pub use resid::SamplingMethod;

/// PAL system clock and video timing.
const PAL_CLOCK: u32 = 985_248;
const LINE_CYCLES: u32 = 63;
const FRAME_LINES: u32 = 312;
const FRAME_CYCLES: u32 = LINE_CYCLES * FRAME_LINES;
/// The KERNAL's default CIA 1 timer: ~60Hz.
const KERNAL_TIMER: u16 = 0x4025;
/// SID registers shadowed for the visualiser.
pub const NUM_REGS: usize = 0x20;
/// Return address that marks "the routine finished": the CPU idles at $0000.
const IDLE: u16 = 0x0000;

// ── header ──────────────────────────────────────────────────────────────────

/// A parsed .sid file and its C64 memory image.
pub struct SidFile {
    pub name: String,
    pub author: String,
    pub songs: u16,
    pub start_song: u16,
    rsid: bool,
    init_addr: u16,
    play_addr: u16,
    load_addr: u16,
    /// Bit n set: song n+1 is timed by CIA 1 instead of the video frame.
    speed: u32,
    /// The SID chips: base address and whether it's an 8580 (2SID and 3SID
    /// tunes have more than one).
    chips: Vec<(u16, bool)>,
    body: Vec<u8>,
}

fn be16(b: &[u8], o: usize) -> u16 {
    u16::from_be_bytes([b[o], b[o + 1]])
}

fn cstr(b: &[u8], o: usize, n: usize) -> String {
    let end = (o..o + n).find(|&i| b[i] == 0).unwrap_or(o + n);
    String::from_utf8_lossy(&b[o..end]).trim().to_string()
}

/// Parse a PSID/RSID (v1-v4) header.
pub fn parse(data: &[u8]) -> Result<SidFile, String> {
    if data.len() < 0x7c {
        return Err("file too small to be a SID".into());
    }
    let rsid = match &data[0..4] {
        b"PSID" => false,
        b"RSID" => true,
        _ => return Err("not a PSID/RSID file".into()),
    };
    let version = be16(data, 0x04);
    let offset = be16(data, 0x06) as usize;
    let mut body = data.get(offset..).ok_or("bad SID data offset")?.to_vec();
    let mut load_addr = be16(data, 0x08);
    // a zero load address means the body starts with one, PRG style
    if load_addr == 0 {
        if body.len() < 2 {
            return Err("empty SID body".into());
        }
        load_addr = u16::from_le_bytes([body[0], body[1]]);
        body.drain(0..2);
    }
    let mut init_addr = be16(data, 0x0a);
    if init_addr == 0 {
        init_addr = load_addr;
    }
    let flags = if version >= 2 { be16(data, 0x76) } else { 0 };
    let is_8580 = flags & 0x30 == 0x20;
    let mut chips = vec![(0xd400, is_8580)];
    // v3 adds a second SID ($Dxx0 as one byte), v4 a third; unknown models follow the first
    for (n, at) in [(3, 0x7a), (4, 0x7b)] {
        let b = data[at];
        if version >= n && b & 1 == 0 && ((0x42..=0x7e).contains(&b) || b >= 0xe0) {
            let model = (flags >> (2 * n)) & 3; // bits 6-7, then 8-9
            chips.push((0xd000 | (b as u16) << 4, if model == 0 { is_8580 } else { model == 2 }));
        }
    }
    Ok(SidFile {
        name: cstr(data, 0x16, 32),
        author: cstr(data, 0x36, 32),
        songs: be16(data, 0x0e).max(1),
        start_song: be16(data, 0x10).max(1),
        rsid,
        init_addr,
        play_addr: be16(data, 0x0c),
        load_addr,
        speed: u32::from_be_bytes([data[0x12], data[0x13], data[0x14], data[0x15]]),
        chips,
        body,
    })
}

// ── the machine ─────────────────────────────────────────────────────────────

#[derive(Default)]
struct Cia {
    counter: [u16; 2],
    latch: [u16; 2],
    control: [u8; 2],
    mask: u8,
    status: u8,
    /// Last values written, read back from the registers not modelled (the
    /// serial data register doubles as a place to keep an RTI, for one).
    written: [u8; 16],
    /// Cycle within the last `tick` at which the interrupt line went active.
    asserted_at: Option<u32>,
}

impl Cia {
    /// Timer `t` is started and counting clock cycles (timer B can count other things).
    fn counting(&self, t: usize) -> bool {
        self.control[t] & 1 != 0 && !(t == 1 && self.control[1] & 0x60 != 0)
    }

    fn tick(&mut self, cycles: u32) {
        let was = self.interrupting();
        self.asserted_at = None;
        for t in 0..2 {
            if !self.counting(t) {
                continue;
            }
            let mut left = cycles;
            while left > 0 {
                let step = left.min(self.counter[t] as u32 + 1);
                left -= step;
                if step > self.counter[t] as u32 {
                    self.status |= 1 << t;
                    if !was && self.interrupting() {
                        // the line goes active a cycle after the underflow
                        let at = cycles - left + 1;
                        self.asserted_at = Some(self.asserted_at.map_or(at, |a| a.min(at)));
                    }
                    self.counter[t] = self.latch[t];
                    if self.control[t] & 0x08 != 0 {
                        self.control[t] &= !1; // one-shot
                        break;
                    }
                } else {
                    self.counter[t] -= step as u16;
                }
            }
        }
    }

    fn interrupting(&self) -> bool {
        self.status & self.mask & 0x1f != 0
    }

    /// Timer `t` as it will read `ahead` cycles from now.
    fn counter_in(&self, t: usize, ahead: u32) -> u16 {
        let (c, latch) = (self.counter[t] as u32, self.latch[t] as u32);
        if !self.counting(t) {
            self.counter[t]
        } else if ahead <= c {
            (c - ahead) as u16
        } else {
            (latch - (ahead - c - 1) % (latch + 1)) as u16 // reloaded on the way
        }
    }

    /// `ahead`: cycles into the current instruction this read happens.
    fn read(&mut self, reg: u16, ahead: u32) -> u8 {
        match reg & 0x0f {
            0x00 | 0x01 => 0xff, // no keys pressed, no joystick
            0x04 => self.counter_in(0, ahead) as u8,
            0x05 => (self.counter_in(0, ahead) >> 8) as u8,
            0x06 => self.counter_in(1, ahead) as u8,
            0x07 => (self.counter_in(1, ahead) >> 8) as u8,
            0x0d => {
                let v = self.status | if self.interrupting() { 0x80 } else { 0 };
                self.status = 0;
                v
            }
            0x0e => self.control[0],
            0x0f => self.control[1],
            r => self.written[r as usize],
        }
    }

    fn write(&mut self, reg: u16, v: u8) {
        self.written[(reg & 0x0f) as usize] = v;
        let set_latch = |latch: &mut u16, hi: bool| *latch = if hi { (*latch & 0xff) | (v as u16) << 8 } else { (*latch & 0xff00) | v as u16 };
        match reg & 0x0f {
            r @ 0x04..=0x07 => {
                let (t, hi) = ((r as usize - 4) / 2, r & 1 != 0);
                set_latch(&mut self.latch[t], hi);
                if hi && self.control[t] & 1 == 0 {
                    self.counter[t] = self.latch[t]; // a stopped timer loads its latch
                }
            }
            0x0d => {
                if v & 0x80 != 0 { self.mask |= v & 0x1f } else { self.mask &= !(v & 0x1f) }
            }
            r @ (0x0e | 0x0f) => {
                let t = (r - 0x0e) as usize;
                if v & 0x10 != 0 {
                    self.counter[t] = self.latch[t]; // force load
                }
                self.control[t] = v & !0x10;
            }
            _ => {}
        }
    }
}

struct C64 {
    ram: Box<[u8; 0x10000]>,
    /// $E000-$FFFF when the KERNAL is banked in: RTS everywhere, plus the
    /// interrupt entry and exit points.
    kernal: Box<[u8; 0x2000]>,
    sids: Vec<Sid>,
    /// Where each SID sits (the first also answers in its mirrors up to $D7FF).
    bases: Vec<u16>,
    regs: Vec<[u8; NUM_REGS]>,
    /// SID writes (chip, register, value) made by the current instruction,
    /// applied once reSID has been clocked up to it.
    writes: Vec<(u8, u8, u8)>,
    cia: [Cia; 2],
    raster: u32, // cycle within the frame
    raster_compare: u16,
    vic_irq: u8,
    vic_mask: u8,
    vic_ctrl: u8,
    nmi_was: bool,
    nmi_pending: bool,
    /// The pending NMI arrived too late in the last instruction to be taken after it.
    nmi_late: bool,
    /// Bus accesses so far in the current instruction: about the cycle it's in,
    /// which cycle-exact code reading the timers depends on.
    access: u32,
}

impl C64 {
    fn new(sids: Vec<Sid>, bases: Vec<u16>) -> C64 {
        let mut kernal = Box::new([0x60u8; 0x2000]); // RTS
        let mut put = |addr: u16, bytes: &[u8]| {
            let o = (addr - 0xe000) as usize;
            kernal[o..o + bytes.len()].copy_from_slice(bytes);
        };
        // IRQ entry: save registers, dispatch through ($0314), or ($0316) for BRK
        put(0xff48, &[0x48, 0x8a, 0x48, 0x98, 0x48, 0xba, 0xbd, 0x04, 0x01, 0x29, 0x10, 0xf0, 0x03, 0x6c, 0x16, 0x03, 0x6c, 0x14, 0x03]);
        // $EA31 (acknowledge CIA 1, then return) and $EA81 (return)
        put(0xea31, &[0xad, 0x0d, 0xdc, 0x68, 0xa8, 0x68, 0xaa, 0x68, 0x40]);
        put(0xea81, &[0x68, 0xa8, 0x68, 0xaa, 0x68, 0x40]);
        // NMI: through ($0318), whose default just returns
        put(0xfe43, &[0x6c, 0x18, 0x03]);
        put(0xfe47, &[0x40]);
        put(0xfffa, &[0x43, 0xfe, 0xe2, 0xfc, 0x48, 0xff]);

        let mut ram = Box::new([0u8; 0x10000]);
        ram[0x01] = 0x37;
        // KERNAL vector defaults
        ram[0x314..0x31a].copy_from_slice(&[0x31, 0xea, 0x66, 0xfe, 0x47, 0xfe]);
        let mut cia: [Cia; 2] = Default::default();
        cia[0].latch[0] = KERNAL_TIMER;
        cia[0].counter[0] = KERNAL_TIMER;
        cia[0].control[0] = 0x01;
        cia[0].mask = 0x01;
        let regs = vec![[0; NUM_REGS]; sids.len()];
        C64 { ram, kernal, sids, bases, regs, writes: Vec::new(), cia, raster: 0, raster_compare: 0, vic_irq: 0, vic_mask: 0, vic_ctrl: 0x1b, nmi_was: false, nmi_pending: false, nmi_late: false, access: 0 }
    }

    /// The SID answering at I/O address `a`.
    fn chip_at(&self, a: u16) -> Option<usize> {
        let extra = self.bases.iter().skip(1).position(|&b| a & !0x1f == b).map(|i| i + 1);
        extra.or((0xd400..=0xd7ff).contains(&a).then_some(0))
    }

    fn port(&self) -> u8 {
        self.ram[1] & 7
    }
    fn io_visible(&self) -> bool {
        self.port() & 4 != 0 && self.port() & 3 != 0
    }

    fn line(&self) -> u16 {
        (self.raster / LINE_CYCLES) as u16
    }

    /// Advance timers and the raster beam; returns the IRQ line (an NMI edge
    /// is latched in `nmi_pending`).
    fn tick(&mut self, cycles: u32) -> bool {
        self.cia[0].tick(cycles);
        self.cia[1].tick(cycles);
        let before = self.line();
        self.raster = (self.raster + cycles) % FRAME_CYCLES;
        let now = self.line();
        let crossed = if now >= before { (before + 1..=now).contains(&self.raster_compare) } else { self.raster_compare > before || self.raster_compare <= now };
        if crossed {
            self.vic_irq |= 1;
        }
        let nmi = self.cia[1].interrupting();
        if nmi && !self.nmi_was {
            self.nmi_pending = true;
            // the 6502 samples interrupts before an instruction's last cycle
            self.nmi_late = self.cia[1].asserted_at.is_some_and(|at| at >= cycles);
        }
        self.nmi_was = nmi;
        self.cia[0].interrupting() || self.vic_irq & self.vic_mask & 0x0f != 0
    }
}

impl Bus for C64 {
    fn read(&mut self, a: u16) -> u8 {
        self.access += 1;
        match a {
            0xd000..=0xdfff if self.io_visible() => match (a, self.chip_at(a)) {
                (_, Some(c)) => {
                    let r = (a & 0x1f) as u8;
                    if matches!(r, 0x1b | 0x1c) { self.sids[c].read(r) } else { self.regs[c][r as usize] }
                }
                (0xd000..=0xd3ff, _) => match a & 0x3f {
                    0x11 => (self.vic_ctrl & 0x7f) | ((self.line() >> 1) as u8 & 0x80),
                    0x12 => self.line() as u8,
                    0x19 => self.vic_irq | if self.vic_irq & self.vic_mask & 0x0f != 0 { 0xf0 } else { 0x70 },
                    0x1a => self.vic_mask | 0xf0,
                    _ => 0,
                },
                (0xdc00..=0xdcff, _) => self.cia[0].read(a, self.access - 1),
                (0xdd00..=0xddff, _) => self.cia[1].read(a, self.access - 1),
                _ => 0,
            },
            0xe000..=0xffff if self.port() & 2 != 0 => self.kernal[(a - 0xe000) as usize],
            _ => self.ram[a as usize],
        }
    }

    fn write(&mut self, a: u16, v: u8) {
        self.access += 1;
        if (0xd000..=0xdfff).contains(&a) && self.io_visible() {
            match (a, self.chip_at(a)) {
                (_, Some(c)) => {
                    let r = (a & 0x1f) as u8;
                    self.regs[c][r as usize] = v;
                    self.writes.push((c as u8, r, v));
                }
                (0xd000..=0xd3ff, _) => match a & 0x3f {
                    0x11 => {
                        self.vic_ctrl = v;
                        self.raster_compare = (self.raster_compare & 0xff) | ((v as u16 & 0x80) << 1);
                    }
                    0x12 => self.raster_compare = (self.raster_compare & 0x100) | v as u16,
                    0x19 => self.vic_irq &= !(v & 0x0f),
                    0x1a => self.vic_mask = v & 0x0f,
                    _ => {}
                },
                (0xdc00..=0xdcff, _) => self.cia[0].write(a, v),
                (0xdd00..=0xddff, _) => self.cia[1].write(a, v),
                _ => {}
            }
        } else {
            self.ram[a as usize] = v; // writes under ROM always reach RAM
        }
    }
}

// ── the player ──────────────────────────────────────────────────────────────

/// SID register snapshot for the visualiser.
#[derive(Clone)]
pub struct Vis {
    pub regs: [u8; NUM_REGS],
    pub frame: u64,
}

pub struct Player {
    cpu: Cpu,
    c64: C64,
    rsid: bool,
    play_addr: u16,
    /// Cycles between `play` calls (PSID).
    period: u32,
    until_play: i64,
    /// Cycles clocked by the CPU that reSID hasn't produced audio for yet.
    pending: u32,
    /// Per-chip samples waiting to be mixed.
    scratch: Vec<Vec<i16>>,
    frame: u64,
    pub name: String,
    pub author: String,
    pub songs: u16,
}

/// The bank PSID players expect for code at `addr` (as sidplay does).
fn bank_for(addr: u16) -> u8 {
    match addr {
        0..=0x9fff => 0x37,
        0xa000..=0xcfff => 0x36,
        0xd000..=0xdfff => 0x34,
        _ => 0x35,
    }
}

impl Player {
    /// Play `song` (1-based, 0 = the tune's default) at `sample_rate` Hz, mono.
    pub fn new(bytes: &[u8], song: u16, sample_rate: u32) -> Result<Player, String> {
        Player::with_sampling(bytes, song, sample_rate, SamplingMethod::Interpolate)
    }

    /// Like [`Player::new`] with an explicit reSID sampling method. `Resample`
    /// band-limits properly (no aliasing on bright waveforms) at a higher CPU cost.
    pub fn with_sampling(bytes: &[u8], song: u16, sample_rate: u32, method: SamplingMethod) -> Result<Player, String> {
        let f = parse(bytes)?;
        let sids = f
            .chips
            .iter()
            .map(|&(_, is_8580)| {
                let mut sid = Sid::new(if is_8580 { ChipModel::Mos8580 } else { ChipModel::Mos6581 });
                sid.set_sampling_parameters(method, PAL_CLOCK, sample_rate);
                sid
            })
            .collect::<Vec<_>>();
        let scratch = vec![Vec::new(); sids.len()];
        let mut c64 = C64::new(sids, f.chips.iter().map(|c| c.0).collect());
        for (i, b) in f.body.iter().enumerate() {
            if let Some(cell) = c64.ram.get_mut(f.load_addr as usize + i) {
                *cell = *b;
            }
        }
        let song = if song == 0 { f.start_song } else { song }.clamp(1, f.songs);
        let cia_timed = f.speed & (1 << ((song - 1).min(31))) != 0;

        let mut cpu = Cpu::new();
        if !f.rsid {
            c64.ram[1] = bank_for(f.init_addr);
        }
        call(&mut cpu, &mut c64, f.init_addr, (song - 1) as u8);
        // PSIDs run init (and play) with interrupts off, unless they have no
        // play routine and rely on the interrupt they install; RSIDs run free.
        if f.rsid || f.play_addr == 0 {
            cpu.p &= !0x04;
        }
        Ok(Player {
            cpu,
            c64,
            rsid: f.rsid,
            play_addr: f.play_addr,
            period: if cia_timed { 0 } else { FRAME_CYCLES },
            until_play: 0,
            pending: 0,
            scratch,
            frame: 0,
            name: f.name,
            author: f.author,
            songs: f.songs,
        })
    }

    /// Advance the machine by one instruction (or an idle stretch); returns cycles.
    fn step(&mut self) -> u32 {
        let (cpu, c64) = (&mut self.cpu, &mut self.c64);
        let idle = cpu.pc == IDLE || cpu.jammed;
        c64.access = 0;
        let cycles = if idle { 8 } else { cpu.step(c64) };

        // PSID: call play when it's time and the previous call has finished
        if !self.rsid && self.play_addr != 0 {
            self.until_play -= cycles as i64;
            if self.until_play <= 0 && idle {
                let period = if self.period > 0 { self.period } else { c64.cia[0].latch[0].max(1) as u32 };
                self.until_play += period as i64;
                c64.ram[1] = bank_for(self.play_addr);
                call(cpu, c64, self.play_addr, cpu.a);
                self.frame += 1;
            }
        }

        let irq = c64.tick(cycles);
        // the interrupt sequence takes cycles too, which the timers must see
        let taken = if c64.nmi_pending && !std::mem::take(&mut c64.nmi_late) {
            c64.nmi_pending = false;
            cpu.interrupt(c64, 0xfffa)
        } else if irq && cpu.irq_enabled() {
            if self.rsid || self.play_addr == 0 {
                self.frame += 1;
            }
            cpu.interrupt(c64, 0xfffe)
        } else {
            0
        };
        c64.tick(taken);
        cycles + taken
    }

    /// Fill `out` (interleaved stereo i16) with the next samples.
    pub fn render(&mut self, out: &mut [i16]) -> Vis {
        let frames = out.len() / 2;
        let mut idx = 0;
        while idx < frames {
            if self.pending > 0 {
                let want = frames - idx;
                // every chip is clocked alike, so each yields the same number of samples
                let mut made = (0, 0);
                for (sid, buf) in self.c64.sids.iter_mut().zip(&mut self.scratch) {
                    if buf.len() < want {
                        buf.resize(want, 0);
                    }
                    made = sid.sample(self.pending, &mut buf[..want], 1);
                }
                let (n, left) = made;
                self.mix(&mut out[idx * 2..(idx + n) * 2]);
                idx += n;
                self.pending = left;
                if left > 0 {
                    break; // buffer full; the rest is produced next time
                }
            } else if !self.c64.writes.is_empty() {
                for (c, r, v) in std::mem::take(&mut self.c64.writes) {
                    self.c64.sids[c as usize].write(r, v);
                }
            } else {
                self.pending = self.step();
            }
        }
        self.snapshot()
    }

    /// Mix the chips into stereo frames: one SID in the middle, two apart, a
    /// third between them.
    fn mix(&self, out: &mut [i16]) {
        const PAN: [&[(f32, f32)]; 3] = [&[(1.0, 1.0)], &[(0.75, 0.3), (0.3, 0.75)], &[(0.6, 0.2), (0.2, 0.6), (0.4, 0.4)]];
        let pan = PAN[self.scratch.len().min(3) - 1];
        for (f, frame) in out.chunks_exact_mut(2).enumerate() {
            let (mut l, mut r) = (0.0, 0.0);
            for (buf, (pl, pr)) in self.scratch.iter().zip(pan) {
                l += buf[f] as f32 * pl;
                r += buf[f] as f32 * pr;
            }
            frame[0] = l.clamp(-32768.0, 32767.0) as i16;
            frame[1] = r.clamp(-32768.0, 32767.0) as i16;
        }
    }

    pub fn snapshot(&self) -> Vis {
        // the first chip in use (some 2SID tunes leave the first one silent)
        let regs = self.c64.regs.iter().find(|r| r.iter().any(|&v| v != 0)).unwrap_or(&self.c64.regs[0]);
        Vis { regs: *regs, frame: self.frame }
    }
}

/// Set the CPU up to run `addr` as a subroutine returning to the idle sentinel.
fn call(cpu: &mut Cpu, c64: &mut C64, addr: u16, a: u8) {
    let ret = IDLE.wrapping_sub(1);
    cpu.push(c64, (ret >> 8) as u8);
    cpu.push(c64, ret as u8);
    cpu.pc = addr;
    cpu.a = a;
    cpu.jammed = false;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A PSID whose play routine writes an incrementing value to $D418.
    fn psid(rsid: bool) -> Vec<u8> {
        let mut f = vec![0u8; 0x7c];
        f[0..4].copy_from_slice(if rsid { b"RSID" } else { b"PSID" });
        f[5] = 2; // version
        f[7] = 0x7c; // data offset
        f[0x0a..0x0c].copy_from_slice(&0x1000u16.to_be_bytes()); // init
        f[0x0c..0x0e].copy_from_slice(&if rsid { 0 } else { 0x1003u16 }.to_be_bytes()); // play
        f[0x0f] = 1;
        f[0x11] = 1;
        f.extend([0x00, 0x10]); // load address $1000 in the body
        if rsid {
            // init: set $0314 -> $1010, CLI, loop forever; IRQ: INC $D418, JMP $EA31
            f.extend([0xa9, 0x10, 0x8d, 0x14, 0x03, 0xa9, 0x10, 0x8d, 0x15, 0x03, 0x58, 0x4c, 0x0b, 0x10, 0, 0]);
            f.extend([0xee, 0x18, 0xd4, 0x4c, 0x31, 0xea]);
        } else {
            f.extend([0x60, 0, 0, 0xee, 0x18, 0xd4, 0x60]); // init: RTS; play: INC $D418; RTS
        }
        f
    }

    #[test]
    fn psid_play_runs_every_frame() {
        let mut p = Player::new(&psid(false), 0, 44100).unwrap();
        let mut buf = vec![0i16; 2 * 44100];
        let vis = p.render(&mut buf);
        assert!((48..=51).contains(&vis.frame), "frames: {}", vis.frame);
        assert_eq!(vis.regs[0x18] as u64, vis.frame & 0xff);
    }

    #[test]
    fn rsid_interrupts_drive_the_tune() {
        let mut p = Player::new(&psid(true), 0, 44100).unwrap();
        let mut buf = vec![0i16; 2 * 44100];
        let vis = p.render(&mut buf);
        // the KERNAL's 60Hz CIA timer fires the tune's IRQ handler
        assert!((58..=62).contains(&vis.frame), "irqs: {}", vis.frame);
        assert!(vis.regs[0x18] > 50);
    }

    #[test]
    fn a_second_sid_gets_its_own_writes() {
        let mut f = psid(false);
        f[5] = 3; // v3 header
        f[0x7a] = 0x42; // second SID at $D420
        let body = f.len() - 7;
        f[body + 4..body + 6].copy_from_slice(&[0x38, 0xd4]); // play: INC $D438
        let mut p = Player::new(&f, 0, 44100).unwrap();
        p.render(&mut vec![0i16; 2 * 4410]);
        assert!(p.c64.regs[1][0x18] > 0);
        assert_eq!(p.c64.regs[0][0x18], 0);
    }

    /// What cycle-exact digi players rely on: code run from the timer and
    /// serial registers.
    #[test]
    fn cia_registers_behave_like_the_chip() {
        let mut cia = Cia::default();
        cia.write(0x04, 0x4c);
        cia.write(0x05, 0x02); // stopped: the high byte loads the counter
        assert_eq!((cia.read(0x04, 0), cia.read(0x05, 0)), (0x4c, 0x02));
        cia.write(0x0c, 0x40); // RTI kept in the serial data register
        assert_eq!(cia.read(0x0c, 0), 0x40);
        cia.write(0x06, 9);
        cia.write(0x0f, 0x11); // timer B: load and start, 10-cycle period
        assert_eq!(cia.read(0x06, 3), 6); // read three cycles into an instruction
        assert_eq!(cia.read(0x06, 12), 7); // after a reload
        cia.write(0x0d, 0x82);
        cia.tick(12);
        assert_eq!(cia.asserted_at, Some(11)); // underflow on cycle 10, line active on 11
    }
}

