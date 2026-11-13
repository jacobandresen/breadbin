/* bubble.c - the bubble the dragon blows: it flies straight for a moment,
 * and a monster it touches while flying is caught inside. */

#include "game.h"

unsigned char bubble_on;       /* frames left of its flight, 0 = none */
int bx, by;
static signed char bdx;

void bubble_blow(void) {
  if (bubble_on) return;
  bubble_on = 24;
  bdx = pleft ? -4 : 4;
  bx = px + (pleft ? -12 : 12);
  by = py_px - 1;
  bb_sfx(SFX_BLOW);
}

void bubble_update(void) {
  unsigned char i;
  if (!bubble_on) {
    bb_spr_hide(SLOT_BUBBLE);
    return;
  }
  --bubble_on;
  /* slows down as it goes; stops at walls */
  if (bubble_on > 8) {
    bx += bdx;
    if (bx < 8 || bx > 296) bubble_on = 8;
  }
  if (bubble_on == 0) {
    bb_spr_hide(SLOT_BUBBLE);
    return;
  }
  bb_spr(SLOT_BUBBLE, bubble_on > 4 ? SPR_BUBBLE : SPR_BUBBLE_POP, bx, by);
  /* caught! */
  for (i = 0; i < MAX_ENEMIES; ++i) {
    if (estate[i] >= E_WALK && estate[i] <= E_GHOST && NEAR(ex[i] - bx, eyp[i] - by, 14, 14)) {
      estate[i] = E_TRAPPED;
      bubble_on = 0;
      bb_spr_hide(SLOT_BUBBLE);
      bb_sfx(SFX_CATCH);
      add_score(10);
      return;
    }
  }
}
