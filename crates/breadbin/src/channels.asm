; Device 8 file channels for the Open ROMs KERNAL (bytes in rom.rs; assemble
; with ACME, the Hack tab's assembler: acme -f plain -o channels.bin channels.asm), which has no drive to talk
; to: OPEN and CLOSE keep the KERNAL's file tables, CHKIN/CHKOUT select the
; channel, CHRIN gives a CR with end-of-file, CHROUT drops bytes. Enough for
; programs that open the drive's command channel. Everything else goes on to
; the Open ROMs' own handlers.
!cpu 6502
* = $f71c

LDTND = $98     ; files open
DFLTN = $99     ; input device
DFLTO = $9a     ; output device
STATUS = $90
LA = $b8
SA = $b9
FA = $ba
LAT = $0259
FAT = $0263
SAT = $026d

open    lda FA
        cmp #8
        bne +
        ldx LDTND
        lda LA
        sta LAT,x
        lda FA
        sta FAT,x
        lda SA
        sta SAT,x
        inc LDTND
        clc
        rts
+       jmp $f34a

; find file A: X = its table index, Z set if it is on device 8
find    ldx LDTND
-       ldy #0
        dex
        bmi +
        cmp LAT,x
        bne -
        ldy FAT,x
+       cpy #8
        rts

close   jsr find
        bne +
        dec LDTND
        ldy LDTND
        lda LAT,y
        sta LAT,x
        lda FAT,y
        sta FAT,x
        lda SAT,y
        sta SAT,x
        clc
        rts
+       jmp $f291

chkin   txa
        jsr find
        bne +
        lda #8
        sta DFLTN
        clc
        rts
+       tax
        jmp $f20e

chkout  txa
        jsr find
        bne +
        lda #8
        sta DFLTO
        clc
        rts
+       tax
        jmp $f250

clrchn  lda #0
        ldx DFLTN
        cpx #8
        bne +
        sta DFLTN
+       lda #3
        ldx DFLTO
        cpx #8
        bne +
        sta DFLTO
+       jmp $f333

chrin   lda DFLTN
        cmp #8
        beq +
        jmp $f157
+       lda #$40
        sta STATUS
        lda #13
        clc
        rts

* = $e524       ; left free by the $E536 patch
chrout  pha
        lda DFLTO
        cmp #8
        beq +
        pla
        jmp $f1ca
+       pla
        clc
        rts
