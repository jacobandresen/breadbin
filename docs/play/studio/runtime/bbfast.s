; bbfast.s - the breadbin Studio runtime's calls that a game makes many
; times a frame, in assembly (see bb.h for what each does). In C, cc65 spends
; hundreds of cycles on a call's arguments alone; Bubbler, with six monsters,
; ran at a third of the frame rate until these moved here.
;
; cc65's calling convention, for these __fastcall__ functions: the last
; argument arrives in A (char) or A/X (int, low byte in A); the others are on
; cc65's own stack, pushed left to right, so the second-to-last is at (c_sp).
; A char takes one byte there, an int two. The function removes them before
; it returns (addysp), and returns a char in A with X = 0.
;
; Scratch: cc65's zero-page temporaries tmp1-tmp4 and ptr1, which any
; runtime routine may use. The music interrupt uses only $FB/$FC.

        .export _bb_spr, _bb_spr_hide, _bb_spr_color, _bb_overlap
        .export _bb_flags_at, _bb_tile, _bb_set_tile
        .import _bb_sprite_color, _bb_sprite_multi, _bb_tile_flags, _bb_tile_color
        .import addysp, popa
        .importzp c_sp, tmp1, tmp2, tmp3, tmp4, ptr1

SCREEN  = $c000
COLOR   = $d800
PTRS    = SCREEN + $3f8         ; the sprite pointers
VIC     = $d000

        .segment "RODATA"
bitof:  .byte $01, $02, $04, $08, $10, $20, $40, $80
notbit: .byte $fe, $fd, $fb, $f7, $ef, $df, $bf, $7f
; row * 40, for the 25 rows
rowlo:  .repeat 25, r
        .byte <(r * 40)
        .endrepeat
rowhi:  .repeat 25, r
        .byte >(r * 40)
        .endrepeat

        .segment "CODE"

; void __fastcall__ bb_spr(unsigned char n, unsigned char img, int x, int y)
; Stack: x at (c_sp)+0/1, img at +2, n at +3; y in A/X.
_bb_spr:
        clc                     ; y + 50: the VIC's line for the screen's top
        adc #50
        sta tmp3
        txa
        adc #0
        bne @far                ; below the screen (or far above it)
        lda tmp3
        cmp #30
        bcc @far                ; above the top border
        cmp #250
        bcc @yok                ; (250 and on: below the bottom border)
