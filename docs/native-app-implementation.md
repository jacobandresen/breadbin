# breadbin native app — detailed implementation handoff

This is the **step‑by‑step build guide** for turning breadbin into a GTK4/libadwaita
desktop app. It is deliberately prescriptive: follow the tasks in order, copy the code
skeletons, and check each task's **Done when** box before moving on. Read
`docs/native-app-plan.md` first for the *why*; this document is the *how*.

> Golden rules (apply to every task):
> 1. **Never block the GTK main thread.** Anything that touches the network, disk
>    scanning, downloads, or spawning VICE runs on a worker thread (see §1.4).
> 2. **Never download ROMs.** Game/disk/tape images: yes. Commodore ROMs: never — they
>    come only from the user's C64 Forever copy (see §9).
> 3. **Spawn, never exec.** The GUI must stay alive while VICE runs.
> 4. After each task, run `cargo build` (and `cargo test` where noted). Do not proceed
>    on a red build.

---

## 0. Conventions

- **Repo layout target** (end state):
  ```
  breadbin/
    Cargo.toml                     # [workspace]
    crates/breadbin-core/          # library (no GTK, no terminal UI)
    crates/breadbin-gui/           # the app (GTK4 + libadwaita)
    docs/
  ```
- The current code lives in `rust/src/*.rs` as one binary. We move the backend modules
  into `breadbin-core` and **delete** the terminal UI + CLI.
- **App ID:** `io.github.jacobandresen.Breadbin` (used in the binary, .desktop,
  metainfo, GSettings schema path `/io/github/jacobandresen/Breadbin/`, Flatpak).
- Rust edition 2021. Keep `rust-version = "1.74"` unless a dependency forces higher.
- When this doc says "make `X` pub", change the function's `fn` to `pub fn` and, if it
  lives in a module not yet exported, add it to `lib.rs`.
- **`glib::clone!` syntax (glib 0.20).** This doc uses the **attribute** form
  throughout: `clone!(#[weak] widget, async move { … })` and `#[strong]` for owned
  captures. The older `clone!(@weak widget => async move { … })` form was **removed** in
  glib 0.18 and will not compile here. If you see `@weak`/`@strong` anywhere, rewrite it
  as `#[weak]`/`#[strong]` with a comma before the closure/async block. For a weak
  capture that needs an early return, use `#[weak]` (panics if gone) or
  `#[weak(rename_to = w)]`; `#[upgrade_or_return]` is available when needed.

---

## 1. Milestone M0 — workspace split & core library (no UI)

Outcome: a `breadbin-core` library that compiles, exposes a clean API, and has the
subprocess‑RPC and `exec` removed. No window yet.

### Task M0.1 — Create the workspace

1. At repo root create `Cargo.toml`:
   ```toml
   [workspace]
   resolver = "2"
   members = ["crates/breadbin-core", "crates/breadbin-gui"]

   [profile.release]
   strip = true

   # reSID needs wrapping arithmetic (see original Cargo.toml comment).
   [profile.dev.package.resid-rs]
   overflow-checks = false
   ```
2. `mkdir -p crates/breadbin-core/src crates/breadbin-gui/src`.
3. Move the existing `rust/src/*.rs` into `crates/breadbin-core/src/` **except**
   `main.rs`, `kiosk.rs`, `menu.rs`, `tunes.rs`, `demos.rs`, `grid.rs`, `boot.rs`
   (those are terminal UI — handled below). Also move `rust/src/collections.tsv`.
4. Create `crates/breadbin-core/Cargo.toml` by copying the `[dependencies]` block from
   the current `rust/Cargo.toml` and removing the terminal‑only crates:
   **remove** `ratatui`, `crossterm`, `ratatui-image`. **Keep** `image`, `which`,
   `shlex`, `regex`, `ureq`, `serde_json`, `flate2`, `rusqlite`, `encoding_rs`, `zip`,
   `resid-rs`, `cpal`. Set `[lib] name = "breadbin_core"`.

**Done when:** `crates/breadbin-core` has the backend `.rs` files and a Cargo.toml; the
workspace `Cargo.toml` lists both members (breadbin-gui can be an empty stub for now —
add `src/main.rs` with `fn main(){}` and a minimal Cargo.toml so the workspace resolves).

### Task M0.2 — Salvage reusable pieces from the terminal UI modules

Before deleting the terminal UIs, **move these pure/reusable items into core** (they are
not terminal‑specific):

| From | Item | Move to |
| --- | --- | --- |
| `kiosk.rs` | `draw_joystick_badge`, `draw_rating_badge`, `fill_circle`/`fill_ellipse`/`fill_disc_gradient`/`fill_rect`/`fill_star`/`fill_polygon`/`point_in_poly` | `core/src/cover.rs` (make the two `draw_*` `pub`) |
| `kiosk.rs` | the section‑ordering logic in `KioskState::new` (genre sort, curated front sections) | new `core/src/library.rs::sections()` (see M0.5) |
| `tunes.rs` | `Tune`, `tunes_index_path`, `parse_release_xml`, `build_index`, `load_tunes`, `group_by_party`, `ensure_sid`, `Audio`, `build_stream`, `Ring` | `core/src/tunes.rs` (data) + `core/src/audio.rs` (Audio/build_stream/ensure_sid) |
| `tunes.rs` | `Rng` (xorshift), `RADIO_SECS`, radio selection logic | `core/src/tunes.rs` (keep `Rng` pub; the *UI* radio loop is rebuilt in the GUI) |
| `demos.rs` | `Demo`, `demos_index_path`, `shots_dir`, `downloads_dir`, `build_index`, `load_demos`, `ensure_shot`, `fetch_and_prepare`, the CSDb parsing | `core/src/demos.rs` (strip the ratatui parts; keep data + fetch) |

Leave behind (delete) everything that imports `ratatui`/`crossterm`: the render
functions, event loops, `VisMode`, `draw_scope/fireball/cubes`, navigation (`grid.rs`),
`boot.rs`'s ratatui drawing. (The visualiser *math* is re‑expressed as shaders in M3;
do not port the braille code.)

**Done when:** the listed items exist in core modules; no core file imports `ratatui`,
`crossterm`, or `ratatui_image` (`grep -rl 'ratatui\|crossterm' crates/breadbin-core/src`
returns nothing).

