//! machine: a small C64 that runs the Studio's programs on its own, without
//! the emulator or any Commodore ROM. It is what the Studio's exports are
//! made of: the same machine in a web page (WebAssembly) and in a Linux or
//! Windows program (the `breadbin-player` crate).
//!
//! It is as much C64 as a program built in the Studio needs: the 6502
//! (cpu.rs), the memory banking of the CPU port, the VIC-II drawn a raster
//! line at a time (text, multicolour text, extended colour, bitmap and
//! multicolour bitmap modes, sprites with their priorities and collisions,
//! the raster interrupt, bad lines stealing the CPU's cycles), both CIAs with
//! the joysticks and the keyboard matrix, and the SID (reSID). The KERNAL is
//! a stub: the interrupt entry and exit points the runtime goes through, the
//! screen output and keyboard input that `printf`, cc65's conio and BASIC-style
//! programs use (CHROUT, PLOT, CLRSCR, GETIN: done in Rust when the CPU gets
//! there, `trap`), and RTS everywhere else. A program starts as BASIC's RUN
//! would start it: at the address its `SYS` line names.

use crate::cpu::{Bus, Cpu};
use crate::sid::{Cia, KERNAL_TIMER, LINE_CYCLES, PAL_CLOCK};
use resid::{ChipModel, SamplingMethod, Sid};

/// The picture: 320x200 and a border around it, as a TV shows a PAL C64.
pub const WIDTH: usize = 384;
pub const HEIGHT: usize = 272;
/// The first raster line in the picture.
const FIRST_LINE: i32 = 16;
/// The sprite X coordinate of the picture's first column (the text starts at 24).
const FIRST_X: i32 = 24 - 32;
const LINES: u16 = 312;
/// Return address that marks "the program ended": the CPU idles at $0000.
const IDLE: u16 = 0x0000;

/// The C64's colours (Pepto's PAL palette), 0x00RRGGBB.
pub const PALETTE: [u32; 16] = [
    0x000000, 0xffffff, 0x68372b, 0x70a4b2, 0x6f3d86, 0x588d43, 0x352879, 0xb8c76f, 0x6f4f25, 0x433900, 0x9a6759, 0x444444, 0x6c6c6c, 0x9ad284,
    0x6c5eb5, 0x959595,
];

/// Joystick bits, as `set_joystick` takes them.
pub const JOY_UP: u8 = 1;
pub const JOY_DOWN: u8 = 2;
pub const JOY_LEFT: u8 = 4;
pub const JOY_RIGHT: u8 = 8;
pub const JOY_FIRE: u8 = 16;

struct Io {
    ram: Box<[u8; 0x10000]>,
    color: [u8; 1024],
    kernal: Box<[u8; 0x2000]>,
    /// The character ROM the VIC sees in banks 0 and 2 (the Studio's own font).
    charrom: Box<[u8; 0x1000]>,
    vic: [u8; 0x40],
    line: u16,
    compare: u16,
    irq_flags: u8,
    cia: [Cia; 2],
    nmi_was: bool,
    sid: Sid,
    sid_regs: [u8; 0x20],
    writes: Vec<(u8, u8)>,
    /// Pressed joystick directions (JOY_*), ports 1 and 2.
    joy: [u8; 2],
    /// The keyboard matrix: for each column (CIA 1 port A bit), the rows held down.
    keys: [u8; 8],
    access: u32,
}

impl Io {
    fn port(&self) -> u8 {
        (self.ram[1] | !self.ram[0]) & 7
    }

    /// CIA 1 port A as the chips drive it: the columns the program selects, and joystick 2.
    fn cia1_a(&self) -> u8 {
        let w = &self.cia[0].written;
        (w[0] | !w[2]) & !self.joy[1]
    }

    fn cia1_b(&self) -> u8 {
        let w = &self.cia[0].written;
        let cols = self.cia1_a();
        let mut rows = 0xff;
        for c in 0..8 {
            if cols & (1 << c) == 0 {
                rows &= !self.keys[c];
            }
        }
        (rows & (w[1] | !w[3])) & !self.joy[0]
    }

    /// The VIC's view: 16 KB of RAM in the bank CIA 2 picks, the character ROM in banks 0 and 2.
    fn vread(&self, a: u16) -> u8 {
        let w = &self.cia[1].written;
        let bank = 3 - ((w[0] | !w[2]) & 3) as u16;
        let a = a & 0x3fff;
        if bank & 1 == 0 && a & 0x3000 == 0x1000 {
            self.charrom[(a & 0xfff) as usize]
        } else {
            self.ram[(bank << 14 | a) as usize]
        }
    }

