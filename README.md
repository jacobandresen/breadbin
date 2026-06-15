# breadbin

A Commodore 64 game library for the terminal: **find · fetch · launch**.

Named after the "breadbin" — the classic beige C64 case.

breadbin is a small toolkit of single-file Python scripts that turn a folder of
C64 disk/tape images into a browsable, rankable, instantly-playable library. It
finds games (ranked by Lemon64 popularity, enriched with GameBase64 facts and box
art), downloads the ones you don't own from public archives, and boots them
straight into the VICE emulator — no launcher GUI, no C64 Forever license needed.

## Quick start

```sh
./breadbin              # default: opens the cover kiosk
./breadbin menu         # ranked master/detail list
./breadbin play barbarian
./breadbin info "bruce lee"
```

Point breadbin at your collection with the `C64_LIB` environment variable
(default `~/Games/Commodore/C64`):

```sh
export C64_LIB="/path/to/your/c64/games"
```

## The `breadbin` command

`breadbin` is a thin umbrella over the toolkit. Each subcommand is also a
standalone script (`c64run`, `c64menu`, …) that works on its own.

| Command | What it does |
| --- | --- |
| `breadbin kiosk` | **(default)** grid of game cards (cover + details); play/fetch on Enter |
| `breadbin menu` | browse ranked games; play the ones you own, fetch the rest |
| `breadbin play <name>` | launch a game straight into the emulator (`LOAD"*",8,1 : RUN`) |
| `breadbin info <name>` | show GameBase64 details (year, genre, author, …) |
| `breadbin get <name>` | download a tape from the Ultimate Tape Archive |
| `breadbin disk <name>` | download a disk from the Internet Archive / TOSEC |
| `breadbin tosec` | browse the whole TOSEC catalogue; download + play on pick |

Add `--help` to any subcommand for its own options, e.g. `breadbin disk --help`.

Running `breadbin` with no arguments opens the **kiosk**.

## The kiosk

![The breadbin kiosk](./screenshot.png)

`breadbin kiosk` is a cover-art "kiosk" organised by genre, rendered right in the
terminal using **WezTerm's inline image protocol**.

- Opens on a genre overview: each genre shows its title bar and its top 3 covers,
  plus a synthetic **"latest played"** section at the top.
- **Click a cover** → download (if needed) and play that game.
- **Click a genre title** → expand the genre into a full grid of its covers.
- In an expanded genre, click any cover to play; **Esc** returns to the overview.
- Arrow keys move the focus, **Enter** activates, **q** quits.

Pass `-w` / `-f` / `-r` to forward `--warp` / `--fullscreen` / `--real` through to
the emulator.

> The kiosk requires WezTerm — it draws covers with `wezterm imgcat`. In other
> terminals the cover art won't render. The text-based `breadbin menu` works
> anywhere (and shows covers too when run inside WezTerm).

## The menu

`breadbin menu` is a keyboard-driven master/detail picker (no fzf):

- Each game is one row: a marker, the title, and an action button.
  - `o` — in your collection · Enter plays it.
  - `v` — downloadable · Enter fetches it (via `c64disk`), then plays it.
- Expand the selected row (`→` / `Tab`) to reveal a detail line with the box
  cover and GameBase64 facts; `←` collapses.
- Type to filter, `Backspace` to edit, `Esc` to clear the filter or quit, `q` to
  quit.
- `breadbin menu --refresh` re-pulls popularity scores, re-scans availability, and
  rebuilds the index.

## Dependencies

### WezTerm (for cover art)

The kiosk and the menu's cover previews draw images with **`wezterm imgcat`**, so
they need [WezTerm](https://wezfurlong.org/wezterm/) and must run inside a WezTerm
window. (The menu detects this via `TERM_PROGRAM=WezTerm`; without it, it falls
back to text-only.)

```sh
brew install --cask wezterm
```

### VICE (for playing games)

`c64run` boots games with the [VICE](https://vice-emu.sourceforge.io/) C64
emulator, using `-autostart` (the equivalent of typing `LOAD"*",8,1` then `RUN`).
It auto-picks an emulator, preferring a **license-free** one:

1. native `x64sc` / `x64` on `PATH` (free VICE, bundled ROMs)
2. the `net.sf.VICE` Flatpak (free VICE, bundled ROMs)
3. Cloanto C64 Forever's `x64.exe` via wine (needs the license) — fallback only

```sh
brew install vice
```

Override the emulator with `C64_EMU` (e.g. `C64_EMU='x64'` or a full Flatpak
command). Defaults are fullscreen, true-drive autostart, and warp fast-forwarding
the load until the game has started.

### Python 3

Every tool is a standalone Python 3 script (no third-party packages). `fzf` is
used as an interactive picker by `c64get` / `c64disk` / `c64tosec` when available.

## How it works (data files)

breadbin keeps its data alongside the scripts:

- `c64_popularity.tsv` — Lemon64 popularity scores (rank, score, votes, title).
- `build_index.py` — matches those scores against your collection and writes
  `c64_index.tsv` (`display<TAB>path`), ordered by popularity, including games
  you've downloaded into `_IA_downloads` / `_UTA_downloads`.
- `gb64.sqlitedb` — the GameBase64 collection as SQLite (~17 MB, ~30k games),
  downloaded and cached once; `c64info` reads it offline thereafter.
- `covers/` + `covers_index.tsv` — cached box-art thumbnails (Libretro set).
- `ia_index.tsv`, `tosec_index.tsv`, `uta_index.html` — cached archive listings.
- `played.tsv` — a play log; powers the "latest played" kiosk section.
- `downloaded.tsv` — maps downloaded archive items to their saved boot-disk path.

## Where games come from

When you don't own a game, breadbin can fetch it from public preservation
archives:

- **Ultimate Tape Archive** (`c64get`) — tape (`.tap`) preservation, one folder
  per game.
- **Internet Archive** (`c64disk`, default source `ia`) — `softwarelibrary_c64`,
  cracked disk dumps.
- **TOSEC** (`c64disk --source tosec`, or `c64tosec`) — the comprehensive C64 set
  with full-title naming, served from the IA zip-of-zips and unpacked locally.

Downloads land in your collection, so a `breadbin menu --refresh` picks them up.
