# tools/playtest

Tools for finding out what runs in breadbin, and why something doesn't.

| File | What it is |
|---|---|
| `playtest.mjs` | Plays every title in headless Chromium and writes `docs/play/verified.json`, the shelves' list. |
| `c64.mjs` | Library: drive the site's emulator (vc64web) from Node. Keys, joystick, memory, screen, debugger. |
| `links.mjs` | Checks every shelf item's Share button (share sheet or clipboard) and that its link, opened by a first-time visitor, starts it. |
| `probe.mjs` | One title, from the command line: a list of steps (wait, press, peek, shoot) built on `c64.mjs`. |
| `vice.mjs` | Library: a VICE workbench with true drive emulation and its monitor, for KERNAL and drive work. |
| `kernal.mjs` | Writes breadbin's patched Open ROMs KERNAL (`rom.rs`) to a file, for VICE. |
| `games.txt`, `demos.txt`, `tunes.txt` | What the shelves offer, handpicked, in order. |

```sh
npm install                 # puppeteer-core, pngjs
make -C ../.. serve-web     # the site on localhost:8064, which c64.mjs drives
```

Chromium is `/usr/bin/chromium` unless `CHROME` says otherwise; `BASE` points
the tools at another copy of the site.

## The playtest

```sh
node playtest.mjs games [--ids <substring>] [--retest]
node playtest.mjs demos [--retest]          # demos.txt
node playtest.mjs demos --scout --top 150   # CSDb's best that the Archive has, to pick from
node playtest.mjs tunes [--retest]          # tunes.txt
node playtest.mjs write     # results.json -> docs/play/verified.json
```

Only what passes goes on a shelf:
- **Demo:** loads, moves and makes sound. A still screen gets what a viewer
  would give it: SPACE when it asks for it (after the Disk button if it asks
  for the other side), else fire on port 2 and then 1 (a menu), but never on a
  blank screen, where a trackmo may be loading.
- **Tune:** loads from where the page gets it (`hvsc.js`) and makes sound in
  breadbin's SID player for 20 s.

Sound in the browser is measured at the speakers. `c64.mjs`'s `audioTap`,
injected into every frame, tees whatever reaches an AudioContext's destination
into an AnalyserNode. `loudness()` reads its RMS. `--fresh <hours>` with
`--retest` skips what was tested that recently.

A game passes when holding the joystick moves the player. Screens are classified
from screenshots and sprite positions, and the result goes to `results.json`.
There is no `--only`; `--ids` filters. A tape title is given time to load: the
harness waits while the datasette motor runs (`$01` bit 5 low), at boot and
before each try at the controls.

How it decides:
- **Sprites:** a sprite moves right while the stick is held right and left
  while it's held left (or down and up), twice. Positions come from the VIC
  itself (`wasm_sprite_info`), since peeking `$D000` reads RAM when a game banks
  the I/O out.
- **Scrolling:** the fine-scroll registers change while the stick is held and
  not while it's left alone.
