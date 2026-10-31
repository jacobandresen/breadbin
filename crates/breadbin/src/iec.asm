; Serial bus fixes for the Open ROMs KERNAL (DEV.210823) when a real 1541 is
; attached: the user's own drive ROM is installed, so vc64web emulates the drive
; and fast loaders talk to it (bytes in rom.rs; acme -f plain -o iec.bin iec.asm).
; It goes where channels.asm would, which is only for the virtual drive.
;
; - CIOUT buffers each byte in BSOUR and sends the previous one with
;   iec_tx_byte, which sends TBTCNT, never set from BSOUR: every byte written
;   after LISTEN/SECOND (M-W, M-E: a fast loader's drive code) was stale.
;   ciout_send_byte's CLC / JSR iec_tx_dispatch and iec_tx_flush's
;   SEC / JSR iec_tx_byte become JSR ciout / JSR flush here, plus a NOP.
; - CLOSE uses .Y as its table index; the stock KERNAL's leaves .Y alone, and
;   loaders such as Last Ninja II's IFFL keep a value there across CLOSE.
;   ICLOSE points at `close`.
!cpu 6502
* = $f71c

BSOUR = $95             ; the byte CIOUT holds back
TBTCNT = $a4            ; the byte iec_tx_byte sends
iec_tx_dispatch = $f0c9
iec_tx_byte = $f0d5
CLOSE = $f291

ciout   lda BSOUR
        sta TBTCNT
        clc             ; without EOI
        jmp iec_tx_dispatch

flush   lda BSOUR
        sta TBTCNT
        sec             ; with EOI: the last byte
        jmp iec_tx_byte

close   tax             ; .A is the logical file number
        tya
        pha
        txa
        jsr CLOSE
        pla
        tay
        rts
