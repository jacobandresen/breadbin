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

/// "Device not present" in the status byte, as the stock KERNAL reports it.
/// When nothing on the bus answers a TALK or LISTEN, iec_return_DEVICE_NOT_FOUND
/// ($FAB0) gives up with the KERNAL error 5 (carry set) but leaves ST ($90) alone;
/// the stock KERNAL sets bit 7 there, and READST is how programs ask. Krill's
/// loader (We Are New and other Fairlight, Censor and Oxyron trackmos) LISTENs to
/// devices 8-14 in turn and counts those with ST bit 7 clear: every one passed
/// for a drive, so it stopped with error $FD ("more than one drive on the bus")
/// and a red border. The routine's final JMP kernalerror_DEVICE_NOT_FOUND goes
/// through NOT_PRESENT, after iec.asm in the free space (real 1541 only: with
/// breadbin's virtual drive the space is full, and no loader probes the bus):
/// LDA #$80 / ORA $90 / STA $90 / JMP kernalerror_DEVICE_NOT_FOUND ($FC7B).
const NOT_PRESENT: (u16, [u8; 9]) = (0xf736, [0xa9, 0x80, 0x05, 0x90, 0x85, 0x90, 0x4c, 0x7b, 0xfc]);
/// iec_return_DEVICE_NOT_FOUND's JMP $FC7B, at $FABB.
const NOT_PRESENT_JMP: (u16, [u8; 3]) = (0xfabb, [0x4c, 0x7b, 0xfc]);

/// SECOND as the stock KERNAL leaves it: its last act releases ATN with LDA $DD00
/// / AND #$F7 / STA $DD00, so it returns with N showing the DATA line, set when no
/// listener holds it. Bitfire's installer (The Star Wars Demo by Censor Design)
/// LISTENs to devices 8-11 and branches on N straight after JSR SECOND to count
/// its drives; the Open ROMs' SECOND left N as it happened to be. The jump table's
/// SECOND ($FF93) goes through this, after NOT_PRESENT: JSR SECOND ($E6B7) /
/// LDA $DD00 / AND #$F7 / RTS. (The KERNAL's own calls keep going straight to $E6B7.)
const SECOND_N: (u16, [u8; 9]) = (0xf73f, [0x20, 0xb7, 0xe6, 0xad, 0x00, 0xdd, 0x29, 0xf7, 0x60]);
const SECOND_JUMP: (u16, [u8; 3]) = (0xff93, [0x4c, 0xb7, 0xe6]);
/// "A byte waits in BSOUR" is bit 7 of C3PO ($94) in the stock KERNAL; the Open
/// ROMs count any value but 0 (and set 1). Programs that use the zero page for
/// themselves leave anything there: Bitfire's depacker (The Star Wars Demo) puts
/// its code at $0020-$011F, and the next LISTEN sent a byte that was never
/// written, to a drive that never took it. CIOUT and iec_tx_flush test bit 7
/// (BIT / BMI, BIT / BPL), and CIOUT marks the buffer full with bit 7: through
/// BUFFER_FULL (LDA #$80 / STA $94 / CLC / RTS) where it was empty, and with DEC
/// after a send, which leaves $94 at 0. (address, published bytes, fix)
const BUFFER_FULL: (u16, [u8; 6]) = (0xf748, [0xa9, 0x80, 0x85, 0x94, 0x18, 0x60]);
const C3PO_BIT7: [(u16, &[u8], &[u8]); 4] = [
    (0xfa0a, &[0xa5, 0x94, 0xd0, 0x07], &[0x24, 0x94, 0x30, 0x07]), // CIOUT: LDA C3PO / BNE -> BIT C3PO / BMI
    (0xfa0e, &[0xe6, 0x94, 0x18], &[0x20, 0x48, 0xf7]),             // INC C3PO / CLC -> JSR BUFFER_FULL
    (0xfa19, &[0xe6, 0x94], &[0xc6, 0x94]),                         // after the send: INC -> DEC
    (0xf8aa, &[0xa5, 0x94, 0xf0, 0x04], &[0x24, 0x94, 0x10, 0x04]), // iec_tx_flush: LDA / BEQ -> BIT / BPL
];

