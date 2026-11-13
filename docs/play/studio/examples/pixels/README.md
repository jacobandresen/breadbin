# Pixels

A tour of the bitmap mode: lines, circles, fills, blits racing plots, bouncing balls, and peek and poke.

Made with the breadbin Studio: open it in the Hack tab of
https://jacobandresen.github.io/breadbin/play/#hack, or build it here with
`make` and a native cc65.

## Files

- `src/`: the game, in C (cc65)
- `assets/sprites.json`, `tiles.json`, `map.json`, `music.json`: the
  SPRITES, TILES, MAP and MUSIC editors' work, as text
- `gen/`: written by every build (the runtime `bb.h` and the assets as C)
- `Makefile`: the same build with a native cc65
- `game.prg`: the program, written by every build when the project is in a
  folder on your disk (it is in `.gitignore`)

## The parts

Fire goes on to the next: shapes, plot against blit (timed), bouncing balls
(blits with the flip pen, pre-shifted to move a pixel at a time, under a
hardware sprite), and peek and poke.