### Task M0.3 — Replace `core::Progress` with a callback sink

`core::Progress` writes to stderr. Replace its public surface with a generic sink so the
GUI can show a progress bar.

In `core/src/core.rs`:
```rust
/// A progress sink: called with (done, total). total == 0 means "unknown".
pub type ProgressFn<'a> = dyn FnMut(u64, u64) + 'a;

/// Convenience no-op sink.
pub fn no_progress(_done: u64, _total: u64) {}
```
Change every internal caller that built a `Progress` to instead accept
`progress: &mut ProgressFn` and call `progress(done, total)`. For functions that are
hard to thread a callback through right now, accept `&mut dyn FnMut(u64,u64)` as a new
last parameter and pass `&mut core::no_progress` from existing call sites.

**Done when:** core has no `eprintln!`-based progress bar; download/index functions take
a progress callback; `cargo build -p breadbin-core` is green.

### Task M0.4 — Turn subprocess‑RPC into direct library calls (CRITICAL)

The current code re‑executes itself (`current_exe()`) to run `disk`, `index`, `play`.
With the CLI gone there is nothing to re‑exec. Extract typed functions.

**(a) disk by IA id.** In `core/src/disk.rs`, extract the `--id` branch of `main` into:
```rust
/// Download an exact Internet Archive item and return the boot disk path.
/// Records the download (downloaded.tsv) like the CLI did. None on failure.
pub fn download_by_id(id: &str, dest: &Path, progress: &mut dyn FnMut(u64,u64)) -> Option<PathBuf> {
    let got = download_ia(id, id, dest, /*dry=*/false); // make download_ia take progress
    let boot = dest.join(got.first()?);
    record_download(id, &boot.to_string_lossy());
    Some(boot)
}
```
**(b) disk by query.** Extract the query path of `main` into:
```rust
pub fn download_query(query: &str, sources: &[&str], dest: &Path,
                      progress: &mut dyn FnMut(u64,u64)) -> Vec<PathBuf>;
```
**(c) IA index build.** `build_ia_index()` already exists — make it
`pub fn build_ia_index(progress: &mut dyn FnMut(u64,u64)) -> std::io::Result<()>`.
**(d) ranked index build.** In `core/src/index.rs` (renamed from `build_index.rs`),
extract `main`'s body into `pub fn build() -> std::io::Result<()>`.

Keep the old `pub fn main(argv)` wrappers temporarily if convenient, but they must call
the new functions, not re‑exec.

**Done when:** `grep -rn 'current_exe' crates/breadbin-core/src` returns **nothing** in
`catalog.rs`, `disk.rs`, `index.rs` (it may still appear in `launch.rs` until M0.6).

### Task M0.5 — Rebuild `tui.rs` as `core/src/catalog.rs` + `library.rs`

Rename `tui.rs` → `catalog.rs`. Keep `Row`, `load_rows`, `canon_of`, `cover_for`,
`group_by_genre`, `classic_canons`, `joystick_canons`, `top_rated_canons`,
`collections`, `record_play`, `recent_plays`. **Rewrite** these three to use M0.4
functions instead of subprocesses:

```rust
// was: refresh() shelled out to `disk --ia-index` then `index`
pub fn refresh(progress: &mut dyn FnMut(u64,u64)) -> std::io::Result<()> {
    crate::disk::build_ia_index(progress)?;
    crate::index::build()?;
    Ok(())
}

// was: resolve() shelled out to `disk --id` / `disk --source ia` then `index`
pub fn resolve(row: &Row, progress: &mut dyn FnMut(u64,u64)) -> Option<PathBuf> {
    if row.is_local() { return Some(PathBuf::from(&row.target)); }
    let dest = crate::disk::dest_default();
    let path = if !row.ident.is_empty() {
        crate::disk::download_by_id(&row.ident, &dest, progress)
    } else {
        crate::disk::download_query(&row.query, &["ia"], &dest, progress).into_iter().next()
    };
    crate::index::build().ok();            // refresh local/available state
    path.filter(|p| p.exists()).or_else(|| {
        load_rows().into_iter().find(|f| f.title == row.title && f.is_local())
            .map(|f| PathBuf::from(f.target))
    })
}
```

New `core/src/library.rs` holds the **kiosk section model** moved from `KioskState::new`:
```rust
pub struct Section { pub title: String, pub rows: Vec<usize> } // indices into all rows

/// Curated front sections (Latest played, Classics, collections.tsv) then genre groups,
/// each ordered most-downloaded first, Arcade/Shoot'em Up pinned, "Other" last.
pub fn sections(all: &[Row]) -> Vec<Section>;
```
Port the exact ordering from the original `KioskState::new` (genre sort_by_key, the
`front` vec of curated sections, `front.append(&mut groups)`).

**Done when:** `cargo test -p breadbin-core` passes (port the existing `tests/cli.rs`
assertions that don't depend on the binary; convert `_norm`/`_disk`/`_sid`/`_cover` dev
aids from `main.rs` into `#[cfg(test)]` unit tests).

### Task M0.6 — `launch.rs`: spawn (not exec) + the ROM policy

Rename `run.rs` → `launch.rs`. **Keep** `joystick_present`, `controls_description`,
`control_flags`, `drive_flags`, `drive_sound_flags`, `emu_help`, `controller_*`,
`write_controls_config`, joymap helpers. **Change** the launch and emulator pick:

