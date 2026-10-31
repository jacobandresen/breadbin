# tools/drive: a free 1541 DOS for trackmos

Most of the great C64 demos are trackmos: their loader uploads code into the
1541 disk drive and reads the disk with it. breadbin's virtual drive (the
KERNAL-level one in `crates/breadbin/src/drive.rs`) can't run drive code, and
Commodore's 1541 ROM isn't free. So breadbin ships `docs/play/dos1541.rom`,
built from:

- **[Pascual DOS](https://github.com/Pascual-Candel-Palazon/Pascual_DOS-1541)**
  by Pascual Candel Palazón, MIT licence: a 1541 DOS written from scratch,
  with a documented provenance policy (no material from Commodore's ROM).
  Pinned at commit `02a4b64` in the Makefile.
- **`breadbin.patch`** (MIT, like the code it patches), what fast loaders need
  from the drive besides LOAD and SAVE:
  - the memory commands M-W, M-R and M-E;
  - the job queue: code in the drive puts job codes in `$00-$04` with track and
    sector at `$06+2n`, and the DOS reads into buffer `$0300+n*$100` from its
    timer interrupt (READ, SEEK, EXECUTE, JUMP);
  - the entry points drive code calls by address: `$F50A` (find the job's
    sector and wait for its data block) and `$F969` (end the job), and the GCR
    decode tables at `$F8A0`/`$F8C0`, plus `$D042` (initialise: disk ID and
    head position) and `$D586` (read a block), and "1541" at `$E5C4` where
    loaders check the drive model;
  - the memory map loaders count on: the stack below `$0146`, `$0180-$01FF`
    and `$0200-$02FF` left to them while they run, the head's track in `$22`;
  - two IEC bus fixes for hosts that release ATN and CLK together.

  All of it follows the drive's published interface (the job queue and entry
  points as *Inside Commodore DOS* and loader sources document them; the tables
  generated from the GCR code), within Pascual DOS's provenance rules.
- **`sign.mjs`**: vc64web takes a drive ROM only when it starts like a known
  one, so this stamps the first bytes and moves the code they covered.

Build it with `make drive` (needs git, node and cc65's `ca65`/`ld65`).

With it these trackmos pass breadbin's playtest in the browser, on free ROMs
only: Edge of Disgrace, Lunatico, Wonderland XII and XIII, The Phoenix Code,
La Linea, Ice Cream Castle, Mekanix, Red Storm and Radio Napalm. Others are
shelved as WIP; Krill's loader, for one, still stalls in its transfer. A demo marked
`[1541]` in `tools/playtest/demos.txt` (or a game in `games.txt`) gets the
emulated 1541 with this ROM; a visitor's own 1541 ROM (Settings) takes its place.
