# Porting breadbin to Rust

A plan + progress log for rewriting the breadbin C64 toolkit (currently ~2,475
lines of single-file Python 3 scripts) as a single Rust binary.

## Decisions (agreed up front)

1. **Packaging — one multi-call binary.** A single `breadbin` binary with
   `clap`-style subcommands plus busybox-style `argv[0]` dispatch: symlinks named
   `c64run`, `c64menu`, … invoke the matching tool directly, so the standalone
   commands keep working. (Replaces both the Python umbrella and the per-tool
   scripts.)
2. **TUIs — ratatui rewrite.** The two terminal UIs (`c64menu`, `c64kiosk`) are
   rebuilt on `ratatui`, with covers drawn via **`ratatui-image`** (its `Picker`
   auto-detects WezTerm's image protocol). We do **not** shell out to
   `wezterm imgcat` from the TUIs — that fights ratatui's diffed back-buffer.
   (`imgcat` is fine for the non-TUI `bb-cover` preview path if we keep one.)
3. **Sequencing — incremental, start with `c64run`.** Port the simplest real tool
   end-to-end first to validate the approach, then work up to the TUIs.

## Layout

```
rust/
  Cargo.toml          # one bin crate "breadbin"
  src/
    main.rs           # argv[0] + subcommand dispatch (umbrella)
    run.rs            # c64run            [DONE]
    core/             # shared lib: norm, http fetch, covers, gb64, terminal  [TODO]
    build_index.rs    # index builder + norm()                                [TODO]
    cover.rs          # cover index + download (was bb-cover)                  [TODO]
    info.rs           # gb64 sqlite reader (was c64info)                       [TODO]
    get.rs disk.rs tosec.rs   # archive downloaders                           [DONE]
    tui.rs            # shared TUI data/actions (rows, genres, covers, play)  [DONE]
    menu.rs kiosk.rs  # ratatui TUIs                                          [DONE]
  PORTING.md          # this file
```

Symlinks `c64run -> breadbin`, `c64menu -> breadbin`, … get installed alongside
the binary so the standalone names resolve.

## Tool inventory (Python → Rust)

| Tool | Lines | Core of it | Status |
|---|---|---|---|
| `breadbin` | 53 | umbrella dispatch | **done** (main.rs) |
| `c64run` | 174 | walk collection, pick VICE, exec | **done** (run.rs) |
| `build_index.py` | 145 | match popularity TSV vs collection; `norm()` everywhere | **done** (build_index.rs) |
| `bb-cover` | 98 | cover index + HTTP download (libretro raw) | **done** (cover.rs; fzf-preview CLI deferred) |
| `c64info` | 215 | read `gb64.sqlitedb`, gzip cache | **done** (info.rs; interactive fzf pick deferred) |
| `c64get` | 167 | Ultimate Tape Archive download | **done** (get.rs) |
| `c64tosec` | 87 | TOSEC catalogue browse | **done** (tosec.rs; cover preview dropped) |
| `c64disk` | 431 | IA / TOSEC download, fuzzy match | **done** (disk.rs; interactive fzf pick deferred) |
| `c64menu` | 574 | master/detail TUI + shared helpers | **done** (menu.rs; shared helpers in tui.rs) |
| `c64kiosk` | 531 | cover-grid TUI (reuses menu helpers) | **done** (kiosk.rs) |

## stdlib → crate mapping

- `urllib.request` → **`ureq`** (blocking, matches the synchronous style)
- `sqlite3` → **`rusqlite`** (bundled) · `gzip` → **`flate2`** · `zipfile` →
  **`zip`** · `json` → **`serde_json`**
- manual PNG/JPEG header parse (`image_size`) → **`imagesize`**
- `shutil.which` → **`which`** · arg parsing → **`clap`**
- `os.execvp` → **`std::os::unix::process::CommandExt::exec`**
- `shlex.split` (C64_EMU) → **`shlex`**
- raw mode + keys + mouse + size (TUIs) → **`ratatui`** + **`crossterm`**
- covers in the TUIs → **`ratatui-image`** (WezTerm protocol via `Picker`)
- cell *pixel* size (`TIOCGWINSZ` xpixel/ypixel, used by `cell_px`) →
  small **`rustix`**/`libc` ioctl if still needed outside ratatui-image

## Parity risks to watch

1. **`difflib.SequenceMatcher.ratio()`** in `c64disk` — RESOLVED. Hand-ported in
   `core::seq_ratio` (recursive longest-match, autojunk omitted since title keys
   are < 200 chars). Verified byte-identical to Python over 1,800 pairs at 6 dp.
2. **Shared helpers** in `c64menu` (`norm`, `fetch`, `canon_of`, cover index,
   `cell_px`) become the `core` module — the one place the `importlib`
   cross-script loading collapses into.
3. **`norm()`** is used by index building, cover canon, and matching — it must be
   a byte-for-byte port; everything keys off it.
4. Data files are unchanged and shared with the Python tools during the port:
   `c64_index.tsv`, `gb64.sqlitedb`, `covers/` + `covers_index.tsv`,
   `ia_index.tsv`, `played.tsv`, `downloaded.tsv`. Keep formats identical so the
   two implementations can coexist while porting.

## Progress log

- **2026-06-16** — Installed rustup (rustc 1.96.0) with `--no-modify-path`.
  Scaffolded `rust/` cargo project (single `breadbin` bin). Ported `c64run`
  (`run.rs`) and the umbrella dispatch (`main.rs`). Verified: `--help`, the
  "no game matching" path, and `-l <query>` produce output identical to the
  Python `c64run` against the real `~/Games/Commodore/C64` collection
  (`wizball` → identical match list). Builds clean, no warnings.
- **2026-06-16** — Ported the data tools. `core.rs` (data-dir, `norm`,
  `title_key`, `fetch` via ureq, `walk_files`), `build_index.rs`, `cover.rs`
  (was bb-cover), `info.rs` (was c64info, rusqlite+flate2, cp1252 decode).
  Parity validated against Python:
    - `norm()` byte-identical over 27,884 strings (IA titles + filenames + edge cases).
    - `build_index` → byte-identical `c64_index.tsv` (13,902 lines).
    - cover `load_index` count identical (2,276); `ensure_cover` paths match;
      downloaded an uncached cover to confirm `quote()` URL-encoding.
    - `c64info` stdout + exit code identical over 20 detail queries and 3
      `--all` listings (incl. cp1252 Bruce Lee crack-doc, ambiguous Zak).
  Deferred (need fzf/tty interactivity, will revisit with the TUIs): bb-cover's
  fzf-preview CLI and c64info's interactive fzf picker.
  Hidden dev subcommands for testing: `_norm`, `_cover`, `index`.
- **2026-06-16** — Ported `c64disk` (`disk.rs`, ~600 lines): IA + TOSEC sources,
  `--ia-index`/`--missing`/`--id`/`--source`/`-n`, JSON (serde_json), zip (zip
  crate), gb64_meta via `info::connect`. Added `core::seq_ratio` (difflib),
  `core::quote`/`unquote`/`urlencode`. Parity validated against Python:
    - `game_title` byte-identical over 4,000 titles.
    - `seq_ratio` identical over 1,800 pairs (6 dp) — the difflib risk is closed.
    - `disk -n <query>` (wizball, boulder dash, commando) → stderr identical
      end-to-end against live archive.org (search + metadata + resolve + dry run).
  Added hidden `_disk title|ratio` dev subcommand.
  Interactive fzf pick in `disk::resolve` is wired (spawns fzf) but untested;
  the non-interactive "list and bail" path is the one verified.
- **2026-06-16** — Ported `c64get` (`get.rs`) and `c64tosec` (`tosec.rs`),
  finishing task #4 (all download tools). `get -n <query>` (wizball, commando,
  boulder dash) → stderr byte-identical to Python end-to-end against live UTA
  (index + per-folder .tap listing). `c64tosec` is a thin wrapper over the
  verified `disk::` functions: fzf browse → download release → rebuild index →
  exec c64run; the cover preview (bb-cover fzf CLI) is dropped.
  Remaining: task #5 (the two ratatui TUIs) and task #6 (symlinks/README).
  Five of seven user-facing tools now run on Rust; `c64menu`/`c64kiosk` still
  print "not ported to Rust yet".
