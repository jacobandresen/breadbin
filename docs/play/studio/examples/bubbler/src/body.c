/* body.c - moving a body (the player or a monster) against the tiles.
 * Platforms are one-way, as in the arcade games: you jump up through them
 * and land on top. Walls (tiles flagged "wall") stop you from the side. The
 * body to move is copied into these globals first (fast on a 6502). */

#include "game.h"

int bx_, by_, byp_;
signed char bvy_;
unsigned char bground_;

/* Is there floor under pixel row `feet`, below either foot? */
static unsigned char floor_at(int feet) {
  return (bb_flags_at(bx_ + BOX_X + 1, feet) | bb_flags_at(bx_ + BOX_X + BOX_W - 2, feet)) & FLAG_SOLID;
}

/* Gravity, and landing on a platform when coming down onto it. */
void body_fall(void) {
  int old_feet = byp_ + FEET;
  int feet, top;
  if (bvy_ < MAX_FALL) bvy_ += GRAVITY;
  by_ += bvy_;
  byp_ = by_ >> SUB;
  feet = byp_ + FEET;
  bground_ = 0;
  if (bvy_ >= 0 && feet >= 8) {
    top = (feet + 1) & ~7;                  /* the top of the tile row under the feet */
    if (old_feet < top && feet + 1 >= top && floor_at(feet + 1)) {
      byp_ = top - 1 - FEET;
      by_ = byp_ << SUB;
      bvy_ = 0;
      bground_ = 1;
    }
  }
  /* through the hole in the floor, and in again at the top */
  if (byp_ > BOTTOM) {
    byp_ = -16;
    by_ = -16 << SUB;
  }
}

/* Would moving dx pixels sideways walk into a wall? (Walls are whole
 * columns of tiles, so one look, at the middle of the body, is enough.) */
unsigned char body_blocked(signed char dx) {
  int x = dx > 0 ? bx_ + BOX_X + BOX_W - 1 + dx : bx_ + BOX_X + dx;
  return bb_flags_at(x, byp_ + 12) & FLAG_WALL;
}

/* Standing, is there no floor a step ahead (an edge to fall off)? */
unsigned char body_edge(signed char dx) {
  int feet = byp_ + FEET + 1;
  int x = dx > 0 ? bx_ + BOX_X + BOX_W : bx_ + BOX_X - 1;
  return !(bb_flags_at(x, feet) & FLAG_SOLID);
}