- **The picture:** for players drawn in characters (a racing game's road) or
  sprites it can't follow, holding the stick changes much more of the screen
  than leaving it alone does, both times.
- **Prompts first:** trainers' Y/N questions and START GAME menus,
  "SPACE"/"F1" prompts, a cartridge menu (the F-key whose line names the
  game), and "insert side 2", which presses the page's Disk button once.

`games.txt` lines can pin a version (`= gb64 path` or `= archive id`), the file
to boot (`> FILE*`), a release in several Archive items (`= a + b + c`), and
`[port 1]` (or 2) for the joystick port a game reads when the playtest can't
tell, `[1541]` for a game that needs a real drive; `demos.txt` lines take `[1541]`
too, for trackmos. Those run on vc64web's emulated 1541 with breadbin's free
DOS (`tools/drive`), or with `ROM1541=<file>` in its place; a trackmo in several
Archive items gets them all as sides, for the Disk button.

## links.mjs: shared links

```sh
node links.mjs                       # every game, demo and tune in docs/play/verified.json
node links.mjs games demos           # some shelves
node links.mjs tunes --ids Hubbard   # titles whose id or name holds a substring
```

Each title gets a fresh browser context, like a friend opening the link from a
chat. A game or demo passes when its card's Share button on a phone hands
`#<tab>/<id>/play` to the share sheet, that link starts the emulator without a
load error, and the player's Share button on a computer copies the same link. A tune passes when its link plays
it, after the one tap the page asks for, and it is heard.

Chromium runs with a phone's autoplay policy (no sound before a tap), and the
page is watched through CDP with `userGesture: false`: Puppeteer's `evaluate`
and `waitForFunction` count as a tap and would unlock the sound by themselves.

## c64.mjs: the site's emulator from Node

```js
import { launch, open, sleep } from './c64.mjs';
const browser = await launch();
const c64 = await open(browser, 'gb64:b3/BRUCELEE_01135_02.zip');
await c64.key('SPACE');                     // a KEYS name, or [row, col] of the matrix
await c64.joy(2, ['PULL_RIGHT'], 1000);     // hold the stick on port 2 for 1 s
console.log(await c64.screenText(), await c64.sprites(), await c64.pcs());
await browser.close();
```

`open(browser, id, { boot, run, roms, kind, wait })` opens the title's card and
presses Play, like a visitor. `roms` installs the user's own ROMs first, as in
Settings: `{ kernal, basic, charset }` count only together, and `floppy` (a
16 KB 1541 ROM) switches disks from breadbin's virtual drive to vc64web's
emulated 1541.

On the returned `C64`:

| Method | |
|---|---|
| `peek(addr, n)` | Memory as the CPU sees it. |
| `type(text)` | vc64web's auto-typer; `\n` is RETURN. |
| `key(name, ms)` | Hold a key. Some games miss auto-typed keys. |
| `joy(port, dirs, ms)`, `fire(port)`, `joyCmd(port, cmd)` | The joystick. |
| `screenText()` | The text screen, wherever the VIC shows it. |
| `sprites()` | Sprite positions and fine scroll. Multiplexing games read as null. |
| `sound()` | The sound level over ~3 s (RMS samples; 0 is silence). |
| `shot(file)` | PNG of the emulator. |
| `shell(...cmds)`, `debug(...cmds)` | RetroShell commands; `debug` runs them paused in the debugger. |
| `pcs(n)`, `disasm(addr)` | Where the CPU is (sampled n times), and disassembly. |
| `driveState()` | vc64web's drive 8: connected, powered, disk, 1541 ROM. |
| `drive`, `jammed`, `error` | Virtual drive events, a jammed CPU, the page's load error. |

`signDriveRom(rom)` makes a 1541 ROM acceptable to vc64web; see below.

## probe.mjs: one title, step by step

```sh
node probe.mjs gb64:b3/BRUCELEE_01135_02.zip wait:30 key:SPACE wait:5 sprites joy:2:PULL_RIGHT:1000 sprites
node probe.mjs <id> parts:<id>,<id2> wait:60 click:#emu-disk key:SPACE wait:30 shot:a.png pcs
```

Running it without arguments lists the steps: `wait`, `type`, `key`, `fire`,
`joy`, `click` (a page button, e.g. `#emu-disk`), `js` (an expression in
vc64web's frame), `page` (one in breadbin's page), `pcs`, `dis`, `mem`, `shell`,
`debug`, `text`, `sprites`, `drive`, `shot`, `insert` (hand a local file such as
a `.crt` to vc64web). The options `rom:`, `kernal:`, `basic:`, `charset:`,
`boot:` and `parts:` can go anywhere among them.

## vice.mjs: the workbench

vc64web can't show the 1541's own CPU; VICE can. `vice.mjs` runs `x64sc` with
true drive emulation on its own Xvfb display, talks to its remote monitor, and
presses real keys with xdotool. It needs VICE 3.x, Xvfb and xdotool.

```js
import { start } from './vice.mjs';
const v = await start({ kernal: 'k.rom', basic: 'b.rom', chargen: 'c.rom', dos: 'free1541.rom', disk: 'game.d64', type: 'load"*",8,1\nrun\n' });
await v.run(20);                        // seconds, in warp unless warp: false
await v.key('space'); await v.key('KP_0', 300);   // the numpad is joystick 2; KP_0 is fire
console.log(await v.regs(), await v.peek(0xdd00, 3), await v.peek(0x500, 16, { drive: true }));
await v.runTo(0xffc0); await v.finish(); // break at OPEN, run until it returns
await v.shot('now.png'); await v.quit();
```

`mon(...cmds)` sends any monitor command (`tr exec e000 ffff` traces the
KERNAL, `attach "side2.d64" 8` swaps disks), and `drive(...cmds)` does the same
on the drive's CPU. Disks are attached read-only.

It needs free ROMs passed in (the Open ROMs; a free 1541 ROM for `dos`) and
refuses to start without them, so VICE never falls back on the Commodore ROMs
it ships. breadbin uses no commercially licensed ROMs or software, in the site
or in its tests.

## The hacks, and why

**vc64web only takes known 1541 ROMs.** VirtualC64 (`VCCore/Media/RomFile.cpp`)
recognises a drive ROM by its first bytes (`97 AA AA` and a few others), so a
ROM written from scratch is refused. `signDriveRom` writes those three bytes at
`$C000`. It moves the code they overwrite to free space at `$FFE8`, followed by a
JMP back, and points the reset vector and every `JMP $C000` at that copy.

**The Open ROMs KERNAL, patched** (`crates/breadbin/src/rom.rs`). The site
downloads the Open ROMs DEV.210823 release and patches it in the browser.
Building the release's commit (113523ab) with symbols reproduces the published
binary except for 16 bytes of version text, so its symbol file gives exact
addresses. With any other KERNAL the patches stay out.

- Always:
  - `$E536` restored. Stock CINT's tail; games jump there.
  - `$FEBC` returns from an interrupt (PLA / TAY / PLA / TAX / PLA / RTI), as
    the stock NMI handler's tail does: raster routines end with `JMP $FEBC`
    (Road of Excess).
  - CHKIN and CKOUT read the secondary address, not the logical file number.
  - CLOSE sends `ORA #$E0`, not `ORA $E0` (zero page).
  - JiffyDOS detection off: a plain 1541 looked like JiffyDOS.
  - Dolphin detection off: it also switched off CIA 2's interrupts.
  - `iec_set_idle` releases CLK, as the stock KERNAL leaves the bus.
  - TKSA sends `ORA #$60`, not `ORA #$90`: a real drive never talked after
    TALK with a "listen" secondary address.
- With breadbin's virtual drive: device 8 file channels (`channels.asm`).
- With a real 1541 (breadbin's free DOS or the user's ROM, `iec.asm`):
  - CIOUT and its flush send the byte they buffered. Before, every `M-W` byte was stale.
  - CLOSE keeps `.Y`. Last Ninja II relies on that.
  - When nothing answers a TALK or LISTEN, ST gets bit 7 ("device not
    present"), as from the stock KERNAL. Krill's loader reads it to check that
    its drive is alone on the bus.
  - SECOND, called through the jump table, returns with N showing the DATA
    line, as the stock one does. Bitfire counts its drives that way.
  - CIOUT and UNLSN at their stock addresses too, `$EDDD` and `$EDFE`, which
    loaders call directly (So-Phisticated III); the Open ROMs' cursor and
    screen code there moves to the free space.
  - `$F5AF` and `$F5D2` (the stock "SEARCHING FOR" and "LOADING" messages,
    silent in a program) return: Crest's loader calls them. They fall in the
    Open ROMs' CTRL key table, so CTRL+D and CTRL+= type `$60` instead.
  - The keyboard scan leaves `$DC00` at `$7F`, the column with SPACE, as the
    stock one does: programs read SPACE with `LDA $DC01 / CMP #$EF` alone.
  - "A byte waits to be sent" is bit 7 of `$94`, as in the stock KERNAL, not
    any value but 0: Bitfire's depacker leaves its own code in the zero page,
    and the next LISTEN sent the drive a byte nobody wrote.

These fixes are what let fast loaders upload their code to the drive.

**The Open ROMs BASIC, patched** (`rom.rs`, `patch_basic`). `$A659` is a stub
(RTS) in the Open ROMs; the stock routine resets the text pointer and clears the
variables. Exomizer's self-extractors end with `JSR $A659 / JSR $A533 / JMP $A7AE`
to RUN the BASIC program they unpacked, and stopped with ?UNDEF'D STATEMENT
ERROR (We Are New). The fix calls the Open ROMs' own RUNC and CLR from free
space at `$A00C`; the WAIT command, whose first bytes the jump takes, gets its
first instruction moved there too. The page swaps it in like the KERNAL, and
`node kernal.mjs basic_generic.rom out.rom basic` writes it for VICE.

**A free 1541 DOS** (`tools/drive`): Pascual DOS plus breadbin's patch, so the
drive code of trackmos (Edge of Disgrace...) finds the job queue, the entry
points and the memory map of a 1541. VICE and VirtualC64 disagree on two
points the patch settles as a real drive does: byte-ready reaches the CPU's SO
pin only once the PCR enables it, and port A latches each byte only with the
ACR's latch bit set. VICE delivers both regardless.

**The page with a real drive** (`docs/play/emu.js`):
- The drive ROM goes in the same message as the patched KERNAL, ahead of it:
  vc64web reloads its stored ROMs after taking a drive ROM, which would put
  the stock KERNAL (and its CIOUT bug) back.
- It waits for READY, then 3 s more. The drive restarts on its ROM, and a LOAD
  sent before then waits forever.
- It switches on vc64web's warp-while-loading, which is off by default, for the
  first program only: a trackmo keeps loading while it plays.
- Multi-disk zips get a Disk button.

**Tapes** need no drive ROM, so a tape release can put a fast-loader game on
the default shelf (`games.txt` pins Turrican II's). The page presses vc64web's
insert-tape button natively and keeps pressing until the dialog closes: vc64web
listens with `addEventListener`, which jQuery's `click()` misses, and it ignores
clicks while the dialog fades in. It switches off reset-before-load, types
`LOAD"",1` and RUN. vc64web doesn't warp for tapes, so a big game takes minutes.
Loaders that lean on the stock KERNAL's tape code still fail: the Last Ninja 2
and Creatures 2 tapes.

**Cartridges cut across disks.** EasyFlash conversions were spread on disks
with EasySplit: each disk holds a part that starts with `easysplt`, the whole
file's length, an id, its number and the count. The rest is one Exomizer 2.0
stream (`exomizer raw -P0`). `disk.rs` finds the parts on the downloaded disks,
and `exo.rs` (a port of Exomizer's zlib-licensed decruncher) joins them back
into the `.crt` in the browser. A cartridge needs neither drive nor drive ROM:
Last Ninja 2 plays this way.

**Sides.** A release in several items that holds no EasySplit parts is a disk
game's sides. The Disk button (also shown for multi-disk zips) puts the next
one in: `bb_drive_swap` for breadbin's virtual drive, keeping its KERNAL hooks,
or vc64web's own load message with a real drive. (Creatures II's cracks all
upload drive code for side 2, Illusion's included; the shelf has Nostalgia's,
which runs on the free 1541 DOS.)

**Only free ROMs and software.** No Commodore ROMs or commercially licensed
tools, in the site or in tests: comparisons run against free ROMs, and games
that need a drive ROM get a drive-free version instead (cartridge, tape or a
crack that loads by name).

## Gotchas

- Pressing fire during a load can break into a game's own tools: Creatures II
  drops into "MOSDOS". Wait for the screen before pressing.
- A KERNAL that sends a drive garbage can write to the disk. Check the image
  before blaming the next run.
- zsh doesn't split unquoted variables: `${=steps}` passes a step list built in
  a loop.