/// CIOUT and UNLSN at their stock addresses, $EDDD and $EDFE: what the jump table
/// ($FFA8, $FFAE) points to on a C64, and loaders call them there (So-Phisticated
/// III's M-W upload by 20th Century Composers, Dutch Breeze by Black Mail: "/" on
/// the screen and back to READY, the Open ROMs' screen editor having run instead).
/// There the Open ROMs have cursor_timer_reset (LDA #$14 / STA $CD, entered by
/// falling in from LDA #0 / STA $CF at $EDD9, and by two branches) and the middle
/// of chrout_screen_literal (ORA $C7 / STA ($D1),Y / LDA $D8 / BEQ +2 / DEC $D8).
/// Both move to the free space after BUFFER_FULL (real 1541 only), each with a
/// jump there from where it was, and $EDDD / $EDFE become JMP CIOUT / JMP UNLSN:
/// - LITERAL_TAIL, from $EDF8: ORA $C7 / STA ($D1),Y / LDA $D8 / BEQ +2 / DEC $D8
///   / JMP $EE02;
/// - UNDRAW_TAIL, from $EDD9: LDA #0 / STA $CF / (the timer) LDA #$14 / STA $CD /
///   RTS; the branches at $EDBF and $EDCB to the timer reset go to a JMP to it in
///   the bytes freed at $EDFB.
const LITERAL_TAIL: (u16, [u8; 13]) = (0xf74e, [0x05, 0xc7, 0x91, 0xd1, 0xa5, 0xd8, 0xf0, 0x02, 0xc6, 0xd8, 0x4c, 0x02, 0xee]);
const UNDRAW_TAIL: (u16, [u8; 9]) = (0xf75b, [0xa9, 0x00, 0x85, 0xcf, 0xa9, 0x14, 0x85, 0xcd, 0x60]);
const STOCK_IEC: [(u16, [u8; 3], [u8; 3]); 7] = [
    (0xedf8, [0x05, 0xc7, 0x91], [0x4c, 0x4e, 0xf7]), // JMP LITERAL_TAIL
    (0xedfb, [0xd1, 0xa5, 0xd8], [0x4c, 0x5f, 0xf7]), // JMP to the timer reset in UNDRAW_TAIL
    (0xedfe, [0xf0, 0x02, 0xc6], [0x4c, 0x09, 0xfd]), // $EDFE: JMP UNLSN
    (0xedd9, [0xa9, 0x00, 0x85], [0x4c, 0x5b, 0xf7]), // JMP UNDRAW_TAIL
    (0xeddd, [0xa9, 0x14, 0x85], [0x4c, 0x09, 0xfa]), // $EDDD: JMP CIOUT
    (0xedbf, [0xd0, 0x1c, 0xa9], [0xd0, 0x3a, 0xa9]), // BNE to $EDFB
    (0xedcb, [0xf0, 0x10, 0x20], [0xf0, 0x2e, 0x20]), // BEQ to $EDFB
];

/// Stock $F5AF and $F5D2 print "SEARCHING FOR" and "LOADING" (or "VERIFYING"),
/// and only in direct mode: called from a program they just return. Crest's
/// loader (Krestage, and Ice Cream Castle after its menu) calls both, with the
/// KERNAL banked in, through a JSR it patches. The Open ROMs have their CTRL key
/// table there, and the CPU jammed in it. An RTS goes in each: CTRL+D and CTRL+=
/// (table entries 18 and 53, $04 and $1F) type $60 instead, with a real 1541
/// only. (address, byte, fix)
const LOAD_MESSAGES: [(u16, u8, u8); 2] = [(0xf5af, 0x04, 0x60), (0xf5d2, 0x1f, 0x60)];

/// The stock KERNAL's keyboard scan leaves CIA 1 port A at $7F, the column with
/// SPACE in it, and programs read SPACE with LDA $DC01 / CMP #$EF and nothing
/// before it (Ice Cream Castle's "please flip disk and press space"); the Open
/// ROMs' scan leaves $FE or $FF there, and no key ever came. SCNKEY in the jump
/// table ($FF9F), which the KERNAL's interrupt calls, goes through SCNKEY_7F,
/// after UNDRAW_TAIL (real 1541 only): JSR scnkey ($E7FD) / LDA #$7F / STA $DC00
/// / RTS.
const SCNKEY_7F: (u16, [u8; 9]) = (0xf764, [0x20, 0xfd, 0xe7, 0xa9, 0x7f, 0x8d, 0x00, 0xdc, 0x60]);
const SCNKEY_JUMP: (u16, [u8; 3]) = (0xff9f, [0x4c, 0xfd, 0xe7]);

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

