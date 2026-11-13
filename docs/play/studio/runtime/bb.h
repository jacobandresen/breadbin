/* bb.h - the breadbin Studio runtime: a small game library for the C64,
 * with a small API. Every Studio project links it; the Studio
 * writes it into the project's gen/ folder with the assets, so the folder
 * also builds with a native cc65 (see the project's Makefile).
 *
 *   #include "bb.h"
 *   #include "assets.h"            // SPR_..., TILE_..., ROOM_..., SONG_..., SFX_...
 *
 *   int main(void) {
 *     bb_init();                   // graphics, sprites, tiles, music IRQ
 *     bb_room(ROOM_START);
 *     bb_music(SONG_THEME);
 *     for (;;) {
 *       bb_frame();                // wait for the next frame (50 per second)
 *       if (bb_btn(BB_LEFT)) --x;
 *       bb_spr(0, SPR_PLAYER, x, y);
 *     }
 *   }
 *
 * Memory: the program has $0801-$BFFF. The runtime keeps VIC bank 3 for
 * itself: the screen at $C000, the tiles (a 256-character set) at $C800 and
 * up to 128 sprite images from $E000, in the RAM under the KERNAL ROM.
 *
 * Coordinates are screen pixels: x 0..319, y 0..199, for sprites too
 * (a sprite at 0,0 has its top-left corner at the screen's). Cells are the
 * 40x25 grid of 8x8 tiles.
 */
#ifndef BB_H
#define BB_H

/* ── joystick ─────────────────────────────────────────────────────────── */

/* Bits of bb_joy(): either joystick port, read and merged. */
#define BB_UP    0x01
#define BB_DOWN  0x02
#define BB_LEFT  0x04
#define BB_RIGHT 0x08
#define BB_FIRE  0x10

/* The joysticks (both ports, merged), as BB_ bits. */
unsigned char bb_joy(void);
/* Held now? `b` is BB_ bits; true if any of them is held. Updated by bb_frame(). */
#define bb_btn(b)  (bb_joy_now & (b))
/* Pressed since the last frame (held now, not before)? */
#define bb_btnp(b) (bb_joy_now & ~bb_joy_was & (b))
/* The joystick bits of this frame and of the one before (bb_frame reads them). */
extern unsigned char bb_joy_now, bb_joy_was;

/* ── the frame ────────────────────────────────────────────────────────── */

/* Set up the screen, tiles, sprites and the music interrupt. Call first. */
void bb_init(void);
/* Wait for the next frame (the bottom of the picture), then read the joysticks. */
void bb_frame(void);
/* Frames since bb_init(), counting up 50 times a second (wraps at 65535). */
extern volatile unsigned int bb_frames;

/* Border, background and the two shared multicolours of the tiles. */
void bb_colors(unsigned char border, unsigned char bg, unsigned char mc1, unsigned char mc2);

/* C64 colours. */
enum {
  BB_BLACK, BB_WHITE, BB_RED, BB_CYAN, BB_PURPLE, BB_GREEN, BB_BLUE, BB_YELLOW,
  BB_ORANGE, BB_BROWN, BB_PINK, BB_DARKGREY, BB_GREY, BB_LIGHTGREEN, BB_LIGHTBLUE, BB_LIGHTGREY
};

/* ── sprites (the eight hardware sprites, slots 0..7) ─────────────────── */

/* Show slot `n` as sprite image `img` (an SPR_ constant) with its top-left at x, y. */
void __fastcall__ bb_spr(unsigned char n, unsigned char img, int x, int y);
/* Hide slot `n`. */
void __fastcall__ bb_spr_hide(unsigned char n);
/* Change slot `n`'s colour (bb_spr sets the image's own colour). */
void __fastcall__ bb_spr_color(unsigned char n, unsigned char color);
/* Do two boxes overlap? (x, y, w, h each) - for sprite-against-sprite hits. */
unsigned char __fastcall__ bb_overlap(int ax, int ay, unsigned char aw, unsigned char ah,
                                      int bx, int by, unsigned char bw, unsigned char bh);

/* ── tiles and rooms ──────────────────────────────────────────────────── */

/* Draw room `n` (a ROOM_ constant, painted in the MAP editor) on the screen. */
void __fastcall__ bb_room(unsigned char n);
/* The tile in cell cx, cy (0..39, 0..24). */
unsigned char __fastcall__ bb_tile(unsigned char cx, unsigned char cy);
/* Put tile `t` in cell cx, cy, in its own colour. */
void __fastcall__ bb_set_tile(unsigned char cx, unsigned char cy, unsigned char t);
/* The flags of tile `t` (the FLAG_ bits ticked in the TILES editor). */
#define bb_fget(t) (bb_tile_flags[(unsigned char)(t)])
/* The flags of the tile under screen pixel x, y (0 off the screen). */
unsigned char __fastcall__ bb_flags_at(int x, int y);
extern const unsigned char bb_tile_flags[256];

/* ── text (the font is tiles 0..63, in screen-code order) ─────────────── */

/* Write `s` (letters, digits, punctuation) at cell cx, cy in `color`. */
void __fastcall__ bb_text(unsigned char cx, unsigned char cy, const char *s, unsigned char color);
/* Write `n` with `digits` digits (leading zeros) at cell cx, cy. */
void __fastcall__ bb_num(unsigned char cx, unsigned char cy, unsigned int n, unsigned char digits, unsigned char color);
/* Fill the screen with tile `t` in `color`. */
void __fastcall__ bb_cls(unsigned char t, unsigned char color);

/* ── sound (the MUSIC editor's songs and sound effects) ───────────────── */

/* Start song `n` (a SONG_ constant); BB_STOP stops the music. */
void __fastcall__ bb_music(unsigned char n);
/* Play pattern `n` (an SFX_ constant) once, on the third voice. */
void __fastcall__ bb_sfx(unsigned char n);

/* bb_music(BB_STOP) stops the music. */
#define BB_STOP 0xff

/* ── odds and ends ────────────────────────────────────────────────────── */

/* A random number 0..255. */
unsigned char bb_rnd(void);
/* Seed the random numbers (bb_init seeds from the raster beam). */
void __fastcall__ bb_seed(unsigned int s);

#endif