- **2026-06-17** — Ported the two TUIs, finishing the port. `tui.rs` holds the
  shared model/actions (the old importlib helpers): `Row`/`load_rows`,
  `group_by_genre`, `canon_of`/`cover_for`, `record_play`/`recent_plays`,
  `resolve`, `refresh`, in-place `launch_inplace` and exec `play_exec`.
  `menu.rs` is the master/detail picker: genre-grouped collapsible list, type-to-
  filter, →/Tab to open a detail card (inline cover + GameBase64 facts via
  `info::best_match`/`record`), Enter to play-owned / fetch-and-play, mouse +
  wheel. `kiosk.rs` is the cover grid: genre overview with a "latest played" row,
  Enter/click a title to expand to a full grid, click/Enter a cover to download-
  and-play in place (returns to the kiosk). Covers render via **ratatui-image**
  (`Picker::from_query_stdio` → WezTerm iTerm2 protocol, half-block fallback
  everywhere else). Verified end-to-end over a pty (pyte): both TUIs render,
  navigate, show covers + details, and exit cleanly; index/genre/latest-played
  all populate from the real data.
  Also made breadbin a **self-contained, publishable crate**: the data dir now
  defaults to the user data dir (`~/.breadbin`), and the menu/kiosk bootstrap a
  fresh install by running `refresh()` (catalogue + index; GB64 DB and covers
  download on demand) — no bundled files. Added `breadbin install-links` to drop
  the `c64*` symlinks next to the binary, crates.io metadata + `README.md` +
  `LICENSE`; `cargo package` verifies. Dropped the unused `imagesize` dep.

## Build / try it

```sh
. "$HOME/.cargo/env"
cd rust && cargo build
./target/debug/breadbin run -l wizball      # list matches (no launch)
./target/debug/breadbin play wizball        # boot into VICE
./target/debug/breadbin menu                # master/detail picker (TUI)
./target/debug/breadbin kiosk               # cover grid (TUI, default)

# Publishable, self-bootstrapping binary (no repo/data files needed):
cargo install --path .        # or: cargo install breadbin
breadbin install-links        # create c64run, c64menu, … next to the binary
breadbin                      # first run downloads + builds everything into ~/.breadbin
```

During the port, point `BREADBIN_HOME` (and `BREADBIN_USER_DATA`) at this repo to
share the existing data files with the Python tools instead of bootstrapping.
