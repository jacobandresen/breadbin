/* Bouncer - bricks, a bat and a ball, for the C64. After blip's Bouncer.
 * Made with the breadbin Studio.
 *
 *   joystick left/right  move the bat        fire  serve the ball
 *
 * Break every coloured brick (steel ones don't break) to clear the level.
 * A broken brick sometimes drops a pickup: catch it with the bat.
 *   W  a wide bat for a while       M  three balls
 *   S  a slower ball for a while    L  a life
 *
 * The levels are rooms in the MAP editor: paint bricks into them (each brick
 * is a left and a right tile, from TILES), and the game counts them. */

#include "bb.h"
#include "assets.h"

#define SLOT_BAT    0              /* and 1, when the bat is wide */
#define SLOT_BALL   2              /* 2..4 */
#define SLOT_PICKUP 5              /* 5..7 */
#define BALLS 3
#define PICKUPS 3

#define LEFT_WALL   24             /* inside the frame: x 24..295 */
#define RIGHT_WALL  296
#define BAT_Y       180            /* the bat's sprite; its top is 2 rows lower */
#define BALL 8                     /* the ball is 8x8, in the sprite's corner */

/* positions in 16ths of a pixel, so the ball can go at any angle */
static int ballx[BALLS], bally[BALLS];
static signed char ballvx[BALLS], ballvy[BALLS];
static unsigned char ballon[BALLS];
static int pickx[PICKUPS], picky[PICKUPS];
static unsigned char pickon[PICKUPS], pickimg[PICKUPS];

static int batx = 148;
static unsigned char batw = 24;    /* 24 or 48 pixels */
static unsigned int wide_time, slow_time;
static unsigned char served, lives, level, bricks;
static unsigned int score;         /* in tens */

static void hud(void) {
  bb_text(3, 0, "SCORE", BB_YELLOW);
  bb_num(9, 0, score, 5, BB_WHITE);
  bb_text(14, 0, "0", BB_WHITE);
  bb_text(18, 0, "LEVEL", BB_YELLOW);
  bb_num(24, 0, level + 1, 2, BB_WHITE);
  bb_text(29, 0, "LIVES", BB_YELLOW);
  bb_num(35, 0, lives, 1, BB_WHITE);
}

static void add_score(unsigned char tens) {
  score += tens;
  bb_num(9, 0, score, 5, BB_WHITE);
}

/* Draw the level's room and count the bricks to break (left halves). */
static void start_level(void) {
  unsigned char cx, cy;
  bb_room(level % ROOM_COUNT);
  bricks = 0;
  for (cy = 2; cy < 25; ++cy)
    for (cx = 3; cx < 37; cx += 2)
      if ((bb_fget(bb_tile(cx, cy)) & (FLAG_BRICK | FLAG_STEEL)) == FLAG_BRICK) ++bricks;
  hud();
}

static void serve(void) {
  unsigned char i;
  for (i = 0; i < BALLS; ++i) {
    ballon[i] = 0;
    bb_spr_hide(SLOT_BALL + i);
  }
  for (i = 0; i < PICKUPS; ++i) {
    pickon[i] = 0;
    bb_spr_hide(SLOT_PICKUP + i);
  }
  ballon[0] = 1;
  served = 0;
  wide_time = slow_time = 0;
  batw = 24;
}

static void launch(unsigned char i, signed char vx) {
  ballon[i] = 1;
  ballvx[i] = vx;
  ballvy[i] = -32;
}

/* The ball hit the tile in cell cx, cy: break it if it is a brick. */
static unsigned char hit(unsigned char cx, unsigned char cy) {
  unsigned char f = bb_fget(bb_tile(cx, cy));
  unsigned char i;
  if (!(f & FLAG_SOLID)) return 0;
  if (f & FLAG_STEEL) {
    bb_sfx(SFX_STEEL);
    return 1;
  }
  if (f & FLAG_BRICK) {
    if ((cx - 3) & 1) --cx;        /* the right half: the brick starts one to the left */
    bb_set_tile(cx, cy, 32);
    bb_set_tile(cx + 1, cy, 32);
    --bricks;
    add_score(5);
    bb_sfx(SFX_BRICK);
    /* now and then, a pickup falls out */
    if (bb_rnd() < 40) {
      for (i = 0; i < PICKUPS; ++i) {
        if (pickon[i]) continue;
        pickon[i] = 1;
        pickx[i] = cx * 8 - 2;
        picky[i] = cy * 8;
        pickimg[i] = SPR_PICKUP_W + (bb_rnd() & 3);
        break;
      }
    }
    return 1;
  }
  bb_sfx(SFX_BAT);                 /* the frame */
  return 1;
}

