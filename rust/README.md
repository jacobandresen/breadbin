# breadbin

A Commodore 64 game library for the terminal: **find, fetch, and launch** C64
games — with box-art covers and a keyboard/mouse TUI, all from one self-contained
binary.

breadbin ranks games by popularity, pulls GameBase64 details and cover art,
downloads disks/tapes from the Internet Archive and TOSEC, and boots them
straight into VICE (no launcher GUI). It is a single multi-call binary: it works
as the `breadbin` umbrella, or as the standalone `c64*` tools via symlinks.

## Install

```sh
cargo install breadbin
breadbin install-links        # create c64run, c64menu, … next to the binary (optional)
```

On **first run** breadbin downloads and builds everything it needs into its data
directory — the ranked game catalogue, the GameBase64 SQLite database, the cover
index, and covers on demand. Nothing is bundled; no repo or data files required.

- Data directory: `$BREADBIN_USER_DATA`, or `~/.breadbin` by default.
- Your local ROM collection (optional): `$C64_LIB`, or `~/Games/Commodore/C64`.
- Emulator: auto-detected VICE (`x64sc`/`x64`, the `net.sf.VICE` flatpak, or
  Cloanto C64 Forever); override with `$C64_EMU`.

## Use

```sh
breadbin                  # the cover kiosk (default)
breadbin kiosk            # grid of covers by genre; click/Enter to play or fetch
breadbin menu             # ranked master/detail list with inline covers + details
breadbin play  <name>     # boot a game straight into the emulator
breadbin info  <name>     # GameBase64 details (year, genre, author, …)
breadbin get   <name>     # download a tape from the Ultimate Tape Archive
breadbin disk  <name>     # download a disk from the Internet Archive / TOSEC
breadbin tosec            # browse the whole TOSEC catalogue; download + play on pick
```

Add `--help` to any subcommand for its options.

### TUIs

Both TUIs render cover art inline. In WezTerm (and other terminals supporting the
iTerm2/Kitty/Sixel image protocols) covers are shown as real images; elsewhere
they fall back to Unicode half-blocks automatically.

- **kiosk** — opens on a genre overview (a "latest played" row on top), each
  genre showing its top covers. Click/Enter a cover to play (downloading first if
  you don't own it); click/Enter a genre title to expand it into a full grid.
  Arrow keys move, Esc backs out / quits, `q` quits.
- **menu** — a collapsible, genre-grouped list. Type to filter, →/Tab expand a
  game into a detail card (cover + GameBase64 facts), Enter plays owned games or
  fetches-and-plays the rest. `--refresh` re-pulls the catalogue.

## License

MIT
