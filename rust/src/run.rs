// c64run - boot a Commodore 64 game straight into the emulator, no launcher GUI.
//
// Auto-picks a VICE emulator, preferring a LICENSE-FREE one (no C64 Forever needed):
//   1. native x64sc / x64 on PATH        (free VICE, its own bundled ROMs)
//   2. the net.sf.VICE flatpak           (free VICE, bundled ROMs)
//   3. Cloanto C64 Forever's x64.exe (wine, needs the license)  - fallback only
// Override with  C64_EMU="..."  (e.g. C64_EMU='x64' or a full flatpak command).
// VICE's -autostart is the emulator equivalent of typing  LOAD"*",8,1  then  RUN.

use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

const WINE_X64: &str =
    "/opt/wine/.wine/drive_c/Program Files (x86)/Cloanto/C64 Forever/VICE/x64.exe";

// Recognised C64 image types.
const EXTS: &[&str] = &[
    ".d64", ".d71", ".d81", ".t64", ".tap", ".prg", ".crt", ".g64", ".nib", ".p00", ".x64",
];

const HELP: &str = "\
c64run - boot a Commodore 64 game straight into the emulator, no launcher GUI.

Usage:
  c64run <game-file>             launch an exact .d64/.tap/.crt/.t64/.prg ...
  c64run <search words>          search your collection, launch the match
  c64run -W | --no-warp <s>      load at authentic speed (don't fast-forward)
  c64run -r | --real <s>         authentic-speed load (alias of --no-warp)
  c64run --windowed <s>          run in a window instead of fullscreen
  c64run -k | --keyboard <s>     numpad→joystick port 2, WASD→port 1
  c64run -l | --list <s>         just list matches, don't launch
  (-w / --warp and -f / --fullscreen are accepted too, but are now the default)
";

fn die(msg: &str) -> ! {
    eprintln!("c64run: {msg}");
    std::process::exit(1);
}

/// Where your games live. Override the root with C64_LIB="/path/to/c64".
fn lib_dirs() -> Vec<PathBuf> {
    let root = std::env::var("C64_LIB").unwrap_or_else(|_| {
        let home = std::env::var("HOME").unwrap_or_default();
        format!("{home}/Games/Commodore/C64")
    });
    vec![PathBuf::from(root)]
}

/// Check for a flatpak app by looking at known install paths — no subprocess.
fn flatpak_installed(app_id: &str) -> bool {
    let home = std::env::var("HOME").unwrap_or_default();
    let roots = [
        format!("{home}/.local/share/flatpak/app"),
        "/var/lib/flatpak/app".to_string(),
    ];
    roots.iter().any(|r| Path::new(r).join(app_id).is_dir())
}

/// Return the emulator command as an argv list (license-free first).
fn pick_emulator() -> Vec<String> {
    if let Ok(env) = std::env::var("C64_EMU") {
        if let Some(parts) = shlex::split(&env) {
            return parts;
        }
        die("could not parse C64_EMU");
    }
    for exe in ["x64sc", "x64"] {
        if which::which(exe).is_ok() {
            return vec![exe.to_string()];
        }
    }
    if which::which("flatpak").is_ok() && flatpak_installed("net.sf.VICE") {
        return ["flatpak", "run", "--command=x64sc", "net.sf.VICE"]
            .map(String::from)
            .to_vec();
    }
    if Path::new(WINE_X64).is_file() {
        // SAFETY: single-threaded at this point; just defaulting env for the child.
        if std::env::var_os("WINEPREFIX").is_none() {
            unsafe { std::env::set_var("WINEPREFIX", "/opt/wine/.wine") };
        }
        if std::env::var_os("WINEDEBUG").is_none() {
            unsafe { std::env::set_var("WINEDEBUG", "-all") };
        }
        return vec!["wine".to_string(), WINE_X64.to_string()];
    }
    die("no VICE found - install 'vice', the net.sf.VICE flatpak, or C64 Forever");
}

fn has_image_ext(name: &str) -> bool {
    let lower = name.to_lowercase();
    EXTS.iter().any(|e| lower.ends_with(e))
}

/// Flags that make drive 8 use VICE's virtual (host-filesystem) device instead of
/// true drive emulation. Needed where VICE ships without the (non-free) 1541 drive
/// ROMs — e.g. the Debian/Ubuntu package — or every LOAD"*",8,1 fails with
/// ?DEVICE NOT PRESENT. VICE renamed this option across versions: older builds use
/// `-virtualdev8`; newer ones (e.g. Homebrew's) split it into `-trapdevice8` +
/// `+drive8truedrive`. Passing the wrong name makes VICE bail with "error parsing
/// command line option" and never start, so we ask the emulator's own `-help`
/// which spelling it understands rather than hardcoding one.
fn virtual_drive_flags(emu: &[String]) -> Vec<String> {
    let help = Command::new(&emu[0])
        .args(&emu[1..])
        .arg("-help")
        .output()
        .map(|o| {
            let mut s = String::from_utf8_lossy(&o.stdout).into_owned();
            s.push_str(&String::from_utf8_lossy(&o.stderr));
            s
        })
        .unwrap_or_default();
    if help.contains("-virtualdev8") {
        vec!["-virtualdev8".into()]
    } else if help.contains("-trapdevice8") {
        vec!["-trapdevice8".into(), "+drive8truedrive".into()]
    } else {
        // Unknown build: pass nothing rather than a flag it will reject. With drive
        // ROMs present (e.g. a full Homebrew/Windows VICE) true drive emulation works
        // anyway; only the ROM-less packages actually need the virtual device.
        Vec::new()
    }
}

/// Walk the library roots for image files whose name contains the query.
fn find_matches(query: &str) -> Vec<PathBuf> {
    let ql = query.to_lowercase();
    let mut out = Vec::new();
    for root in lib_dirs() {
        walk(&root, &mut |p| {
            if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
                if has_image_ext(name) && name.to_lowercase().contains(&ql) {
                    out.push(p.to_path_buf());
                }
            }
        });
    }
    out.sort();
    out
}

