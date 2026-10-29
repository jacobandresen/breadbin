//! A virtual 1541 at the KERNAL level, for running D64 images on the free Open
//! ROMs, which have no drive ROM. The KERNAL's LOAD vector ($0330) is pointed
//! at a small 6502 stub that raises a flag and spins. Each `tick` (called by the
//! host every few milliseconds) looks for that flag, reads the requested name,
//! copies the file from the disk image straight into C64 memory and releases
//! the stub, as a finished LOAD would. Fast loaders that program the drive
//! itself still need a real 1541 ROM.
//!
//! The file may overwrite the stub itself (autostarting loaders often cover
//! page 2), so before copying, the CPU is parked in a second loop outside the
//! file's range: the stub's spin is patched into a JMP there, and the copy
//! happens once the park code reports in.

use crate::disk::{petscii, sys_stub, D64};

/// The C64 as the host exposes it (vc64web's peek/poke), plus a notifier.
pub trait Machine {
    fn peek(&mut self, addr: u16) -> u8;
    fn poke(&mut self, addr: u16, value: u8);
    fn event(&mut self, event: Event);
}

pub enum Event {
    Load { name: String, addr: u16, end: u16 },
    Missing { name: String },
}

const STUB: u16 = 0x02a7;
const FLAG: u16 = 0x02ff;
/// STX $C3 / STY $C4 / STA $93 / LDA #$80 / STA FLAG / spin: BIT FLAG / BMI spin / SEC / RTS
const ENTRY: [u8; 18] = [0x86, 0xc3, 0x84, 0xc4, 0x85, 0x93, 0xa9, 0x80, 0x8d, 0xff, 0x02, 0x2c, 0xff, 0x02, 0x30, 0xfb, 0x38, 0x60];
const SPIN: u16 = STUB + 11;
const PARK_LEN: u16 = 24;
const PARK_SPOTS: [u16; 5] = [0x02c8, 0x033c, 0x0110, 0xcfe0, 0x9fe0];
/// Vectors autostarting programs hook, in the order the stock ROMs call them
/// after a LOAD typed at READY: print READY (CHROUT), re-enter the BASIC loop
/// (IMAIN), then the rest.
const AUTOSTART: [u16; 6] = [0x326, 0x302, 0x328, 0x300, 0x324, 0x318];
/// The stock KERNAL/BASIC vector table ($0300-$0333). Programs often load a
/// copy of it; those addresses mean nothing in the Open ROMs, so they are
/// swapped for the Open ROMs' own handlers.
const STOCK: [u16; 26] = [
    0xe38b, 0xa483, 0xa57c, 0xa71a, 0xa7e4, 0xae86, 0, 0, 0, 0, 0xea31, 0xfe66, 0xfe47, 0xf34a, 0xf291, 0xf20e, 0xf250, 0xf333,
    0xf157, 0xf1ca, 0xf6ed, 0xf13e, 0xf32f, 0xfe66, 0xf4a5, 0xf5ed,
];

/// The park loop at `at`: LDA #$40 / STA FLAG / wait: BIT DONE / BMI wait / BVS err /
/// LDX $AE / LDY $AF / CLC / RTS / NOP / err: LDA #4 / SEC / RTS / DONE
fn park(at: u16) -> [u8; PARK_LEN as usize] {
    let done = (at + 23).to_le_bytes();
    [
        0xa9, 0x40, 0x8d, 0xff, 0x02, 0x2c, done[0], done[1], 0x30, 0xfb, 0x70, 0x07, 0xa6, 0xae, 0xa4, 0xaf, 0x18, 0x60, 0xea, 0xa9,
        0x04, 0x38, 0x60, 0x80,
    ]
}

fn overlaps(a: u16, a_end: u16, b: u16, b_end: u16) -> bool {
    (a as u32) < b_end as u32 && (b as u32) < a_end as u32
}

/// A file being served: name, data (load address first), load and end address.
type Served = (String, Vec<u8>, u16, u16);

struct Job {
    spot: u16,
    file: Option<Served>,
}

pub struct Drive {
    img: Vec<u8>,
    rom_load: u16,
    own_vectors: [u16; 26],
    job: Option<Job>,
    pub loads: u32,
    /// Address of the first program's SYS stub, for the boot script.
    pub sys: u16,
}