/// The hardware IRQ vector ($FFFE) points at the Open ROMs' entry $EA53; the
/// stock KERNAL's is $FF48. Games that copy the KERNAL to the RAM under it,
/// bank the ROM out and then reuse $E000-$EFFF (WarGames) keep the stock entry,
/// but wipe the Open ROMs' one and BRK forever. The stock entry, plus the Open
/// ROMs' CLD, goes in dolphindos_detect's body, dead since DOLPHIN: PHA / TXA /
/// PHA / TYA / PHA / CLD / TSX / LDA $0104,X / AND #$10 / BEQ +3 / JMP ($0316) /
/// JMP ($0314).
const IRQ_ENTRY: (u16, [u8; 20]) =
    (0xfe7b, [0x48, 0x8a, 0x48, 0x98, 0x48, 0xd8, 0xba, 0xbd, 0x04, 0x01, 0x29, 0x10, 0xf0, 0x03, 0x6c, 0x16, 0x03, 0x6c, 0x14, 0x03]);
/// dolphindos_detect after its first byte, overwritten by IRQ_ENTRY.
const DOLPHIN_BODY: [u8; 20] = [0x7f, 0x8d, 0x0d, 0xdd, 0xa9, 0x00, 0x8d, 0x03, 0xdd, 0xa2, 0x20, 0xad, 0x0d, 0xdd, 0xc9, 0x10, 0xf0, 0x07, 0xad, 0x01];
const IRQ_VECTOR: (u16, u16) = (0xfffe, 0xea53);

