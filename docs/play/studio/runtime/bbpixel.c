/* bbpixel.c - the breadbin Studio runtime's pixels (see bb.h): the bitmap
 * mode, lines, rectangles, circles, blits and text in the bitmap. The inner
 * loops are in assembly, in bbdraw.s. The Studio adds these two files to a
 * project only when its code calls one of the pixel functions, so a game of
 * tiles and sprites doesn't carry them. */

#include "bb.h"

#define SCREEN ((unsigned char *)0xc000)
#define BITMAP ((unsigned char *)0xe000)
#define VIC ((volatile unsigned char *)0xd000)

extern const unsigned char bb_multicolor_tiles;

static const unsigned int row[25] = {
  0, 40, 80, 120, 160, 200, 240, 280, 320, 360, 400, 440, 480,
  520, 560, 600, 640, 680, 720, 760, 800, 840, 880, 920, 960,
};

/* A string's character to the screen code of its font tile (as in bb.c). */
static unsigned char code(unsigned char ch) {
  if (ch == 0xa4) return 31;           /* "_" (PETSCII $A4 in cc65) */
  if (ch >= 0xc0) return ch - 0xc0;
  if (ch >= 0x60) return ch & 0x1f;
  if (ch >= 0x40) return ch - 0x40;
  return ch;
}

/* From bbdraw.s: its arguments, and the loops that use them. */
extern unsigned int bbd_x0, bbd_x1;
extern unsigned char bbd_y, bbd_cx, bbd_wb, bbd_h;
extern const unsigned char *bbd_src;
void bbd_span(void);
void bbd_vspan(void);
void bbd_blit(void);
void bbd_grab(void);
void __fastcall__ bbd_stride(unsigned char w);

void __fastcall__ bb_bitmap(unsigned char ink, unsigned char paper) {
  unsigned char *p = BITMAP;
  unsigned int n = 8000;
  while (n--) *p++ = 0;                /* writes reach the RAM under the KERNAL */
  p = SCREEN;                          /* the screen holds each cell's two colours */
  n = 1000;
  while (n--) *p++ = (ink << 4) | (paper & 15);
  VIC[0x16] &= ~0x10;                  /* hires */
  VIC[0x18] = 0x08;                    /* colours at $C000, the bitmap at $E000 */
  /* bitmap mode. (Bit 7 of $D011 must be written 0: read, it is the current
   * raster line's bit 8; written, it is the raster interrupt's. Written back
   * as read low on the screen, the frame interrupt would move to a line that
   * doesn't exist, and bb_frame() would wait for ever.) */
  VIC[0x11] = (VIC[0x11] & 0x7f) | 0x20;
}

void bb_textmode(void) {
  VIC[0x11] = VIC[0x11] & 0x5f;        /* not bitmap mode (and bit 7 0, as above) */
  VIC[0x18] = 0x02;
  VIC[0x16] = bb_multicolor_tiles ? 0x18 : 0x08;
  bb_cls(32, BB_WHITE);
}

/* A span from x0 to x1 on row y, clipped to the screen. */
static void span(int x0, int x1, int y) {
  int t;
  if (x0 > x1) {
    t = x0;
    x0 = x1;
    x1 = t;
  }
  if (y < 0 || y >= 200 || x1 < 0 || x0 >= 320) return;
  if (x0 < 0) x0 = 0;
  if (x1 > 319) x1 = 319;
  bbd_x0 = x0;
  bbd_x1 = x1;
  bbd_y = (unsigned char)y;
  bbd_span();
}

/* A column from y0 to y1 at x, clipped to the screen. */
static void vspan(int x, int y0, int y1) {
  int t;
  if (y0 > y1) {
    t = y0;
    y0 = y1;
    y1 = t;
  }
  if (x < 0 || x >= 320 || y1 < 0 || y0 >= 200) return;
  if (y0 < 0) y0 = 0;
  if (y1 > 199) y1 = 199;
  bbd_x0 = x;
  bbd_y = (unsigned char)y0;
  bbd_h = (unsigned char)(y1 - y0 + 1);
  bbd_vspan();
}

void __fastcall__ bb_line(int x0, int y0, int x1, int y1) {
  /* Bresenham's: one pixel a step along the longer axis */
  int dx = x1 - x0, dy = y1 - y0, err, e2;
  signed char sx = 1, sy = 1;
  if (dy == 0) {
    span(x0, x1, y0);                  /* flat: a span, a byte at a time */
    return;
  }
  if (dx == 0) {
    vspan(x0, y0, y1);                 /* upright: a column, a row at a time */
    return;
  }
  if (dx < 0) dx = -dx, sx = -1;
  if (dy < 0) dy = -dy, sy = -1;
  err = dx - dy;
  for (;;) {
    bb_plot(x0, y0);
    if (x0 == x1 && y0 == y1) return;
    e2 = err << 1;
    if (e2 > -dy) {
      err -= dy;
      x0 += sx;
    }
    if (e2 < dx) {
      err += dx;
      y0 += sy;
    }
  }
}

