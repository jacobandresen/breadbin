; bbsound.s - the breadbin Studio's SID player: the songs and sound effects
; of the MUSIC editor, in games (called 50 times a second by bbirq.s) and in
; the editor itself (wrapped in a PSID file and played by breadbin's SID
; engine, so what you hear while composing is exactly what the game plays).
;
; The data comes from gen/music.s (written by the Studio, see music.js):
;
;   instruments  bbs_ins_wave, _ad, _sr, _pw, _pwm, _vib, _arp, _slide, _flags
;                (one byte each per instrument; see music.js for the meaning)
;   patterns     bbs_pat_lo/hi -> speed, length, then per step:
;                  note    0 = nothing new, 1..96 = C-0..B-7, $FF = release
;                  insfx   instrument (low nybble), effect (high nybble)
;                  param   the effect's parameter
;   songs        bbs_song_lo/hi -> rows, loop row ($FF = play once), then
;                per row a pattern for each of the three voices ($FF = rest)
;
; Voices 0-2 play the song. Voice 3 is the sound-effect voice: it borrows
; the SID's third voice while an effect plays, and gives it back after.
;
; Effects: 1 slide up, 2 slide down (param = speed), 3 vibrato (param =
; depth), 4 cut (release after param frames).
;
; Zero page $FB/$FC (free for programs on the C64) is the data pointer.

        .export bbs_init, bbs_play, bbs_sfx, _bb_music, _bb_sfx
        .import bbs_ins_wave, bbs_ins_ad, bbs_ins_sr, bbs_ins_pw, bbs_ins_pwm
        .import bbs_ins_vib, bbs_ins_arp, bbs_ins_slide, bbs_ins_flags
        .import bbs_pat_lo, bbs_pat_hi, bbs_song_lo, bbs_song_hi, bbs_song_count

ptr     = $fb
SID     = $d400

FX_UP   = 1
FX_DOWN = 2
FX_VIB  = 3
FX_CUT  = 4
DRUM    = $01                   ; bbs_ins_flags: a noise click on the first frame

        .segment "BSS"
; per voice (0-2 the song, 3 the sound effect)
v_on:     .res 4                ; reading a pattern
v_base_lo:.res 4                ; the pattern's first step
v_base_hi:.res 4
v_len:    .res 4                ; steps in the pattern
v_speed:  .res 4                ; frames per step
v_step:   .res 4                ; the next step to read
v_tick:   .res 4                ; frames into the step
v_note:   .res 4                ; 0..95
v_ins:    .res 4
v_fx:     .res 4
v_param:  .res 4
v_ctrl:   .res 4                ; waveform | gate
v_trig:   .res 4                ; a new note: hard restart on the next write
v_frame:  .res 4                ; frames since the note began (stops at 255)
v_slo:    .res 4                ; the slide so far (16 bits, signed)
v_shi:    .res 4
v_pwlo:   .res 4                ; pulse width (12 bits)
v_pwhi:   .res 4
v_cut:    .res 4                ; frames until a cut releases the note
v_flo:    .res 4                ; the frequency to write
v_fhi:    .res 4
; the song
s_on:     .res 1
s_base_lo:.res 1
s_base_hi:.res 1
s_rows:   .res 1
s_loop:   .res 1
s_row:    .res 1
s_lead:   .res 1                ; the voice whose pattern ends the row ($FF none)
sfx_on:   .res 1
give_back:.res 1                ; the sound effect ended: restore voice 2
frames:   .res 1                ; counts frames, for arpeggios and vibrato
tmp:      .res 2

        .segment "CODE"

; bbs_init: A = song (or $FF for silence). Clears the player and starts it.
bbs_init:
        pha
        ldx #3
@clr:   lda #0
        sta v_on,x
        sta v_ctrl,x
        sta v_trig,x
        sta v_flo,x
        sta v_fhi,x
        dex
        bpl @clr
        sta sfx_on
        sta give_back
        sta s_on
        ldx #$18
@sid:   sta SID,x               ; every SID register to zero
        dex
        bpl @sid
        lda #$0f
        sta SID+$18             ; full volume, no filter
        pla
        jmp music

; void __fastcall__ bb_music(unsigned char n): start song n ($FF stops).
_bb_music:
        php
        sei
        jsr music
        plp
        rts