```rust
pub struct LaunchOpts {
    pub warp: bool,            // default true
    pub fullscreen: bool,      // default true
    pub keyboard: bool,        // force keyboard for both players
    pub drive_sound: Option<bool>,
    pub forever: crate::roms::Forever, // C64 Forever, REQUIRED (see §9 / M0.7)
}

/// Spawn VICE as a child (NEVER exec). Returns the Child so the GUI can watch it exit.
pub fn spawn(game: &std::path::Path, opts: &LaunchOpts) -> std::io::Result<std::process::Child> {
    // Emulator + ROM source come from the C64 Forever detection (M0.7).
    let (emu, rom_args, wine_prefix): (Vec<String>, Vec<String>, Option<std::path::PathBuf>) =
        match &opts.forever {
            // VERIFIED path: Cloanto's bundled VICE finds its own licensed ROMs.
            crate::roms::Forever::BundledVice { x64_exe, wine_prefix } =>
                (vec!["wine".into(), x64_exe.to_string_lossy().into()], vec![], wine_prefix.clone()),
            // SECONDARY path (confirm filenames first): native VICE + extracted ROMs.
            crate::roms::Forever::RomDir(dir) =>
                (native_vice_cmd(), rom_args(dir, /*help probed below*/ ""), None),
        };
    let help = emu_help(&emu);
    let mut args: Vec<String> = drive_flags(&help, /*virtual=*/false); // TDE always (ROMs present)
    args.extend(drive_sound_flags(&help, opts.drive_sound));
    if opts.warp { args.push("-autostart-warp".into()); }
    if opts.fullscreen { args.push("-VICIIfull".into()); }
    let joystick = !opts.keyboard && joystick_present();
    args.extend(control_flags(joystick));
    args.extend(rom_args); // empty for BundledVice

    #[cfg(target_os = "linux")]
    { if std::env::var_os("SDL_VIDEODRIVER").is_none() {
        unsafe { std::env::set_var("SDL_VIDEODRIVER", "x11"); } } }

    let mut cmd = std::process::Command::new(&emu[0]);
    cmd.args(&emu[1..]).args(&args).arg("-autostart").arg(game);
    if let Some(pfx) = wine_prefix { cmd.env("WINEPREFIX", pfx).env("WINEDEBUG", "-all"); }
    cmd.spawn()
}
```
- Delete `play_exec` and the `cmd.exec()` path. Delete `missing_tde_rom` /
  virtual‑drive auto‑fallback (keep `C64_VIRTUAL_DRIVE` only as a manual override if you
  want; optional).
- **Implement and test the `BundledVice` branch first** — it mirrors the wine branch
  that already works in `run.rs`. The `RomDir`/native path is secondary and depends on
  confirming Cloanto's ROM filenames (M0.7 note).

**Done when:** `grep -rn '\.exec()' crates/breadbin-core/src` returns nothing; `spawn`
returns a `Child`; `cargo build -p breadbin-core` green.

### Task M0.7 — C64 Forever discovery (`core/src/roms.rs`)

> **Two ways to use the licensed ROMs. Default to the VERIFIED one.**
> The existing `run.rs` never fed ROM files to native VICE — it ran **Cloanto's own
> bundled VICE** (`.../Cloanto/C64 Forever/VICE/x64.exe`) via wine, which finds its
> licensed ROMs itself (the `WINE_X64` constant + `WINEPREFIX` env). That is the path
> we have actually seen work, so **make it the primary launch path** (M0.6
> `pick_emulator`). The alternative — extracting C64 Forever's ROM files and pointing a
> *native* `x64sc` at them with `-kernal/-basic/-chargen` — is **UNVERIFIED**: Cloanto's
> exact ROM filenames and on‑disk layout have not been confirmed. Treat it as a
> secondary path and **confirm the real filenames before relying on it.**

```rust
use std::path::PathBuf;

/// Detect a usable C64 Forever installation. Returns how to launch it.
pub enum Forever {
    /// VERIFIED: run Cloanto's bundled VICE (it locates its own licensed ROMs).
    BundledVice { x64_exe: PathBuf, wine_prefix: Option<PathBuf> },
    /// UNVERIFIED: a directory of extracted ROM files to feed a native VICE.
    /// Only construct this once the real filenames/layout are confirmed.
    RomDir(PathBuf),
}

/// Locate C64 Forever, or None. Order: 1) saved setting  2) C64_FOREVER_ROMS env
/// 3) known Cloanto install paths (the existing WINE_X64 location and siblings).
pub fn detect(configured: Option<&str>) -> Option<Forever> {
    // Prefer the bundled-VICE form (verified). Fall back to a configured ROM dir.
    if let Some(exe) = find_cloanto_x64() { // checks WINE_X64 + common wine prefixes
        return Some(Forever::BundledVice { x64_exe: exe, wine_prefix: default_wine_prefix() });
    }
    let dir = configured.map(PathBuf::from)
        .or_else(|| std::env::var_os("C64_FOREVER_ROMS").map(PathBuf::from))?;
    // has_required_roms() filenames are a GUESS — confirm against a real C64 Forever
    // install before trusting this branch (see the note above).
    has_required_roms(&dir).then_some(Forever::RomDir(dir))
}

pub fn present(configured: Option<&str>) -> bool { detect(configured).is_some() }

/// UNVERIFIED filename guess — confirm before relying on it.
fn has_required_roms(dir: &std::path::Path) -> bool {
    ["kernal", "basic", "chargen"].iter().all(|f| dir.join(f).is_file())
}
```
In `launch.rs`, `pick_emulator`/`spawn` consume `Forever`:
- `BundledVice` → command `["wine", x64_exe]` with `WINEPREFIX`/`WINEDEBUG` set (exactly
  as the current `run.rs` wine branch does); **no `rom_args` needed** — Cloanto's VICE
  supplies its own ROMs. This is the path to implement and test first.
- `RomDir` (secondary, once verified) → native `x64sc` plus `rom_args(dir)` emitting
  `-kernal/-basic/-chargen` (+ `-dos1541`), flag spellings validated via `emu_help`.

### Task M0.8 — `core/src/lib.rs` public surface