    fn irq(&self) -> bool {
        self.cia[0].interrupting() || self.irq_flags & self.vic[0x1a] & 0x0f != 0
    }
}

impl Bus for Io {
    fn read(&mut self, a: u16) -> u8 {
        self.access += 1;
        let port = self.port();
        match a {
            0xd000..=0xdfff if port & 3 != 0 && port & 4 != 0 => match a {
                0xd000..=0xd3ff => {
                    let r = (a & 0x3f) as usize;
                    match r {
                        0x11 => (self.vic[0x11] & 0x7f) | ((self.line >> 1) as u8 & 0x80),
                        0x12 => self.line as u8,
                        0x16 => self.vic[r] | 0xc0,
                        0x18 => self.vic[r] | 0x01,
                        0x19 => self.irq_flags | 0x70 | if self.irq_flags & self.vic[0x1a] & 0x0f != 0 { 0x80 } else { 0 },
                        0x1a => self.vic[r] | 0xf0,
                        0x1e | 0x1f => std::mem::take(&mut self.vic[r]),
                        0x20..=0x2e => self.vic[r] | 0xf0,
                        0x2f..=0x3f => 0xff,
                        _ => self.vic[r],
                    }
                }
                0xd400..=0xd7ff => match (a & 0x1f) as u8 {
                    r @ (0x1b | 0x1c) => self.sid.read(r),
                    0x19 | 0x1a => 0xff,
                    _ => 0,
                },
                0xd800..=0xdbff => self.color[(a - 0xd800) as usize] | 0xf0,
                0xdc00..=0xdcff => match a & 0x0f {
                    0 => self.cia1_a(),
                    1 => self.cia1_b(),
                    _ => self.cia[0].read(a, self.access - 1),
                },
                0xdd00..=0xddff => match a & 0x0f {
                    0 => {
                        let w = &self.cia[1].written;
                        w[0] | !w[2]
                    }
                    _ => self.cia[1].read(a, self.access - 1),
                },
                _ => 0xff,
            },
            0xd000..=0xdfff if port & 3 != 0 => self.charrom[(a & 0xfff) as usize],
            0xe000..=0xffff if port & 2 != 0 => self.kernal[(a - 0xe000) as usize],
            _ => self.ram[a as usize],
        }
    }

    fn write(&mut self, a: u16, v: u8) {
        self.access += 1;
        let port = self.port();
        if (0xd000..=0xdfff).contains(&a) && port & 3 != 0 && port & 4 != 0 {
            match a {
                0xd000..=0xd3ff => {
                    let r = (a & 0x3f) as usize;
                    match r {
                        0x11 => {
                            self.vic[r] = v;
                            self.compare = (self.compare & 0xff) | ((v as u16 & 0x80) << 1);
                        }
                        0x12 => self.compare = (self.compare & 0x100) | v as u16,
                        0x19 => self.irq_flags &= !(v & 0x0f),
                        0x1a => self.vic[r] = v & 0x0f,
                        0x1e | 0x1f => {}
                        _ => self.vic[r] = v,
                    }
                }
                0xd400..=0xd7ff => {
                    let r = (a & 0x1f) as u8;
                    self.sid_regs[r as usize] = v;
                    self.writes.push((r, v));
                }
                0xd800..=0xdbff => self.color[(a - 0xd800) as usize] = v & 0x0f,
                0xdc00..=0xdcff => self.cia[0].write(a, v),
                0xdd00..=0xddff => self.cia[1].write(a, v),
                _ => {}
            }
        } else {
            self.ram[a as usize] = v; // writes under ROM reach the RAM
        }
    }
}