; void __fastcall__ bb_sfx(unsigned char n): play pattern n on voice 3.
_bb_sfx:
bbs_sfx:
        php
        sei
        ldx #3
        jsr start_pattern
        lda #1
        sta sfx_on
        plp
        rts

; A = song: start it, or stop the music.
music:  cmp bbs_song_count
        bcc @go
        lda #0                  ; stop: release the three voices
        sta s_on
        ldx #2
@off:   sta v_on,x
        lda v_ctrl,x
        and #$fe
        sta v_ctrl,x
        lda #0
        dex
        bpl @off
        rts
@go:    tax
        lda bbs_song_lo,x
        sta s_base_lo
        lda bbs_song_hi,x
        sta s_base_hi
        lda #0
        sta s_row
        ldy #0
        jsr song_byte           ; rows
        sta s_rows
        iny
        jsr song_byte           ; loop row
        sta s_loop
        lda #1
        sta s_on
        ; fall through: start row 0

; Start the patterns of row s_row on voices 0-2.
start_row:
        lda #$ff
        sta s_lead
        lda s_row               ; offset 2 + row * 3
        asl
        clc
        adc s_row
        clc
        adc #2
        tay
        ldx #0
@voice: jsr song_byte
        cmp #$ff
        beq @rest
        stx tmp
        sty tmp+1
        jsr start_pattern
        ldx tmp
        ldy tmp+1
        lda s_lead
        bpl @next
        stx s_lead              ; the first voice with a pattern leads
        jmp @next
@rest:  lda #0
        sta v_on,x
        lda v_ctrl,x
        and #$fe
        sta v_ctrl,x
@next:  iny
        inx
        cpx #3
        bne @voice
        rts

; A = the song's byte at offset Y.
song_byte:
        lda s_base_lo
        sta ptr
        lda s_base_hi
        sta ptr+1
        lda (ptr),y
        rts

; Start pattern A on voice X.
start_pattern:
        tay
        lda bbs_pat_lo,y
        sta ptr
        lda bbs_pat_hi,y
        sta ptr+1
        ldy #0
        lda (ptr),y
        sta v_speed,x
        iny
        lda (ptr),y
        sta v_len,x
        lda ptr                 ; the steps start after speed and length
        clc
        adc #2
        sta v_base_lo,x
        lda ptr+1
        adc #0
        sta v_base_hi,x
        lda #0
        sta v_step,x
        sta v_tick,x
        lda #1
        sta v_on,x
        rts

; ── once a frame ────────────────────────────────────────────────────────

bbs_play:
        inc frames
        lda s_on
        beq @sfx
        ldx #0
@song:  jsr tick
        inx
        cpx #3
        bne @song
        ; the row ends when its leading voice has played its pattern (a row
        ; where every voice rests has none: it lasts this one frame)
        ldx s_lead
        bmi @next
        lda v_on,x
        bne @sfx
@next:  inc s_row
        lda s_row
        cmp s_rows
        bcc @row
        lda s_loop
        cmp #$ff
        bne @loop
        lda #$ff                ; played once: stop
        jsr music
        jmp @sfx
@loop:  sta s_row
@row:   jsr start_row
@sfx:   lda sfx_on
        beq @write
        lda v_on+3              ; its last step played in full last frame?
        bne @tick3
        lda #0                  ; the effect is over: voice 2 is the song's again
        sta sfx_on
        lda #1
        sta give_back
        sta v_trig+2
        bne @write
@tick3: ldx #3
        jsr tick
@write: ldx #0
        ldy #0
        jsr write
        ldx #1
        ldy #7
        jsr write
        ldx #2
        lda sfx_on
        beq @v2
        ldx #3
@v2:    ldy #14
        jmp write

; Advance voice X by one frame.
tick:   lda v_on,x
        bne @on
        jmp effects             ; nothing to read: the held note goes on
@on:    lda v_tick,x
        beq @read
        jmp @count
@read:  lda v_step,x            ; Y = step * 3
        asl
        clc
        adc v_step,x
        tay
        lda v_base_lo,x
        sta ptr
        lda v_base_hi,x
        sta ptr+1
        lda (ptr),y             ; note
        beq @fx
        cmp #$ff
        bne @note
        lda v_ctrl,x            ; release
        and #$fe
        sta v_ctrl,x
        jmp @fx
