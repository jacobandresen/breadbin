//! Exomizer 2.0 raw decruncher (forward, the bit stream `exomizer raw -P0`
//! writes), as EasyProg uses it to unpack EasySplit files.
//!
//! Altered from exodec.c in Exomizer by Magnus Lind:
//! Copyright (c) 2002 - 2005 Magnus Lind.
//! This software is provided 'as-is', without any express or implied warranty.
//! In no event will the authors be held liable for any damages arising from
//! the use of this software. Permission is granted to anyone to use this
//! software for any purpose, including commercial applications, and to alter it
//! and redistribute it freely, subject to the following restrictions:
//!   1. The origin of this software must not be misrepresented; you must not
//!   claim that you wrote the original software. If you use this software in a
//!   product, an acknowledgment in the product documentation would be
//!   appreciated but is not required.
//!   2. Altered source versions must be plainly marked as such, and must not
//!   be misrepresented as being the original software.
//!   3. This notice may not be removed or altered from any source distribution.
//! This is an altered version: a Rust port of the decruncher for one protocol.

struct Bits<'a> {
    data: &'a [u8],
    pos: usize,
    buf: u8,
}

impl Bits<'_> {
    fn byte(&mut self) -> Result<u8, String> {
        let b = *self.data.get(self.pos).ok_or("crunched data ends early")?;
        self.pos += 1;
        Ok(b)
    }

    /// `count` bits, first bit highest; the buffer shifts right and refills
    /// with a marker bit, as the 6502 decruncher's ROR does
    fn bits(&mut self, count: u32) -> Result<usize, String> {
        let mut val = 0;
        for _ in 0..count {
            let mut carry = self.buf & 1;
            self.buf >>= 1;
            if self.buf == 0 {
                let b = self.byte()?;
                carry = b & 1;
                self.buf = (b >> 1) | 0x80;
            }
            val = (val << 1) | carry as usize;
        }
        Ok(val)
    }
}

/// Decrunch `data`, crunched with `exomizer raw -P0`.
pub fn decrunch(data: &[u8]) -> Result<Vec<u8>, String> {
    let mut b = Bits { data, pos: 0, buf: 0 };
    b.buf = b.byte()?;
    // 52 entries of base + extra bits: 16 for lengths, then offsets for
    // sequences of length 1 (4 entries), 2 (16) and 3 or more (16)
    let (mut base, mut extra) = ([0usize; 52], [0u32; 52]);
    let mut a = 0usize;
    let mut bits = 0u32;
    for i in 0..52 {
        a = if i & 15 == 0 { 1 } else { a + (1 << bits) };
        base[i] = a;
        bits = b.bits(4)? as u32;
        extra[i] = bits;
    }
    const OFF: [(usize, u32); 3] = [(48, 2), (32, 4), (16, 4)];
    let mut out: Vec<u8> = Vec::new();
    loop {
        if b.bits(1)? == 1 {
            out.push(b.byte()?);
            continue;
        }
        let mut code = 0;
        while b.bits(1)? == 0 {
            code += 1;
        }
        match code {
            16 => return Ok(out),
            17 => {
                for _ in 0..b.bits(16)? {
                    out.push(b.byte()?);
                }
            }
            c if c < 16 => {
                let len = base[c] + b.bits(extra[c])?;
                let (start, n) = OFF[len.min(3) - 1];
                let i = start + b.bits(n)?;
                let offset = base[i] + b.bits(extra[i])?;
                let from = out.len().checked_sub(offset).ok_or("crunched data points before the start")?;
                for k in 0..len {
                    out.push(out[from + k]);
                }
            }
            _ => return Err("bad crunched data".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unpacks_what_exomizer_packs() {
        // `exomizer raw -P0` of the text below six times, then bytes 0..64
        const CRUNCHED: [u8; 139] = [
            0x04, 0x00, 0x01, 0x22, 0x69, 0x00, 0x00, 0x00, 0x00, 0x32, 0x20, 0x00, 0x31, 0x20, 0x31, 0x80, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x00, 0x00, 0x00, 0x01, 0x31, 0xe3, 0x52, 0x45, 0x7f, 0x41, 0x44, 0x59, 0x2e, 0x20, 0x4c, 0x4f, 0xda, 0x22, 0x2a,
            0xe2, 0x2c, 0x38, 0x62, 0x31, 0x7d, 0x53, 0xbe, 0xff, 0x43, 0x48, 0x49, 0x4e, 0x47, 0xbe, 0x46, 0xd7, 0xc8, 0x0e, 0x0b,
            0x54, 0xe0, 0xa8, 0xea, 0x55, 0xc1, 0x04, 0xc0, 0x07, 0x00, 0x20, 0x80, 0x00, 0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06,
            0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a,
            0x1b, 0x1c, 0x1d, 0x1e, 0x1f, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2d, 0x2e,
            0x2f, 0x30, 0x31, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x3a, 0x3b, 0x3c, 0x3d, 0x3e, 0x3f, 0x00, 0x80,
        ];
        let mut want = b"READY. LOAD\"*\",8,1 SEARCHING FOR * LOADING READY. RUN READY. ".repeat(6);
        want.extend(0..64u8);
        assert_eq!(decrunch(&CRUNCHED).unwrap(), want);
        assert!(decrunch(&CRUNCHED[..60]).is_err());
    }
}
