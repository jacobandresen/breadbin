; bbdraw.s - the breadbin Studio runtime's pixels, in assembly (see bb.h):
; plotting, horizontal spans and blits on the 320x200 hires bitmap at $E000.
; The shapes (lines, circles, rectangles) are in bb.c, built on these.
;
; The bitmap is in the RAM under the KERNAL ROM. Writing there reaches the
; RAM anyway, but reading it (to set one pixel among eight, or to flip one)
; would read the ROM: so each routine switches the KERNAL out ($01 = $35,
; I/O kept in) and back, with interrupts off meanwhile (their vector at
; $FFFE would come from RAM). The routines are short; a long blit delays the
; frame interrupt, it doesn't lose it.
;
; The bitmap's layout is the VIC's: 25 rows of 40 cells, a cell 8 bytes, one
; byte per pixel row of the cell. So pixel x, y is in the byte at
;   $E000 + (y / 8) * 320 + (x / 8) * 8 + (y & 7),   bit 7 - (x & 7).
;
; The C side (bb.c) clips, then calls these with arguments in the bbd_
; variables below: cheaper than cc65's argument stack for inner loops.

        .export _bb_plot, _bb_pget, _bb_pen_mode
        .export _bbd_span, _bbd_vspan, _bbd_blit, _bbd_grab
        .export _bbd_x0, _bbd_x1, _bbd_y, _bbd_src, _bbd_cx, _bbd_wb, _bbd_h
        .import addysp
        .importzp c_sp, ptr1, ptr2, tmp1, tmp2, tmp3, tmp4

BITMAP  = $e000

; bb_pen modes
DRAW    = 0                     ; set (OR)
ERASE   = 1                     ; clear (AND NOT)
FLIP    = 2                     ; flip (EOR)
COPY    = 3                     ; blit: replace

        .segment "DATA"
_bb_pen_mode:
        .byte DRAW

        .segment "BSS"
_bbd_x0:  .res 2                ; a span from x0 to x1 (0..319) on row y
_bbd_x1:  .res 2
_bbd_y:   .res 1                ; a blit's first row, or the span's row
_bbd_src: .res 2                ; a blit's picture (or a grab's buffer)
_bbd_cx:  .res 1                ; a blit's first cell column (0..39)
_bbd_wb:  .res 1                ; its width in bytes, already clipped
_bbd_h:   .res 1                ; its height in rows, already clipped
xlo:      .res 1                ; the pixel being located
xhi:      .res 1
ylo:      .res 1
srcw:     .res 1                ; a blit's picture width (unclipped), its row stride

        .segment "RODATA"
; the address of each cell row: $E000 + row * 320
rowlo:  .repeat 25, r
        .byte <(BITMAP + r * 320)
        .endrepeat
rowhi:  .repeat 25, r
        .byte >(BITMAP + r * 320)
        .endrepeat
bitmask:.byte $80, $40, $20, $10, $08, $04, $02, $01
; a span's first byte keeps the pixels left of x0 out, its last those right of x1
leftmask:  .byte $ff, $7f, $3f, $1f, $0f, $07, $03, $01
rightmask: .byte $80, $c0, $e0, $f0, $f8, $fc, $fe, $ff

        .segment "CODE"

