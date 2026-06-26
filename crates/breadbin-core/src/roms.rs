// roms - locate the user's licensed C64 Forever installation. breadbin NEVER downloads
// Commodore ROMs; they come only from the user's C64 Forever copy. The verified launch
// path runs Cloanto's own bundled VICE (which finds its own ROMs); a secondary path
// would feed extracted ROM files to a native VICE, but Cloanto's exact filenames/layout
// are unconfirmed, so that branch is gated behind has_required_roms() and must be
// verified against a real install before being trusted.

use std::path::{Path, PathBuf};

/// The default Cloanto C64 Forever VICE path under the repo's wine prefix (mirrors the
/// WINE_X64 constant the old run.rs used).
const CLOANTO_X64: &str =
    "/opt/wine/.wine/drive_c/Program Files (x86)/Cloanto/C64 Forever/VICE/x64.exe";
const CLOANTO_WINEPREFIX: &str = "/opt/wine/.wine";

/// How to launch using the licensed C64 Forever ROMs.
#[derive(Clone, Debug)]
pub enum Forever {
    /// VERIFIED: run Cloanto's bundled VICE (it locates its own licensed ROMs).
    BundledVice { x64_exe: PathBuf, wine_prefix: Option<PathBuf> },
    /// UNVERIFIED: a directory of extracted ROM files to feed a native VICE. Only
    /// constructed once the real Cloanto filenames/layout are confirmed.
    RomDir(PathBuf),
}

/// Detect a usable C64 Forever installation, or None.
/// Order: prefer the verified bundled-VICE form; else a configured/env ROM directory.
pub fn detect(configured: Option<&str>) -> Option<Forever> {
    if let Some(exe) = find_cloanto_x64() {
        return Some(Forever::BundledVice {
            x64_exe: exe,
            wine_prefix: Some(PathBuf::from(CLOANTO_WINEPREFIX)),
        });
    }
    let dir = configured
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("C64_FOREVER_ROMS").map(PathBuf::from))?;
    has_required_roms(&dir).then_some(Forever::RomDir(dir))
}

/// True when a usable C64 Forever install is present.
pub fn present(configured: Option<&str>) -> bool {
    detect(configured).is_some()
}

/// Locate Cloanto's bundled VICE x64.exe at known install paths.
fn find_cloanto_x64() -> Option<PathBuf> {
    let p = Path::new(CLOANTO_X64);
    p.is_file().then(|| p.to_path_buf())
}

/// UNVERIFIED filename guess (Cloanto/VICE C64 ROMs, no extension) — confirm against a
/// real C64 Forever install before relying on the RomDir branch.
fn has_required_roms(dir: &Path) -> bool {
    ["kernal", "basic", "chargen"].iter().all(|f| dir.join(f).is_file())
}