/// The stub KERNAL: RTS everywhere, the interrupt entry and exit points, the vectors.
fn stub_kernal() -> Box<[u8; 0x2000]> {
    let mut k = Box::new([0x60u8; 0x2000]); // RTS
    let mut put = |addr: u16, bytes: &[u8]| {
        let o = (addr - 0xe000) as usize;
        k[o..o + bytes.len()].copy_from_slice(bytes);
    };
    // IRQ entry: save A, X, Y; dispatch through ($0314), or ($0316) for BRK
    put(0xff48, &[0x48, 0x8a, 0x48, 0x98, 0x48, 0xba, 0xbd, 0x04, 0x01, 0x29, 0x10, 0xf0, 0x03, 0x6c, 0x16, 0x03, 0x6c, 0x14, 0x03]);
    // $EA31, the KERNAL's own interrupt (acknowledge CIA 1, return) and $EA81 (return)
    put(0xea31, &[0xad, 0x0d, 0xdc, 0x68, 0xa8, 0x68, 0xaa, 0x68, 0x40]);
    put(0xea81, &[0x68, 0xa8, 0x68, 0xaa, 0x68, 0x40]);
    // BRK: the program stops (back to the idle address)
    put(0xfe66, &[0x4c, 0x00, 0x00]);
    // NMI: through ($0318), whose default returns
    put(0xfe43, &[0x6c, 0x18, 0x03]);
    put(0xfe47, &[0x40]);
    // GETIN, CHRIN: no key (A = 0, carry clear)
    put(0xffe4, &[0xa9, 0x00, 0x18, 0x60]);
    put(0xffcf, &[0xa9, 0x00, 0x18, 0x60]);
    put(0xfffa, &[0x43, 0xfe, 0xe2, 0xfc, 0x48, 0xff]);
    k
}

/// Where a PRG starts: the address in its BASIC `SYS` line, or where it loads.
fn start_address(prg: &[u8]) -> Option<u16> {
    if prg.len() < 3 {
        return None;
    }
    let load = u16::from_le_bytes([prg[0], prg[1]]);
    let body = &prg[2..];
    if load == 0x0801 && body.len() > 6 {
        // link, line number, then SYS (token $9E) and its digits
        let rest = &body[4..];
        if let Some(i) = rest.iter().take(8).position(|&b| b == 0x9e) {
            let digits: String = rest[i + 1..].iter().take_while(|&&b| b != 0).map(|&b| b as char).filter(|c| c.is_ascii_digit()).collect();
            if let Ok(n) = digits.parse::<u32>() {
                return u16::try_from(n).ok();
            }
        }
    }
    Some(load)
}

pub struct Machine {
    cpu: Cpu,
    io: Io,
    /// The picture, 0x00RRGGBB, WIDTH x HEIGHT.
    pub pixels: Vec<u32>,
    /// Sound made since the host last took it: mono, at the sample rate.
    pub audio: Vec<i16>,
    sid_pending: u32,
    /// Cycles the last line's last instruction ran into the next.
    debt: i32,
    pub frame: u64,
}

impl Machine {
    /// A C64 with `prg` loaded and started; `charrom` is the font the VIC
    /// shows in banks 0 and 2 (4 KB; shorter is padded).
    pub fn new(prg: &[u8], charrom: &[u8], sample_rate: u32) -> Result<Machine, String> {
        let start = start_address(prg).ok_or("not a program")?;
        let load = u16::from_le_bytes([prg[0], prg[1]]) as usize;
        let mut ram = Box::new([0u8; 0x10000]);
        for (i, b) in prg[2..].iter().enumerate() {
            if let Some(c) = ram.get_mut(load + i) {
                *c = *b;
            }
        }
        ram[0] = 0x2f;
        ram[1] = 0x37;
        // the KERNAL's vectors, and what it leaves: the screen at $0400, cleared
        ram[0x314..0x31a].copy_from_slice(&[0x31, 0xea, 0x66, 0xfe, 0x47, 0xfe]);
        ram[0x288] = 0x04; // the screen's page
        ram[0x286] = 14; // the text colour
        ram[0x2a6] = 1; // PAL
        ram[0xcc] = 1; // the cursor is off
        ram[0x400..0x7e8].fill(0x20);
        let mut cr = Box::new([0u8; 0x1000]);
        let n = charrom.len().min(0x1000);
        cr[..n].copy_from_slice(&charrom[..n]);
        let mut cia: [Cia; 2] = Default::default();
        // CIA 1's timer A runs the KERNAL's 60 Hz interrupt until a program stops it
        cia[0].latch[0] = KERNAL_TIMER;
        cia[0].counter[0] = KERNAL_TIMER;
        cia[0].control[0] = 0x01;
        cia[0].mask = 0x01;
        cia[0].written[0] = 0x7f;
        cia[0].written[2] = 0xff;
        // CIA 2: VIC bank 0
        cia[1].written[0] = 0x97;
        cia[1].written[2] = 0x3f;
        let mut vic = [0u8; 0x40];
        vic[0x11] = 0x1b;
        vic[0x16] = 0xc8;
        vic[0x18] = 0x15;
        vic[0x20] = 14;
        vic[0x21] = 6;
        let mut sid = Sid::new(ChipModel::Mos6581);
        sid.set_sampling_parameters(SamplingMethod::Interpolate, PAL_CLOCK, sample_rate);
        let io = Io {
            ram,
            color: [14; 1024],
            kernal: stub_kernal(),
            charrom: cr,
            vic,
            line: 0,
            compare: 0,
            irq_flags: 0,
            cia,
            nmi_was: false,
            sid,
            sid_regs: [0; 0x20],
            writes: Vec::new(),
            joy: [0; 2],
            keys: [0; 8],
            access: 0,
        };
        let mut m = Machine { cpu: Cpu::new(), io, pixels: vec![0; WIDTH * HEIGHT], audio: Vec::new(), sid_pending: 0, debt: 0, frame: 0 };
        // as BASIC's RUN does SYS: a subroutine, returning to the idle address
        m.cpu.s = 0xf6;
        let ret = IDLE.wrapping_sub(1);
        m.cpu.push(&mut m.io, (ret >> 8) as u8);
        m.cpu.push(&mut m.io, ret as u8);
        m.cpu.pc = start;
        m.cpu.p &= !0x04; // interrupts on, as under BASIC
        m.set_cursor(0, 0);
        Ok(m)
    }

