; bbirq.s - the breadbin Studio runtime's frame interrupt (see bb.h).
;
; A raster interrupt at line 250, in the bottom border, once per frame
; (50 times a second on a PAL C64): it counts frames for bb_frame() and runs
; the music player (bbsound.s). The KERNAL's own 60 Hz timer interrupt, which
; scans the keyboard and blinks the cursor, is switched off: games read the
; joysticks, and a game's frame must not be shared with the keyboard scan.
;
; The KERNAL's IRQ entry ($FF48) pushes A, X and Y and jumps through $0314,
; so the handler ends by pulling them itself.

        .export _bb_irq_on, _bb_frames
        .import bbs_play

        .segment "BSS"
_bb_frames:
        .res 2                  ; bb_frames (bb.h), counted by the interrupt

        .segment "CODE"

; void bb_irq_on(void): start the frame interrupt (bb_init calls it).
_bb_irq_on:
        sei
        lda #$7f
        sta $dc0d               ; CIA 1: no timer interrupts
        lda $dc0d               ; ...and none still pending
        lda #<irq
        sta $0314
        lda #>irq
        sta $0315
        lda #250                ; the bottom border
        sta $d012
        lda $d011
        and #$7f                ; raster line bit 8 clear
        sta $d011
        lda #$01
        sta $d01a               ; VIC: raster interrupts on
        sta $d019               ; acknowledge any old one
        cli
        rts

irq:    lda #$01
        sta $d019               ; acknowledge the raster interrupt
        inc _bb_frames
        bne @play
        inc _bb_frames+1
@play:  jsr bbs_play
        pla                     ; Y, X, A, as the KERNAL pushed them
        tay
        pla
        tax
        pla
        rti