void __fastcall__ bb_fill(int x, int y, int w, int h) {
  int r;
  if (w <= 0) return;
  for (r = 0; r < h; ++r) span(x, x + w - 1, y + r);
}

void __fastcall__ bb_rect(int x, int y, int w, int h) {
  if (w <= 0 || h <= 0) return;
  span(x, x + w - 1, y);
  if (h > 1) span(x, x + w - 1, y + h - 1);
  if (h > 2) {
    vspan(x, y + 1, y + h - 2);
    if (w > 1) vspan(x + w - 1, y + 1, y + h - 2);
  }
}

/* The midpoint circle: eight points at a time. */
void __fastcall__ bb_circle(int cx, int cy, unsigned char r) {
  int x = r, y = 0, err = 1 - (int)r;
  while (x >= y) {
    bb_plot(cx + x, cy + y);
    bb_plot(cx - x, cy + y);
    bb_plot(cx + x, cy - y);
    bb_plot(cx - x, cy - y);
    bb_plot(cx + y, cy + x);
    bb_plot(cx - y, cy + x);
    bb_plot(cx + y, cy - x);
    bb_plot(cx - y, cy - x);
    ++y;
    if (err < 0) {
      err += 2 * y + 1;
    } else {
      --x;
      err += 2 * (y - x) + 1;
    }
  }
}

/* A disc, a row at a time: each row once, so BB_FLIP flips it cleanly. The
 * half-width of row dy is the largest h with h*h + dy*dy <= r*r; it only
 * shrinks as dy grows, so it is found by stepping down, no square roots. */
void __fastcall__ bb_disc(int cx, int cy, unsigned char r) {
  unsigned int rr = (unsigned int)r * r;
  unsigned int h = r;
  unsigned int dy;
  for (dy = 0; dy <= r; ++dy) {
    while (h * h + dy * dy > rr) --h;
    span(cx - (int)h, cx + (int)h, cy + (int)dy);
    if (dy) span(cx - (int)h, cx + (int)h, cy - (int)dy);
  }
}

void __fastcall__ bb_ink(unsigned char cx, unsigned char cy, unsigned char w, unsigned char h,
                         unsigned char ink, unsigned char paper) {
  unsigned char c = (ink << 4) | (paper & 15), i;
  unsigned char *p;
  for (; h && cy < 25; --h, ++cy) {
    p = SCREEN + row[cy] + cx;
    for (i = 0; i < w && cx + i < 40; ++i) p[i] = c;
  }
}

/* Clip a blit or grab to the screen, set bbdraw.s's arguments; false if nothing shows. */
static unsigned char blitclip(int x, int y, const unsigned char *src, unsigned char wb, unsigned char h) {
  int cx = x >> 3;
  bbd_stride(wb);
  if (y < 0) {                         /* rows above the top: skip them */
    if (-y >= h) return 0;
    src += (unsigned int)(-y) * wb;
    h -= (unsigned char)(-y);
    y = 0;
  }
  if (cx < 0) {                        /* bytes left of the screen: skip them */
    if (-cx >= wb) return 0;
    src -= cx;
    wb += (signed char)cx;
    cx = 0;
  }
  if (y >= 200 || cx >= 40 || !wb || !h) return 0;
  if (y + h > 200) h = (unsigned char)(200 - y);
  if (cx + wb > 40) wb = (unsigned char)(40 - cx);
  bbd_src = src;
  bbd_cx = (unsigned char)cx;
  bbd_y = (unsigned char)y;
  bbd_wb = wb;
  bbd_h = h;
  return 1;
}

void __fastcall__ bb_blit(int x, int y, const unsigned char *src, unsigned char wb, unsigned char h) {
  if (blitclip(x, y, src, wb, h)) bbd_blit();
}

void __fastcall__ bb_grab(int x, int y, unsigned char *dst, unsigned char wb, unsigned char h) {
  if (blitclip(x, y, dst, wb, h)) bbd_grab();
}

void __fastcall__ bb_btext(unsigned char cx, unsigned char cy, const char *s) {
  while (*s && cx < 40) {
    bb_blit(cx << 3, cy << 3, bb_tile_picture(code(*s++)), 1, 8);
    ++cx;
  }
}