    /// Has the program returned (or stopped at a BRK, or jammed the CPU)?
    pub fn ended(&self) -> bool {
        self.cpu.pc == IDLE || self.cpu.jammed
    }

    /// Joystick `port` (1 or 2): the JOY_* bits held.
    pub fn set_joystick(&mut self, port: usize, bits: u8) {
        if (1..=2).contains(&port) {
            self.io.joy[port - 1] = bits & 0x1f;
        }
    }

    /// A key of the C64's keyboard matrix (column 0-7, row 0-7) up or down.
    pub fn set_key(&mut self, col: usize, row: usize, down: bool) {
        if col < 8 && row < 8 {
            if down { self.io.keys[col] |= 1 << row } else { self.io.keys[col] &= !(1 << row) }
        }
    }

    /// A key typed (PETSCII), into the KERNAL's keyboard buffer: GETIN, cc65's cgetc() and kbhit() read it.
    pub fn type_key(&mut self, petscii: u8) {
        let n = self.io.ram[0xc6] as usize;
        if n < 10 {
            self.io.ram[0x277 + n] = petscii;
            self.io.ram[0xc6] = n as u8 + 1;
        }
    }

    /// The next key from the keyboard buffer (0 if there is none).
    fn next_key(&mut self) -> u8 {
        let ram = &mut self.io.ram;
        let n = ram[0xc6] as usize;
        if n == 0 {
            return 0;
        }
        let k = ram[0x277];
        ram.copy_within(0x278..0x277 + n, 0x277);
        ram[0xc6] = n as u8 - 1;
        k
    }

    // ── the KERNAL's screen and keyboard, in Rust ──────────────────────────

    /// The CPU is at a KERNAL routine done here: do it and return from it (RTS). False if `pc` is none of them.
    fn trap(&mut self) -> bool {
        let pc = self.cpu.pc;
        if pc < 0xe000 || self.io.port() & 2 == 0 {
            return false;
        }
        match pc {
            0xffd2 => self.chrout(self.cpu.a),
            0xe544 => self.clear_screen(),
            0xfff0 => {
                if self.cpu.p & 1 != 0 {
                    self.cpu.x = self.io.ram[0xd6];
                    self.cpu.y = self.io.ram[0xd3];
                } else {
                    self.set_cursor(self.cpu.x.min(24), self.cpu.y.min(39));
                }
            }
            0xeab2 => {
                let line = u16::from_le_bytes([self.io.ram[0xd1], self.io.ram[0xd2]]);
                let at = 0xd800 + line.wrapping_sub((self.io.ram[0x288] as u16) << 8);
                self.io.ram[0xf3..0xf5].copy_from_slice(&at.to_le_bytes());
            }
            // READST: the I/O status (0: fine); and the channel calls `printf` makes to reach the screen, which succeed
            0xffb7 => self.cpu.a = self.io.ram[0x90],
            0xffba | 0xffbd | 0xffc0 | 0xffc3 | 0xffc6 | 0xffc9 | 0xffcc | 0xffe7 => {}
            0xffe4 | 0xe5b4 | 0xffcf => {
                let k = self.next_key();
                self.cpu.a = if pc == 0xffcf && k == 0 { 13 } else { k };
                self.cpu.p = (self.cpu.p & !0x82) | if self.cpu.a == 0 { 0x02 } else { 0 } | (self.cpu.a & 0x80);
            }
            _ => return false,
        }
        self.cpu.p &= !1; // carry clear: no error
        // RTS
        let s = self.cpu.s;
        let lo = self.io.ram[0x100 + s.wrapping_add(1) as usize];
        let hi = self.io.ram[0x100 + s.wrapping_add(2) as usize];
        self.cpu.s = s.wrapping_add(2);
        self.cpu.pc = u16::from_le_bytes([lo, hi]).wrapping_add(1);
        true
    }