/// Stock $FEBC is the end of the NMI handler, PLA / TAY / PLA / TAX / PLA / RTI,
/// and programs end their own interrupt routines with JMP $FEBC (Road of Excess
/// by Triangle 3532, whose raster interrupt jammed the CPU there). The Open ROMs
/// have lvs_advance_MEMUSS at $FEB9-$FEBF (INC $C3 / BNE +2 / INC $C4 / RTS), so
/// $FEBC becomes JMP $EA81, where the Open ROMs keep the stock sequence, and the
/// routine moves to $FE8F, dead since DOLPHIN (after IRQ_ENTRY), its four callers
/// with it. In either mode.
const FEBC: (u16, [u8; 3], [u8; 3]) = (0xfebc, [0x02, 0xe6, 0xc4], [0x4c, 0x81, 0xea]);
const RESTORE_REGS: (u16, [u8; 6]) = (0xea81, [0x68, 0xa8, 0x68, 0xaa, 0x68, 0x40]);
const MEMUSS_ADVANCE: [u8; 7] = [0xe6, 0xc3, 0xd0, 0x02, 0xe6, 0xc4, 0x60];
const MEMUSS_FROM: u16 = 0xfeb9;
const MEMUSS_TO: (u16, [u8; 7]) = (0xfe8f, [0xdd, 0xca, 0xd0, 0xf3, 0x60, 0xa9, 0x02]);
const MEMUSS_CALLS: [u16; 4] = [0xe9e6, 0xee72, 0xeef9, 0xf0a6];

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
        || at(k, IRQ_ENTRY.0, DOLPHIN_BODY.len()) != DOLPHIN_BODY
        || at(k, IRQ_VECTOR.0, 2) != IRQ_VECTOR.1.to_le_bytes()
        || IEC_CALLS.iter().any(|(a, old, _)| at(k, *a, 4) != old)
        || at(k, NOT_PRESENT_JMP.0, 3) != NOT_PRESENT_JMP.1
        || at(k, MEMUSS_FROM, MEMUSS_ADVANCE.len()) != MEMUSS_ADVANCE
        || at(k, MEMUSS_TO.0, MEMUSS_TO.1.len()) != MEMUSS_TO.1
        || at(k, RESTORE_REGS.0, RESTORE_REGS.1.len()) != RESTORE_REGS.1
        || MEMUSS_CALLS.iter().any(|&a| at(k, a, 3) != [0x20, 0xb9, 0xfe])
        || at(k, SECOND_JUMP.0, 3) != SECOND_JUMP.1
        || C3PO_BIT7.iter().any(|&(a, old, _)| at(k, a, old.len()) != old)
        || STOCK_IEC.iter().any(|(a, old, _)| at(k, *a, 3) != old)
        || LOAD_MESSAGES.iter().any(|&(a, b, _)| at(k, a, 1)[0] != b)
        || at(k, SCNKEY_JUMP.0, 3) != SCNKEY_JUMP.1
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
    at(k, IRQ_ENTRY.0, IRQ_ENTRY.1.len()).copy_from_slice(&IRQ_ENTRY.1);
    at(k, IRQ_VECTOR.0, 2).copy_from_slice(&IRQ_ENTRY.0.to_le_bytes());
    // the branch inside the routine is relative, so it runs as is at $FE8F
    at(k, MEMUSS_TO.0, MEMUSS_ADVANCE.len()).copy_from_slice(&MEMUSS_ADVANCE);
    for a in MEMUSS_CALLS {
        at(k, a + 1, 2).copy_from_slice(&MEMUSS_TO.0.to_le_bytes());
    }
    at(k, FEBC.0, 3).copy_from_slice(&FEBC.2);
    if drive {
        at(k, CHANNELS.0, IEC.len()).copy_from_slice(&IEC);
        for (a, _, new) in IEC_CALLS {
            at(k, a, 4).copy_from_slice(&new);
        }
        at(k, VECTORS + (0x31c - 0x314), 2).copy_from_slice(&(CHANNELS.0 + 16).to_le_bytes()); // ICLOSE
        at(k, NOT_PRESENT.0, NOT_PRESENT.1.len()).copy_from_slice(&NOT_PRESENT.1);
        at(k, NOT_PRESENT_JMP.0 + 1, 2).copy_from_slice(&NOT_PRESENT.0.to_le_bytes());
        at(k, SECOND_N.0, SECOND_N.1.len()).copy_from_slice(&SECOND_N.1);
        at(k, SECOND_JUMP.0 + 1, 2).copy_from_slice(&SECOND_N.0.to_le_bytes());
        at(k, BUFFER_FULL.0, BUFFER_FULL.1.len()).copy_from_slice(&BUFFER_FULL.1);
        for (a, _, fix) in C3PO_BIT7 {
            at(k, a, fix.len()).copy_from_slice(fix);
        }
        at(k, LITERAL_TAIL.0, LITERAL_TAIL.1.len()).copy_from_slice(&LITERAL_TAIL.1);
        at(k, UNDRAW_TAIL.0, UNDRAW_TAIL.1.len()).copy_from_slice(&UNDRAW_TAIL.1);
        for (a, _, fix) in STOCK_IEC {
            at(k, a, 3).copy_from_slice(&fix);
        }
        for (a, _, fix) in LOAD_MESSAGES {
            at(k, a, 1)[0] = fix;
        }
        at(k, SCNKEY_7F.0, SCNKEY_7F.1.len()).copy_from_slice(&SCNKEY_7F.1);
        at(k, SCNKEY_JUMP.0 + 1, 2).copy_from_slice(&SCNKEY_7F.0.to_le_bytes());
    } else {
        at(k, CHANNELS_LOW.0, CHANNELS_LOW.1.len()).copy_from_slice(&CHANNELS_LOW.1);
        at(k, CHANNELS.0, CHANNELS.1.len()).copy_from_slice(&CHANNELS.1);
        for (vector, hook) in HOOKS {
            at(k, VECTORS + (vector - 0x314), 2).copy_from_slice(&hook.to_le_bytes());
        }
    }
    true
}

// ── BASIC (basic_generic.rom, $A000-$BFFF) ──────────────────────────────────