; The KERNAL out, interrupts off: the bitmap reads as RAM. (A macro: it
; leaves $01's old value and the flags on the stack for CLOSE.)
.macro OPEN
        php
        sei
        lda $01
        pha
        and #$f8
        ora #$05                ; KERNAL and BASIC out, I/O in
        sta $01
.endmacro
.macro CLOSE
        pla
        sta $01
        plp
.endmacro

; Point ptr1 at the byte of pixel (xlo/xhi, ylo) and tmp1 at its bit; carry
; set if it is off the screen.
locate: lda xhi
        beq @x
        cmp #1
        bne @off                ; x >= 512, or negative
        lda xlo
        cmp #64
        bcs @off                ; x >= 320
@x:     lda ylo
        cmp #200
        bcs @off
        lsr
        lsr
        lsr
        tax
        lda xlo
        and #$f8
        clc
        adc rowlo,x
        sta ptr1
        lda xhi
        adc rowhi,x
        sta ptr1+1
        lda ylo                 ; rowlo is a multiple of 64 and x & $f8 of 8:
        and #7                  ; the row within the cell goes in the low bits
        ora ptr1
        sta ptr1
        lda xlo
        and #7
        tax
        lda bitmask,x
        sta tmp1
        clc
        rts
@off:   sec
        rts

; Apply the pen to the bits tmp1 of the byte at ptr1 (the KERNAL out).
pixel:  ldy #0
        ldx _bb_pen_mode
        cpx #ERASE
        beq @erase
        cpx #FLIP
        beq @flip
        lda (ptr1),y            ; DRAW (and COPY: a pixel is drawn)
        ora tmp1
        sta (ptr1),y
        rts
@erase: lda tmp1
        eor #$ff
        and (ptr1),y
        sta (ptr1),y
        rts
@flip:  lda (ptr1),y
        eor tmp1
        sta (ptr1),y
        rts

; Take x (on the C stack) and y (A/X) of a fastcall (int x, int y) call
; into xlo/xhi/ylo and drop x from the stack; carry set if y is off the screen.
takexy: cpx #0
        bne @off                ; y < 0 or y > 255
        sta ylo
        ldy #0
        lda (c_sp),y
        sta xlo
        iny
        lda (c_sp),y
        sta xhi
        ldy #2
        jsr addysp
        clc
        rts
@off:   ldy #2
        jsr addysp
        sec
        rts

; void __fastcall__ bb_plot(int x, int y)
_bb_plot:
        jsr takexy
        bcs @done
        jsr locate
        bcs @done
        OPEN
        jsr pixel
        CLOSE
@done:  rts

; unsigned char __fastcall__ bb_pget(int x, int y)
_bb_pget:
        jsr takexy
        bcs @no
        jsr locate
        bcs @no
        OPEN
        ldy #0
        lda (ptr1),y
        and tmp1
        sta tmp2
        CLOSE
        lda tmp2
        beq @no
        lda #1
        ldx #0
        rts
@no:    lda #0
        tax
        rts

; void bbd_span(void): the pen from bbd_x0 to bbd_x1 (0..319, x0 <= x1) on
; row bbd_y (0..199), a byte at a time.
_bbd_span:
        lda _bbd_x0
        sta xlo
        lda _bbd_x0+1
        sta xhi
        lda _bbd_y
        sta ylo
        jsr locate              ; ptr1: the first byte
        bcs @done
        ; the byte columns: from x0 / 8 to x1 / 8
        lda _bbd_x0+1
        lsr
        lda _bbd_x0
        ror
        lsr
        lsr
        sta tmp3                ; the first column
        lda _bbd_x1+1
        lsr
        lda _bbd_x1
        ror
        lsr
        lsr
        sta tmp4                ; the last
        OPEN
@byte:  lda #$ff
        ldx tmp3
        cpx tmp4
        bne @notlast
        ldx _bbd_x1             ; the last byte: up to x1
        pha
        txa
        and #7
        tax
        pla
        and rightmask,x
@notlast:
        ; the first byte: from x0 (later bytes start at their left edge)
        ldx _bbd_x0
        pha
        txa
        and #7
        tax
        pla
        and leftmask,x
        sta tmp1
        lda #0                  ; ...and only the first one
        sta _bbd_x0
        jsr pixel
        lda ptr1                ; the next byte, 8 on
        clc
        adc #8
        sta ptr1
        bcc :+
        inc ptr1+1
:       inc tmp3
        lda tmp4
        cmp tmp3
        bcs @byte
        CLOSE
@done:  rts

; void bbd_vspan(void): the pen down column bbd_x0 (0..319) from row bbd_y,
; bbd_h rows (all on the screen): the byte found once, then a row at a time
; (the next byte down, or 313 on at a cell's last row).
_bbd_vspan:
        lda _bbd_x0
        sta xlo
        lda _bbd_x0+1
        sta xhi
        lda _bbd_y
        sta ylo
        jsr locate
        bcs @done
        lda _bbd_y
        and #7
        sta tmp2                ; the row within the cell
        OPEN
@row:   jsr pixel
        inc tmp2
        lda tmp2
        cmp #8
        beq @cell
        inc ptr1                ; the next row of the cell: the next byte
        bne @next               ; (a cell's 8 rows never cross a page)
@cell:  lda #0
        sta tmp2
        lda ptr1                ; the next cell row: 320 - 7 bytes on
        clc
        adc #<(320 - 7)
        sta ptr1
        lda ptr1+1
        adc #>(320 - 7)
        sta ptr1+1
@next:  dec _bbd_h
        bne @row
        CLOSE
@done:  rts

; Point ptr1 at bbd_cx's byte in the bitmap row bbd_y (0..199).
blitrow:
        lda _bbd_y
        lsr
        lsr
        lsr
        tax
        lda _bbd_cx
        asl                     ; cx * 8 (cx < 40: up to 312)
        asl
        asl
        sta tmp2
        lda #0
        rol
        tay                     ; the high byte of cx * 8
        lda tmp2
        clc
        adc rowlo,x
        sta ptr1
        tya
        adc rowhi,x
        sta ptr1+1
        lda _bbd_y
        and #7
        ora ptr1
        sta ptr1
        rts

; void bbd_blit(void): copy the picture at bbd_src (rows of srcw = bbd_wb
; bytes... see bb.c: bbd_wb is the clipped width, the stride is srcw, set by
; bbd_stride) onto the bitmap at cell column bbd_cx, row bbd_y, bbd_h rows,
; with the pen.
_bbd_blit:
        ; the pen goes into the inner loop's instructions
        ldx _bb_pen_mode
        lda invert,x
        sta @inv+1
        lda opcode,x
        sta @op
        lda operand,x
        sta @op+1
        lda _bbd_src
        sta ptr2
        lda _bbd_src+1
        sta ptr2+1
        OPEN
@row:   jsr blitrow
        lda #0
        sta tmp3                ; the byte within the picture's row
        sta tmp4                ; ...and within the bitmap (8 apart)
        ldx _bbd_wb
@byte:  ldy tmp3
        lda (ptr2),y
@inv:   eor #$00                ; $FF to erase: AND with the picture's inverse
        ldy tmp4
@op:    ora (ptr1),y            ; or AND, EOR (zp),y, or ORA #0 to copy
        sta (ptr1),y
        inc tmp3
        lda tmp4
        clc
        adc #8
        sta tmp4
        dex
        bne @byte
        lda ptr2                ; the picture's next row
        clc
        adc srcw
        sta ptr2
        bcc :+
        inc ptr2+1
:       inc _bbd_y
        dec _bbd_h
        bne @row
        CLOSE
        rts

; the blit's inner loop, by pen: DRAW, ERASE, FLIP, COPY
invert: .byte $00, $ff, $00, $00
opcode: .byte $11, $31, $51, $09        ; ORA (zp),y; AND (zp),y; EOR (zp),y; ORA #
operand:.byte <ptr1, <ptr1, <ptr1, $00

; void bbd_grab(void): the bitmap at cell column bbd_cx, row bbd_y into the
; buffer at bbd_src (bbd_h rows of bbd_wb bytes, stride srcw).
_bbd_grab:
        lda _bbd_src
        sta ptr2
        lda _bbd_src+1
        sta ptr2+1
        OPEN
@row:   jsr blitrow
        lda #0
        sta tmp3
        sta tmp4
        ldx _bbd_wb
@byte:  ldy tmp4
        lda (ptr1),y
        ldy tmp3
        sta (ptr2),y
        inc tmp3
        lda tmp4
        clc
        adc #8
        sta tmp4
        dex
        bne @byte
        lda ptr2
        clc
        adc srcw
        sta ptr2
        bcc :+
        inc ptr2+1
:       inc _bbd_y
        dec _bbd_h
        bne @row
        CLOSE
        rts

; void __fastcall__ bbd_stride(unsigned char w): a blit's picture row width.
        .export _bbd_stride
_bbd_stride:
        sta srcw
        rts