    fn screen_base(&self) -> u16 {
        (self.io.ram[0x288] as u16) << 8
    }

    /// The cursor to `row`, `col`, with the KERNAL's pointers to its line (cc65's conio writes through them).
    fn set_cursor(&mut self, row: u8, col: u8) {
        let ram = &mut self.io.ram;
        ram[0xd6] = row;
        ram[0xd3] = col;
        let line = ((ram[0x288] as u16) << 8) + row as u16 * 40;
        ram[0xd1..0xd3].copy_from_slice(&line.to_le_bytes());
        ram[0xf3..0xf5].copy_from_slice(&(0xd800 + row as u16 * 40).to_le_bytes());
    }

    fn clear_screen(&mut self) {
        let base = self.screen_base() as usize;
        self.io.ram[base..base + 1000].fill(0x20);
        let c = self.io.ram[0x286] & 15;
        self.io.color[..1000].fill(c);
        self.set_cursor(0, 0);
    }

    fn scroll(&mut self) {
        let base = self.screen_base() as usize;
        self.io.ram.copy_within(base + 40..base + 1000, base);
        self.io.ram[base + 960..base + 1000].fill(0x20);
        self.io.color.copy_within(40..1000, 0);
        let c = self.io.ram[0x286] & 15;
        self.io.color[960..1000].fill(c);
    }

    fn newline(&mut self) {
        let row = self.io.ram[0xd6];
        if row >= 24 {
            self.scroll();
            self.set_cursor(24, 0);
        } else {
            self.set_cursor(row + 1, 0);
        }
    }

    /// CHROUT: a PETSCII character to the screen, as the KERNAL's screen editor shows it.
    fn chrout(&mut self, c: u8) {
        const COLOURS: [(u8, u8); 16] = [
            (0x90, 0), (0x05, 1), (0x1c, 2), (0x9f, 3), (0x9c, 4), (0x1e, 5), (0x1f, 6), (0x9e, 7),
            (0x81, 8), (0x95, 9), (0x96, 10), (0x97, 11), (0x98, 12), (0x99, 13), (0x9a, 14), (0x9b, 15),
        ];
        let (row, col) = (self.io.ram[0xd6], self.io.ram[0xd3]);
        if let Some(&(_, colour)) = COLOURS.iter().find(|(k, _)| *k == c) {
            self.io.ram[0x286] = colour;
            return;
        }
        match c {
            0x0d | 0x8d => {
                self.io.ram[0xc7] = 0;
                self.newline();
            }
            0x0a => {}
            0x93 => self.clear_screen(),
            0x13 => self.set_cursor(0, 0),
            0x11 => {
                if row < 24 { self.set_cursor(row + 1, col) } else { self.scroll() }
            }
            0x91 => self.set_cursor(row.saturating_sub(1), col),
            0x1d => {
                if col < 39 { self.set_cursor(row, col + 1) } else { self.newline() }
            }
            0x9d => {
                if col > 0 { self.set_cursor(row, col - 1) } else if row > 0 { self.set_cursor(row - 1, 39) }
            }
            0x12 => self.io.ram[0xc7] = 0x80,
            0x92 => self.io.ram[0xc7] = 0,
            0x0e => self.io.vic[0x18] |= 2,
            0x8e => self.io.vic[0x18] &= !2,
            0x14 => {
                // delete: the character left of the cursor
                if col > 0 {
                    self.set_cursor(row, col - 1);
                    let at = self.screen_base() as usize + row as usize * 40 + col as usize - 1;
                    self.io.ram[at] = 0x20;
                }
            }
            _ => {
                let code = match c {
                    0x20..=0x3f => c,
                    0x40..=0x5f => c - 0x40,
                    0x60..=0x7f => c - 0x20,
                    0xa0..=0xbf => c - 0x40,
                    0xc0..=0xfe => c - 0x80,
                    0xff => 0x5e,
                    _ => return, // other control codes
                };
                let at = row as usize * 40 + col as usize;
                let base = self.screen_base() as usize;
                self.io.ram[base + at] = code | self.io.ram[0xc7];
                self.io.color[at] = self.io.ram[0x286] & 15;
                if col < 39 { self.set_cursor(row, col + 1) } else { self.newline() }
            }
        }
    }

