/* Pixels - a tour of the runtime's bitmap mode, blits and peek/poke.
 * Made with the breadbin Studio. Fire goes on to the next part.
 *
 *   1  shapes: lines, rectangles, circles, discs, fills, colours, text
 *   2  plot against blit: the same ball, drawn a pixel at a time, then
 *      a byte (8 pixels) at a time, timed in frames
 *   3  bouncing balls: blits with the flip pen (draw twice and it's gone),
 *      pre-shifted so they move a pixel at a time, under a hardware sprite
 *   4  peek and poke: the VIC's raster line and the joystick port, live,
 *      and the border colour, written
 */

#include "bb.h"
#include "assets.h"

#define BALL_W 3                         /* the ball: 16x16, in a 24-pixel (3-byte) box */
#define BALL_H 16
#define BALL_BYTES (BALL_W * BALL_H)
#define BALLS 8

/* The ball, shifted 0..7 pixels right: blits go a byte at a time, so a
 * thing that moves a pixel at a time needs a copy for each of the 8 offsets. */
unsigned char ball[8][BALL_BYTES];

unsigned int frames_taken;
unsigned int plot_frames, blit_frames;   /* the race's result (the Variables panel shows them) */

static void wait_fire(void) {
  do bb_frame(); while (!bb_btnp(BB_FIRE));
}

/* A heading across the top: white letters on blue cells. */
static void heading(const char *s) {
  bb_ink(0, 0, 40, 1, BB_WHITE, BB_BLUE);
  bb_btext(1, 0, s);
  bb_ink(0, 24, 40, 1, BB_LIGHTBLUE, BB_BLUE);
  bb_btext(1, 24, "FIRE: ON");
}

/* A number in the bitmap, 3 digits (bb_num writes to text mode's screen). */
static void bnum(unsigned char cx, unsigned char cy, unsigned int v) {
  char n[4];
  unsigned char i;
  n[3] = 0;
  for (i = 3; i--;) {
    n[i] = '0' + v % 10;
    v /= 10;
  }
  bb_btext(cx, cy, n);
}

/* ── 1: shapes ──────────────────────────────────────────────────────────── */

static void shapes(void) {
  unsigned char i;
  bb_bitmap(BB_LIGHTGREY, BB_BLACK);
  heading("SHAPES");
  bb_pen(BB_DRAW);

  /* a fan of lines */
  for (i = 0; i <= 16; ++i) bb_line(80, 100, i * 10, 16);
  for (i = 0; i <= 16; ++i) bb_line(80, 100, i * 10, 184);

  /* rectangles inside each other, and a filled one */
  for (i = 0; i < 6; ++i) bb_rect(176 + i * 6, 16 + i * 6, 128 - i * 12, 72 - i * 12);
  bb_fill(232, 44, 16, 16);

  /* circles, and discs: the colours come from the cells they're in */
  for (i = 1; i <= 5; ++i) bb_circle(200, 144, i * 7);
  bb_disc(272, 144, 30);
  bb_ink(30, 14, 8, 8, BB_YELLOW, BB_BLACK);
  bb_ink(20, 14, 10, 8, BB_LIGHTGREEN, BB_BLACK);
  bb_ink(22, 2, 16, 9, BB_CYAN, BB_BLACK);

  /* erasing: a slot cut out of the disc */
  bb_pen(BB_ERASE);
  bb_fill(264, 116, 16, 56);
  bb_pen(BB_DRAW);
  wait_fire();
}

/* ── 2: plot against blit ───────────────────────────────────────────────── */

/* Make the 8 shifted balls: draw a disc in a spare corner of the screen
 * (one the race draws over later), grab it, erase it. */
static void make_balls(void) {
  unsigned char s;
  bb_pen(BB_DRAW);
  for (s = 0; s < 8; ++s) {
    bb_pen(BB_ERASE);                     /* a clear box... */
    bb_fill(0, 96, 24, 16);
    bb_pen(BB_DRAW);
    bb_disc(s + 7, 103, 7);               /* ...a ball in it, s pixels further right... */
    bb_grab(0, 96, ball[s], BALL_W, BALL_H);  /* ...taken into ball[s] */
  }
  bb_pen(BB_ERASE);
  bb_fill(0, 96, 24, 16);
  bb_pen(BB_DRAW);
}

