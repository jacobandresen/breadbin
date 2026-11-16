/* bb.c - the breadbin Studio runtime (see bb.h). The assets it shows come
 * from gen/assets.c, which the Studio writes from the SPRITES, TILES and MAP
 * editors. The calls a game makes many times a frame (sprites, tiles,
 * overlaps) are in assembly, in bbfast.s; the music player and the frame
 * interrupt are in bbsound.s and bbirq.s. */

#include "bb.h"

#define POKE(a, v) (*(volatile unsigned char *)(a) = (v))
#define PEEK(a) (*(volatile unsigned char *)(a))

#define SCREEN ((unsigned char *)0xc000)
#define CHARSET ((unsigned char *)0xc800)
#define SPRITES ((unsigned char *)0xd000)
#define COLOR ((unsigned char *)0xd800)
#define VIC ((volatile unsigned char *)0xd000)

/* From gen/assets.c. */
extern const unsigned char bb_sprite_data[];
extern const unsigned char bb_sprite_count;
extern const unsigned char bb_sprite_mc[2];
extern const unsigned char bb_charset[2048];
extern const unsigned char bb_tile_color[256];
extern const unsigned char bb_screen_colors[4];
extern const unsigned char bb_multicolor_tiles;
extern const unsigned char *const bb_rooms[];
extern const unsigned char bb_room_count;

/* From bbirq.s. */
void bb_irq_on(void);

unsigned char bb_joy_now, bb_joy_was;

/* ── the tape: the joystick, frame by frame, for the Studio ────────────────
 * Every bb_frame()'s joystick bits are kept here, run-length coded (pairs of
 * bits and a count of frames, 1-255), with the random seed bb_init used:
 * everything that makes one play differ from another. The Studio reads it to
 * play a run again: it writes it into the next build's program (these are
 * initialised, so they are in the PRG, not cleared at start) with
 * bb_tape_mode 2, and the game then takes its joystick and its seed from the
 * tape until the tape ends, and records on from there (mode 1 again). With
 * the emulator at warp speed that brings the new build to the same place in
 * the game: edit and continue. Mode 0: neither (the tape full). */
unsigned char bb_tape_mode = 1;
unsigned int bb_tape_len = 0;     /* bytes used */
unsigned int bb_tape_pos = 0;     /* replaying: the next pair */
unsigned char bb_tape_left = 0;   /* replaying: frames left of that pair */
unsigned int bb_tape_seed = 0;
unsigned int bb_tape_frames = 0;  /* bb_frame()s since bb_init */
unsigned char bb_tape[BB_TAPE_SIZE] = { 0 };

static unsigned char tape_rec(unsigned char live) {
  if (bb_tape_len >= 2 && bb_tape[bb_tape_len - 2] == live && bb_tape[bb_tape_len - 1] < 255) ++bb_tape[bb_tape_len - 1];
  else if (bb_tape_len + 2 <= BB_TAPE_SIZE) {
    bb_tape[bb_tape_len] = live;
    bb_tape[bb_tape_len + 1] = 1;
    bb_tape_len += 2;
  } else bb_tape_mode = 0; /* full: no more recording */
  return live;
}

static unsigned char tape(unsigned char live) {
  ++bb_tape_frames;
  if (bb_tape_mode == 2) {
    if (bb_tape_left == 0) {
      if (bb_tape_pos >= bb_tape_len) {
        bb_tape_mode = 1; /* the end of the tape: recording on from here */
        return tape_rec(live);
      }
      bb_tape_left = bb_tape[bb_tape_pos + 1];
      bb_tape_pos += 2;
    }
    --bb_tape_left;
    return bb_tape[bb_tape_pos - 2];
  }
  return bb_tape_mode == 1 ? tape_rec(live) : live;
}

/* The screen address of each row: cy * 40, without a multiply. */
static const unsigned int row[25] = {
  0, 40, 80, 120, 160, 200, 240, 280, 320, 360, 400, 440, 480,
  520, 560, 600, 640, 680, 720, 760, 800, 840, 880, 920, 960,
};
static void copy(unsigned char *to, const unsigned char *from, unsigned int n) {
  while (n--) *to++ = *from++;
}

void bb_colors(unsigned char border, unsigned char bg, unsigned char mc1, unsigned char mc2) {
  VIC[0x20] = border;
  VIC[0x21] = bg;
  VIC[0x22] = mc1;
  VIC[0x23] = mc2;
}

