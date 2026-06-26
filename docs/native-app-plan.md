# breadbin as a native Linux app — implementation plan

Turning breadbin from a terminal toolkit into a **native Linux desktop app** in the
spirit of [Cartridges](https://apps.gnome.org/Cartridges/): a clean libadwaita game
library you double‑click from your menu, browse as a grid of cover art, and launch
games from. It keeps breadbin's three signature experiences — the **kiosk** (cover
library), **tunes** (SID jukebox) and **demos** (demoscene browser) — drops the
command‑line surface, stays in Rust, and installs cleanly on Debian, Ubuntu and Arch.

---

## 1. Goal & scope

**In scope**

- A GTK4 / libadwaita desktop application (window, menu entry, icon, AppStream metadata).
- **Kiosk mode** — a Cartridges‑style cover grid: browse by genre/collection, search,
  fetch‑on‑demand, launch into VICE. This *absorbs* today's `menu` (search/filter/
  fetch) so there is one library view, not two.
- **Tunes mode** — the SID jukebox: ranked tunes by party, in‑app playback (pure‑Rust
  reSID + cpal), live visualisers, radio mode.
- **Demos mode** — the demoscene browser: top CSDb demos by party, shown as screenshot
  covers; download‑and‑run a demo in VICE.
- Packaging for Debian/Ubuntu (`.deb` + Flatpak) and Arch (PKGBUILD/AUR + Flatpak).

**Explicitly out of scope (per the request)**

- The command‑line bindings: the busybox‑style `argv[0]` dispatch, the `c64*`
  symlinks, `install-links`, and the user‑facing subcommands (`play`, `info`, `get`,
  `disk`, `tosec`, …). The app launches straight into the GUI.
- `menu` as a separate view — **dropped**; its search/filter/fetch‑on‑demand are folded
  into the kiosk so there is one library view, not two.

---

## 2. The reuse map — what stays, what goes

breadbin is already well factored: a backend that knows nothing about the terminal,
and a thin ratatui UI layer on top. The port keeps the backend almost verbatim and
replaces only the UI.