fn peek16(m: &mut impl Machine, a: u16) -> u16 {
    u16::from_le_bytes([m.peek(a), m.peek(a + 1)])
}
fn poke16(m: &mut impl Machine, a: u16, v: u16) {
    let [lo, hi] = v.to_le_bytes();
    m.poke(a, lo);
    m.poke(a + 1, hi);
}

impl Drive {
    /// Attach to a machine that has finished booting (READY on screen).
    pub fn new(img: Vec<u8>, m: &mut impl Machine) -> Result<Drive, String> {
        D64::new(&img)?;
        let mut own_vectors = [0; 26];
        for (i, v) in own_vectors.iter_mut().enumerate() {
            *v = peek16(m, 0x300 + 2 * i as u16);
        }
        let d = Drive { img, rom_load: peek16(m, 0x330), own_vectors, job: None, loads: 0, sys: 0 };
        d.install(m);
        Ok(d)
    }

    fn install(&self, m: &mut impl Machine) {
        for (i, b) in ENTRY.iter().enumerate() {
            m.poke(STUB + i as u16, *b);
        }
        m.poke(FLAG, 0);
        poke16(m, 0x330, STUB);
    }

    /// Serve any pending LOAD. Call every few milliseconds.
    pub fn tick(&mut self, m: &mut impl Machine) {
        match self.job.take() {
            None => {
                if m.peek(FLAG) == 0x80 && peek16(m, 0x330) == STUB {
                    self.start(m);
                } else if peek16(m, 0x330) == self.rom_load {
                    // a program that resets the KERNAL vectors (RESTOR) gets the drive back
                    self.install(m);
                }
            }
            Some(job) if m.peek(FLAG) != 0x40 => self.job = Some(job), // not parked yet
            Some(job) => self.finish(m, job),
        }
    }

    /// A LOAD arrived: look the file up and park the CPU out of its way.
    fn start(&mut self, m: &mut impl Machine) {
        let len = m.peek(0xb7) as u16;
        let ptr = peek16(m, 0xbb);
        let name: Vec<u8> = (0..len).map(|i| m.peek(ptr.wrapping_add(i))).collect();
        let file = (m.peek(0xba) == 8 && !name.is_empty()).then(|| self.lookup(&name)).flatten();
        if file.is_none() && !name.is_empty() {
            m.event(Event::Missing { name: petscii(&name) });
        }
        let file = file.filter(|(_, d)| d.len() >= 2).map(|(n, d)| {
            let addr = if m.peek(0xb9) == 0 { peek16(m, 0xc3) } else { u16::from_le_bytes([d[0], d[1]]) };
            let end = addr.wrapping_add(d.len() as u16 - 2);
            (n, d, addr, end)
        });
        let (addr, end) = file.as_ref().map(|f| (f.2, f.3)).unwrap_or((0, 0));
        let spot = PARK_SPOTS
            .into_iter()
            .find(|&p| {
                !overlaps(p, p + PARK_LEN, addr, end)
                    && !overlaps(p, p + PARK_LEN, STUB, STUB + ENTRY.len() as u16)
                    && !overlaps(p, p + PARK_LEN, FLAG, FLAG + 1)
            })
            .unwrap_or(PARK_SPOTS[0]);
        for (i, b) in park(spot).iter().enumerate() {
            m.poke(spot + i as u16, *b);
        }
        m.poke(SPIN, 0x4c); // JMP park
        poke16(m, SPIN + 1, spot);
        self.job = Some(Job { spot, file });
    }

    fn lookup(&self, name: &[u8]) -> Option<(String, Vec<u8>)> {
        let disk = D64::new(&self.img).ok()?;
        if name == b"$" {
            return Some(("$".into(), disk.directory()));
        }
        let e = disk.find(name)?;
        Some((petscii(&e.name), disk.read(&e)))
    }