/// Recursively visit every file under `dir`, calling `f` for each.
fn walk(dir: &Path, f: &mut dyn FnMut(&Path)) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        match entry.file_type() {
            Ok(t) if t.is_dir() => walk(&path, f),
            Ok(_) => f(&path),
            Err(_) => {}
        }
    }
}

/// Resolve multiple matches: interactive numbered pick on a tty, else bail.
fn choose(matches: &[PathBuf], query: &str) -> PathBuf {
    use std::io::{IsTerminal, Write};
    eprintln!("Multiple matches for \"{query}\":");
    if !std::io::stdin().is_terminal() {
        for m in matches {
            eprintln!("  {}", m.display());
        }
        die("be more specific, or pass the full path");
    }
    for (i, m) in matches.iter().enumerate() {
        eprintln!("  {}) {}", i + 1, m.display());
    }
    loop {
        eprint!("# ? ");
        let _ = std::io::stderr().flush();
        let mut line = String::new();
        if std::io::stdin().read_line(&mut line).unwrap_or(0) == 0 {
            std::process::exit(0); // EOF
        }
        if let Ok(n) = line.trim().parse::<usize>() {
            if (1..=matches.len()).contains(&n) {
                return matches[n - 1].clone();
            }
        }
    }
}

pub fn main(argv: Vec<String>) -> ExitCode {
    let (mut warp, mut fullscreen, mut list_only, mut keyboard) = (true, true, false, false);
    let mut words: Vec<String> = Vec::new();

    let mut i = 0;
    while i < argv.len() {
        let a = argv[i].as_str();
        match a {
            "-w" | "--warp" => warp = true, // default; back-compat
            "-W" | "--no-warp" => warp = false,
            "-r" | "--real" => warp = false, // authentic-speed load
            "-f" | "--fullscreen" => fullscreen = true, // default; back-compat
            "--windowed" => fullscreen = false,
            "-k" | "--keyboard" => keyboard = true,
            "-l" | "--list" => list_only = true,
            "-h" | "--help" => {
                print!("{HELP}");
                return ExitCode::SUCCESS;
            }
            "--" => {
                words.extend(argv[i + 1..].iter().cloned());
                break;
            }
            _ if a.starts_with('-') => die(&format!("unknown option: {a} (try --help)")),
            _ => words.push(a.to_string()),
        }
        i += 1;
    }

    if words.is_empty() {
        die("give a game file or search words (try --help)");
    }

    // A single argument that is an existing file is taken verbatim; otherwise the
    // words form a search query against the collection.
    let game: PathBuf = if words.len() == 1 && Path::new(&words[0]).is_file() {
        PathBuf::from(&words[0])
    } else {
        let query = words.join(" ");
        let matches = find_matches(&query);
        match matches.len() {
            0 => die(&format!("no game matching: {query}")),
            _ if list_only => {
                for m in &matches {
                    println!("{}", m.display());
                }
                return ExitCode::SUCCESS;
            }
            1 => matches.into_iter().next().unwrap(),
            _ => choose(&matches, &query),
        }
    };

    let emu = pick_emulator();

    // Serve the disk image through VICE's virtual filesystem rather than hardware
    // 1541 TDE, so a ROM-less VICE still boots games (see virtual_drive_flags for the
    // version-dependent option names). Fully compatible with standard d64/t64/crt
    // autostart; in-game fastloaders that need real TDE won't work when ROMs are absent.
    let mut opts: Vec<String> = virtual_drive_flags(&emu);
    if warp {
        opts.push("-autostart-warp".into()); // fast-forward loading, then normal speed
    }
    if fullscreen {
        opts.push("-VICIIfull".into()); // VIC-II (C64) fullscreen
    }
    if keyboard {
        // Map keyboard keysets to joystick ports so the game is playable without a gamepad.
        // Port 2 (most games): numpad  8=up 2=down 4=left 6=right 0=fire
        // Port 1 (2-player):   WASD + Left-Shift=fire
        opts.extend(
            [
                "-joydev2", "3", // port 2 → keyset B (numpad)
                "-joydev1", "2", // port 1 → keyset A (WASD)
                "-keysetbup", "KP_8", "-keysetbdown", "KP_2", "-keysetbleft", "KP_4",
                "-keysetbright", "KP_6", "-keysetbfire", "KP_0", "-keysetaup", "w",
                "-keysetadown", "s", "-keysetaleft", "a", "-keysetaright", "d",
                "-keysetafire", "shift",
            ]
            .map(String::from),
        );
    }

    // On Linux, force the X11 backend so VICE doesn't end up on a Wayland renderer
    // (an SDL issue). Never do this elsewhere: on macOS there is no X server by
    // default, so SDL_VIDEODRIVER=x11 makes VICE fail to open a window and exit
    // (which looked like "VICE never starts").
    #[cfg(target_os = "linux")]
    if std::env::var_os("SDL_VIDEODRIVER").is_none() {
        // SAFETY: single-threaded; setting a default for the exec'd child.
        unsafe { std::env::set_var("SDL_VIDEODRIVER", "x11") };
    }

    if std::env::var_os("C64_QUIET").is_none() {
        let emu_name = Path::new(&emu[0])
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| emu[0].clone());
        let game_name = game
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        eprintln!("LOAD\"*\",8,1 : RUN   ->  {game_name}   [{emu_name}]");
    }

    // Replace this process with the emulator (no shell, no return path).
    let mut cmd = Command::new(&emu[0]);
    cmd.args(&emu[1..]).args(&opts).arg("-autostart").arg(&game);
    let err = cmd.exec(); // only returns on failure
    die(&format!("could not exec {}: {err}", emu[0]));
}
