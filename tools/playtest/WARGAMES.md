# WarGames — resolved

Status: fixed in `crates/breadbin/src/rom.rs` (`IRQ_ENTRY`);
`node playtest.mjs games --retest --ids Compilation_Wargames` passes.

## Root cause

The Open ROMs' hardware IRQ vector (`$FFFE`) points at `$EA53`; the stock KERNAL's at
`$FF48`. WarGames:

1. File `A` (`JSR $1000`) copies the KERNAL ROM `$E000-$FFFF` into the RAM under it.
2. The loader sets `$01=$35` (KERNAL banked out), so IRQs vector through the RAM copy.
3. `D` clears RAM `$E000-$EFFF` (`$80E9`: 16 pages from `$E000`) for its own use.

With the stock KERNAL the entry at `$FF48` survives step 3. With the Open ROMs, `$EA53` is
wiped: every raster IRQ hits `00` (BRK), vectors back to `$EA53`, forever. The PC sat at
`$EA53`, `$A2` stayed 0, and the game's handler at `$8AC6` (`$0314`) was never reached.

Fix: a stock-style IRQ entry (`PHA/TXA/PHA/TYA/PHA/CLD/TSX/LDA $0104,X/AND #$10/BEQ/
JMP ($0316)/JMP ($0314)`) at `$FE7B`, in `dolphindos_detect`'s body (dead since the
`DOLPHIN` patch made it RTS), with `$FFFE` pointed there.

## Corrections to the earlier handoff

- `$8AC6` is NOT uninitialised in a good boot: it holds the handler (`D8 AD 0D DC…`). The
  `00 00 00 00 FF FF FF FF` reads came from runs where nothing loaded (flaky boot below).
- `wasm_peek` reads ROM under `$E000-$FFFF` whatever `$01` says; "`$FFFE = $EA53`" was the
  ROM view, not what the CPU saw. VICE's `bank ram` shows the RAM under ROM.
- VICE 3.10 (`x64sc`) is installed. Free ROMs for it:
  `https://mega65.github.io/open-roms/bin/{kernal_generic,basic_generic,chargen_openroms}.rom`,
  KERNAL patched with `kernal.mjs`. Open ROMs don't autostart through `$0302` after
  `LOAD"WARGAMES",8,1`; type `SYS705` (the loader's entry, `$02C1`).

## Still open

- **Flaky boot**: often `LOAD"WARGAMES",8,1:` gives `? ERROR` with no drive `load`/`missing`
  events (the drive's stub was not hit: `$0330` was not the stub when LOAD ran). ~5 of 8
  probe runs failed this way; the playtest passed. Not investigated.
- **Copy protection**: `B` seeks track 37 via M-W/M-R on the command channel and stores the
  job result in `$03FF` (1 = original disk). On a D64 it fails (`$03FF=3`, tape buffer filled
  with `$80`); `D` then skips `JSR $AFCE` (`$8125`) and `JSR $0801` (`$8260`). The game plays
  regardless. If those matter, a games.txt live patch on B such as
  `{7d51:a90fa208a00f20ba=a9018dff034c9e7d}` (LDA #1 / STA $03FF / JMP $7D9E) skips the check.
- Other stock KERNAL addresses in `$F000+` that games jump to (e.g. `$FEBC`, the IRQ exit)
  are a likely sibling class.

## Turrican II harness bug (separate)

In `playtest.mjs` `controlledPort()`, trainer prompts match against the first 12
`[A-Z]{3,}` runs of the screen. Turrican II's crack header pushes "START GAME" past run 12,
so `/INFINITE/` misfires and types `n` instead of navigating the cursor menu (cursor-down
`key(0,7)` moves it; the activation key was not found: RETURN, SPACE, F1/F3/F5/F7 and fire
on both ports all failed).

## RetroShell gotchas

- `.` + RETURN pauses and prints `(scanline,cycle) $pc:` (what `pcs()` reads).
- `d <addr>` ignores its address (disassembles from the PC); no `r`/`registers` command.
  Read memory with `mem:<addr>:<n>`.
