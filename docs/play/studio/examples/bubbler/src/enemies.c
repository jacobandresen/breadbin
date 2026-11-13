/* enemies.c - the monsters: robots walk the platforms, bunnies hop after
 * you, ghosts float through everything. A monster caught in a bubble floats
 * up; touch it and it pops into a cherry. Leave it too long and it breaks
 * out, angry (and faster).
 *
 * A 6502 at 1 MHz has about 20000 cycles a frame, and a monster in C costs
 * a few thousand: so each monster thinks every other frame (half of them on
 * odd frames, half on even), and moves twice as far when it does. */

#include "game.h"

unsigned char ekind[MAX_ENEMIES], estate[MAX_ENEMIES];
int ex[MAX_ENEMIES], ey[MAX_ENEMIES], eyp[MAX_ENEMIES];
static signed char evy[MAX_ENEMIES], edx[MAX_ENEMIES], edy[MAX_ENEMIES];
static unsigned int etime[MAX_ENEMIES];
static unsigned char eangry[MAX_ENEMIES], eground[MAX_ENEMIES];

#define TRAP_TIME 200          /* thinks: 8 seconds in the bubble */
#define FRUIT_TIME 150

void enemies_clear(void) {
  unsigned char i;
  for (i = 0; i < MAX_ENEMIES; ++i) {
    estate[i] = E_NONE;
    bb_spr_hide(SLOT_ENEMY + i);
  }
}

void enemy_add(unsigned char kind, int x, int y) {
  unsigned char i;
  for (i = 0; i < MAX_ENEMIES; ++i) {
    if (estate[i] != E_NONE) continue;
    ekind[i] = estate[i] = kind;
    ex[i] = x;
    eyp[i] = y;
    ey[i] = y << SUB;
    evy[i] = 0;
    edx[i] = (bb_rnd() & 1) ? 1 : -1;
    edy[i] = 1;
    etime[i] = bb_rnd() & 31;
    eangry[i] = 0;
    eground[i] = 0;
    return;
  }
}

/* Monsters still to catch (cherries don't count). */
unsigned char enemies_left(void) {
  unsigned char i, n = 0;
  for (i = 0; i < MAX_ENEMIES; ++i) if (estate[i] >= E_WALK && estate[i] <= E_TRAPPED) ++n;
  return n;
}

/* Copy monster i into the body globals (body.c) and back. */
static void take(unsigned char i) {
  bx_ = ex[i];
  by_ = ey[i];
  byp_ = eyp[i];
  bvy_ = evy[i];
  bground_ = eground[i];
}
static void put(unsigned char i) {
  ex[i] = bx_;
  ey[i] = by_;
  eyp[i] = byp_;
  evy[i] = bvy_;
  eground[i] = bground_;
}

/* Two frames of gravity, for a monster that thinks every other frame. */
static void fall2(void) {
  body_fall();
  if (!bground_) body_fall();
}

static void walk(unsigned char i) {
  /* a robot: along the platform, sometimes off the edge, sometimes up a level */
  signed char dx = edx[i];
  if (eangry[i] || round_no > 3) dx <<= 1;
  take(i);
  if (bground_) {
    if (body_blocked(dx) || (body_edge(dx) && (bb_rnd() & 3))) edx[i] = -edx[i];
    else bx_ += dx;
    /* the player is above: jump after them now and then */
    if (py_px + 16 < byp_ && bb_rnd() < 6) bvy_ = JUMP;
  } else if (!body_blocked(dx)) bx_ += dx;
  fall2();
  put(i);
}

static void hop(unsigned char i) {
  /* a bunny: waits, then jumps toward the player */
  take(i);
  if (bground_) {
    if (++etime[i] > (eangry[i] ? 15u : 35u)) {
      etime[i] = bb_rnd() & 7;
      edx[i] = px < bx_ ? -1 : 1;
      bvy_ = py_px < byp_ ? JUMP : JUMP / 2;
    }
  } else if (!body_blocked(edx[i] << 1)) bx_ += edx[i] << 1;
  fall2();
  put(i);
}

static void ghost(unsigned char i) {
  /* a ghost: floats diagonally, bouncing off the edges of the room */
  int y = eyp[i];
  signed char d = eangry[i] ? 2 : 1;
  ex[i] += edx[i] * d;
  y += edy[i] * d;
  if (ex[i] < 16 || ex[i] > 288) edx[i] = -edx[i];
  if (y < 12 || y > 168) edy[i] = -edy[i];
  eyp[i] = y;
  ey[i] = y << SUB;
}

static void trapped(unsigned char i) {
  /* in the bubble: up to the ceiling, wobbling, until it breaks out */
  if (eyp[i] > 14) ey[i] = --eyp[i] << SUB;
  ex[i] += (tick & 16) ? 0 : (tick & 2) ? 1 : -1;
  if (ex[i] < 16) ex[i] = 16;
  if (ex[i] > 288) ex[i] = 288;
  if (++etime[i] > TRAP_TIME) {
    estate[i] = ekind[i];
    eangry[i] = 1;
    etime[i] = 0;
    evy[i] = 0;
  }
}

static void fruit(unsigned char i) {
  take(i);
  if (!bground_) fall2();
  put(i);
  if (++etime[i] > FRUIT_TIME) estate[i] = E_NONE;
}

void enemies_update(void) {
  unsigned char i, st, img, slot;
  int y;
  for (i = tick & 1; i < MAX_ENEMIES; i += 2) {
    st = estate[i];
    slot = SLOT_ENEMY + i;
    if (st == E_NONE) {
      bb_spr_hide(slot);
      continue;
    }
    if (st == E_WALK) walk(i);
    else if (st == E_HOP) hop(i);
    else if (st == E_GHOST) ghost(i);
    else if (st == E_TRAPPED) trapped(i);
    else fruit(i);

    y = eyp[i];
    /* touching the player */
    if (palive && NEAR(ex[i] - px, y - py_px, BOX_W, BOX_H)) {
      if (st == E_TRAPPED) {
        estate[i] = E_FRUIT;
        etime[i] = 0;
        evy[i] = -20;
        eground[i] = 0;
        bb_sfx(SFX_POP);
        add_score(100);
      } else if (st == E_FRUIT) {
        estate[i] = E_NONE;
        bb_sfx(SFX_YUM);
        add_score(500);
        bb_spr_hide(slot);
        continue;
      } else player_hit();
    }

    /* the picture */
    if (st == E_TRAPPED) img = (tick & 16) ? SPR_TRAPPED : SPR_TRAPPED_WOBBLE;
    else if (st == E_FRUIT) img = SPR_CHERRY;
    else {
      img = ekind[i] == E_WALK ? ((tick & 16) ? SPR_ROBOT : SPR_ROBOT_STEP)
          : ekind[i] == E_HOP ? SPR_BUNNY : SPR_GHOST;
      if (edx[i] < 0) ++img;   /* the mirror */
    }
    bb_spr(slot, img, ex[i], y);
    if (eangry[i] && st != E_TRAPPED && st != E_FRUIT) bb_spr_color(slot, (tick & 4) ? BB_RED : BB_LIGHTGREY);
    /* a trapped monster blinks before it breaks out */
    if (st == E_TRAPPED && etime[i] > TRAP_TIME - 45 && (tick & 4)) bb_spr_color(slot, BB_RED);
  }
}