static void race(void) {
  unsigned int t;
  unsigned char i, x, y;
  bb_bitmap(BB_WHITE, BB_BLACK);
  heading("PLOT AGAINST BLIT");
  make_balls();

  /* 40 balls, a pixel at a time: every set pixel of the picture, plotted */
  bb_ink(0, 2, 20, 21, BB_PINK, BB_BLACK);
  t = bb_frames;
  for (i = 0; i < 40; ++i) {
    for (y = 0; y < BALL_H; ++y)
      for (x = 0; x < 16; ++x)
        if (ball[0][y * BALL_W + (x >> 3)] & (0x80 >> (x & 7))) bb_plot(8 + (i % 8) * 18 + x, 24 + (i / 8) * 26 + y);
  }
  frames_taken = bb_frames - t;
  plot_frames = frames_taken;
  bb_btext(1, 22, "PLOT");
  bnum(6, 22, frames_taken);
  bb_btext(10, 22, "FRAMES");

  /* the same 40 balls as blits: 3 bytes x 16 rows each */
  bb_ink(20, 2, 20, 21, BB_LIGHTGREEN, BB_BLACK);
  t = bb_frames;
  for (i = 0; i < 40; ++i) bb_blit(168 + (i % 8) * 18, 24 + (i / 8) * 26, ball[(i % 8) * 2 & 7], BALL_W, BALL_H);
  frames_taken = bb_frames - t;
  blit_frames = frames_taken;
  bb_btext(21, 22, "BLIT");
  bnum(26, 22, frames_taken);
  bb_btext(30, 22, "FRAMES");
  wait_fire();
}

/* ── 3: bouncing balls ─────────────────────────────────────────────────── */

int bx[BALLS], by[BALLS];
signed char bdx[BALLS], bdy[BALLS];

static void draw_ball(unsigned char i) {
  /* the flip pen: drawn once it shows, drawn again it's gone, the pattern
   * under it as it was */
  bb_blit(bx[i] & ~7, by[i], ball[bx[i] & 7], BALL_W, BALL_H);
}

static void balls(void) {
  unsigned char i;
  int ux = 0;
  bb_bitmap(BB_YELLOW, BB_BROWN);
  heading("BOUNCING BALLS");
  /* a pattern to bounce over */
  bb_pen(BB_DRAW);
  for (i = 0; i < 20; ++i) bb_rect(i * 8, 8 + i * 4, 320 - i * 16, 184 - i * 8);
  bb_pen(BB_FLIP);
  for (i = 0; i < BALLS; ++i) {
    bx[i] = 20 + i * 34;
    by[i] = 20 + (i * 37) % 140;
    bdx[i] = (i & 1) ? 1 : -2;
    bdy[i] = (i & 2) ? 2 : -1;
    draw_ball(i);
  }
  for (;;) {
    bb_frame();
    for (i = 0; i < BALLS; ++i) {
      draw_ball(i);                       /* off: flipped back */
      bx[i] += bdx[i];
      by[i] += bdy[i];
      if (bx[i] < 2 || bx[i] > 300) bdx[i] = -bdx[i], bb_sfx(SFX_PING);
      if (by[i] < 10 || by[i] > 174) bdy[i] = -bdy[i];
      draw_ball(i);                       /* on again, moved */
    }
    /* a hardware sprite over the bitmap */
    if (++ux > 340) ux = -24;
    bb_spr(0, SPR_UFO, ux, 60 + (bb_rnd() & 1));
    if (bb_btnp(BB_FIRE)) break;
  }
  bb_spr_hide(0);
  bb_pen(BB_DRAW);
}

/* ── 4: peek and poke ───────────────────────────────────────────────────── */

static void memory(void) {
  unsigned char c = 0;
  bb_textmode();
  bb_text(1, 1, "PEEK AND POKE", BB_YELLOW);
  bb_text(1, 4, "BB_PEEK(0XD012), THE RASTER LINE:", BB_GREY);
  bb_text(1, 8, "BB_PEEK(0XDC00), JOYSTICK 2 (RAW):", BB_GREY);
  bb_text(1, 12, "BB_POKE(0XD020, C), THE BORDER:", BB_GREY);
  bb_text(1, 23, "FIRE: BACK TO THE START", BB_LIGHTBLUE);
  for (;;) {
    bb_frame();                           /* (first: the press that got us here is old news) */
    if (bb_btnp(BB_FIRE)) break;
    bb_num(3, 5, bb_peek(0xd012), 3, BB_WHITE);
    bb_num(3, 9, bb_peek(0xdc00), 3, BB_WHITE);
    if ((bb_frames & 15) == 0) bb_poke(0xd020, ++c & 15);
    bb_num(3, 13, bb_peek(0xd020) & 15, 2, BB_WHITE);
  }
  bb_poke(0xd020, BB_BLACK);
}

int main(void) {
  bb_init();
  bb_colors(BB_BLACK, BB_BLACK, BB_DARKGREY, BB_GREY);
  for (;;) {
    shapes();
    race();
    balls();
    memory();
  }
}
