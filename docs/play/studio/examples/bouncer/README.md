# Bouncer

Bricks, a bat and a ball: catch the falling pickups. After blip's Bouncer.

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

Joystick left and right move the bat, fire launches the ball. Pickups:
W a wide bat, M three balls, S a slower ball, L a life.