const BASIC_BASE: u16 = 0xa000;
/// Free (zeroed) space after BASIC's cold/warm start vectors and "CBMBASIC".
const BASIC_FREE: u16 = 0xa00c;
/// The Open ROMs' "setcharptr" at stock $A659 is a one-byte stub (RTS), and their
/// WAIT command (cmd_wait) starts right after it, with JSR fetch_uint16 ($BA2E).
const SETCHARPTR: (u16, [u8; 4]) = (0xa659, [0x60, 0x20, 0x2e, 0xba]);
/// WAIT's entry in BASIC's command dispatch table (address - 1, for an RTS jump),
/// split into low bytes at $BD7F and high bytes at $BDA6, token $92 - $80.
const WAIT_LO: u16 = 0xbd7f + 0x12;
const WAIT_HI: u16 = 0xbda6 + 0x12;
/// Stock $A659 resets the text pointer to the start of the program and clears
/// the variables: RUN without the GOTO. Exomizer's self-extracting decruncher
/// ends with JSR $A659 / JSR $A533 (LINKPRG) / JMP $A7AE (NEWSTT) to RUN the
/// BASIC program it unpacked; with the stub, NEWSTT went on reading at the old
/// program's text pointer, inside what had been unpacked over it, and stopped
/// with ?UNDEF'D STATEMENT ERROR IN 20 (We Are New by Fairlight, whose
/// "20 SYS2061" unpacks the demo's loader). The routine goes in BASIC_FREE:
/// JSR RUNC ($A68E, the Open ROMs' "TXTPTR = TXTTAB - 1") / JMP do_clr ($BA00,
/// their CLR). $A659 becomes JMP to it, which takes the first two bytes of WAIT,
/// so WAIT's first instruction moves along, followed by a jump back:
/// JSR fetch_uint16 / JMP $A65D, and the dispatch table points WAIT there.
const RUN_CLR: [u8; 6] = [0x20, 0x8e, 0xa6, 0x4c, 0x00, 0xba];
const WAIT_HEAD: [u8; 6] = [0x20, 0x2e, 0xba, 0x4c, 0x5d, 0xa6];
/// RUNC and do_clr as the published build has them, checked before the patch calls them.
const RUNC: (u16, [u8; 4]) = (0xa68e, [0xa5, 0x2b, 0x38, 0xe9]);
const DO_CLR: (u16, [u8; 3]) = (0xba00, [0x20, 0xe7, 0xff]);

fn at_basic(b: &mut [u8], addr: u16, len: usize) -> &mut [u8] {
    let i = (addr - BASIC_BASE) as usize;
    &mut b[i..i + len]
}

