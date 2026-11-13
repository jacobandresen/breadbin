# Builds Bubbler with a native cc65 (https://cc65.github.io), the same way
# the breadbin Studio builds it in the browser:
#
#   make            # -> game.prg; run it with x64sc game.prg, or any C64 emulator
#
# The Studio writes gen/ (the runtime and the assets as C) on every build; it
# is in the folder so this Makefile has it too. The program ends below $C000:
# the runtime keeps $C000-$FFFF for the screen, the tiles and the sprites.
# -Cl keeps local variables at fixed addresses (much faster on a 6502; the
# one catch is that a function can't call itself), as the Studio does.

# (sorted: the order the Studio links them in, so both make the same program)
SOURCES = $(sort $(wildcard src/*.c src/*.s gen/*.c gen/*.s))

game.prg: $(SOURCES) $(wildcard src/*.h) $(wildcard gen/*.h)
	cl65 -t c64 -Oirs -Cl -I src -I gen -Wl -D,__HIMEM__=0xC000 -m game.map -o $@ $(SOURCES)

clean:
	rm -f game.prg game.map src/*.o gen/*.o

.PHONY: clean
