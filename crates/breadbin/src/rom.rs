//! Compatibility patches for the MEGA65 Open ROMs' KERNAL (kernal_generic.rom,
//! DEV.210823). Games call a few stock KERNAL addresses that are not entry points
//! at all; where the Open ROMs have something else there, the CPU jams. Each
//! patch puts the stock behaviour back without disturbing the Open ROMs' own code.
//!
//! Then it depends on the drive. With breadbin's virtual drive (no 1541 ROM), the
//! KERNAL gets device 8 file channels (channels.asm), since programs that open the
//! drive's command channel otherwise stop at "device not present". With a real
//! 1541 emulated (the user's own drive ROM), it gets serial bus fixes instead
//! (iec.asm, in the same space), so fast loaders can upload their drive code.
//! Addresses come from building the Open ROMs at the release's commit (113523ab)
//! with symbols; the build matches the published binary except its version text.

const BASE: u16 = 0xe000;
/// Free (zeroed) space in kernal_generic.rom for code moved out of the way.
const FREE: u16 = 0xf6f9;

/// The Open ROMs routine at $E521 (it ends at $E543), which stock entry $E536 cuts through.
const MOVED: [u8; 35] = [
    0xb9, 0x59, 0x02, 0xd0, 0x03, 0x4c, 0x7b, 0xf2, 0xb9, 0x63, 0x02, 0x20, 0xcd, 0xf4, 0x90, 0x03, 0x4c, 0x40, 0xf9, 0xb9, 0x59,
    0x02, 0x09, 0x60, 0x20, 0xb7, 0xe6, 0x90, 0x03, 0x4c, 0x40, 0xf9, 0x4c, 0x73, 0xf2,
];

/// Stock $E536 (the end of CINT): STA $0286 (text colour) / LDA #4 / STA $028B /
/// LDA #$0C / STA $CD / STA $CC, then on into clear screen at $E544.
const SET_COLOUR: [u8; 14] = [0x8d, 0x86, 0x02, 0xa9, 0x04, 0x8d, 0x8b, 0x02, 0xa9, 0x0c, 0x85, 0xcd, 0x85, 0xcc];

/// channels.asm, assembled: CHROUT's wrapper in the bytes $E536 frees up ...
const CHANNELS_LOW: (u16, [u8; 14]) = (0xe524, [0x48, 0xa5, 0x9a, 0xc9, 0x08, 0xf0, 0x04, 0x68, 0x4c, 0xca, 0xf1, 0x68, 0x18, 0x60]);
/// ... and the rest after the moved routine.
const CHANNELS: (u16, [u8; 152]) = (
    0xf71c,
    [
        0xa5, 0xba, 0xc9, 0x08, 0xd0, 0x15, 0xa6, 0x98, 0xa5, 0xb8, 0x9d, 0x59, 0x02, 0xa5, 0xba, 0x9d, 0x63, 0x02, 0xa5, 0xb9, 0x9d,
        0x6d, 0x02, 0xe6, 0x98, 0x18, 0x60, 0x4c, 0x4a, 0xf3, 0xa6, 0x98, 0xa0, 0x00, 0xca, 0x30, 0x08, 0xdd, 0x59, 0x02, 0xd0, 0xf6,
        0xbc, 0x63, 0x02, 0xc0, 0x08, 0x60, 0x20, 0x3a, 0xf7, 0xd0, 0x18, 0xc6, 0x98, 0xa4, 0x98, 0xb9, 0x59, 0x02, 0x9d, 0x59, 0x02,
        0xb9, 0x63, 0x02, 0x9d, 0x63, 0x02, 0xb9, 0x6d, 0x02, 0x9d, 0x6d, 0x02, 0x18, 0x60, 0x4c, 0x91, 0xf2, 0x8a, 0x20, 0x3a, 0xf7,
        0xd0, 0x06, 0xa9, 0x08, 0x85, 0x99, 0x18, 0x60, 0xaa, 0x4c, 0x0e, 0xf2, 0x8a, 0x20, 0x3a, 0xf7, 0xd0, 0x06, 0xa9, 0x08, 0x85,
        0x9a, 0x18, 0x60, 0xaa, 0x4c, 0x50, 0xf2, 0xa9, 0x00, 0xa6, 0x99, 0xe0, 0x08, 0xd0, 0x02, 0x85, 0x99, 0xa9, 0x03, 0xa6, 0x9a,
        0xe0, 0x08, 0xd0, 0x02, 0x85, 0x9a, 0x4c, 0x33, 0xf3, 0xa5, 0x99, 0xc9, 0x08, 0xf0, 0x03, 0x4c, 0x57, 0xf1, 0xa9, 0x40, 0x85,
        0x90, 0xa9, 0x0d, 0x18, 0x60,
    ],
);
/// iec.asm, assembled: `ciout` at $F71C, `flush` at $F724, `close` at $F72C.
const IEC: [u8; 26] = [
    0xa5, 0x95, 0x85, 0xa4, 0x18, 0x4c, 0xc9, 0xf0, 0xa5, 0x95, 0x85, 0xa4, 0x38, 0x4c, 0xd5, 0xf0, 0xaa, 0x98, 0x48, 0x8a, 0x20,
    0x91, 0xf2, 0x68, 0xa8, 0x60,
];
/// Calls into iec.asm: ciout_send_byte's CLC / JSR iec_tx_dispatch and
/// iec_tx_flush's SEC / JSR iec_tx_byte, each becoming JSR + NOP.
const IEC_CALLS: [(u16, [u8; 4], [u8; 4]); 2] = [
    (0xfa15, [0x18, 0x20, 0xc9, 0xf0], [0x20, 0x1c, 0xf7, 0xea]),
    (0xf8ae, [0x38, 0x20, 0xd5, 0xf0], [0x20, 0x24, 0xf7, 0xea]),
];