/// Patch an 8 KB Open ROMs BASIC in place. Returns false (and leaves it alone)
/// for any other BASIC.
pub fn patch_basic(b: &mut [u8]) -> bool {
    if b.len() != 0x2000
        || at_basic(b, SETCHARPTR.0, 4) != SETCHARPTR.1
        || at_basic(b, RUNC.0, 4) != RUNC.1
        || at_basic(b, DO_CLR.0, 3) != DO_CLR.1
        || at_basic(b, WAIT_LO, 1)[0] != 0x59
        || at_basic(b, WAIT_HI, 1)[0] != 0xa6
        || at_basic(b, BASIC_FREE, RUN_CLR.len() + WAIT_HEAD.len()).iter().any(|&x| x != 0)
    {
        return false;
    }
    let wait = BASIC_FREE + RUN_CLR.len() as u16;
    at_basic(b, BASIC_FREE, RUN_CLR.len()).copy_from_slice(&RUN_CLR);
    at_basic(b, wait, WAIT_HEAD.len()).copy_from_slice(&WAIT_HEAD);
    let [lo, hi] = BASIC_FREE.to_le_bytes();
    at_basic(b, SETCHARPTR.0, 3).copy_from_slice(&[0x4c, lo, hi]);
    let [lo, hi] = (wait - 1).to_le_bytes();
    at_basic(b, WAIT_LO, 1)[0] = lo;
    at_basic(b, WAIT_HI, 1)[0] = hi;
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
        at(&mut k, IRQ_ENTRY.0, DOLPHIN_BODY.len()).copy_from_slice(&DOLPHIN_BODY);
        at(&mut k, IRQ_VECTOR.0, 2).copy_from_slice(&IRQ_VECTOR.1.to_le_bytes());
        for (a, old, _) in IEC_CALLS {
            at(&mut k, a, 4).copy_from_slice(&old);
        }
        at(&mut k, NOT_PRESENT_JMP.0, 3).copy_from_slice(&NOT_PRESENT_JMP.1);
        at(&mut k, MEMUSS_FROM, MEMUSS_ADVANCE.len()).copy_from_slice(&MEMUSS_ADVANCE);
        at(&mut k, MEMUSS_TO.0, MEMUSS_TO.1.len()).copy_from_slice(&MEMUSS_TO.1);
        at(&mut k, RESTORE_REGS.0, RESTORE_REGS.1.len()).copy_from_slice(&RESTORE_REGS.1);
        for a in MEMUSS_CALLS {
            at(&mut k, a, 3).copy_from_slice(&[0x20, 0xb9, 0xfe]);
        }
        at(&mut k, SECOND_JUMP.0, 3).copy_from_slice(&SECOND_JUMP.1);
        for (a, old, _) in C3PO_BIT7 {
            at(&mut k, a, old.len()).copy_from_slice(old);
        }
        for (a, old, _) in STOCK_IEC {
            at(&mut k, a, 3).copy_from_slice(&old);
        }
        for (a, b, _) in LOAD_MESSAGES {
            at(&mut k, a, 1)[0] = b;
        }
        at(&mut k, SCNKEY_JUMP.0, 3).copy_from_slice(&SCNKEY_JUMP.1);
        k
    }

    #[test]
    fn the_keyboard_scan_leaves_the_space_column() {
        let mut k = open_roms();
        assert!(patch_kernal(&mut k, true));
        assert_eq!(at(&mut k, 0xff9f, 3), [0x4c, 0x64, 0xf7]);
        assert_eq!(at(&mut k, 0xf764, 3), [0x20, 0xfd, 0xe7]); // the Open ROMs' scan first
        assert_eq!(UNDRAW_TAIL.0 + UNDRAW_TAIL.1.len() as u16, SCNKEY_7F.0);
        assert!(SCNKEY_7F.0 as usize + SCNKEY_7F.1.len() <= FREE as usize + 187);
    }

    #[test]
    fn the_load_messages_return_with_a_real_drive() {
        let mut k = open_roms();
        assert!(patch_kernal(&mut k, true));
        assert_eq!([at(&mut k, 0xf5af, 1)[0], at(&mut k, 0xf5d2, 1)[0]], [0x60, 0x60]);
        let mut k = open_roms();
        assert!(patch_kernal(&mut k, false));
        assert_eq!([at(&mut k, 0xf5af, 1)[0], at(&mut k, 0xf5d2, 1)[0]], [0x04, 0x1f]);
    }

    #[test]
    fn ciout_and_unlsn_at_their_stock_addresses() {
        let mut k = open_roms();
        assert!(patch_kernal(&mut k, true));
        assert_eq!(at(&mut k, 0xeddd, 3), [0x4c, 0x09, 0xfa]); // JMP CIOUT
        assert_eq!(at(&mut k, 0xedfe, 3), [0x4c, 0x09, 0xfd]); // JMP UNLSN
        // the branches reach the JMP at $EDFB, which leads to the timer reset
        assert_eq!(0xedbf + 2 + 0x3a, 0xedfb);
        assert_eq!(0xedcb + 2 + 0x2e, 0xedfb);
        assert_eq!(UNDRAW_TAIL.0 + 4, 0xf75f);
        // one after the other in the free space
        assert_eq!(BUFFER_FULL.0 + BUFFER_FULL.1.len() as u16, LITERAL_TAIL.0);
        assert_eq!(LITERAL_TAIL.0 + LITERAL_TAIL.1.len() as u16, UNDRAW_TAIL.0);
        assert!(UNDRAW_TAIL.0 as usize + UNDRAW_TAIL.1.len() <= FREE as usize + 187);
        let mut k = open_roms();
        assert!(patch_kernal(&mut k, false));
        assert_eq!(at(&mut k, 0xeddd, 3), [0xa9, 0x14, 0x85]); // the virtual drive's is left alone
    }

    #[test]
    fn febc_returns_from_an_interrupt() {
        for drive in [false, true] {
            let mut k = open_roms();
            assert!(patch_kernal(&mut k, drive));
            assert_eq!(at(&mut k, 0xfebc, 3), [0x4c, 0x81, 0xea]); // JMP $EA81
            assert_eq!(at(&mut k, 0xfe8f, 7), MEMUSS_ADVANCE);
            assert_eq!(at(&mut k, 0xe9e6, 3), [0x20, 0x8f, 0xfe]);
            // the moved routine sits after IRQ_ENTRY, inside dolphindos_detect ($FE7A-$FE98)
            assert_eq!(IRQ_ENTRY.0 as usize + IRQ_ENTRY.1.len(), MEMUSS_TO.0 as usize);
            assert!(MEMUSS_TO.0 as usize + MEMUSS_ADVANCE.len() <= 0xfe99);
        }
    }

    #[test]
    fn second_and_the_byte_buffer_as_in_the_stock_kernal() {
        let mut k = open_roms();
        assert!(patch_kernal(&mut k, true));
        assert_eq!(at(&mut k, 0xff93, 3), [0x4c, 0x3f, 0xf7]);
        assert_eq!(at(&mut k, 0xfa0e, 3), [0x20, 0x48, 0xf7]); // JSR BUFFER_FULL
        assert_eq!(at(&mut k, 0xf8aa, 4), [0x24, 0x94, 0x10, 0x04]);
        // one after the other in the free space, after iec.asm and NOT_PRESENT
        assert_eq!(NOT_PRESENT.0 + NOT_PRESENT.1.len() as u16, SECOND_N.0);
        assert_eq!(SECOND_N.0 + SECOND_N.1.len() as u16, BUFFER_FULL.0);
        assert!(BUFFER_FULL.0 as usize + BUFFER_FULL.1.len() <= FREE as usize + 187);
        // the virtual drive's KERNAL is left as it was
        let mut k = open_roms();
        assert!(patch_kernal(&mut k, false));
        assert_eq!(at(&mut k, 0xff93, 3), SECOND_JUMP.1);
        assert_eq!(at(&mut k, 0xfa0a, 4), [0xa5, 0x94, 0xd0, 0x07]);
    }

    /// A BASIC with the published build's bytes where the patch goes.
    fn open_roms_basic() -> Vec<u8> {
        let mut b = vec![0u8; 0x2000];
        at_basic(&mut b, SETCHARPTR.0, 4).copy_from_slice(&SETCHARPTR.1);
        at_basic(&mut b, RUNC.0, 4).copy_from_slice(&RUNC.1);
        at_basic(&mut b, DO_CLR.0, 3).copy_from_slice(&DO_CLR.1);
        at_basic(&mut b, WAIT_LO, 1)[0] = 0x59;
        at_basic(&mut b, WAIT_HI, 1)[0] = 0xa6;
        b
    }

    #[test]
    fn sets_device_not_present_with_a_real_drive() {
        let mut k = open_roms();
        assert!(patch_kernal(&mut k, true));
        assert_eq!(at(&mut k, 0xfabb, 3), [0x4c, 0x36, 0xf7]);
        assert_eq!(at(&mut k, 0xf736, 3), [0xa9, 0x80, 0x05]); // LDA #$80 / ORA $90
        assert_eq!(at(&mut k, 0xf73c, 3), [0x4c, 0x7b, 0xfc]); // on to kernalerror_DEVICE_NOT_FOUND
        // right after iec.asm, inside the free space
        assert_eq!(CHANNELS.0 as usize + IEC.len(), NOT_PRESENT.0 as usize);
        assert!(NOT_PRESENT.0 as usize + NOT_PRESENT.1.len() <= FREE as usize + 187);
        // the virtual drive's KERNAL keeps the Open ROMs' jump: its free space is full
        let mut k = open_roms();
        assert!(patch_kernal(&mut k, false));
        assert_eq!(at(&mut k, 0xfabb, 3), NOT_PRESENT_JMP.1);
    }

    #[test]
    fn basic_gets_a_working_a659() {
        let mut b = open_roms_basic();
        assert!(patch_basic(&mut b));
        assert_eq!(at_basic(&mut b, 0xa659, 3), [0x4c, 0x0c, 0xa0]); // JMP RUN_CLR
        assert_eq!(at_basic(&mut b, 0xa00c, 6), RUN_CLR);
        assert_eq!(at_basic(&mut b, 0xa012, 6), WAIT_HEAD);
        // WAIT dispatches (RTS-style, address - 1) to the moved head at $A012
        assert_eq!([at_basic(&mut b, WAIT_LO, 1)[0], at_basic(&mut b, WAIT_HI, 1)[0]], [0x11, 0xa0]);
        // and WAIT's own code goes on after the bytes the JMP took
        assert_eq!(&at_basic(&mut b, 0xa65c, 1)[..], [0xba]);
        assert!(!patch_basic(&mut b)); // only once
        assert!(!patch_basic(&mut vec![0u8; 0x2000])); // not the Open ROMs
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
        // the IRQ entry lives above $F000, inside the dead detection routine
        for drive in [false, true] {
            let mut k = open_roms();
            assert!(patch_kernal(&mut k, drive));
            assert_eq!(at(&mut k, 0xfffe, 2), [0x7b, 0xfe]);
            assert_eq!(at(&mut k, 0xfe7a, 2), [0x60, 0x48]); // RTS, then PHA
            assert!(IRQ_ENTRY.0 as usize + IRQ_ENTRY.1.len() <= 0xfe99); // dolphindos_detect ends at $FE98
        }
        // iec.asm's entries where IEC_CALLS and ICLOSE expect them
        assert_eq!((IEC[0], IEC[8], IEC[16]), (0xa5, 0xa5, 0xaa));
    }
}
