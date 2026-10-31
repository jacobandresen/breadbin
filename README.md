# Breadbin

**Your Commodore 64, in a browser tab.** Breadbin is a shelf of the best C64
games, demos and SID music that runs at
**[jacobandresen.github.io/breadbin](https://jacobandresen.github.io/breadbin/play/)**,
with nothing to install and no Commodore ROMs needed.

It's named after the "breadbin", the nickname of the case that sat on desks in
a million bedrooms, and it has a look of its own: a midnight bakery. Plum-ink
night, warm toast and butter for what you touch, pixel type for names, and a
logo that is a pixel slice of toast with a play button in the crumb.

## What's on the shelf

- **Games**: GameBase64's top-rated titles, ranked by rating, GameBase64's
  "classic" pick and how popular they are on the Internet Archive.
- **Demos**: CSDb's top-rated demos, by decade and by group, and trackmos
  such as Edge of Disgrace and Lunatico on a free 1541 DOS.
- **Tunes**: CSDb's top-rated SID music, grouped by the party it was released
  at, with three visualisers in a real 320x200 framebuffer, each a nod to a
  classic demo (Comaland, Edge of Disgrace, Deus Ex Machina).

Everything offered has been playtested (see below). A title that hasn't passed
yet stays on the shelf with a **WIP** badge and the reason, since it may not
start or play right. On a phone or tablet the emulator gets an on-screen
joystick.

## How it works

- **Emulation**: [vc64web](https://vc64web.github.io/) (VirtualC64 in
  WebAssembly) running the free [MEGA65 Open ROMs](https://github.com/MEGA65/open-roms).
  Most titles load through breadbin's virtual drive: it answers the KERNAL's
  LOAD straight from the D64 (tape images are turned into one first), handles
  autostarting loaders, patches a few cracks' loaders live and steps around
  the places where the Open ROMs differ from the originals. Trackmos, whose
  loaders run their own code in the disk drive, get an emulated 1541 with a free
  DOS instead: [Pascual DOS](https://github.com/Pascual-Candel-Palazon/Pascual_DOS-1541)
  with breadbin's patch for fast loaders (job queue, memory commands, the entry
  points and memory map loaders expect; see `tools/drive`). If you own the
  Commodore ROMs you can add them under Settings (they stay in your browser).
- **Engine**: the virtual drive, disk-image handling, SID player (reSID driven
  by a small cycle-counted C64: 6502, CIAs and raster interrupt, enough for
  digi tricks; 2SID and 3SID tunes play in stereo) and visualisers are Rust, compiled to WebAssembly
  (`crates/breadbin`). JavaScript (`docs/play`) does the page and browser APIs.
- **Data**: nothing is bundled. Each shelf loads when it scrolls into view and
  the browser caches what it fetches: game files from GameBase64's collection on
  the Internet Archive, demos from the Internet Archive, SID files from the
  Archive's copy of HVSC 79 (then [hvsc.de](https://www.hvsc.de/)), box art from
  libretro-thumbnails. The playtested list itself is `docs/play/verified.json`;
  the only ROM breadbin ships is its own build of the free 1541 DOS.
- **CSDb**: csdb.dk only lets sites on its allowlist read it from a web page, so
  the details shown for demos and tunes go through a relay:
  [r.jina.ai](https://jina.ai/reader/) by default, or your own
  (`docs/play/relay/worker.js` is a Cloudflare Worker for it).

## Playtests

`tools/playtest` decides what the shelves offer. It plays each candidate in
headless Chromium against a local copy of the site (pressing FIRE, SPACE, F1,
saying no to trainer menus and moving the stick), classifies the screenshots,
and plays tunes through the WebAssembly SID player (they must stay audible and
keep changing), then writes the ones that work to `verified.json`:

```sh
make serve-web &                       # build the engine, serve docs/ on :8064
cd tools/playtest && npm install
node playtest.mjs games --top 300      # also: demos, tunes
node playtest.mjs write                # results.json -> docs/play/verified.json
```

Downloads are cached in `tools/playtest/cache/` and results in `results.json`,
so later runs only test new titles.

## Credits

Breadbin runs on free software, each under its own licence:

- [vc64web](https://github.com/vc64web/virtualc64web) by mithrendal, on
  [VirtualC64](https://github.com/dirkwhoffmann/virtualc64) by Dirk W. Hoffmann:
  the emulator (GPL-3.0), loaded from vc64web.github.io.
- [MEGA65 Open ROMs](https://github.com/MEGA65/open-roms): the free KERNAL,
  BASIC and character set (LGPL-3.0). Breadbin patches the KERNAL in the
  browser (`crates/breadbin/src/rom.rs`, its fixes listed there).
- [Pascual DOS](https://github.com/Pascual-Candel-Palazon/Pascual_DOS-1541) by
  Pascual Candel Palazón: the free 1541 DOS (MIT), with breadbin's patch
  (`tools/drive`, MIT as well).
- [resid-rs](https://github.com/digitalstreamio/resid-rs), the Rust port of
  Dag Lem's reSID: the SID of the music player (GPL-3.0).
- [Exomizer](https://bitbucket.org/magli143/exomizer) by Magnus Lind: its
  decruncher, ported to Rust to unpack EasySplit files (`crates/breadbin/src/exo.rs`,
  under Exomizer's zlib-style licence).
- [EasyFlash](https://skoe.de/easyflash/) by Thomas Giesel: the cartridge and
  EasySplit formats.
- [ACME](https://github.com/meonwax/acme) by Marco Baye: the assembler of the
  Hack tab, built to WebAssembly (GPL-2.0).
- Pixelify Sans, Silkscreen and Figtree from Google Fonts (SIL Open Font License).

The games, demos and music are the work of their coders, musicians,
graphicians and crackers, credited on every card from the scene's own records:
files and facts from [GameBase64](https://www.gb64.com/) and the
[Internet Archive](https://archive.org/details/softwarelibrary_c64), scene data
from [CSDb](https://csdb.dk/), music from the
[High Voltage SID Collection](https://www.hvsc.de/) and box art from
[libretro-thumbnails](https://github.com/libretro-thumbnails/Commodore_-_64).
Thanks to everyone who keeps the C64 preserved.
