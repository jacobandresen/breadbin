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
  - for Creatures II's crack loader (NOSDOS V1.8, Nostalgia): the drive model
    where it looks ("1541" at `$E5C4`, its last character with bit 7 set as in
    the DOS's message table, and `$0D` at `$FEA0`); the entry points it calls,
    `$F556` (wait for SYNC), `$F7E8` (decode 5 GCR bytes to `$52-$55`), `$F418`
    (end a job OK) and `$C194` (end an M-E command); `$43` set to the track's
    sector count for EXECUTE jobs; a file opened on channel 0 and closed unread
    leaves its first block in `$18/$19`; and the DOS's own job state and
    buffers out of the stack page above `$0146`, where it keeps its code. Its
    high score saver copies the ROM's block writer from `$F575`, which this DOS
    doesn't have: there an RTS, so high scores are not written back;
  - a reset entry at `$EAA0`, where programs reset the drive (M-E) and where
    VirtualC64 starts the drive's CPU instead of reading the reset vector.
  - for Krill's loader (We Are New and other Fairlight, Censor and Oxyron
    trackmos): the GCR encode table at `$F77F`, from which its drive code builds
    its decode table, and a drive that lets go of the bus when ATN brings a
    LISTEN or TALK for another device, as a 1541 does. The loader LISTENs to
    devices 8-14 to make sure its drive is alone on the bus, and stopped with
    error `$FD` when every number answered.
  - for Bitfire (The Star Wars Demo): `$C11B`, `ORA $1C00 / STA $1C00 / CLI /
    RTS` as at the end of a 1541's LED routine, which Bitfire calls with
    A = `$04` to start the motor. To make room for it, the serial routines that
    followed the reset code moved up into CODE2.

  - reading on tracks 25-30. The sector finder read each header's data block
    before decoding the header, and the decode let the next header go by, so
    it saw every second sector; on those tracks' 18 sectors the same half came
    round for ever, and LOAD stopped with READ ERROR (Natural Wonders). It now
    compares the header's raw GCR with the wanted sector's and reads only that
    one's data block.
  - for Natural Wonders' loader (Oxyron), which reads files with EXECUTE
    jobs of its own: `$F8E0`, decode the data block the job read (256 GCR
    bytes in its buffer, 69 at `$01BA`) in place, with the block's mark in `$38`
    and its checksum in `$3A`; and `$F505`, end the job with OK.
  - a drive that lets go of DATA when an ATN sequence leaves it idle, as a
    1541 does: Bitfire's installer (The Star Wars Demo, Wonderland XIII) waits
    for the bus to go free before its drive code takes the line.
  - commands received at `$0200`, the 1541's command buffer (up to 64 bytes):
    at `$0300`, buffer 0, every command overwrote drive code uploaded there
    with M-W (Krestology, Krestage); and `$F4D1`, read the running job's sector
    into the buffer whose page is in `$31` and end the job, which their drive
    code jumps to.
  - the `&` command: load a USR file of records (address, count, bytes,
    checksum) into drive memory and run it, the command line copied to `$0200`.

Dutch Breeze (Black Mail) runs its drive code with `&`, which now loads, but the
code decrypts itself with bytes of Commodore's 1541 ROM, so it cannot run on a
free DOS.


  All of it follows the drive's published interface (the job queue and entry
  points as *Inside Commodore DOS* and loader sources document them; the tables
  generated from the GCR code), within Pascual DOS's provenance rules.
- **`sign.mjs`**: vc64web takes a drive ROM only when it starts like a known
  one, so this stamps the first bytes and moves the code they covered.

Build it with `make drive` (needs git, node and cc65's `ca65`/`ld65`).

With it these pass breadbin's playtest in the browser, on free ROMs only: Edge
of Disgrace, Lunatico, Wonderland XII and XIII, The Star Wars Demo, The Phoenix
Code, La Linea, Mekanix, Red Storm, Radio Napalm, We Are New, Desert Dream,
Amplifire, Another Beginning, Natural Wonders 1 and 2, 1991, Krestology, The
Megademo, Tower Power, So-Phisticated III, Cycle, Soiled Legacy, Road of Excess,
and the game Creatures II. Others are shelved as WIP, with the reason. A demo marked
`[1541]` in `tools/playtest/demos.txt` (or a game in `games.txt`) gets the
emulated 1541 with this ROM; a visitor's own 1541 ROM (Settings) takes its place.
