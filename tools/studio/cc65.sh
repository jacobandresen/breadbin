#!/bin/sh
# Builds the cc65 C compiler, ca65 assembler and ld65 linker as WebAssembly for
# the Studio (the Hack tab), plus the "sysroot" they need: the C64 headers,
# the assembler includes, the linker config and the C64 runtime library.
#
#   tools/studio/cc65.sh            (needs git, make, a C compiler and emcc)
#
# cc65 is zlib-licensed (https://github.com/cc65/cc65). The C64 library is
# built with the native cc65 first (it is written in C and ca65 assembly),
# without debug info, so the sysroot stays small.
set -eu
COMMIT=77b6484615427746e55fda2755c9cbc320a1fb51
ROOT=$(cd "$(dirname "$0")/../.." && pwd)
SRC=${CC65_SRC:-$ROOT/target/cc65}     # (both overridable, to try a fresh build elsewhere)
OUT=${CC65_OUT:-$ROOT/docs/play/studio/cc65}

test -d "$SRC" || git clone -q https://github.com/cc65/cc65.git "$SRC"
git -C "$SRC" checkout -q "$COMMIT"

# native tools, then the C64 library with them
make -s -C "$SRC" -j4 bin >/dev/null
mkdir -p "$SRC/lib"
make -s -C "$SRC/libsrc" -j4 c64 CA65FLAGS= CC65FLAGS="-Or -W error" >/dev/null

# the three tools as ES modules that run in a browser, a worker or Node
mkdir -p "$OUT"
for t in cc65 ca65 ld65; do
  (cd "$SRC/src" && emcc -O2 -w -I common -DBUILD_ID='"breadbin"' common/*.c $t/*.c -o "$OUT/$t.mjs" \
    -sMODULARIZE -sEXPORT_ES6 -sEXPORT_NAME=create_$t -sINVOKE_RUN=0 -sFORCE_FILESYSTEM \
    -sALLOW_MEMORY_GROWTH -sEXIT_RUNTIME=0 -sEXPORTED_RUNTIME_METHODS=FS,callMain \
    -sENVIRONMENT=web,worker,node)
done

# the sysroot: only what a C64 program needs (the generic and CBM headers,
# not those of the other 30 targets)
STAGE=$(mktemp -d)
mkdir -p "$STAGE/include" "$STAGE/asminc" "$STAGE/cfg" "$STAGE/lib"
(cd "$SRC/include" && for f in *.h sys/*.h; do mkdir -p "$STAGE/include/$(dirname "$f")"; cp "$f" "$STAGE/include/$f"; done)
for f in apple2.h apple2enh.h atari.h atari2600.h atari5200.h atari7800.h atmos.h agat.h \
         creativision.h gamate.h geos.h kim1.h lynx.h nes.h osic1p.h pce.h rp6502.h \
         sim6502.h sim65c02.h supervision.h sym1.h telestrat.h vic20.h cx16.h pet.h plus4.h c16.h c128.h cbm510.h cbm610.h; do
  rm -f "$STAGE/include/$f"
done
cp "$SRC"/asminc/*.inc "$SRC"/asminc/*.mac "$STAGE/asminc/" 2>/dev/null || cp "$SRC"/asminc/* "$STAGE/asminc/"
cp "$SRC/cfg/c64.cfg" "$SRC/cfg/c64-asm.cfg" "$STAGE/cfg/"
cp "$SRC/lib/c64.lib" "$STAGE/lib/"
(cd "$STAGE" && tar --format=ustar --owner=0 --group=0 --mtime=@0 --sort=name -cf - include asminc cfg lib) | gzip -9n > "$OUT/sysroot.tar.gz"
rm -rf "$STAGE"
ls -la "$OUT"