void bb_init(void) {
  /* VIC bank 3 ($C000-$FFFF): CIA2 port A bits 0-1 = %00 */
  POKE(0xdd02, PEEK(0xdd02) | 3);
  POKE(0xdd00, PEEK(0xdd00) & 0xfc);
  /* screen at $C000 (+$0000), character set at $C800 (+$0800) */
  VIC[0x18] = 0x02;
  /* the KERNAL's screen editor writes to $C000 too, should anything print */
  POKE(0x0288, 0xc0);
  /* the keyboard matrix lines high, so $DC00 reads joystick 2 alone */
  POKE(0xdc00, 0xff);

  copy(CHARSET, bb_charset, 2048);
  /* The sprite images go to $D000, in the RAM under the I/O chips (the VIC
   * reads that RAM; the bitmap has $E000). The CPU only reaches it with the
   * I/O switched out ($01 = $34): interrupts off meanwhile, as the KERNAL is
   * out then too. */
  {
    unsigned char port = PEEK(1);
    __asm__("sei");
    POKE(1, (port & 0xf8) | 0x04);
    copy(SPRITES, bb_sprite_data, (unsigned int)bb_sprite_count * 64);
    POKE(1, port);
    __asm__("cli");
  }

  VIC[0x15] = 0;                       /* all sprites off */
  VIC[0x17] = 0;                       /* no double height */
  VIC[0x1d] = 0;                       /* no double width */
  VIC[0x1b] = 0;                       /* sprites in front of the tiles */
  VIC[0x25] = bb_sprite_mc[0];
  VIC[0x26] = bb_sprite_mc[1];
  /* multicolour character mode when any tile uses it, 40 columns */
  VIC[0x16] = bb_multicolor_tiles ? 0x18 : 0x08;
  bb_colors(bb_screen_colors[0], bb_screen_colors[1], bb_screen_colors[2], bb_screen_colors[3]);
  bb_cls(32, BB_WHITE);
  /* the seed: the tape's when it is being played, else the raster's (and kept on the tape) */
  if (bb_tape_mode == 2) bb_seed(bb_tape_seed);
  else {
    bb_tape_seed = VIC[0x12] | ((unsigned int)PEEK(0xdc04) << 8);
    bb_seed(bb_tape_seed);
  }
  bb_tape_frames = 0;
  /* A fixed start: the frame interrupt begins on the raster's line 0 with the
   * frame count at 0, however long the C64 took to get here (the Studio's
   * RUN arrives at a different moment each time). A run played again from
   * the tape then meets its frames as the first one did. */
  while (VIC[0x12] != 0 || (VIC[0x11] & 0x80)) {}
  bb_irq_on();
  bb_frames = 0;
  bb_joy_now = bb_joy_was = 0;
}

unsigned char bb_joy(void) {
  /* both ports, active low: bits 0-4 up, down, left, right, fire */
  return ~(PEEK(0xdc00) & PEEK(0xdc01)) & 0x1f;
}

/* The Studio's frame meter: the raster line the game called bb_frame() at
 * (its frame's work done; the frame began at line 250, with the interrupt),
 * and how many times it was late (the work took more than a frame, so the
 * game ran at half speed for a frame). */
unsigned int bb_busy_line;
unsigned char bb_busy_late;
static unsigned char last_frame;

void bb_frame(void) {
  unsigned char f = (unsigned char)bb_frames;
  bb_busy_line = VIC[0x12] | ((unsigned int)(VIC[0x11] & 0x80) << 1);
  if (f != last_frame) ++bb_busy_late; /* the interrupt came while the game worked: a frame missed */
  while ((unsigned char)bb_frames == f) {}
  last_frame = (unsigned char)bb_frames;
  bb_joy_was = bb_joy_now;
  bb_joy_now = tape(bb_joy());
}

/* ── rooms ───────────────────────────────────────────────────────────────── */

/* A room is run-length coded, row by row (40x25 cells): a byte is a tile,
 * except $FF, which is followed by a count and the tile to repeat. */
void __fastcall__ bb_room(unsigned char n) {
  const unsigned char *p;
  unsigned char *s = SCREEN;
  unsigned char *c = COLOR;
  unsigned int left = 1000;
  unsigned char t, k;
  if (n >= bb_room_count) return;
  p = bb_rooms[n];
  while (left) {
    t = *p++;
    k = 1;
    if (t == 0xff) {
      k = *p++;
      t = *p++;
    }
    if (k > left) k = (unsigned char)left;
    left -= k;
    while (k--) {
      *s++ = t;
      *c++ = bb_tile_color[t];
    }
  }
}

/* ── text ──────────────────────────────────────────────────────────────── */

/* A string's character to the screen code of its font tile. cc65 stores
 * string literals in PETSCII: "a" is $41 and "A" is $C1, both the letter A,
 * screen code 1. @A-Z[\]^_ are screen codes 0..31, space to ? are 32..63. */
static unsigned char code(unsigned char ch) {
  if (ch == 0xa4) return 31;           /* "_": cc65 writes it as PETSCII $A4 */
  if (ch >= 0xc0) return ch - 0xc0;    /* PETSCII capitals ("A" in C) */
  if (ch >= 0x60) return ch & 0x1f;    /* the graphics range: as letters */
  if (ch >= 0x40) return ch - 0x40;    /* PETSCII letters ("a" in C), @ [ ] */
  return ch;                           /* space, digits, punctuation */
}

void __fastcall__ bb_text(unsigned char cx, unsigned char cy, const char *s, unsigned char color) {
  unsigned int o = row[cy] + cx;
  while (*s && cx < 40) {
    SCREEN[o] = code(*s++);
    COLOR[o] = color;
    ++o;
    ++cx;
  }
}

void __fastcall__ bb_num(unsigned char cx, unsigned char cy, unsigned int n, unsigned char digits, unsigned char color) {
  char buf[6];
  unsigned char i = digits > 5 ? 5 : digits;
  buf[i] = 0;
  while (i) {
    buf[--i] = '0' + (unsigned char)(n % 10);
    n /= 10;
  }
  bb_text(cx, cy, buf, color);
}

void __fastcall__ bb_cls(unsigned char t, unsigned char color) {
  unsigned char *s = SCREEN;
  unsigned char *c = COLOR;
  unsigned int n = 1000;
  while (n--) {
    *s++ = t;
    *c++ = color;
  }
}

/* ── random numbers: a 16-bit xorshift ─────────────────────────────────── */

static unsigned int rnd = 0x2a5f;

void __fastcall__ bb_seed(unsigned int s) {
  rnd = s ? s : 0x2a5f;
}

unsigned char bb_rnd(void) {
  rnd ^= rnd << 7;
  rnd ^= rnd >> 9;
  rnd ^= rnd << 8;
  return (unsigned char)rnd;
}