```rust
pub mod core;       // util: palette, data dirs, fetch, norm, group_by_party, ...
pub mod catalog;    // Row, load_rows, refresh, resolve, record_play, recent_plays
pub mod library;    // Section model for the kiosk
pub mod cover;      // ensure_cover, load_index, badge compositing
pub mod disk;       // download_by_id, download_query, build_ia_index, game_title
pub mod get;        // tape downloads (UTA)
pub mod tosec;      // tosec entries
pub mod info;       // connect, best_match, record, InfoRecord
pub mod index;      // build()
pub mod launch;     // LaunchOpts, spawn, joystick_present, controls_description
pub mod roms;       // Forever, detect, present
pub mod sid;        // Player, Vis, NUM_REGS
pub mod audio;      // Audio, ensure_sid
pub mod tunes;      // Tune, load_tunes, build_index, group_by_party
pub mod demos;      // Demo, load_demos, build_index, fetch_and_prepare, ensure_shot
```
Keep module name `core` (it's the existing util module) — refer to it as
`breadbin_core::core` from the GUI.

**M0 acceptance:** `cargo build` (whole workspace) and `cargo test -p breadbin-core`
both green. No `ratatui`/`crossterm` anywhere. No `current_exe`/`exec` in the launch or
catalog paths.

---

## 2. Milestone M1 — GUI skeleton + C64 skin

Outcome: the app window opens, shows the C64 boot splash, a view switcher with three
empty pages (Kiosk/Tunes/Demos), a preferences window, and runs first‑run bootstrap with
a progress screen. The C64 theme is loaded.

### Task M1.1 — `breadbin-gui` crate setup

`crates/breadbin-gui/Cargo.toml`:
```toml
[package]
name = "breadbin-gui"
version = "0.2.0"
edition = "2021"

[[bin]]
name = "breadbin"
path = "src/main.rs"

[dependencies]
breadbin-core = { path = "../breadbin-core" }
gtk = { package = "gtk4", version = "0.9", features = ["v4_14"] }
adw = { package = "libadwaita", version = "0.7", features = ["v1_5"] }
gdk = { package = "gdk4", version = "0.9" }
glib = "0.20"
async-channel = "2"
image = "0.25"
# GL visualisers (M3):
epoxy = "0.1"
glow = "0.14"
libloading = "0.8"
```
> Use the latest `0.x` of the gtk4-rs family that matches the installed system GTK
> (`pkg-config --modversion gtk4`, need ≥ 4.14) and libadwaita (≥ 1.5). If `cargo add`
> reports newer compatible versions, take them and adjust the `v4_xx`/`v1_x` features.

Install build deps:
- **Debian/Ubuntu:** `sudo apt install libgtk-4-dev libadwaita-1-dev build-essential`
- **Arch:** `sudo pacman -S gtk4 libadwaita base-devel`

### Task M1.2 — `main.rs` + `app.rs`

`crates/breadbin-gui/src/main.rs`:
```rust
mod app; mod config; mod task; mod widgets;
mod kiosk; mod tunes; mod demos;

use adw::prelude::*;

const APP_ID: &str = "io.github.jacobandresen.Breadbin";

fn main() -> glib::ExitCode {
    breadbin_core::core::ensure_user_data_dir();
    let app = adw::Application::builder().application_id(APP_ID).build();
    app.connect_startup(|_| crate::app::load_styles());   // CSS + font (M1.5)
    app.connect_activate(crate::app::build_ui);
    app.run()
}
```
`app.rs::build_ui(app: &adw::Application)` builds an `adw::ApplicationWindow` with:
- `adw::ToolbarView` + `adw::HeaderBar`.
- An `adw::ViewStack` with three pages added via `add_titled_with_icon`: `"kiosk"`
  ("Games"), `"tunes"` ("Tunes"), `"demos"` ("Demos").
- An `adw::ViewSwitcher` (in the header) bound to the stack, and an
  `adw::ViewSwitcherBar` for narrow widths.
- A menu button → `gio::Menu` with "Refresh catalogue", "Preferences", "About".
- Default size 1100×740; `window.present()`.

Store the three page widgets as `kiosk::KioskPage`, `tunes::TunesPage`,
`demos::DemosPage` (each a `glib` `Box` subtree or a thin struct holding its root
widget). For M1 they can be `adw::StatusPage` placeholders.

**Done when:** `cargo run -p breadbin-gui` opens a window with a working 3‑way switcher.

### Task M1.3 — Settings (`config.rs`) + GSettings schema

Schema `crates/breadbin-gui/data/io.github.jacobandresen.Breadbin.gschema.xml`:
```xml
<schemalist>
  <schema id="io.github.jacobandresen.Breadbin"
          path="/io/github/jacobandresen/Breadbin/">
    <key name="games-folder" type="s"><default>""</default></key>
    <key name="emulator-command" type="s"><default>""</default></key>
    <key name="c64forever-roms" type="s"><default>""</default></key>
    <key name="warp" type="b"><default>true</default></key>
    <key name="fullscreen-launch" type="b"><default>true</default></key>
    <key name="drive-sound" type="b"><default>true</default></key>
    <key name="crt-effect" type="b"><default>false</default></key>
    <key name="ui-drive-sounds" type="b"><default>true</default></key>
    <key name="display-font" type="s"><default>"unscii"</default></key>
  </schema>
</schemalist>
```
For **dev runs** (schema not installed system‑wide), compile it locally and point
`GSETTINGS_SCHEMA_DIR` at it:
```sh
glib-compile-schemas crates/breadbin-gui/data
GSETTINGS_SCHEMA_DIR=$PWD/crates/breadbin-gui/data cargo run -p breadbin-gui
```
`config.rs`:
```rust
pub struct Settings(gio::Settings);
impl Settings {
    pub fn new() -> Self { Self(gio::Settings::new(crate::APP_ID)) }
    pub fn games_folder(&self) -> String { /* setting or breadbin_core::core::c64_lib() */ }
    pub fn forever(&self) -> Option<breadbin_core::roms::Forever> {
        breadbin_core::roms::detect(Some(&self.0.string("c64forever-roms")))
    }
    /// None when C64 Forever is not present (Play/Run gate on this — §9).
    pub fn launch_opts(&self) -> Option<breadbin_core::launch::LaunchOpts> {
        Some(breadbin_core::launch::LaunchOpts {
            warp: self.0.boolean("warp"),
            fullscreen: self.0.boolean("fullscreen-launch"),
            keyboard: false,
            drive_sound: Some(self.0.boolean("drive-sound")),
            forever: self.forever()?,   // <- None here propagates: no launch without C64 Forever
        })
    }
    // ...getters/setters per key
}
```
Preferences window: an `adw::PreferencesWindow` with groups "Library" (games folder
chooser), "Emulation" (C64 Forever ROM folder chooser — see §9, emulator command,
warp, fullscreen, drive sound), "Appearance" (CRT effect, UI drive sounds, display
font). Bind boolean keys to `adw::SwitchRow` via `settings.bind(...)`.

**Done when:** Preferences opens, toggles persist across restarts (verify with
`gsettings get io.github.jacobandresen.Breadbin warp`).

### Task M1.4 — Worker thread helper (`task.rs`)

```rust
/// Run a blocking closure off the main thread; await the result on the GTK loop.
pub async fn run_blocking<T, F>(f: F) -> T
where T: Send + 'static, F: FnOnce() -> T + Send + 'static {
    let (tx, rx) = async_channel::bounded(1);
    std::thread::spawn(move || { let _ = tx.send_blocking(f()); });
    rx.recv().await.expect("worker panicked")
}
```
For **progress streaming**, pass an `async_channel::Sender<(u64,u64)>` into the closure
and have the closure call `tx.send_blocking(...)` from its progress callback; on the UI
side, `glib::spawn_future_local(async move { while let Ok((d,t)) = rx.recv().await { bar.set_fraction(...) } })`.

Usage pattern everywhere (note the glib 0.20 `#[weak]` attribute form):
```rust
glib::spawn_future_local(clone!(#[weak] widget, async move {
    let result = run_blocking(move || breadbin_core::catalog::load_rows()).await;
    // ...update widget on the main thread...
}));
```

### Task M1.5 — C64 skin: CSS + font + boot splash

1. **CSS** `data/style.css` (loaded in `app::load_styles` via `gtk::CssProvider` +
   `gtk::style_context_add_provider_for_display`, using
   `gtk::STYLE_PROVIDER_PRIORITY_APPLICATION`). GTK's CSS engine does **not** support
   `:root`/`var(--x)`; use `@define-color`. To recolour libadwaita you must **override
   its named colors** (not just `window {}`). Hex values mirror
   `breadbin_core::core::palette`:
   ```css
   @define-color c64_screen    #40318D;
   @define-color c64_lightblue #706DEB;
   @define-color c64_yellow    #EDF171;

   /* override libadwaita's palette so all adw widgets follow the C64 look */
   @define-color window_bg_color  @c64_screen;
   @define-color window_fg_color  @c64_lightblue;
   @define-color view_bg_color    @c64_screen;
   @define-color view_fg_color    @c64_lightblue;
   @define-color accent_bg_color  @c64_yellow;

   .c64-font   { font-family: "unscii"; }              /* applied to headers/rows */
   .cover-card { border: 3px solid @c64_lightblue; border-radius: 6px; }
   *:focus     { outline: 3px solid @c64_yellow; }
   ```
2. **Font.** Default **Unscii** (public domain). Bundle `unscii-16.ttf` under
   `data/fonts/`. GTK/Pango have **no API to load a `.ttf` from a path**; fonts must be
   visible to **fontconfig**. Use this, in order of preference:
   - **Packaged (the real answer):** M4 installs the TTF into a system font dir
     (`/usr/share/fonts/...`); after install it's available by family name. No runtime
     loading code at all.
   - **Dev convenience:** at startup, copy/symlink `data/fonts/unscii-16.ttf` into
     `~/.local/share/fonts/` if absent and run `fc-cache -f` once (shell out), then use
     the family name. *Or* add the optional [`fontconfig`](https://crates.io/crates/fontconfig)
     crate and call its app‑font add (`FcConfigAppFontAddFile`) before building any
     widgets. Keep this behind a `cfg!(debug_assertions)` guard.

   Reference the family in CSS by name (e.g. `font-family: "unscii"` — confirm the exact
   family with `fc-scan data/fonts/unscii-16.ttf | grep family`). Add a `display-font`
   setting; when set to `c64pro` **and** the Style64 family is installed, switch the CSS
   to `font-family: "C64 Pro Mono"` (plan §6c — Style64 is opt‑in, never the default).
3. **Boot splash** `widgets/boot.rs`: an `adw::Bin` showing, in the C64 font on the
   screen‑blue background:
   ```
   **** COMMODORE 64 BASIC V2 ****
   64K RAM SYSTEM  38911 BASIC BYTES FREE

   READY.
   █                       (blinking via a 500ms glib::timeout_add_local)
   ```
   Show it on first launch and as the first‑run bootstrap backdrop. This is also where
   the **C64 Forever requirement notice** appears (§9).

### Task M1.6 — First‑run bootstrap flow

On `activate`, after building the window:
```rust
let index = breadbin_core::catalog::index_path();
let needs_build = std::fs::metadata(&index).map(|m| m.len()==0).unwrap_or(true);
if needs_build {
    show_bootstrap_page(&window);   // boot splash + progress bar + "SEARCHING / LOADING"
    let (tx, rx) = async_channel::unbounded::<(u64,u64)>();
    // stream progress to the bar:
    glib::spawn_future_local(clone!(#[weak] bar, async move {
        while let Ok((d,t)) = rx.recv().await { bar.set_fraction(if t>0 {d as f64/t as f64} else {0.0}); }
    }));
    glib::spawn_future_local(clone!(#[weak] window, async move {
        run_blocking(move || {
            let mut p = move |d,t| { let _ = tx.send_blocking((d,t)); };
            breadbin_core::catalog::refresh(&mut p)
        }).await.ok();
        load_all_views(&window);     // populate kiosk/tunes/demos
    }));
}
```

**M1 acceptance:** fresh run (empty `~/.breadbin`) shows the boot splash, builds the
index with a live progress bar, then reveals the three views; toggles persist.

---

## 3. Milestone M2 — Kiosk view (the library)

Outcome: a Cartridges‑style cover grid with sections, badges, search, a detail pane, a
controls dialog, spawn‑launch (gated on C64 Forever ROMs), download‑on‑demand, and a
"recently played" refresh on game exit.

### Task M2.1 — Cover → `gdk::Texture`

`widgets/cover.rs`:
```rust
use breadbin_core::cover; // draw_joystick_badge, draw_rating_badge now pub here

pub fn texture_for(path: &std::path::Path, joystick: bool, top_rated: bool) -> Option<gdk::Texture> {
    let mut rgba = image::open(path).ok()?.to_rgba8();
    if top_rated { cover::draw_rating_badge(&mut rgba); }
    if joystick  { cover::draw_joystick_badge(&mut rgba); }
    let (w, h) = rgba.dimensions();
    let stride = (w * 4) as usize;
    let bytes = glib::Bytes::from_owned(rgba.into_raw());
    Some(gdk::MemoryTexture::new(w as i32, h as i32,
        gdk::MemoryFormat::R8g8b8a8, &bytes, stride).upcast())
}
```
Use it in a "cover card" widget: an `adw::Bin` with css class `cover-card`, a
`gtk::Picture` (`set_content_fit(Cover)`), a title label under it (C64 font), and a
"download" overlay badge when the row is not local.

### Task M2.2 — Data model & sections

On the worker thread, load once:
```rust
let rows = breadbin_core::catalog::load_rows();              // Vec<Row>
let cidx = breadbin_core::cover::load_index();               // HashMap
let classics = breadbin_core::catalog::classic_canons();
let joystick = breadbin_core::catalog::joystick_canons();
let top_rated = breadbin_core::catalog::top_rated_canons();
let sections = breadbin_core::library::sections(&rows);      // Vec<Section>
```
Cover resolution can fetch on first use, so do it lazily/off‑thread per card
(`run_blocking(move || catalog::cover_for(&row, &cidx))` then set the texture).

### Task M2.3 — The grid

Use a vertically scrolled `gtk::Box` of **sections**; each section is a header label
(C64 colour‑bar chip via `palette::bar_for(title)` as inline CSS) + a `gtk::FlowBox`
(`set_homogeneous(true)`, `set_selection_mode(None)`, `set_max_children_per_line(12)`).
Populate each FlowBox with cover cards for that section's row indices.

> Performance: for the full catalogue use one `gtk::GridView` with a section model
> (`gtk::SectionModel`/`GtkListView` factory) instead of many FlowBoxes if scroll
> stutters. FlowBox‑per‑section is fine to start.

Clicking a card → open the **detail pane** (M2.4) for that row.

### Task M2.4 — Detail pane

An `adw::NavigationView` (push a detail page) or `adw::OverlaySplitView`. The detail
page shows the large cover and GameBase64 facts:
```rust
let con = breadbin_core::info::connect();
let facts = breadbin_core::info::best_match(&con, &row.query)
    .map(|gid| breadbin_core::info::record(&con, gid)); // InfoRecord { name, year, rows, note }
```
Render `facts.rows` (Vec<(label,value)>) as `adw::ActionRow`s in an
`adw::PreferencesGroup`. Buttons: **Play** (always visible; disabled with a tooltip if
ROMs missing — §9) and **Download** (visible only when `!row.is_local()`).

### Task M2.5 — Search

Add a `gtk::SearchEntry` (toggled from the header). On `search-changed`, filter visible
cards by `breadbin_core::core::norm(query)` contained in `norm(row.display)`/`title`.
Implement by setting each FlowBoxChild `visible` (or use a `gtk::FilterListModel` if you
moved to GridView). Empty query → all visible.

### Task M2.6 — Download‑on‑demand

**Download** button handler:
```rust
btn.set_sensitive(false); show_spinner();
glib::spawn_future_local(clone!(#[strong] row, async move {
    let path = run_blocking(move || {
        let mut p = |_d,_t| {};
        breadbin_core::catalog::resolve(&row, &mut p)
    }).await;
    match path { Some(_) => mark_local_and_enable_play(), None => show_error("Download failed") }
}));
```
Stream progress to a `gtk::ProgressBar` using the (u64,u64) channel pattern (M1.4).

### Task M2.7 — Launch flow (spawn + ROM gate + controls dialog)

**Play** handler:
```rust
// Gate on C64 Forever: launch_opts() is None when it isn't present (§9).
let Some(opts) = settings.launch_opts() else { show_rom_required_dialog(); return; };
// 1. resolve the disk (download if needed)
let path = run_blocking({ let row=row.clone(); move || {
    let mut p=|_,_|{}; breadbin_core::catalog::resolve(&row,&mut p) }}).await;
let Some(path) = path else { show_error("Could not load"); return; };
// 2. controls dialog
if !controls_dialog(&window).await { return; }     // AdwAlertDialog, Start/Cancel
// 3. record + spawn  (opts carries the C64 Forever launch info)
breadbin_core::catalog::record_play(&row);
match breadbin_core::launch::spawn(&path, &opts) {
    Ok(mut child) => watch_child(child),            // refresh "Latest played" on exit
    Err(e) => show_error(&format!("Could not start: {e}")),
}
```
`controls_dialog`: build the lines from
`breadbin_core::launch::controls_description(breadbin_core::launch::joystick_present())`
and show them in an `adw::AlertDialog` with responses `start`/`cancel`. Return `true`
for `start`.

`watch_child`: spawn a thread that calls `child.wait()`, then notify the main loop to
refresh the "Latest played" section:
```rust
let (tx, rx) = async_channel::bounded(1);
std::thread::spawn(move || { let _ = child.wait(); let _ = tx.send_blocking(()); });
glib::spawn_future_local(clone!(#[weak] window, async move {
    let _ = rx.recv().await; reload_latest_played(&window);
}));
```

**M2 acceptance:** with C64 Forever ROMs configured and VICE installed, clicking a local
game shows the controls dialog and launches it in VICE while the window stays open; on
quitting VICE, "Latest played" updates. A non‑local game downloads then plays. With no
ROMs set, Play shows the requirement dialog instead.

---

## 4. Milestone M3 — Tunes view (jukebox + GL visualisers)

Outcome: ranked tunes by party, in‑app playback (audio ported verbatim), GL‑shader
visualisers reacting to the live SID, radio mode.

### Task M3.1 — Tune list

```rust
let tunes = breadbin_core::tunes::load_tunes();             // Vec<Tune>
let groups = breadbin_core::tunes::group_by_party(&tunes);  // Vec<(String, Vec<usize>)>
```
Render as a scrolled list grouped by party (header rows with `palette::BARS[i]` chips,
tune rows showing name/rating/group/year, ▶ on the now‑playing, ★ when rating ≥ 9.5).
Bootstrap: if `tunes` is empty, build the index off‑thread
(`breadbin_core::tunes::build_index(600, &mut progress)`), same pattern as M1.6.

### Task M3.2 — Playback (port `Audio` as‑is)

`Audio` is already GUI‑agnostic. To play:
```rust
let t = tunes[idx].clone();
let bytes = run_blocking(move || breadbin_core::audio::ensure_sid(&t)).await?; // downloads+caches .sid
let audio = breadbin_core::audio::Audio::start(bytes, 1)?;  // holds the cpal stream + gen thread
self.audio = Some(audio);                                   // drop to stop
```
Transport: play/pause → `audio.toggle_pause()`; next → next index in the row order;
radio → reuse the `Rng` + a 90s timer (`RADIO_SECS`) via `glib::timeout_add_seconds_local`.

### Task M3.3 — GL visualiser harness (`tunes/vis.rs`)

Use a `gtk::GLArea`. Load GL function pointers with `epoxy` and drive them with `glow`:
```rust
fn setup_gl_epoxy() {
    // once, before realizing any GLArea
    #[cfg(target_os="linux")]
    let lib = unsafe { libloading::Library::new("libepoxy.so.0") }.unwrap();
    epoxy::load_with(|name| unsafe {
        lib.get::<_>(name.as_bytes()).map(|s| *s).unwrap_or(std::ptr::null())
    });
}

area.connect_realize(|area| {
    area.make_current();
    let gl = unsafe { glow::Context::from_loader_function(|s| epoxy::get_proc_addr(s)) };
    // compile program from quad.vert + the active .frag; create a 1x256 RGBA/float
    // texture for the scope buffer; store gl + handles in the widget state.
});
area.connect_render(move |_area, _ctx| {
    // upload uniforms from the latest snapshot, draw a fullscreen triangle/quad
    glib::Propagation::Stop
});
// ~60 FPS:
area.add_tick_callback(|area, _clock| { area.queue_render(); glib::ControlFlow::Continue });
```
**Uniform feed** each frame, from the audio:
```rust
let vis = audio.snapshot();   // breadbin_core::sid::Vis { regs, frame }
let scope = audio.scope();    // Vec<i16>, length 256
// u_time = vis.frame as f32 / 50.0
// u_volume = vis.volume() as f32 / 15.0
// per voice v in 0..3: vis.voice_freq(v), vis.voice_gate(v) as f32, vis.voice_wave(v) as f32
// upload `scope` (normalize i16 -> f32 in [-1,1]) into the 256-wide texture
```

### Task M3.4 — Shaders (`data/shaders/`)

A shared `quad.vert` (fullscreen triangle) and one `.frag` per mode. Start with three,
re‑imagining the originals as true shaders (no braille):
- `scope.frag` — sample the scope texture across `u_time`, glow/bloom, C64 palette line.
- `fireball.frag` — radial turbulent flame; reuse the *heat ramp* as a GLSL `mix` over
  the C64 ember→red→orange→yellow→white stops; modulate radius by `u_volume`/scope energy.
- `cubes.frag` (or a vertex‑shader cube field) — reactive tumbling grid.
Target **GLSL ES 3.00** (`#version 300 es`, `precision highp float;`) for portability.
`VisMode` in the GUI = which `.frag` is bound; radio cycles them. Bundle the shaders as
files under `data/shaders/` (installed in M4) and load at realize time.

**M3 acceptance:** selecting a tune plays audio and animates the visualiser in time with
it; pause/next/radio work; switching visualiser swaps the shader live; runs on llvmpipe
(software GL) without crashing (test: `LIBGL_ALWAYS_SOFTWARE=1 cargo run`).

---

## 5. Milestone M3b — Demos view

Outcome: a screenshot grid by party; activating a demo downloads and runs it in VICE.

```rust
let demos = breadbin_core::demos::load_demos();              // Vec<Demo>
let groups = breadbin_core::core::group_by_party(&demos, |d| d.party.as_str(),
                                                 |d| d.rating, 2, 50, true);
```
- Reuse the **cover‑card widget** and the **section grid** from M2; screenshots come
  from `breadbin_core::demos::ensure_shot(&demo)` → `texture_for` (no badges).
- Bootstrap with `breadbin_core::demos::build_index(1000, &mut progress)` if empty.
- Activate a demo:
  ```rust
  let Some(opts) = settings.launch_opts() else { show_rom_required_dialog(); return; };
  let path = run_blocking(move || breadbin_core::demos::fetch_and_prepare(&demo)).await?; // PathBuf
  let child = breadbin_core::launch::spawn(&path, &opts)?; // no controls dialog (demos aren't interactive)
  watch_child(child);
  ```

**M3b acceptance:** demos load as screenshots grouped by party; activating one (ROMs
present) downloads and runs it in VICE.

---

## 6. Milestone M4 — Packaging & desktop integration

### Task M4.1 — Desktop assets (`crates/breadbin-gui/data/`)
- `io.github.jacobandresen.Breadbin.desktop`:
  ```ini
  [Desktop Entry]
  Name=breadbin
  Comment=Commodore 64 game library
  Exec=breadbin
  Icon=io.github.jacobandresen.Breadbin
  Terminal=false
  Type=Application
  Categories=Game;Emulator;
  ```
- `io.github.jacobandresen.Breadbin.metainfo.xml` (AppStream): `<id>`, `<name>`,
  `<summary>`, `<description>`, `<screenshots>`, `<releases>`,
  `<content_rating type="oars-1.1"/>`, `<launchable>...desktop</launchable>`. Validate
  with `appstreamcli validate`.
- Icon: scalable SVG at `data/icons/hicolor/scalable/apps/io.github.jacobandresen.Breadbin.svg`.
- The compiled GSettings schema (M1.3), `style.css`, `data/fonts/unscii-16.ttf`,
  `data/shaders/*`. Decide install prefix: `/usr/share/breadbin/{shaders,style.css}` and
  load via that path (with a dev fallback to the source tree).

### Task M4.2 — `.deb` (Debian/Ubuntu) with `cargo-deb`
Add to `breadbin-gui/Cargo.toml`:
```toml
[package.metadata.deb]
depends = "libgtk-4-1, libadwaita-1-0"
recommends = "vice"
assets = [
  ["target/release/breadbin", "usr/bin/", "755"],
  ["data/io.github.jacobandresen.Breadbin.desktop", "usr/share/applications/", "644"],
  ["data/io.github.jacobandresen.Breadbin.metainfo.xml", "usr/share/metainfo/", "644"],
  ["data/icons/hicolor/scalable/apps/io.github.jacobandresen.Breadbin.svg", "usr/share/icons/hicolor/scalable/apps/", "644"],
  ["data/io.github.jacobandresen.Breadbin.gschema.xml", "usr/share/glib-2.0/schemas/", "644"],
  ["data/fonts/unscii-16.ttf", "usr/share/fonts/truetype/breadbin/", "644"],
  ["data/style.css", "usr/share/breadbin/", "644"],
  ["data/shaders/*", "usr/share/breadbin/shaders/", "644"],
]
maintainer-scripts = "debian/"   # postinst: glib-compile-schemas /usr/share/glib-2.0/schemas
```
Build: `cargo deb -p breadbin-gui`. Test in a clean Debian + Ubuntu container.

### Task M4.3 — Arch `PKGBUILD`
```bash
pkgname=breadbin
depends=(gtk4 libadwaita)
optdepends=('vice: launch games and demos')
# build: cargo build --release --locked
# package: install -Dm755 binary; install desktop/metainfo/icon/schema/css/shaders/font
# post_install(): glib-compile-schemas /usr/share/glib-2.0/schemas; update-desktop-database
```
Publish `breadbin` (release) and optionally `breadbin-git` to the AUR.

### Task M4.4 — Flatpak (`io.github.jacobandresen.Breadbin.yaml`)
- `runtime: org.gnome.Platform` (current stable), `sdk: org.gnome.Sdk`,
  `sdk-extensions: [org.freedesktop.Sdk.Extension.rust-stable]`.
- Build the Rust app with `flatpak-cargo-generator.py` producing
  `cargo-sources.json` (offline deps).
- **finish-args:** `--share=network`, `--socket=pulseaudio`, `--socket=fallback-x11`,
  `--socket=wayland`, `--device=dri` (GL), `--talk-name=org.freedesktop.Flatpak` (for
  `flatpak-spawn`), `--filesystem=` for the games folder and the C64 Forever ROM folder.
- **Launching host VICE from the sandbox:** in `launch::spawn`, when running inside
  Flatpak (`/.flatpak-info` exists), prefix the command with
  `["flatpak-spawn", "--host", ...]`. The ROM dir and game paths must be on a permitted
  `--filesystem=` mount. Add a small `is_sandboxed()` helper and branch in `spawn`.
- Bundle Unscii in the Flatpak. (Style64 stays opt‑in only.)

**M4 acceptance:** `.deb` installs and launches on fresh Debian and Ubuntu; PKGBUILD
installs and launches on Arch; Flatpak builds, runs, plays tunes, and launches a game
via `flatpak-spawn --host` to host VICE. `appstreamcli validate` and
`desktop-file-validate` pass.

---

## 7. Milestone M5 — Polish (optional, after v1)

- **Fullscreen "kiosk"/big‑picture mode** (F11): larger cards, then **gamepad
  navigation** (read `/dev/input/js*` or use `gilrs`; map d‑pad/stick to focus moves,
  A to activate). Honour the existing controller detection in `launch.rs`.
- **CRT post‑process shader**: a final fragment pass (scanlines, barrel curvature,
  phosphor glow, vignette) over the GLArea content; bound to the `crt-effect` setting;
  default on in fullscreen kiosk, off in the windowed library.
- **Per‑game overrides** (warp/fullscreen/keyboard per title), persisted in a small
  TSV/JSON under the data dir.
- Additional shader visualisers (plasma, tunnel, raster bars, starfield).

---

## 8. C64 Forever ROM handling — concrete checklist

This is the policy from plan §4b/§9, implemented across the milestones:

1. **Never download ROMs.** No code path fetches KERNAL/BASIC/CHARGEN/1541. (Game/disk/
   tape images from IA + the Ultimate Tape Archive are downloaded as before.)
2. **Detection** (`core/src/roms.rs`, M0.7): `roms::detect(configured)` returns a
   `Forever` — **prefer `BundledVice`** (Cloanto's own `x64.exe` via wine, the verified
   path) over `RomDir` (extracted ROM files, secondary/unverified filenames). It checks
   the `c64forever-roms` setting and `C64_FOREVER_ROMS`, plus known Cloanto install paths
   (the existing `WINE_X64` location).
3. **Launch** (`launch.rs::spawn`, M0.6/M0.7): for `BundledVice`, run wine + Cloanto's
   VICE (it supplies its own ROMs) — no ROM flags. For `RomDir` (only after confirming
   filenames), run native VICE with `-kernal/-basic/-chargen` (+ `-dos1541`), flag
   spellings validated via `emu_help`. With the licensed 1541 ROM present, **TDE is
   always on**; the ROM‑less virtual‑drive fallback is removed.
4. **Gating** (M2.7, M3b): Play/Run call `settings.launch_opts()`, which is `None` when
   `roms::detect` finds no C64 Forever; in that case show the **requirement dialog** (a
   "point me at C64 Forever" file chooser + a link to obtain it) instead of launching.
5. **Startup notice** (M1.5): the boot splash / first‑run screen states *"breadbin
   requires a licensed copy of C64 Forever for its ROMs. Tunes work without it."* A
   dismissible banner persists while ROMs are missing.
6. **Tunes never gated.** The SID engine needs no ROM — Tunes is always available.

---

## 9. Testing & verification matrix

| Layer | How to test |
| --- | --- |
| core unit | `cargo test -p breadbin-core` (norm, seq_ratio, disk title/ratio, SID render peak/rms, sections ordering) |
| core SID | a `#[test]` that renders ~2s of a bundled .sid and asserts non‑silence (port the old `_sid` dev aid) |
| GUI smoke | `cargo run -p breadbin-gui` with empty `~/.breadbin` → bootstrap → 3 views |
| launch | with ROMs + VICE: play a local game; confirm window stays open and "Latest played" updates on exit |
| no‑ROM | unset ROM dir → Play shows requirement dialog; Tunes still plays |
| GL | `LIBGL_ALWAYS_SOFTWARE=1 cargo run` → visualisers render (llvmpipe) |
| packaging | install `.deb` on clean Debian & Ubuntu; PKGBUILD on Arch; Flatpak build+run; `appstreamcli validate`, `desktop-file-validate` |

---

## 10. Order of work (dependency‑sorted task list)

1. M0.1 → M0.2 → M0.3 → M0.4 → M0.5 → M0.6 → M0.7 → M0.8  (core compiles & tested)
2. M1.1 → M1.2 → M1.3 → M1.4 → M1.5 → M1.6  (window + skin + bootstrap)
3. M2.1 → M2.2 → M2.3 → M2.4 → M2.5 → M2.6 → M2.7  (kiosk)
4. M3.1 → M3.2 → M3.3 → M3.4  (tunes + GL)
5. M3b  (demos — reuses M2 widgets + M3 nothing)
6. M4.1 → M4.2 → M4.3 → M4.4  (packaging)
7. M5  (polish, optional)

Do not start a milestone before the previous one's **acceptance** line passes.
```