@note:  sec
        sbc #1
        sta v_note,x
        iny
        lda (ptr),y
        and #$0f
        sta v_ins,x
        lda (ptr),y
        lsr
        lsr
        lsr
        lsr
        sta v_fx,x
        iny
        lda (ptr),y
        sta v_param,x
        ; a new note
        lda #0
        sta v_frame,x
        sta v_slo,x
        sta v_shi,x
        sta v_pwlo,x
        sta v_cut,x
        lda v_fx,x
        cmp #FX_CUT
        bne @nocut
        lda v_param,x
        sta v_cut,x
@nocut: ldy v_ins,x
        lda bbs_ins_pw,y
        and #$0f
        sta v_pwhi,x
        lda bbs_ins_wave,y
        ora #$01
        sta v_ctrl,x
        lda #1
        sta v_trig,x
        jmp @next
@fx:    ; no new note: an effect may still change for the held one
        lda v_step,x
        asl
        clc
        adc v_step,x
        tay
        iny
        lda (ptr),y
        lsr
        lsr
        lsr
        lsr
        beq @next
        sta v_fx,x
        iny
        lda (ptr),y
        sta v_param,x
        lda v_fx,x
        cmp #FX_CUT
        bne @next
        lda v_param,x
        sta v_cut,x
@next:  inc v_step,x
@count: inc v_tick,x
        lda v_tick,x
        cmp v_speed,x
        bcc effects
        lda #0
        sta v_tick,x
        lda v_step,x
        cmp v_len,x
        bcc effects
        lda #0                  ; the pattern is over
        sta v_on,x
        ; fall through

; The frame's effects on voice X, and the frequency to write.
effects:
        lda v_frame,x
        cmp #$ff
        beq @slide
        inc v_frame,x
@slide: ldy v_ins,x
        lda bbs_ins_slide,y     ; the instrument's own glide (signed)
        jsr add_slide
        lda v_fx,x
        cmp #FX_UP
        bne @down
        lda v_param,x
        jsr add_slide
        jmp @cut
@down:  cmp #FX_DOWN
        bne @cut
        lda #0
        sec
        sbc v_param,x
        jsr add_slide
@cut:   lda v_cut,x
        beq @pwm
        dec v_cut,x
        bne @pwm
        lda v_ctrl,x
        and #$fe
        sta v_ctrl,x
@pwm:   ldy v_ins,x
        lda bbs_ins_pwm,y       ; pulse width sweep: + speed * 16 a frame
        beq @pitch
        asl
        asl
        asl
        asl
        clc
        adc v_pwlo,x
        sta v_pwlo,x
        lda v_pwhi,x
        adc #0
        and #$0f
        sta v_pwhi,x

@pitch: ; the note plus the arpeggio's step for this frame
        ldy v_ins,x
        lda bbs_ins_arp,y
        beq @base
        sta tmp
        lda frames
@mod3:  cmp #3
        bcc @arp
        sbc #3
        bcs @mod3
@arp:   cmp #1
        bcc @base0
        beq @hi
        lda tmp                 ; third frame: the low nybble
        and #$0f
        jmp @add
@hi:    lda tmp                 ; second frame: the high nybble
        lsr
        lsr
        lsr
        lsr
        jmp @add
@base0:
@base:  lda #0
@add:   clc
        adc v_note,x
        cmp #96
        bcc @tab
        lda #95
@tab:   tay
        lda freq_lo,y
        clc
        adc v_slo,x
        sta v_flo,x
        lda freq_hi,y
        adc v_shi,x
        sta v_fhi,x

        ; vibrato: depth from the effect, or the instrument's after 8 frames
        lda v_fx,x
        cmp #FX_VIB
        bne @insvib
        lda v_param,x
        jmp @vib
@insvib:ldy v_ins,x
        lda v_frame,x
        cmp #8
        bcc @done
        lda bbs_ins_vib,y
@vib:   beq @done
        ; amplitude = frequency high byte * depth, applied along a triangle
        tay
        lda #0
        sta tmp
        sta tmp+1
@mul:   lda tmp
        clc
        adc v_fhi,x
        sta tmp
        bcc @m1
        inc tmp+1
@m1:    dey
        bne @mul
        lda frames              ; phase 0..7: 0 +1/2 +1 +1/2 0 -1/2 -1 -1/2
        lsr
        and #$07
        tay
        lda tri,y               ; 0 nothing, 1 half, 2 the whole amplitude
        beq @done
        cmp #2
        beq @full
        lsr tmp+1
        ror tmp
