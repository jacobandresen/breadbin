# Breadbin

A native **GTK4 / libadwaita** Commodore 64 kiosk for Linux.

Named after the "breadbin" — the classic beige C64 case shape.

![Breadbin kiosk](./screenshot.png)

## What it does

Breadbin turns your C64 collection into a cover-art kiosk organised by genre,
with a demoscene browser and a SID music player — all styled with the authentic
Pepto VIC-II colour palette.

| Tab | What it shows |
| --- | --- |
| **Games** | One horizontal strip per genre; click a cover to see details and play. Genres with more than 8 entries get a **Show All** button that slides open a full grid. |
| **Demos** | Same kiosk layout, grouped by demoparty (data from CSDb). |
| **Tunes** | SID music jukebox with a live oscilloscope visualiser. |

Click any card to open a detail dialog with the full box art, metadata, and
**Play** / **Download** buttons.

## Requirements

| Dependency | Purpose |
| --- | --- |
| [VICE](https://vice-emu.sourceforge.io/) (`x64sc`) | Launching games and demos |
| GTK 4 + libadwaita | UI toolkit (usually pre-installed on modern desktops) |

C64 Forever is optional — breadbin detects it automatically and uses it for ROM
support when present. Without it, VICE's bundled free ROMs are used.

Point breadbin at your game collection with the `C64_LIB` environment variable
(default `~/Games/Commodore/C64`):

```sh
export C64_LIB="/path/to/your/c64/games"
```

## Install

### Arch Linux (recommended)

Build and install system-wide (build runs as you; only the install step needs root):

```sh
packaging/install.sh build   # cargo build --release (as your user)
sudo packaging/install.sh    # copy files into /usr (as root)
```

Then launch from your application menu or run `breadbin` in a terminal.

To build an AUR-style package instead:

```sh
cd packaging/arch
makepkg -si
```

### Ubuntu / Debian

```sh
packaging/install.sh build
sudo packaging/install.sh
```

Or build a `.deb`:

```sh
cp -r packaging/debian debian
dpkg-buildpackage -us -uc -b
sudo dpkg -i ../breadbin_*.deb
```

### From source (dev loop)

```sh
cargo build          # debug build — no --release needed
./breadbin.sh        # sets GSETTINGS_SCHEMA_DIR and launches
```

`breadbin.sh` picks up `target/release/breadbin` if present, falls back to
`target/debug/breadbin`.

## How data is managed

Breadbin stores everything in `~/.breadbin/` (override with `BREADBIN_HOME`).
On first run it downloads and builds the indexes it needs — nothing is bundled
in the binary or the package:

| File / folder | Contents |
| --- | --- |
| `c64_index.tsv` | Ranked game catalogue (GameBase64 + Internet Archive) |
| `gb64.sqlitedb` | GameBase64 database (~17 MB, cached once) |
| `covers/` + `covers_index.tsv` | Box-art thumbnails (Libretro set) |
| `demos_index.json` | Demoscene index from CSDb |
| `sids/` | SID music files |

Use **Refresh catalogue** in Preferences to rebuild the game index.

## Configuration

Open **Preferences** (the gear icon in the header bar) to set:

- Path to your C64 game collection
- C64 Forever ROM directory
- Launch options: warp speed, fullscreen, drive sound

## Where games come from

When a game isn't in your collection, clicking **Download** in the detail dialog
fetches it from public preservation archives:

- **Internet Archive** — `softwarelibrary_c64` cracked disk dumps
- **Ultimate Tape Archive** — tape preservation

Downloads land in your collection automatically.