| Module | Role | Fate in the GUI |
| --- | --- | --- |
| `core.rs` | title norm, data dirs, HTTP fetch, CSDb scraping, fuzzy match, palette | **Keep** (drop `Progress`'s stderr bar; expose progress as callbacks) |
| `tui.rs` | catalogue data model (`Row`), grouping, cover resolve, play log, download‑on‑demand, launch | **Keep the data model; rewrite the launch/resolve plumbing** (see §4) |
| `cover.rs` | libretro/IA boxart cache | **Keep** — render path gets *simpler* (no terminal graphics protocol) |
| `disk.rs`, `get.rs`, `tosec.rs` | download from IA / UTA / TOSEC | **Keep**; call as library functions, not subprocesses |
| `info.rs` | GameBase64 SQLite details | **Keep**; surface in a game‑detail pane |
| `build_index.rs` | build/refresh the ranked index | **Keep**; drive from a GUI "refresh" action with a progress view |
| `run.rs` | pick VICE, build flags, joystick/controls detection | **Keep the flag/detection logic; change the launch mechanism** (spawn, not `exec`) |
| `sid.rs` | pure‑Rust 6502 + reSID engine | **Keep verbatim** (already UI‑agnostic) |
| tunes audio (`Audio`, cpal stream, generator thread) | live SID playback + `Vis`/scope snapshots | **Keep verbatim** — it's already thread‑based and GUI‑agnostic |
| `demos.rs` backend (`ensure_shot`, `ensure_demo`/download, CSDb scrape, `group_by_party`) | demo screenshots + runnable image fetch + ranking | **Keep**; call as library functions |
| badge compositing (`draw_joystick_badge`, `draw_rating_badge`, …) | paint badges into `image::RgbaImage` | **Keep** — operates on a bitmap, independent of how it's displayed |
| visualiser math (`draw_scope`/`fireball`/`cubes`, `rot3`, `project`, `heat_color`) | the geometry/heat ramps | **Reimagine as GLSL shaders** on a `GtkGLArea`; reuse the concepts/palette, not the braille point code (§8) |
| `kiosk.rs`, `menu.rs`, `tunes.rs`, `demos.rs` (UI parts), `grid.rs`, `boot.rs` | ratatui rendering + event loops + navigation | **Replace** with GTK widgets/views |
| `main.rs` | busybox dispatch, CLI, hidden dev aids | **Replace** with a GTK `main` + `Application` (dev aids → tests) |

---

## 3. Proposed architecture — a Cargo workspace

Split the single binary into a library + a GUI binary. This makes the "keep the
backend, replace the UI" boundary explicit and testable.

```
breadbin/
  Cargo.toml                # [workspace]
  crates/
    breadbin-core/          # the reusable backend (lib)
      src/
        core.rs  tui.rs(→catalog.rs)  cover.rs  disk.rs  get.rs
        tosec.rs  info.rs  build_index.rs  run.rs(→launch.rs)
        sid.rs   demos.rs(→demo backend)
        lib.rs              # public API surface (see §4)
    breadbin-gui/           # the GTK4/libadwaita app (bin)
      src/
        main.rs            # adw::Application setup
        app.rs             # top-level window, view switcher (Kiosk / Tunes / Demos)
        kiosk/             # library grid, detail pane, search, launch dialog
        tunes/             # jukebox list, player, visualisers
        demos/             # demo screenshot grid by party, download + launch
        widgets/           # cover/screenshot card, badge, progress, controls dialog
        config.rs          # GSettings <-> Settings struct
      data/
        *.desktop  *.metainfo.xml  *.gschema.xml  icons/  blueprints or .ui
```

- `breadbin-core` exposes a clean, **synchronous** API (it already is) plus
  callback/channel hooks for progress. The GUI runs blocking core calls on worker
  threads and marshals results back to the main loop (glib channel / `MainContext`).
- Keep `resid-rs` overflow‑checks workaround and the `[profile.release] strip` in the
  workspace root.

**GUI toolkit decision.** `gtk4-rs` + `libadwaita` (the `adw` crate). This is the
unambiguous choice: it is what "native Linux app like Cartridges" means, it's mature
in Rust, and it gives us `AdwApplicationWindow`, `AdwViewSwitcher`, `GtkGridView`/
`FlowBox`, `AdwPreferencesWindow`, etc. for free.
*Optional:* layer **relm4** on top for Elm‑style state management — attractive here
because we have two modes and live, ticking audio/visualiser state. Recommendation:
start with plain `gtk4-rs` + `glib` channels for v1 (fewer moving parts), adopt relm4
only if state wiring gets unwieldy. (Iced/egui were considered and rejected: they
don't give the native GNOME look‑and‑feel the request is asking for.)

---

## 4. The load‑bearing change: spawn, don't exec; internalise the RPC

This is the single most important refactor and is easy to miss.

**Problem A — the app must not replace itself with VICE.** Every current launch path
ends in `cmd.exec()` (`run.rs` / `play_exec`), which *replaces* the process. A
desktop app must stay alive while a game runs (like Cartridges), so it can return to
the grid and refresh "recently played" when the game exits.

> Fix: add a `launch::spawn(game, opts) -> std::io::Result<Child>` to core that
> `Command::spawn()`s VICE (never `exec`), returning the child. The GUI either fires
> and forgets, or watches the child on a worker thread and pings the main loop on exit
> to refresh the "recently played" row. `run.rs`'s flag‑probing and joystick/controls
> logic stay as‑is; the final `exec` becomes `spawn`; and the emulator‑pick / drive‑mode
> logic is reworked for the C64 Forever ROM policy (see §4b).

**Problem B — the codebase uses `current_exe()` re‑invocation as internal RPC.**
`tui::resolve` shells out to `breadbin disk --id …` and `breadbin index`;
`launch_inplace` shells out to `breadbin play …`; `refresh` shells out to
`disk --ia-index` + `index`. Once the subcommand dispatch is gone, these calls have
nothing to re‑exec.

> Fix: turn each into a direct library call. `disk::download_by_id(...) -> PathBuf`,
> `build_index::build(...)`, `build_index::refresh_ia(...)`, `launch::spawn(...)` —
> all already exist as logic inside the subcommand `main(args)` functions; extract the
> bodies into typed functions and have the (now‑deleted) CLI shims fall away. This
> also removes the brittle "parse the boot path off the child's stdout" step in
> `resolve`.

**Threading model.** All of the above are blocking (network, disk, VICE). Run them on
a `std::thread` / small pool; report progress and completion to the GTK main loop via
a `glib::MainContext` channel. Never block the UI thread.

---

## 4b. Emulation & ROM policy — requires C64 Forever

**Policy (firm):** the app runs games/demos using the **licensed Commodore ROMs the
user already owns via [Cloanto C64 Forever](https://www.c64forever.com/)** (KERNAL,
BASIC, CHARGEN, and the 1541 drive ROM). **breadbin never downloads ROMs from the
internet** — not the free‑VICE ROMs, not anything. No ROM ever ships in the package or
is fetched at runtime.

**Draw the line clearly — ROMs vs game images:**

- **NOT downloaded — copyrighted Commodore *ROMs*** (KERNAL/BASIC/CHARGEN/1541). These
  come *only* from the user's licensed C64 Forever copy.
- **Downloaded as before — *game and demo images* (the user's software library):** disk
  images from the **Internet Archive** (`softwarelibrary_c64` / TOSEC) and tape images
  from the **Ultimate Tape Archive**, plus covers, GameBase64 details and SID files.
  This is breadbin's whole point (`disk.rs`/`get.rs`/`tosec.rs`/`cover.rs`) and is
  unchanged — the C64 Forever requirement is purely about the system ROMs, not about
  where games come from.

This **reverses today's `run.rs` ordering.** Currently `run.rs` prefers a *license‑free*
VICE (distro `x64sc`/flatpak with its bundled free ROMs) and only falls back to C64
Forever. The new order is C64‑Forever‑first, and the free‑ROM path is dropped:

1. **Locate the user's C64 Forever ROM set** — from its install dir (the existing
   `WINE_X64` Cloanto path on Linux/wine, or a configured `C64_FOREVER_ROMS` /
   preferences folder). Point native VICE at those ROMs (`-kernal/-basic/-chargen` and
   the `DRIVES` path / `VICE_DATADIR`), **or** launch Cloanto's bundled VICE (`x64.exe`
   via wine) directly. Either way the ROMs come only from the user's licensed copy.
2. **If C64 Forever's ROMs aren't found:** do **not** silently substitute free ROMs and
   do **not** download. Instead surface the requirement (see startup notice below) and
   disable the *launch* actions until a valid C64 Forever ROM location is set.

**What still works without C64 Forever:** the **library browsing, search, downloads,
and especially Tunes** — the pure‑Rust SID engine (`sid.rs`) synthesises audio with no
Commodore ROM at all, so the jukebox is fully usable. Only *running* a game or a demo in
VICE needs the ROMs. The UI should reflect this: Tunes always available; Play/Run gated
on ROM presence.

**Startup notice (required).** On launch, state plainly that **breadbin requires a copy
of C64 Forever** for its licensed ROMs, with a link to obtain it and a button to point
the app at the install. Show it as: (a) a first‑run dialog/`AdwStatusPage`, and (b) a
persistent, dismissible banner whenever ROMs are missing. Keep it in C64 idiom (§6c) —
e.g. a `READY.`‑style screen reading *"breadbin needs your licensed C64 Forever ROMs to
run games. Tunes work without them."*

**Knock‑on simplifications in `run.rs`:** the `missing_tde_rom`/virtual‑drive
auto‑fallback existed only to cope with a free distro VICE lacking the non‑free 1541
drive ROM. With the licensed C64 Forever drive ROM guaranteed present before launch,
**True Drive Emulation is always available** and the ROM‑less virtual‑drive workaround
can be retired (keep `C64_VIRTUAL_DRIVE` only as a manual advanced override).

---

## 5. Kiosk mode — the Cartridges‑inspired library

The marquee view. Mirrors Cartridges' UX, specialised for C64.

**Layout**
- `AdwApplicationWindow` with an `AdwHeaderBar`: title, a search toggle, a view
  switcher (Kiosk ⇄ Tunes), and a hamburger menu (Refresh catalogue, Preferences,
  About).
- Main area: a responsive **cover grid** (`GtkGridView` with a `GtkGridView`/`FlowBox`
  of cover *cards*) that reflows with window width — exactly Cartridges' feel.
- Covers come straight from `cover::ensure_cover` as files → load into `GdkTexture` →
  `GtkPicture`. The joystick/rating **badges** are composited into the `RgbaImage`
  first (reuse `draw_*_badge`), then uploaded as the texture. No terminal graphics
  protocol, no halfblocks fallback — this is the big simplification.

**Organisation** — preserve the kiosk's curated structure:
- Section bands like Cartridges collections: **Latest played**, **Classics**, the
  `collections.tsv` sets, then genres (Arcade and Shoot'em Up pinned, "Other" last) —
  reuse the exact ordering logic in `kiosk::KioskState::new`.
- Implement as either an `AdwViewStack` of sections or a single scrolled grid with
  sticky section headers (recommended: a `GtkListView`/`GridView` with section header
  factory).

**Search & fetch‑on‑demand (absorbs `menu`)**
- Header search filters the grid live (reuse `core::norm`/title matching).
- A game you don't own shows a "download" affordance on its card; activating it runs
  `resolve()` on a worker thread with a progress spinner, then enables play. This is
  `menu`'s `o` (owned) / `v` (downloadable) distinction, made visual.

**Game detail**
- Selecting a card opens an `AdwBottomSheet`/detail pane (or navigation page) with the
  large cover and GameBase64 facts from `info.rs` (year, genre, author, rating), a
  **Play** button, and a **Download** button when not local.

**Launch flow**
- **Play** → a **Controls dialog** (`AdwAlertDialog`) showing the same scheme
  `run::controls_description(joystick_present())` produces today (PlayStation/THEC64/
  generic icons included), with "Start"/"Cancel". On Start: `record_play`, then
  `launch::spawn`. Window stays open; on the game's exit, refresh "Latest played".

**Big‑picture / kiosk fullscreen (optional, true to the name)**
- An F11 fullscreen mode with larger cards and **gamepad navigation** (the app already
  detects controllers in `run.rs`). This is the literal "kiosk": couch‑friendly,
  controller‑driven cover browsing. Ship basic fullscreen in v1, gamepad nav as a
  follow‑up.

---

## 6. Tunes mode — the SID jukebox view

The audio engine ports unchanged; only the rendering changes.

**Browser**
- A `GtkListView` grouped by party (reuse `group_by_party`), each row showing
  title / rating / group / year, a ▶ marker on the now‑playing tune, a ★ on top‑rated.
- Activate a row → play. A **Radio** toggle in the header starts random‑tune/random‑
  visual mode (reuse `radio_next`, the `RADIO_SECS` timer, the xorshift `Rng`).

**Player**
- A "now playing" header (title, composer, group, year, elapsed), transport controls
  (play/pause, next), and a visualiser canvas.
- Audio: keep `Audio`/`build_stream`/the generator thread and `ensure_sid` exactly.
  The UI reads `audio.snapshot()` (`Vis`) and `audio.scope()` on a GTK tick.

**Visualisers — GPU shaders, not text art**
- Drop the braille/text aesthetic entirely. The ratatui `Canvas` versions only used
  braille points because the terminal had nothing else; a native app has a GPU, so the
  visualisers become proper **GLSL fragment shaders** rendered on a `GtkGLArea`. This
  removes the braille‑grid sampling caps and the CPU per‑cell point loops; the GPU does
  the per‑pixel work full‑resolution at the display refresh rate.
- **Pipeline.** `GtkGLArea` with a `render` signal + `add_tick_callback` (~60 Hz)
  pumping `queue_render`. A full‑screen quad runs a fragment shader per visualiser. The
  live audio drives the shader through **uniforms / a small data texture**, fed from
  `audio.snapshot()` (`Vis`) and `audio.scope()` each frame:
  - `u_time`, `u_resolution`
  - `u_volume`, per‑voice `freq`/`gate`/`waveform`/`sustain` (from `Vis`) as a uniform array
  - the 256‑sample scope buffer (`SCOPE_PTS`) uploaded as a 1D/`R16`/float texture for
    the oscilloscope and as an energy/FFT source for the reactive effects.
- **The math we keep is conceptual, not code.** The fireball heat‑ramp, the marching
  raster‑bar palette, the C64 colour identity — re‑expressed as shader functions
  (`heat_color` → a GLSL gradient `mix`, the C64 palette → a `const vec3[]`). The
  cubes/3D math (`rot3`/`project`) is trivially redone in‑shader or via a vertex shader.
  This is a rewrite *upgrade*, not a 1:1 port.
- **Effects.** Start by re‑imagining the existing three as shaders — a glowing
  oscilloscope with bloom, a raymarched/turbulent fireball, a tumbling reactive cube
  field — then add scene‑grade extras (plasma, tunnels, raster bars, starfields) that
  were impossible in the terminal. Each shader is a small `.frag` asset; `VisMode`
  becomes "which shader is bound", and radio mode cycles them.
- **Portability.** Target GLSL ES 3.0 / OpenGL ES 2–3 (what `GtkGLArea` gives across
  Mesa/Wayland/X11); keep shaders simple enough to compile everywhere and degrade
  gracefully (a plain scope) if `GtkGLArea` reports no GL.

---

## 6b. Demos mode — the demoscene browser

Structurally this is the **kiosk grid applied to demos**, so it reuses both the kiosk's
card/grid widgets and tunes' party grouping — very little net‑new UI.

**Browser**
- A screenshot **cover grid grouped by party** (reuse `core::group_by_party`), each
  party showing its top‑rated demos. Screenshots come from `ensure_shot` (cached like
  game covers) → `GdkTexture`/`GtkPicture`. Same section‑header + reflow grid as the
  kiosk, so the `widgets/` card and grid factories are shared.
- Search filters by title/group; section headers carry the party name and demo count.

**Launch flow**
- Activate a demo → `ensure_demo`/download fetches the runnable image
  (`.d64`/`.prg`/`.crt`/…) on a worker thread with a progress spinner, then
  `launch::spawn` runs it in VICE (same spawn‑not‑exec path as games; demos benefit
  from the same warp/fullscreen runopts). No controls dialog needed — demos aren't
  interactive — though we still keep the window alive and refresh on exit.

This view is the reason the demo backend stays in `breadbin-core`; only `demos.rs`'s
ratatui rendering/navigation is replaced.

---

## 6c. Visual identity — make it look like a real C64

The app should *read as a Commodore 64*, not a generic launcher. We stay a proper
libadwaita app structurally (real `adw` widgets, adaptive layouts, GNOME HIG
behaviours) but **skin it as a C64** wherever it doesn't fight usability. "When you
can, make it look like an actual C64."

**Palette — one source of truth.**
Promote `core::palette` (the Pepto VIC‑II values already in the code) to *the* theme
source. Default surface = `SCREEN` dark blue, default text/lines = `LIGHTBLUE`, accents
in `YELLOW`/`CYAN`/`LIGHTGREEN`, with the raster‑bar set (`BARS`) used for section
headers (`palette::bar_for`). Generate GTK named colours from these so every widget
inherits the 16‑colour look.

**Font — PETSCII/CBM (licensing resolved; libre by default).**
Use a Commodore pixel font for headings, list rows, the boot splash and chrome; keep a
clean fallback sans for long descriptions where pixel type hurts readability.

- **Default (shipped everywhere): *Unscii*** (`unscii-16`, **public domain**; already
  packaged in Guix/pkgsrc/NetBSD and carries PETSCII glyphs). Zero licensing friction —
  it bundles cleanly in the Flatpak, `.deb`, the AUR package, *and* Debian's official
  archive with no special clauses. **Avoid the `unscii-16-full` variant — it's GPL** (it
  bundles Unifont data); the base variants are public domain.
- **Optional authentic upgrade: Style64 *C64 Pro Mono*** (TrueType, the genuine C64
  look). Its license forbids selling, repackaging into font collections, or offering
  direct download — *but explicitly permits* including it **unmodified, under its
  original filenames, in a freely‑provided software package** (which breadbin is). We do
  **not** make it the default (to keep the package unambiguously libre); offer it as an
  opt‑in the user can drop in, or as a separate non‑default package, with the
  "unmodified / original filenames / free app" constraints honoured. The font setting
  (§8) lets the user switch to it when present.
- Note: Unicode 13.0's "Symbols for Legacy Computing" block now encodes PETSCII glyphs
  properly, so either font maps cleanly via real codepoints rather than PUA hacks.

**Theming over libadwaita.**
Ship a custom `style.css` loaded via `AdwStyleManager`/`GtkCssProvider` that recolours
adw widgets to the palette: dark `SCREEN` window, `LIGHTBLUE` text, chunky `YELLOW`
focus rings, and **PETSCII rounded borders** (reuse the `PETSCII_BORDER` concept as a
CSS border / 9‑slice) around cover cards and dialogs. Section headers render as C64
colour‑bar chips. The header bar and buttons get the pixel font. Net effect: a GNOME
app wearing a C64 costume.

**Boot & loading screens like the machine itself.**
- **App splash / first run:** reproduce the power‑on screen — blue border, the
  `**** COMMODORE 64 BASIC V2 ****  64K RAM SYSTEM  38911 BASIC BYTES FREE` banner,
  `READY.` and a blinking block cursor. Reuse `boot.rs`'s `boot_screen` concept,
  redrawn as a GTK widget; it doubles as the first‑run catalogue‑build screen. This is
  also where the **C64 Forever requirement notice** (§4b) lives — phrased in C64 idiom.
- **Downloads / index build:** present progress in C64 idiom — `SEARCHING FOR …`,
  `LOADING`, then `READY.` — with the blinking cursor as the "spinner", optionally with
  the 1541 drive‑load sound (we already drive VICE's drive sound; the same sample can
  cue here).
- **Launch transition:** a brief `LOAD"*",8,1 : RUN` screen (mirroring `c64run`'s
  banner) before VICE takes over.

**CRT treatment (reuses the §8 GL pipeline).**
Because the visualisers already run through `GtkGLArea`, add an optional **CRT
post‑process** fragment shader — scanlines, gentle barrel curvature, phosphor glow,
vignette. Strongest in the fullscreen "kiosk"/big‑picture mode and on the tunes/demos
screens; lighter or off for the library grid so covers stay crisp. A preferences toggle
controls it (default: on for fullscreen kiosk, off for the windowed library).

This identity is cross‑cutting: §5 cards, §6/§6b headers, §7 bootstrap and §8
visualisers all draw from this section.

---

## 7. First‑run bootstrap & download UX

Today first run silently builds the catalogue with stderr progress bars; that doesn't
exist in a GUI.

- **First launch:** if `c64_index.tsv` is missing/empty, show an `AdwStatusPage`
  "Setting up your library…" with a progress bar while `refresh_ia` + `build_index`
  run on a worker thread (`core::Progress` → a callback that updates the bar via the
  glib channel). Replace `Progress`'s `is_terminal` stderr drawing with a
  `Fn(done, total)` sink so both CLI‑less GUI and tests can consume it.
- **Per‑game download:** card/detail shows a spinner + percentage during `resolve()`.
- **Refresh catalogue:** a menu action re‑runs the index build behind the same
  progress UI (replaces `menu --refresh`).
- **Offline:** everything cached under the data dir keeps working (catalogue, covers,
  SQLite, SIDs) — unchanged behaviour.

---

## 8. Configuration — env vars → libadwaita preferences

Today all behaviour is env‑driven. Replace with an `AdwPreferencesWindow` backed by
**GSettings** (a `.gschema.xml`), still honouring env overrides for power users.

| Today (env) | Preference |
| --- | --- |
| `C64_LIB` | "Games folder" file chooser |
| `C64_EMU` | "Emulator command" (advanced; default = auto‑pick) |
| *(new)* `C64_FOREVER_ROMS` | **"C64 Forever ROM folder"** — points VICE at the user's licensed ROMs (§4b); launch is gated until set |
| `-w/--warp` (default on) | "Fast‑forward loading" toggle |
| `-f/--fullscreen` (default on) | "Launch games fullscreen" toggle |
| `C64_DRIVE_SOUND`, `…_VOLUME` | "1541 drive sound" toggle + volume |
| `C64_JOYSTICK`, `C64_JOYDEV`, `C64_JOYMAP` | "Controller" section (auto‑detect + overrides) |
| `C64_VIRTUAL_DRIVE` | advanced: "Force virtual drive" |
| `BREADBIN_USER_DATA` / `BREADBIN_HOME` | follow XDG (`$XDG_DATA_HOME/breadbin`); keep env override |
| *(new)* | "CRT effect" toggle (§6c) — default on in fullscreen kiosk, off in the windowed library |
| *(new)* | "Drive‑load sounds in the UI" toggle (the C64 loading idiom, §6c) |
| *(new)* | "Display font" — default *Unscii* (libre); switch to Style64 *C64 Pro Mono* when installed (§6c) |

Map settings → the `Vec<String>` runopts `launch::spawn` consumes, so `run.rs`'s flag
logic is untouched.

---

## 9. Dropping the CLI cleanly

- Delete `main.rs`'s dispatch, `TOOL_NAMES`, `install_links`, `resolve_subcommand`,
  `run_tool`, the `_norm`/`_cover`/`_disk`/`_sid` dev aids (move their checks into
  `#[cfg(test)]` tests in `breadbin-core`).
- New `breadbin-gui/src/main.rs`: build an `adw::Application`
  (`app_id = "io.github.jacobandresen.Breadbin"`), `ensure_user_data_dir()`, present
  the window.
- The binary takes no meaningful args (maybe `--version`). No subcommands ship.

---

## 10. Packaging & distribution

Two complementary tracks. **Flatpak is the recommended primary** (one artifact for
all three distros, matches Cartridges' Flathub model); native packages serve users
who prefer their distro's tooling.

### A. Flatpak (primary, universal)
- Manifest `io.github.jacobandresen.Breadbin.yaml`, runtime `org.gnome.Platform`
  (current stable), `rust-stable` SDK extension; build with
  `flatpak-cargo-generator` for offline cargo deps.
- **Sandbox caveat (must address):** a sandboxed app can't `exec` host `x64sc` or read
  `~/Games` directly. Options:
  1. **`flatpak-spawn --host`** to launch VICE on the host (Cartridges launches games
     this way), plus `--filesystem=` permission for the games folder. Note the
     existing `net.sf.VICE` flatpak path becomes *flatpak‑spawn → flatpak run
     net.sf.VICE*.
  2. Bundle nothing for emulation; require host VICE and document the perms.
  - Audio (`cpal`/PulseAudio) and the network (catalogue/cover/SID downloads) need
    `--socket=pulseaudio`, `--share=network`. SID playback works fully inside the
    sandbox; only *game launching* crosses to the host.
- Ship AppStream metainfo + icon (required for Flathub).

### B. Native `.deb` (Debian/Ubuntu)
- `cargo-deb` to produce a package. `Depends: libgtk-4-1, libadwaita-1-0`;
  `Recommends: vice` (app is useful for tunes even without it). Install `.desktop`,
  icon, metainfo, GSettings schema (+ `glib-compile-schemas` trigger).

### C. Arch (PKGBUILD / AUR)
- `PKGBUILD` building with the system Rust. `depends=(gtk4 libadwaita)`,
  `optdepends=('vice: launch games')`. Publish to the AUR
  (`breadbin` / `breadbin-git`).

### Desktop integration assets (needed by *all* tracks)
- `io.github.jacobandresen.Breadbin.desktop` (Categories: `Game;Emulator;`).
- `io.github.jacobandresen.Breadbin.metainfo.xml` (AppStream: summary, description,
  screenshots, releases) — required for Flathub and good for Software centres.
- A scalable icon (SVG) + the C64 "breadbin" identity (reuse the existing aesthetic).
- `io.github.jacobandresen.Breadbin.gschema.xml` for the preferences.
- **C64 skin assets** (§6c): the bundled PETSCII/CBM font, `style.css`, the
  PETSCII‑border 9‑slice, and the CRT shader — installed under the app's data dir and
  loaded at startup.

### Runtime dependencies summary
| Distro | App needs | To launch games | ROMs | Cover art | Audio |
| --- | --- | --- | --- | --- | --- |
| Debian/Ubuntu | `libgtk-4-1`, `libadwaita-1-0` | `vice` (`x64sc`) | **C64 Forever (user‑owned)** | built‑in (no WezTerm!) | PulseAudio/Pipewire |
| Arch | `gtk4`, `libadwaita` | `vice` | **C64 Forever (user‑owned)** | built‑in | PulseAudio/Pipewire |
| Any (Flatpak) | GNOME runtime | host VICE via flatpak‑spawn | **C64 Forever (user‑owned)** | built‑in | `--socket=pulseaudio` |

> **ROMs:** VICE is the *engine*, but breadbin runs games with the user's licensed **C64
> Forever** ROMs only (§4b) — never bundled, never downloaded. Package `Recommends`/
> `optdepends` `vice` (Tunes works without it); the ROMs are the user's responsibility
> and the app states this at startup. Flatpak reads the host ROM folder via a
> `--filesystem=` permission on the configured C64 Forever path.
>
> Note: the GUI **removes the WezTerm dependency entirely** — covers are real textures
> now, not terminal images. `setup-dependencies.sh` shrinks to "install VICE" and the
> Nerd Font / WezTerm logic goes away.

---

## 11. Phased roadmap

**M0 — Workspace split.** Carve `breadbin-core` out of `rust/src`; move backend
modules in; behind it, refactor `resolve`/`launch`/`refresh` from subprocess RPC into
library functions and add `launch::spawn` (no `exec`). Backend compiles, unit tests
(norm/seq_ratio/disk matching/SID render) pass. *No UI yet.*

**M1 — App skeleton + C64 skin.** `adw::Application` + window + view switcher +
preferences window (GSettings) + first‑run bootstrap progress view. Land the C64 theme
early (§6c): palette‑derived CSS, PETSCII font, power‑on boot splash — so every later
view is built on the skin, not retrofitted. Empty Kiosk/Tunes/Demos shells.

**M2 — Kiosk v1.** Cover grid with sections, badges, search, detail pane, controls
dialog, spawn‑launch, recently‑played refresh, download‑on‑demand. This is the
Cartridges‑equivalent milestone.

**M3 — Tunes v1.** Jukebox list, player transport, audio (ported as‑is), GPU shader
visualisers on a `GtkGLArea` (§8), radio mode.

**M3b — Demos v1.** Screenshot grid by party (reusing the kiosk grid + the party
grouping), download‑on‑demand, spawn‑launch in VICE. Small increment on top of M2+M3.

**M4 — Packaging.** `.desktop`/metainfo/icon/schema; `cargo-deb`; PKGBUILD; Flatpak
manifest with `flatpak-spawn` launch + perms. Test install on Debian, Ubuntu, Arch.

**M5 — Polish / stretch.** Fullscreen "kiosk" big‑picture + gamepad nav; the CRT
post‑process shader (§6c) and its preferences toggle; per‑game overrides; additional
shader visualisers. (The CRT shader can land here since it reuses the M3 GL pipeline.)

---

## 12. Risks & open questions

- **Flatpak ↔ host VICE.** The `flatpak-spawn --host` path needs validation early
  (M4 spike, but design for it in M0's `launch::spawn` so the GUI just calls one
  function regardless of packaging). If it proves painful, native packages are the
  documented primary and Flatpak ships "tunes + library; launch needs host setup".
- **Shader portability.** `GtkGLArea` exposes whatever GL/GLES Mesa provides; keep the
  fragment shaders to GLSL ES 3.0 features so they compile across Wayland/X11 and
  Intel/AMD/NVIDIA. Provide a no‑GL fallback (a basic CPU‑drawn scope) for the rare
  case `GtkGLArea` fails to realise. Validate on a software renderer (llvmpipe) early,
  since CI and some VMs have no GPU.
- **GSettings vs plain config file.** GSettings is idiomatic for GNOME and required‑ish
  for a polished preferences window, but adds a schema‑compile step to every package.
  A TOML config under `$XDG_CONFIG_HOME` is the lighter alternative. *Recommendation:*
  GSettings (matches the Cartridges‑class target); document the trade‑off.
- **App ID / namespace.** Pick the final reverse‑DNS app id before M4 (it's baked into
  desktop file, metainfo, schema path, Flatpak). `io.github.jacobandresen.Breadbin`
  assumed above.
- **Legal/preservation framing.** The download‑on‑demand from IA/UTA/TOSEC is the same
  as today; nothing changes, but a Software‑centre listing gives it more visibility —
  keep the README's "public preservation archives" framing in the metainfo.
```