/// Serial bus bugs fixed in place, in either mode: (address, byte, fix).
const FIXES: [(u16, u8, u8); 8] = [
    // chkin_iec reads the logical file number (LAT, $0259) where it means the
    // secondary address (SAT, $026D): reading logical file 1 failed with FILE
    // NOT INPUT, and TKSA asked for the wrong channel. ckout_iec has the same
    // two loads, in the routine moved to FREE (+1 and +20 there).
    (0xfc48, 0x59, 0x6d),
    (0xfc5d, 0x59, 0x6d),
    (FREE + 1, 0x59, 0x6d),
    (FREE + 20, 0x59, 0x6d),
    // close_iec: ORA $E0 (zero page) for ORA #$E0, so CLOSE sent the drive a
    // random command byte
    (0xff71, 0x05, 0x09),
    // jiffydos_detect: a plain 1541 acknowledging quickly passed for JiffyDOS and
    // the KERNAL switched protocol; the BPL now just goes on waiting
    (0xfadd, 0x0b, 0x00),
    // iec_set_idle (after UNLSN and other commands) holds CLK; the stock KERNAL
    // releases it. Fast loaders that signal by flipping $DD02 (Creatures II's)
    // then never see the drive. AND #$D7 becomes AND #$C7, after its ORA #$10.
    (0xf8a4, 0xd7, 0xc7),
    // tksa: ORA #$90 sends the drive "listen" ($90-$9F) after TALK where the
    // secondary address must be $60-$6F, so a real drive never talked back
    // (reading the command channel, LOAD"$"); ORA #$60 as the stock KERNAL does
    (0xf389, 0x90, 0x60),
];
/// dolphindos_detect turns off CIA 2's interrupts and its port B on every OPEN,
/// LOAD and SAVE looking for a parallel cable; it returns at once instead.
const DOLPHIN: (u16, u8) = (0xfe7a, 0xa9);

/// The KERNAL's default vector table (copied to $0314 by RESTOR), and the
/// channels.asm entry points for IOPEN, ICLOSE, ICHKIN, ICKOUT, ICLRCH, IBASIN, IBSOUT.
const VECTORS: u16 = 0xfd30;
const HOOKS: [(u16, u16); 7] = [(0x31a, 0xf71c), (0x31c, 0xf74c), (0x31e, 0xf76c), (0x320, 0xf77c), (0x322, 0xf78c), (0x324, 0xf7a3), (0x326, 0xe524)];

fn at(k: &mut [u8], addr: u16, len: usize) -> &mut [u8] {
    let i = (addr - BASE) as usize;
    &mut k[i..i + len]
}

