/* Bubbler - a one-screen platformer for the C64, after blip's Bubbler
 * (and the arcade games it remembers). Made with the breadbin Studio.
 *
 *   joystick left/right  run        up    jump
 *   fire                 blow a bubble: catch a monster, then touch it
 *
 * Clear the room of monsters to go to the next round. The rooms are in the
 * MAP editor (ROOM_ROUND_1...), the monsters' starting places are marked
 * in them with the "spawn" tiles, which the game swaps for empty space. */

#include "game.h"

unsigned int score, best;       /* in tens of points: every score ends in 0 */
unsigned char lives, round_no;
unsigned char tick;
static unsigned char clear_timer;
int start_x = 24, start_y = 171;

void add_score(unsigned int points) {
  score += points / 10;
  bb_num(7, 0, score, 5, BB_WHITE);
  bb_text(12, 0, "0", BB_WHITE);
}

void show_lives(void) {
  bb_num(36, 0, lives, 1, BB_WHITE);
}

static void hud(void) {
  bb_text(1, 0, "SCORE", BB_YELLOW);
  add_score(0);
  bb_text(17, 0, "ROUND", BB_YELLOW);
  bb_num(23, 0, round_no, 2, BB_WHITE);
  bb_text(30, 0, "LIVES", BB_YELLOW);
  show_lives();
}

/* Draw round `n`'s room and put the monsters where its spawn tiles are. */
static void start_round(void) {
  unsigned char cx, cy, t, kind;
  bb_room(ROOM_ROUND_1 + (round_no - 1) % ROOM_COUNT);
  enemies_clear();
  for (cy = 1; cy < 25; ++cy) {
    for (cx = 0; cx < 40; ++cx) {
      t = bb_tile(cx, cy);
      if (t == TILE_SPAWN_PLAYER) {
        bb_set_tile(cx, cy, 32);
        start_x = cx * 8 - 8;       /* the box over the cell, the feet on its bottom row */
        start_y = cy * 8 - 13;
        continue;
      }
      kind = t == TILE_SPAWN_ROBOT ? E_WALK : t == TILE_SPAWN_BUNNY ? E_HOP : t == TILE_SPAWN_GHOST ? E_GHOST : 0;
      if (!kind) continue;
      bb_set_tile(cx, cy, 32);    /* a spawn tile is a marker: it leaves a space */
      enemy_add(kind, cx * 8 - 8, cy * 8 - 13);
    }
  }
  player_start();
  bubble_on = 0;
  clear_timer = 0;
  hud();
}

static void wait_fire(void) {
  do bb_frame(); while (!bb_btnp(BB_FIRE));
}

static void title(void) {
  unsigned char i;
  enemies_clear();
  bb_spr_hide(SLOT_PLAYER);
  bb_spr_hide(SLOT_BUBBLE);
  bb_room(ROOM_TITLE);
  bb_music(SONG_THEME);
  if (best) {
    bb_text(14, 18, "BEST", BB_YELLOW);
    bb_num(19, 18, best, 5, BB_WHITE);
    bb_text(24, 18, "0", BB_WHITE);
  }
  for (i = 0;; ++i) {
    bb_frame();
    bb_text(14, 21, (bb_frames & 32) ? "PRESS  FIRE" : "           ", BB_LIGHTGREEN);
    bb_spr(0, (bb_frames & 16) ? SPR_BUB_WALK : SPR_BUB_STAND, 148, 112);
    if (bb_btnp(BB_FIRE)) break;
  }
}

int main(void) {
  bb_init();
  for (;;) {
    title();
    score = 0;
    lives = 3;
    round_no = 1;
    start_round();
    while (palive) {
      bb_frame();
      tick = (unsigned char)bb_frames;
      player_update();
      bubble_update();
      enemies_update();
      if (!enemies_left()) {
        if (clear_timer == 0) bb_sfx(SFX_CLEAR);
        if (++clear_timer > 120) {
          ++round_no;
          start_round();
        }
      }
    }
    if (score > best) best = score;
    bb_text(15, 12, "GAME  OVER", BB_RED);
    bb_music(BB_STOP);
    bb_sfx(SFX_OUCH);
    wait_fire();
  }
}