@far:   jmp @off                ; (a branch can't reach that far)
@yok:   ldy #0                  ; x + 24: the VIC's column for the screen's left
        lda (c_sp),y
        clc
        adc #24
        sta tmp1
        iny
        lda (c_sp),y
        adc #0
        sta tmp2
        bmi @off                ; left of the screen
        beq @x0
        cmp #2
        bcs @off
        lda tmp1                ; 256..367 shows
        cmp #<368
        bcs @off
        bcc @show
@x0:    lda tmp1
        beq @off                ; x = 0: hidden at the very left
@show:  ldy #3
        lda (c_sp),y              ; n
        tax
        dey
        lda (c_sp),y              ; img
        sta tmp4
        clc
        adc #$40                ; images from $D000: pointer ($D000 - $C000) / 64 + img
        sta PTRS,x
        ldy tmp4
        lda _bb_sprite_color,y
        sta VIC+$27,x
        lda _bb_sprite_multi,y
        beq @hires
        lda VIC+$1c
        ora bitof,x
        bne @mc
@hires: lda VIC+$1c
        and notbit,x
@mc:    sta VIC+$1c
        txa
        asl
        tay
        lda tmp1
        sta VIC,y               ; x, low 8 bits
        lda tmp3
        sta VIC+1,y             ; y
        lda tmp2
        beq @lo
        lda VIC+$10             ; x bit 8
        ora bitof,x
        bne @msb
@lo:    lda VIC+$10
        and notbit,x
@msb:   sta VIC+$10
        lda VIC+$15
        ora bitof,x
        sta VIC+$15             ; on
        ldy #4
        jmp addysp
@off:   ldy #3
        lda (c_sp),y
        tax
        lda VIC+$15
        and notbit,x
        sta VIC+$15
        ldy #4
        jmp addysp

; void __fastcall__ bb_spr_hide(unsigned char n)
_bb_spr_hide:
        tax
        lda VIC+$15
        and notbit,x
        sta VIC+$15
        rts

; void __fastcall__ bb_spr_color(unsigned char n, unsigned char color)
_bb_spr_color:
        pha
        jsr popa                ; n
        tax
        pla
        sta VIC+$27,x
        rts

; unsigned char __fastcall__ bb_overlap(int ax, int ay, unsigned char aw, unsigned char ah,
;                                       int bx, int by, unsigned char bw, unsigned char bh)
; Stack: bw +0, by +1/2, bx +3/4, ah +5, aw +6, ay +7/8, ax +9/10; bh in A.
; True when ax < bx+bw, bx < ax+aw, ay < by+bh and by < ay+ah.
_bb_overlap:
        sta tmp4                ; bh
        ; by + bh - ay > 0 ?
        ldy #1
        lda (c_sp),y
        clc
        adc tmp4
        sta tmp1
        iny
        lda (c_sp),y
        adc #0
        sta tmp2
        ldy #7
        lda tmp1
        sec
        sbc (c_sp),y
        sta tmp1
        iny
        lda tmp2
        sbc (c_sp),y
        jsr positive
        bcc @no
        ; ay + ah - by > 0 ?
        ldy #5
        lda (c_sp),y              ; ah
        ldy #7
        clc
        adc (c_sp),y
        sta tmp1
        iny
        lda (c_sp),y
        adc #0
        sta tmp2
        ldy #1
        lda tmp1
        sec
        sbc (c_sp),y
        sta tmp1
        iny
        lda tmp2
        sbc (c_sp),y
        jsr positive
        bcc @no
        ; bx + bw - ax > 0 ?
        ldy #0
        lda (c_sp),y              ; bw
        ldy #3
        clc
        adc (c_sp),y
        sta tmp1
        iny
        lda (c_sp),y
        adc #0
        sta tmp2
        ldy #9
        lda tmp1
        sec
        sbc (c_sp),y
        sta tmp1
        iny
        lda tmp2
        sbc (c_sp),y
        jsr positive
        bcc @no
        ; ax + aw - bx > 0 ?
        ldy #6
        lda (c_sp),y              ; aw
        ldy #9
        clc
        adc (c_sp),y
        sta tmp1
        iny
        lda (c_sp),y
        adc #0
        sta tmp2
        ldy #3
        lda tmp1
        sec
        sbc (c_sp),y
        sta tmp1
        iny
        lda tmp2
        sbc (c_sp),y
        jsr positive
        bcc @no
        lda #1
        .byte $2c               ; BIT abs: skips the LDA #0
@no:    lda #0
        ldx #0
        ldy #11
        jmp addysp

; Carry set if the 16-bit value (A = high byte, tmp1 = low byte) is > 0.
positive:
        bmi @neg
        bne @pos
        lda tmp1
        beq @neg
@pos:   sec
        rts
@neg:   clc
        rts

; unsigned char __fastcall__ bb_flags_at(int x, int y)
; Stack: x at +0/1; y in A/X.
_bb_flags_at:
        cpx #0
        bne @out                ; y < 0 or y >= 256
        cmp #200
        bcs @out
        lsr                     ; the row
        lsr
        lsr
        tax
        ldy #1
        lda (c_sp),y              ; x, high byte
        beq @x
        cmp #1
        bne @out                ; x < 0 or x >= 512
        dey
        lda (c_sp),y
        cmp #64
        bcs @out                ; x >= 320
        lsr                     ; 256..319: columns 32..39
        lsr
        lsr
        ora #32
        bne @col
@x:     dey
        lda (c_sp),y
        lsr
        lsr
        lsr
@col:   clc
        adc rowlo,x
        sta ptr1
        lda rowhi,x
        adc #>SCREEN
        sta ptr1+1
        ldy #0
        lda (ptr1),y            ; the tile there
        tay
        lda _bb_tile_flags,y
        ldx #0
        ldy #2
        jmp addysp
@out:   lda #0
        ldx #0
        ldy #2
        jmp addysp

; Point ptr1 at screen cell (tmp1, A) = (cx, cy); carry set if off the screen.
cell:   cmp #25
        bcs @off
        tax
        lda tmp1
        cmp #40
        bcs @off
        adc rowlo,x             ; carry is clear
        sta ptr1
        lda rowhi,x
        adc #>SCREEN
        sta ptr1+1
        clc
@off:   rts

; unsigned char __fastcall__ bb_tile(unsigned char cx, unsigned char cy)
_bb_tile:
        pha
        jsr popa
        sta tmp1
        pla
        jsr cell
        bcs @none
        ldy #0
        lda (ptr1),y
        ldx #0
        rts
@none:  lda #0
        tax
        rts

; void __fastcall__ bb_set_tile(unsigned char cx, unsigned char cy, unsigned char t)
; Stack: cy at +0, cx at +1; t in A.
_bb_set_tile:
        sta tmp2
        ldy #1
        lda (c_sp),y
        sta tmp1
        dey
        lda (c_sp),y
        jsr cell
        bcs @done
        ldy #0
        lda tmp2
        sta (ptr1),y            ; the tile
        lda ptr1+1              ; and its colour, at the same place in colour RAM
        clc
        adc #>(COLOR - SCREEN)
        sta ptr1+1
        ldx tmp2
        lda _bb_tile_color,x
        sta (ptr1),y
@done:  ldy #2
        jmp addysp
