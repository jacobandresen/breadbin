# Bubbler

A one-screen platformer: blow bubbles, trap the monsters, pop them for points. After blip's Bubbler.

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

## Playing

Joystick left and right to run, up to jump, fire to blow a bubble. A
monster caught in a bubble floats up: touch it to pop it into a cherry.
Clear the room to go on to the next round.