    /// A byte of RAM.
    pub fn peek(&self, a: u16) -> u8 {
        self.io.ram[a as usize]
    }

    /// One instruction (or an idle stretch), with any interrupt it lets in; returns cycles.
    fn step(&mut self) -> u32 {
        let io = &mut self.io;
        io.access = 0;
        let mut cycles = if self.cpu.pc == IDLE || self.cpu.jammed {
            8
        } else if self.cpu.pc >= 0xe000 && self.trap() {
            30 // about what the KERNAL's routine would have taken, at least
        } else {
            self.cpu.step(&mut self.io)
        };
        let io = &mut self.io;
        io.cia[0].tick(cycles);
        io.cia[1].tick(cycles);
        let nmi = io.cia[1].interrupting();
        if nmi && !io.nmi_was {
            cycles += self.cpu.interrupt(io, 0xfffa);
        } else if io.irq() && self.cpu.irq_enabled() && self.cpu.pc != IDLE {
            cycles += self.cpu.interrupt(io, 0xfffe);
        }
        io.nmi_was = nmi;
        self.sid_pending += cycles;
        if !io.writes.is_empty() {
            self.flush_sid();
        }
        cycles
    }

    /// Clock the SID up to now, then give it the writes the CPU made.
    fn flush_sid(&mut self) {
        let mut buf = [0i16; 256];
        while self.sid_pending > 0 {
            let (n, left) = self.io.sid.sample(self.sid_pending, &mut buf, 1);
            self.audio.extend_from_slice(&buf[..n]);
            if left == self.sid_pending && n == 0 {
                break;
            }
            self.sid_pending = left;
        }
        for (r, v) in std::mem::take(&mut self.io.writes) {
            self.io.sid.write(r, v);
        }
    }

    /// Cycles the VIC takes from the CPU on `line`: bad lines and sprites.
    fn stolen(&self, line: u16) -> i32 {
        let v = &self.io.vic;
        let den = v[0x11] & 0x10 != 0;
        let bad = den && (0x30..=0xf7).contains(&line) && (line & 7) as u8 == v[0x11] & 7;
        let mut n = if bad { 40 } else { 0 };
        for s in 0..8 {
            if v[0x15] & (1 << s) != 0 {
                let y = v[1 + 2 * s] as i32;
                let h = if v[0x17] & (1 << s) != 0 { 42 } else { 21 };
                let d = line as i32 - y - 1;
                if (0..h).contains(&d) {
                    n += 2;
                }
            }
        }
        n
    }

    /// Run one PAL frame: 312 lines, each drawn when its cycles are done.
    pub fn run_frame(&mut self) {
        for line in 0..LINES {
            self.io.line = line;
            if line == self.io.compare {
                self.io.irq_flags |= 1;
            }
            // the VIC's cycles stop the CPU, not the clocks of the other chips
            let stolen = self.stolen(line);
            if stolen > 0 {
                self.io.cia[0].tick(stolen as u32);
                self.io.cia[1].tick(stolen as u32);
                self.sid_pending += stolen as u32;
            }
            let mut budget = LINE_CYCLES as i32 - stolen - self.debt;
            while budget > 0 {
                budget -= self.step() as i32;
            }
            self.debt = -budget;
            self.draw_line(line);
        }
        self.flush_sid();
        self.frame += 1;
    }