static void move_ball(unsigned char i) {
  int x, y, nx, ny;
  signed char vx = ballvx[i], vy = ballvy[i];
  if (slow_time) {
    vx /= 2;
    vy /= 2;
  }
  x = ballx[i] >> 4;
  y = bally[i] >> 4;
  /* across: a tile on the leading edge turns it round */
  nx = (ballx[i] + vx) >> 4;
  if (hit((vx > 0 ? nx + BALL - 1 : nx) >> 3, (y + BALL / 2) >> 3)) ballvx[i] = -ballvx[i];
  else ballx[i] += vx;
  /* up and down, the same */
  x = ballx[i] >> 4;
  ny = (bally[i] + vy) >> 4;
  if (ny >= 8 && hit((x + BALL / 2) >> 3, (vy > 0 ? ny + BALL - 1 : ny) >> 3)) ballvy[i] = -ballvy[i];
  else bally[i] += vy;
  y = bally[i] >> 4;
  /* the bat: the further from its middle, the steeper the bounce */
  if (ballvy[i] > 0 && y + BALL >= BAT_Y + 2 && y + BALL <= BAT_Y + 8 && x + BALL > batx && x < batx + batw) {
    ballvy[i] = -ballvy[i];
    ballvx[i] = (signed char)(((x + BALL / 2) - (batx + batw / 2)) * (batw == 24 ? 3 : 2) / 2);
    if (ballvx[i] > 36) ballvx[i] = 36;
    if (ballvx[i] < -36) ballvx[i] = -36;
    if (ballvx[i] > -6 && ballvx[i] < 6) ballvx[i] = ballvx[i] < 0 ? -6 : 6;
    bb_sfx(SFX_BAT);
  }
  if (y > 200) {
    ballon[i] = 0;
    bb_spr_hide(SLOT_BALL + i);
    return;
  }
  bb_spr(SLOT_BALL + i, SPR_BALL, x, y);
}

static void pickups(void) {
  unsigned char i, j;
  for (i = 0; i < PICKUPS; ++i) {
    if (!pickon[i]) continue;
    ++picky[i];
    if (picky[i] > 200) {
      pickon[i] = 0;
      bb_spr_hide(SLOT_PICKUP + i);
      continue;
    }
    if (bb_overlap(pickx[i], picky[i], 24, 10, batx, BAT_Y + 2, batw, 6)) {
      pickon[i] = 0;
      bb_spr_hide(SLOT_PICKUP + i);
      bb_sfx(SFX_PICKUP);
      add_score(10);
      if (pickimg[i] == SPR_PICKUP_W) wide_time = 750, batw = 48;
      else if (pickimg[i] == SPR_PICKUP_S) slow_time = 500;
      else if (pickimg[i] == SPR_PICKUP_L && lives < 9) ++lives, hud();
      else if (pickimg[i] == SPR_PICKUP_M) {
        /* two more balls, from the first one still in play */
        for (j = 0; j < BALLS && !ballon[j]; ++j) {}
        if (j < BALLS && served) {
          unsigned char k;
          for (k = 0; k < BALLS; ++k) {
            if (ballon[k]) continue;
            ballx[k] = ballx[j];
            bally[k] = bally[j];
            launch(k, k & 1 ? -20 : 20);
          }
        }
      }
      continue;
    }
    bb_spr(SLOT_PICKUP + i, pickimg[i], pickx[i], picky[i]);
  }
}

static void wait_fire(void) {
  do bb_frame(); while (!bb_btnp(BB_FIRE));
}

int main(void) {
  unsigned char i, any;
  bb_init();
  for (;;) {
    score = 0;
    lives = 3;
    level = 0;
    start_level();
    bb_music(SONG_MARCH);
    bb_text(13, 14, "PRESS FIRE", BB_WHITE);
    wait_fire();
    bb_text(13, 14, "          ", BB_WHITE);
    bb_music(BB_STOP);
    serve();
    while (lives) {
      bb_frame();
      /* the bat */
      if (bb_btn(BB_LEFT)) batx -= 3;
      if (bb_btn(BB_RIGHT)) batx += 3;
      if (batx < LEFT_WALL) batx = LEFT_WALL;
      if (batx > RIGHT_WALL - batw) batx = RIGHT_WALL - batw;
      if (wide_time && --wide_time == 0) batw = 24;
      if (slow_time) --slow_time;
      bb_spr(SLOT_BAT, SPR_BAT, batx, BAT_Y);
      if (batw > 24) bb_spr(SLOT_BAT + 1, SPR_BAT, batx + 24, BAT_Y);
      else bb_spr_hide(SLOT_BAT + 1);

      if (!served) {
        /* the ball waits on the bat */
        ballx[0] = (batx + batw / 2 - BALL / 2) << 4;
        bally[0] = (BAT_Y + 2 - BALL) << 4;
        bb_spr(SLOT_BALL, SPR_BALL, batx + batw / 2 - BALL / 2, BAT_Y + 2 - BALL);
        if (bb_btnp(BB_FIRE)) {
          served = 1;
          launch(0, bb_rnd() & 1 ? 16 : -16);
        }
        continue;
      }
      any = 0;
      for (i = 0; i < BALLS; ++i) if (ballon[i]) (move_ball(i), any |= ballon[i]);
      pickups();
      if (!any) {
        --lives;
        hud();
        bb_sfx(SFX_LOST);
        serve();
      }
      if (!bricks) {
        bb_sfx(SFX_CLEAR);
        for (i = 0; i < 100; ++i) bb_frame();
        ++level;
        start_level();
        serve();
      }
    }
    bb_text(15, 14, "GAME OVER", BB_RED);
    wait_fire();
  }
}
