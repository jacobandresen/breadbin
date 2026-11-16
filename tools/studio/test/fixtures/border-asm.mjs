// A small ACME program in two files, for the assembly tests (unit/asmdebug,
// browser/game): the border's colour goes up by `speed` a frame.
export const MAIN = `; a border that changes colour by "speed" a frame
* = $0801
!byte $0b,$08,$0a,$00,$9e,$32,$30,$36,$31,$00,$00,$00   ; 10 SYS2061
counter = $fb
SCREEN = $0400
        sei
loop    lda $d012
        cmp #$80
        bne loop
        inc counter             ; a frame went by
        jsr move
wait    lda $d012
        cmp #$80
        beq wait
        jmp loop

!source "move.asm"
`;
export const MOVE = `move    lda pos
        clc
        adc speed
        sta pos
        sta $d020
        rts

speed   !byte 3                 ; how far pos goes each frame
pos     !byte 0
table
        !byte 1, 2, 3, 4
        !word 1000
`;
