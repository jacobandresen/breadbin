# Breadbin

**Your Commodore 64, in a browser tab.** Breadbin is a shelf of the best C64
games, demos and SID music that runs at
**[jacobandresen.github.io/breadbin](https://jacobandresen.github.io/breadbin/play/)**,
with nothing to install and no Commodore ROMs needed.

It's named after the "breadbin", the beige case that sat on desks in a million
bedrooms, and it's styled after that hardware: the header is the case, the cards
are keycaps, the Commodore rainbow stripe runs through it and the text is a C64
pixel font.

## What's on the shelf

- **Games**: GameBase64's top-rated titles, ranked by rating, GameBase64's
  "classic" pick and how popular they are on the Internet Archive.
- **Demos**: CSDb's top-rated demos, by decade and by group.
- **Tunes**: CSDb's top-rated SID music, grouped by the party it was released
  at, with three visualisers in a real 320x200 framebuffer, each a nod to a
  classic demo (Comaland, Edge of Disgrace, Deus Ex Machina).

Everything offered has been playtested (see below), so every card starts. On a
phone or tablet the emulator gets an on-screen joystick.

## How it works

- **Emulation**: [vc64web](https://vc64web.github.io/) (VirtualC64 in
  WebAssembly) running the free [MEGA65 Open ROMs](https://github.com/MEGA65/open-roms).
  Without a 1541 drive ROM, breadbin serves disk images itself: its virtual
  drive answers the KERNAL's LOAD straight from the D64, handles autostarting
  loaders and steps around the few places where the Open ROMs differ from the
  originals. Titles with their own fast loader need a real drive ROM; if you own
  the ROMs, add them under Settings (they stay in your browser).
- **Engine**: the virtual drive, disk-image handling, SID player (reSID plus a
  6502 driver) and visualisers are Rust, compiled to WebAssembly
  (`crates/breadbin`). JavaScript (`docs/play`) does the page and browser APIs.
- **Data**: nothing is bundled. Each shelf loads when it scrolls into view and
  the browser caches what it fetches: game files from GameBase64's collection on
  the Internet Archive, demos from the Internet Archive, SID files from
  [hvsc.de](https://www.hvsc.de/), box art from libretro-thumbnails. The
  playtested list itself is `docs/play/verified.json`.
- **CSDb**: csdb.dk only lets sites on its allowlist read it from a web page, so
  the details shown for demos and tunes go through a relay:
  [r.jina.ai](https://jina.ai/reader/) by default, or your own
  (`docs/play/relay/worker.js` is a Cloudflare Worker for it).

## Playtests

`tools/playtest` decides what the shelves offer. It plays each candidate in
headless Chromium against a local copy of the site (pressing FIRE, SPACE, F1 and
moving the stick), classifies the screenshots, and plays tunes through the
WebAssembly SID player, then writes the ones that work to `verified.json`:

```sh
make serve-web &                       # build the engine, serve docs/ on :8064
cd tools/playtest && npm install
node playtest.mjs games --top 300      # also: demos, tunes
node playtest.mjs write                # results.json -> docs/play/verified.json
```

Downloads are cached in `tools/playtest/cache/` and results in `results.json`,
so later runs only test new titles.

## Credits

Game data and files come from [GameBase64](https://www.gb64.com/) and the
[Internet Archive](https://archive.org/details/softwarelibrary_c64), scene data
from [CSDb](https://csdb.dk/), music from the
[High Voltage SID Collection](https://www.hvsc.de/) and box art from
[libretro-thumbnails](https://github.com/libretro-thumbnails/Commodore_-_64).
Thanks to everyone who keeps the C64 preserved.