    /// The CPU is parked: copy the file, then release it the way the stock ROMs would.
    fn finish(&mut self, m: &mut impl Machine, job: Job) {
        let done = job.spot + 23;
        let Some((name, data, addr, end)) = job.file else {
            if m.peek(STUB) == ENTRY[0] {
                self.install(m);
            }
            m.poke(done, 0x40); // release with "file not found"
            return;
        };
        let before: Vec<u16> = AUTOSTART.iter().map(|&v| peek16(m, v)).collect();
        if m.peek(0x93) == 0 {
            for (i, b) in data[2..].iter().enumerate() {
                m.poke(addr.wrapping_add(i as u16), *b);
            }
            self.translate_vectors(m, addr, end);
        }
        poke16(m, 0xae, end);
        m.poke(0x90, 0x40); // status: end of file

        if self.loads == 0 {
            self.autostart(m, job.spot, addr, end, &before);
            if addr == 0x0801 {
                self.sys = sys_stub(&data).unwrap_or(0);
            }
        }
        // put the entry stub back unless the program just loaded over it
        if !overlaps(STUB, FLAG + 1, addr, end) {
            for (i, b) in ENTRY.iter().enumerate() {
                m.poke(STUB + i as u16, *b);
            }
            m.poke(FLAG, 0);
            if !overlaps(0x330, 0x332, addr, end) {
                self.install(m);
            }
        }
        self.loads += 1;
        m.poke(done, 0x00); // release
        m.event(Event::Load { name, addr, end });
    }

    fn translate_vectors(&self, m: &mut impl Machine, addr: u16, end: u16) {
        for (i, &stock) in STOCK.iter().enumerate() {
            let v = 0x300 + 2 * i as u16;
            if stock != 0 && overlaps(v, v + 2, addr, end) && peek16(m, v) == stock {
                poke16(m, v, if v == 0x330 { STUB } else { self.own_vectors[i] });
            }
        }
    }

    /// Under the stock ROMs a LOAD typed at READY returns with the stack pointer
    /// at $F7, so files covering $01F8 plant their own return address there;
    /// after that, BASIC prints READY and re-enters its loop through vectors
    /// other cracks hook. The Open ROMs differ in both, so the parked CPU
    /// leaves the way the stock ROMs would have.
    fn autostart(&self, m: &mut impl Machine, spot: u16, addr: u16, end: u16, before: &[u16]) {
        let exit = spot + 16; // CLC / RTS / NOP
        if overlaps(0x1f8, 0x1fa, addr, end) {
            let target = peek16(m, 0x1f8).wrapping_add(1);
            m.poke(exit, 0x4c); // JMP planted return address + 1
            poke16(m, exit + 1, target);
        } else if let Some(v) = AUTOSTART.iter().zip(before).find(|(&v, &old)| overlaps(v, v + 2, addr, end) && peek16(m, v) != old) {
            m.poke(exit, 0x6c); // JMP (vector)
            poke16(m, exit + 1, *v.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Ram([u8; 0x10000], Vec<String>);
    impl Machine for Ram {
        fn peek(&mut self, a: u16) -> u8 {
            self.0[a as usize]
        }
        fn poke(&mut self, a: u16, v: u8) {
            self.0[a as usize] = v;
        }
        fn event(&mut self, e: Event) {
            if let Event::Load { name, .. } = e {
                self.1.push(name);
            }
        }
    }

    #[test]
    fn serves_a_kernal_load() {
        let img = crate::disk::tests::tiny_disk();
        let mut ram = Ram([0; 0x10000], vec![]);
        ram.0[0x330] = 0xa5;
        ram.0[0x331] = 0xf4;
        let mut d = Drive::new(img, &mut ram).unwrap();
        assert_eq!(peek16(&mut ram, 0x330), STUB);
        // the stub ran: LOAD"*",8,1
        ram.0[0xb7] = 1;
        ram.0[0xbb] = 0x00;
        ram.0[0xbc] = 0x02;
        ram.0[0x200] = b'*';
        ram.0[0xba] = 8;
        ram.0[0xb9] = 1;
        ram.0[FLAG as usize] = 0x80;
        d.tick(&mut ram);
        ram.0[FLAG as usize] = 0x40; // the park code reported in
        d.tick(&mut ram);
        assert_eq!(ram.1, ["GAME"]);
        assert_eq!(&ram.0[0x0801..0x0804], &[0, 0, 0]); // data copied (zeros in the test file)
        assert_eq!(peek16(&mut ram, 0xae), 0x0801 + 254 + 1); // end address
    }
}
