/* player.c - the bubble dragon: run, jump (up), blow a bubble (fire). */

#include "game.h"

int px, py, py_px;
signed char pvy;
unsigned char pleft, palive, phurt;
static unsigned char step, ground, dying;

/* Back at the start of the round, safe for a moment. */
void player_start(void) {
  px = start_x;
  py_px = start_y;
  py = start_y << SUB;
  pvy = 0;
  pleft = 0;
  palive = 1;
  phurt = 100;                 /* frames of not being hurt */
  dying = 0;
}

void player_hit(void) {
  if (phurt || dying) return;
  dying = 90;
  bb_sfx(SFX_OUCH);
}

void player_update(void) {
  unsigned char img;
  if (dying) {
    /* spin, then the next life (or the end) */
    bb_spr(SLOT_PLAYER, (dying & 8) ? SPR_BUB_HURT : SPR_BUB_HURT_FLIP, px, py_px);
    if (--dying == 0) {
      if (lives) --lives;
      show_lives();
      if (lives) player_start();
      else palive = 0;
    }
    return;
  }
  if (phurt) --phurt;

  bx_ = px;
  by_ = py;
  byp_ = py_px;
  bvy_ = pvy;
  if (bb_btn(BB_LEFT)) {
    pleft = 1;
    if (!body_blocked(-1)) --bx_;
    ++step;
  } else if (bb_btn(BB_RIGHT)) {
    pleft = 0;
    if (!body_blocked(1)) ++bx_;
    ++step;
  }
  if (bb_btnp(BB_UP) && ground) {
    bvy_ = JUMP;
    bb_sfx(SFX_JUMP);
  }
  body_fall();
  ground = bground_;
  px = bx_;
  py = by_;
  pvy = bvy_;
  py_px = byp_;

  if (bb_btnp(BB_FIRE)) bubble_blow();

  /* the picture: walking, jumping or blowing, facing the way it goes */
  if (bubble_on && bubble_on > 14) img = SPR_BUB_BLOW;
  else if (!ground) img = SPR_BUB_JUMP;
  else img = (step & 8) ? SPR_BUB_WALK : SPR_BUB_STAND;
  if (pleft) ++img;            /* every bub image has its mirror right after it */
  if (phurt && (phurt & 4)) bb_spr_hide(SLOT_PLAYER);
  else bb_spr(SLOT_PLAYER, img, px, py_px);
}
