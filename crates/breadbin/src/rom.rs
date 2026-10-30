//! Compatibility patches for the MEGA65 Open ROMs' KERNAL (kernal_generic.rom).
//! Games call a few stock KERNAL addresses that are not entry points at all;
//! where the Open ROMs have something else there, the CPU jams. Each patch
//! puts the stock behaviour back without disturbing the Open ROMs' own code.

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

fn at(k: &mut [u8], addr: u16, len: usize) -> &mut [u8] {
    let i = (addr - BASE) as usize;
    &mut k[i..i + len]
}

/// Patch an 8 KB Open ROMs KERNAL in place. Returns false (and leaves it alone)
/// for any other KERNAL, such as a newer Open ROMs build or the user's own.
pub fn patch_kernal(k: &mut [u8]) -> bool {
    if k.len() != 0x2000 || at(k, 0xe521, MOVED.len()) != MOVED || at(k, FREE, MOVED.len()).iter().any(|&b| b != 0) {
        return false;
    }
    // the routine's branches are relative and its jumps leave it, so it runs as is elsewhere
    at(k, FREE, MOVED.len()).copy_from_slice(&MOVED);
    let [lo, hi] = FREE.to_le_bytes();
    at(k, 0xe521, 3).copy_from_slice(&[0x4c, lo, hi]);
    at(k, 0xe536, SET_COLOUR.len()).copy_from_slice(&SET_COLOUR);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moves_the_routine_and_restores_the_stock_entry() {
        let mut k = vec![0u8; 0x2000];
        at(&mut k, 0xe521, MOVED.len()).copy_from_slice(&MOVED);
        assert!(patch_kernal(&mut k));
        assert_eq!(at(&mut k, FREE, MOVED.len()), MOVED);
        assert_eq!(at(&mut k, 0xe521, 3), [0x4c, 0xf9, 0xf6]);
        assert_eq!(at(&mut k, 0xe536, 3), [0x8d, 0x86, 0x02]);
        // patched once: the original bytes are gone, so a second call changes nothing
        assert!(!patch_kernal(&mut k));
    }
}