/// Patch an 8 KB Open ROMs KERNAL in place, for a real 1541 if `drive` (else
/// for the virtual drive). Returns false (and leaves it alone) for any other
/// KERNAL, such as a newer Open ROMs build or the user's own.
pub fn patch_kernal(k: &mut [u8], drive: bool) -> bool {
    // (the fixes inside FREE apply to MOVED's copy, checked by the tests)
    let moved = |a: u16| (FREE..FREE + MOVED.len() as u16).contains(&a);
    if k.len() != 0x2000
        || at(k, 0xe521, MOVED.len()) != MOVED
        || at(k, FREE, 187).iter().any(|&b| b != 0)
        || FIXES.iter().any(|&(a, b, _)| !moved(a) && at(k, a, 1)[0] != b)
        || at(k, DOLPHIN.0, 1)[0] != DOLPHIN.1
        || IEC_CALLS.iter().any(|(a, old, _)| at(k, *a, 4) != old)
    {
        return false;
    }
    // the routine's branches are relative and its jumps leave it, so it runs as is elsewhere
    at(k, FREE, MOVED.len()).copy_from_slice(&MOVED);
    let [lo, hi] = FREE.to_le_bytes();
    at(k, 0xe521, 3).copy_from_slice(&[0x4c, lo, hi]);
    at(k, 0xe536, SET_COLOUR.len()).copy_from_slice(&SET_COLOUR);
    for (a, _, fix) in FIXES {
        at(k, a, 1)[0] = fix;
    }
    at(k, DOLPHIN.0, 1)[0] = 0x60; // RTS
    if drive {
        at(k, CHANNELS.0, IEC.len()).copy_from_slice(&IEC);
        for (a, _, new) in IEC_CALLS {
            at(k, a, 4).copy_from_slice(&new);
        }
        at(k, VECTORS + (0x31c - 0x314), 2).copy_from_slice(&(CHANNELS.0 + 16).to_le_bytes()); // ICLOSE
    } else {
        at(k, CHANNELS_LOW.0, CHANNELS_LOW.1.len()).copy_from_slice(&CHANNELS_LOW.1);
        at(k, CHANNELS.0, CHANNELS.1.len()).copy_from_slice(&CHANNELS.1);
        for (vector, hook) in HOOKS {
            at(k, VECTORS + (vector - 0x314), 2).copy_from_slice(&hook.to_le_bytes());
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A KERNAL with the published build's bytes where the patches go.
    fn open_roms() -> Vec<u8> {
        let mut k = vec![0u8; 0x2000];
        at(&mut k, 0xe521, MOVED.len()).copy_from_slice(&MOVED);
        for (a, b, _) in FIXES.into_iter().filter(|&(a, ..)| a < FREE || a >= FREE + 35) {
            at(&mut k, a, 1)[0] = b;
        }
        at(&mut k, DOLPHIN.0, 1)[0] = DOLPHIN.1;
        for (a, old, _) in IEC_CALLS {
            at(&mut k, a, 4).copy_from_slice(&old);
        }
        k
    }

    #[test]
    fn moves_the_routine_and_restores_the_stock_entry() {
        let mut k = open_roms();
        assert!(patch_kernal(&mut k, false));
        assert_eq!(at(&mut k, 0xe521, 3), [0x4c, 0xf9, 0xf6]);
        assert_eq!(at(&mut k, 0xe536, 3), [0x8d, 0x86, 0x02]);
        assert_eq!(at(&mut k, VECTORS + 6, 2), [0x1c, 0xf7]); // IOPEN
        assert_eq!(CHANNELS.0 as usize + CHANNELS.1.len(), FREE as usize + 187); // fills the free space exactly
        // patched once: the original bytes are gone, so a second call changes nothing
        assert!(!patch_kernal(&mut k, false));
    }

    #[test]
    fn fixes_the_serial_bus() {
        // ckout_iec's two LDA LAT,Y in the moved copy
        assert_eq!([MOVED[0], MOVED[1], MOVED[19], MOVED[20]], [0xb9, 0x59, 0xb9, 0x59]);
        for drive in [false, true] {
            let mut k = open_roms();
            assert!(patch_kernal(&mut k, drive));
            assert_eq!(at(&mut k, FREE, 3), [0xb9, 0x6d, 0x02]); // LDA SAT,Y
            assert_eq!(at(&mut k, 0xff71, 1), [0x09]); // ORA #$E0
            assert_eq!(at(&mut k, DOLPHIN.0, 1), [0x60]);
            assert_eq!(at(&mut k, 0xfa15, 4)[0] == 0x20, drive);
            assert_eq!(at(&mut k, VECTORS + 8, 2) == [0x2c, 0xf7], drive); // ICLOSE
        }
        // iec.asm's entries where IEC_CALLS and ICLOSE expect them
        assert_eq!((IEC[0], IEC[8], IEC[16]), (0xa5, 0xa5, 0xaa));
    }
}
