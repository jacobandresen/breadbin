/* My game - made with the breadbin Studio.
 *
 * A start: a hero you steer with the joystick, who walks on the ground of
 * the room ROOM_START, jumps with up and blips when you press fire. Change
 * it, press Ctrl+Enter, and it runs on the C64 on the right. F1 has the API.
 */

#include "bb.h"
#include "assets.h"

int x = 152;                   /* the hero's top-left corner, in pixels */
int y = 100;
signed char fall;              /* falling speed, pixels a frame */
unsigned char left, walk;

/* Is there ground (a tile with FLAG_SOLID, ticked in TILES) under the feet? */
unsigned char on_ground(void) {
  return (bb_flags_at(x + 8, y + 21) | bb_flags_at(x + 15, y + 21)) & FLAG_SOLID;
}

int main(void) {
  bb_init();
  bb_room(ROOM_START);
  bb_music(SONG_TUNE);
  bb_text(2, 1, "HELLO FROM THE STUDIO!", BB_YELLOW);
  bb_text(2, 3, "STEER WITH THE JOYSTICK, UP JUMPS", BB_LIGHTBLUE);

  for (;;) {
    bb_frame();

    if (bb_btn(BB_LEFT) && x > 0) --x, left = 1, ++walk;
    if (bb_btn(BB_RIGHT) && x < 296) ++x, left = 0, ++walk;

    /* gravity, and a jump from the ground */
    if (fall >= 0 && on_ground()) {
      fall = 0;
      if (bb_btnp(BB_UP)) fall = -6;
    } else if ((bb_frames & 3) == 0 && fall < 4) {
      ++fall;
    }
    y += fall;
    /* landed inside the ground: up onto it */
    while (fall >= 0 && ((bb_flags_at(x + 8, y + 20) | bb_flags_at(x + 15, y + 20)) & FLAG_SOLID)) --y;

    if (bb_btnp(BB_FIRE)) bb_sfx(SFX_BLIP);

    /* every image with "flip" ticked has its mirror right after it */
    bb_spr(0, ((walk & 8) ? SPR_HERO_WALK : SPR_HERO) + left, x, y);
  }
}