@full:  cpy #4
        bcc @up
        lda v_flo,x             ; phases 4-7: below the note
        sec
        sbc tmp
        sta v_flo,x
        lda v_fhi,x
        sbc tmp+1
        sta v_fhi,x
        rts
@up:    lda v_flo,x             ; above it
        clc
        adc tmp
        sta v_flo,x
        lda v_fhi,x
        adc tmp+1
        sta v_fhi,x
@done:  rts

; Add A (signed) * 4 to voice X's slide.
add_slide:
        sta tmp
        ldy #0
        cmp #$80
        bcc @pos
        dey                     ; Y = $FF: the sign
@pos:   sty tmp+1
        asl tmp
        rol tmp+1
        asl tmp
        rol tmp+1
        lda v_slo,x
        clc
        adc tmp
        sta v_slo,x
        lda v_shi,x
        adc tmp+1
        sta v_shi,x
        rts

; Write voice X's state to the SID voice at offset Y.
write:  lda v_trig,x
        beq @regs
        lda #0
        sta v_trig,x
        lda v_ctrl,x
        and #$f0                ; gate off for a moment: the envelope restarts
        ora #$08                ; test bit: the oscillator restarts with it
        sta SID+4,y
        stx tmp
        lda v_ins,x
        tax
        lda bbs_ins_ad,x
        sta SID+5,y
        lda bbs_ins_sr,x
        sta SID+6,y
        ldx tmp
@regs:  lda v_flo,x
        sta SID+0,y
        lda v_fhi,x
        sta SID+1,y
        lda v_pwlo,x
        sta SID+2,y
        lda v_pwhi,x
        sta SID+3,y
        ; a drum: noise on its first two frames, then its own waveform
        stx tmp
        lda v_ins,x
        tax
        lda bbs_ins_flags,x
        ldx tmp
        and #DRUM
        beq @wave
        lda v_frame,x
        cmp #2
        bcs @wave
        lda v_ctrl,x
        and #$01
        ora #$80
        sta SID+4,y
        lda #$28                ; a high noise
        sta SID+1,y
        rts
@wave:  lda v_ctrl,x
        sta SID+4,y
        rts

        .segment "RODATA"
tri:    .byte 0, 1, 2, 1, 0, 1, 2, 1
; PAL SID frequencies of C-0..B-7 (B-7 is out of the SID's range: $FFFF)
freq_lo:
        .byte $16, $27, $39, $4b, $5f, $74, $8a, $a1, $ba, $d4, $f0, $0e, $2d, $4e, $71, $96, $be, $e7, $14
        .byte $42, $74, $a9, $e0, $1b, $5a, $9c, $e2, $2d, $7b, $cf, $27, $85, $e8, $51, $c1, $37, $b4, $38
        .byte $c4, $59, $f7, $9d, $4e, $0a, $d0, $a2, $81, $6d, $67, $70, $89, $b2, $ed, $3b, $9c, $13, $a0
        .byte $45, $02, $da, $ce, $e0, $11, $64, $da, $76, $39, $26, $40, $89, $04, $b4, $9c, $c0, $23, $c8
        .byte $b4, $eb, $72, $4c, $80, $12, $08, $68, $39, $80, $45, $90, $68, $d6, $e3, $99, $00, $24, $10
        .byte $ff
freq_hi:
        .byte $01, $01, $01, $01, $01, $01, $01, $01, $01, $01, $01, $02, $02, $02, $02, $02, $02, $02, $03
        .byte $03, $03, $03, $03, $04, $04, $04, $04, $05, $05, $05, $06, $06, $06, $07, $07, $08, $08, $09
        .byte $09, $0a, $0a, $0b, $0c, $0d, $0d, $0e, $0f, $10, $11, $12, $13, $14, $15, $17, $18, $1a, $1b
        .byte $1d, $1f, $20, $22, $24, $27, $29, $2b, $2e, $31, $34, $37, $3a, $3e, $41, $45, $49, $4e, $52
        .byte $57, $5c, $62, $68, $6e, $75, $7c, $83, $8b, $93, $9c, $a5, $af, $b9, $c4, $d0, $dd, $ea, $f8
        .byte $ff
