/* game.h - what Bubbler's files share. */
#ifndef GAME_H
#define GAME_H

#include "bb.h"
#include "assets.h"

/* sprite slots */
#define SLOT_PLAYER 0
#define SLOT_BUBBLE 1
#define SLOT_ENEMY  2              /* enemies use slots 2..7 */
#define MAX_ENEMIES 6

/* heights are in 16ths of a pixel, for smooth jumps and falls */
#define SUB 4                      /* 1 << SUB sixteenths per pixel */
#define GRAVITY 3                  /* 16ths of a pixel per frame, per frame */
#define MAX_FALL 36
#define JUMP (-62)                 /* about five tiles: one platform up */

/* the hit box inside a 24x21 sprite */
#define BOX_X 6
#define BOX_W 12
#define BOX_Y 5
#define BOX_H 16
#define FEET 20                    /* the sprite row the feet are on */

/* the screen: row 0 is the score line, the room is rows 1..24 */
#define BOTTOM 200

extern unsigned int score;
extern unsigned char lives, round_no;
extern unsigned char tick;         /* the frame counter's low byte, read once a frame */
extern int start_x, start_y;      /* where the round's player spawn tile was */

/* Are two sprites' boxes touching? Their corners within `w` across and `h`
 * down of each other. One unsigned compare per axis: much cheaper on a 6502
 * than bb_overlap's eight arguments, for checks made every frame. */
#define NEAR(dx, dy, w, h) ((unsigned int)((dx) + (w)) < 2 * (w) && (unsigned int)((dy) + (h)) < 2 * (h))

/* player.c */
extern int px, py;                 /* py in 16ths of a pixel */
extern int py_px;                  /* py in pixels, worked out once a frame */
extern signed char pvy;
extern unsigned char pleft, palive, phurt;
void player_start(void);
void player_update(void);
void player_hit(void);

/* bubble.c */
extern unsigned char bubble_on;
extern int bx, by;
void bubble_blow(void);
void bubble_update(void);

/* enemies.c */
enum { E_NONE, E_WALK, E_HOP, E_GHOST, E_TRAPPED, E_FRUIT };
extern unsigned char ekind[MAX_ENEMIES], estate[MAX_ENEMIES];
extern int ex[MAX_ENEMIES], ey[MAX_ENEMIES], eyp[MAX_ENEMIES];  /* ey in 16ths, eyp in pixels */
void enemies_clear(void);
void enemy_add(unsigned char kind, int x, int y);
void enemies_update(void);
unsigned char enemies_left(void);

/* body.c: walking and falling against the tiles, for everyone */
extern int bx_, by_;               /* the body being moved: x, and y in 16ths */
extern int byp_;                   /* ...and y in pixels (by_ >> SUB, kept up to date) */
extern signed char bvy_;
extern unsigned char bground_;
void body_fall(void);
unsigned char body_blocked(signed char dx);
unsigned char body_edge(signed char dx);

void add_score(unsigned int points);
void show_lives(void);

#endif