    /// Draw raster line `line` into the picture.
    fn draw_line(&mut self, line: u16) {
        let row = line as i32 - FIRST_LINE;
        if !(0..HEIGHT as i32).contains(&row) {
            return;
        }
        let io = &self.io;
        let v = &io.vic;
        let out = &mut self.pixels[row as usize * WIDTH..(row as usize + 1) * WIDTH];
        let border = PALETTE[(v[0x20] & 15) as usize];
        let (top, bottom) = if v[0x11] & 8 != 0 { (51, 251) } else { (55, 247) };
        let (left, right) = if v[0x16] & 8 != 0 { (24, 344) } else { (31, 335) };
        let den = v[0x11] & 0x10 != 0;
        let l = line as i32;
        if !den || l < top || l >= bottom {
            out.fill(border);
            return;
        }
        // the graphics: colours, and which pixels are foreground (for sprite priority and collisions)
        let mut col = [0u8; WIDTH];
        let mut fg = [false; WIDTH];
        let bg0 = v[0x21] & 15;
        col.fill(bg0);
        let mode = (v[0x11] & 0x60) >> 4 | (v[0x16] & 0x10) >> 4; // ECM BMM MCM
        let ys = (v[0x11] & 7) as i32;
        let xs = (v[0x16] & 7) as i32;
        let rel = l - (0x30 + ys);
        if (0..200).contains(&rel) {
            let (cy, py) = (rel / 8, (rel % 8) as u16);
            let screen = ((v[0x18] >> 4) as u16) << 10;
            let chars = (((v[0x18] >> 1) & 7) as u16) << 11;
            let bitmap = ((v[0x18] & 8) as u16) << 10;
            for cx in 0..40 {
                let idx = (cy * 40 + cx) as u16;
                let code = io.vread(screen + idx);
                let c = io.color[idx as usize] & 15;
                let byte = match mode {
                    2 | 3 | 6 | 7 => io.vread(bitmap + idx * 8 + py),
                    4 | 5 => io.vread(chars + (code as u16 & 0x3f) * 8 + py),
                    _ => io.vread(chars + code as u16 * 8 + py),
                };
                let x0 = 24 + xs + cx * 8 - FIRST_X;
                let multi = match mode {
                    1 => c & 8 != 0,
                    3 => true,
                    _ => false,
                };
                for b in 0..8 {
                    let (colour, front) = if multi {
                        let pair = (byte >> (6 - (b & 6))) & 3;
                        let colour = match (mode, pair) {
                            (_, 0) => bg0,
                            (1, 1) => v[0x22] & 15,
                            (1, 2) => v[0x23] & 15,
                            (1, _) => c & 7,
                            (_, 1) => code >> 4,
                            (_, 2) => code & 15,
                            _ => c,
                        };
                        (colour, pair >= 2)
                    } else {
                        let set = byte & (0x80 >> b) != 0;
                        let colour = match (mode, set) {
                            (0, true) => c,
                            (1, true) => c & 7,
                            (2, true) => code >> 4,
                            (2, false) => code & 15,
                            (4, true) => c,
                            (4, false) => v[0x21 + (code >> 6) as usize] & 15,
                            (0 | 1, false) => bg0,
                            _ => 0, // the invalid modes show black
                        };
                        (colour, set)
                    };
                    let x = x0 + b;
                    if (0..WIDTH as i32).contains(&x) {
                        col[x as usize] = colour;
                        fg[x as usize] = front;
                    }
                }
            }
        }
        // the sprites, 7 first so that 0 ends on top
        let screen = ((v[0x18] >> 4) as u16) << 10;
        let mut owner = [0u8; WIDTH];
        let (mut ss, mut sb) = (0u8, 0u8);
        for s in (0..8).rev() {
            let bit = 1u8 << s;
            if v[0x15] & bit == 0 {
                continue;
            }
            let ye = v[0x17] & bit != 0;
            let d = l - v[1 + 2 * s] as i32 - 1;
            if !(0..if ye { 42 } else { 21 }).contains(&d) {
                continue;
            }
            let sline = (if ye { d / 2 } else { d }) as u16;
            let ptr = io.vread(screen + 0x3f8 + s as u16) as u16;
            let at = ptr * 64 + sline * 3;
            let data = (io.vread(at) as u32) << 16 | (io.vread(at + 1) as u32) << 8 | io.vread(at + 2) as u32;
            let sx = v[2 * s] as i32 | ((v[0x10] as i32 >> s) & 1) << 8;
            let xe = v[0x1d] & bit != 0;
            let multi = v[0x1c] & bit != 0;
            let behind = v[0x1b] & bit != 0;
            for i in 0..if xe { 48 } else { 24 } {
                let p = if xe { i / 2 } else { i };
                let colour = if multi {
                    match (data >> (22 - (p & !1))) & 3 {
                        0 => continue,
                        1 => v[0x25],
                        2 => v[0x27 + s],
                        _ => v[0x26],
                    }
                } else if data & (0x80_0000 >> p) != 0 {
                    v[0x27 + s]
                } else {
                    continue;
                };
                let x = sx + i - FIRST_X;
                if !(0..WIDTH as i32).contains(&x) {
                    continue;
                }
                let x = x as usize;
                if owner[x] != 0 {
                    ss |= owner[x] | bit;
                }
                owner[x] |= bit;
                if fg[x] {
                    sb |= bit;
                }
                if !(behind && fg[x]) {
                    col[x] = colour & 15;
                }
            }
        }
        let io = &mut self.io;
        if ss != 0 {
            if io.vic[0x1e] == 0 {
                io.irq_flags |= 4;
            }
            io.vic[0x1e] |= ss;
        }
        if sb != 0 {
            if io.vic[0x1f] == 0 {
                io.irq_flags |= 2;
            }
            io.vic[0x1f] |= sb;
        }
        // the side borders cover all of it
        for (x, px) in out.iter_mut().enumerate() {
            let sx = x as i32 + FIRST_X;
            *px = if sx < left || sx >= right { border } else { PALETTE[col[x] as usize] };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A PRG: "10 SYS 2061" then `code` at $080D.
    fn prg(code: &[u8]) -> Vec<u8> {
        let mut p = vec![0x01, 0x08, 0x0b, 0x08, 0x0a, 0x00, 0x9e, b'2', b'0', b'6', b'1', 0, 0, 0];
        p.extend_from_slice(code);
        p
    }

    #[test]
    fn starts_at_the_sys_address() {
        assert_eq!(start_address(&prg(&[])), Some(2061));
        assert_eq!(start_address(&[0x00, 0xc0, 0x60]), Some(0xc000));
    }

    #[test]
    fn border_colour_and_returning() {
        // LDA #2, STA $D020, RTS
        let mut m = Machine::new(&prg(&[0xa9, 0x02, 0x8d, 0x20, 0xd0, 0x60]), &[], 44100).unwrap();
        m.run_frame();
        assert!(m.ended());
        assert_eq!(m.pixels[0], PALETTE[2]);
        // the screen, cleared to light blue on blue
        assert_eq!(m.pixels[(60 - FIRST_LINE as usize) * WIDTH + 100], PALETTE[6]);
    }

    #[test]
    fn raster_interrupts_count_frames() {
        // SEI; $0314 -> irq; CIA 1 quiet; raster line 250; raster IRQ on; CLI; loop
        // irq: INC $C000; ASL $D019; JMP $EA81
        let code = [
            0x78, 0xa9, 0x30, 0x8d, 0x14, 0x03, 0xa9, 0x08, 0x8d, 0x15, 0x03, 0xa9, 0x7f, 0x8d, 0x0d, 0xdc, 0xa9, 0xfa, 0x8d, 0x12, 0xd0, 0xa9, 0x01,
            0x8d, 0x1a, 0xd0, 0x58, 0x4c, 0x28, 0x08,
        ];
        let mut p = prg(&code);
        p.resize(2 + 0x30 - 1, 0xea);
        p.extend_from_slice(&[0xee, 0x00, 0xc0, 0x0e, 0x19, 0xd0, 0x4c, 0x81, 0xea]);
        let mut m = Machine::new(&p, &[], 44100).unwrap();
        for _ in 0..10 {
            m.run_frame();
        }
        assert!((9..=11).contains(&m.peek(0xc000)), "{}", m.peek(0xc000));
        assert!(!m.audio.is_empty());
    }

    #[test]
    fn printing_through_the_kernal() {
        // LDX #0; loop: LDA msg,X; BEQ done; JSR $FFD2; INX; BNE loop; done: GETIN; STA $C000; RTS; msg: "HI", CR, "A", 0
        let code = [0xa2, 0x00, 0xbd, 0x21, 0x08, 0xf0, 0x06, 0x20, 0xd2, 0xff, 0xe8, 0xd0, 0xf5, 0x20, 0xe4, 0xff, 0x8d, 0x00, 0xc0, 0x60, b'H', b'I', 13, b'A', 0];
        let mut m = Machine::new(&prg(&code), &[], 44100).unwrap();
        m.type_key(b'Q');
        m.run_frame();
        assert!(m.ended());
        assert_eq!(&m.io.ram[0x400..0x402], &[8, 9]); // H I in screen codes
        assert_eq!(m.io.ram[0x428], 1); // A on the next line
        assert_eq!((m.io.ram[0xd6], m.io.ram[0xd3]), (1, 1));
        assert_eq!(m.peek(0xc000), b'Q');
        assert_eq!(m.io.ram[0xc6], 0);
    }

    #[test]
    fn joystick_reads_on_cia1() {
        let mut m = Machine::new(&prg(&[0x60]), &[], 44100).unwrap();
        m.set_joystick(2, JOY_LEFT | JOY_FIRE);
        assert_eq!(m.io.read(0xdc00) & 0x1f, 0x1f & !(JOY_LEFT | JOY_FIRE));
        assert_eq!(m.io.read(0xdc01) & 0x1f, 0x1f);
    }
}
