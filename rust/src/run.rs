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
  c64run -k | --keyboard <s>     force keyboard for both players (ignore any joystick)
  c64run --drive-sound <s>       play the authentic 1541 drive loading sound (default)
  c64run --no-drive-sound <s>    silence the emulated drive
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

/// The emulator's `-help` text (stdout + stderr), used to probe which option
/// spellings this VICE build understands. Empty if the emulator can't be run.
fn emu_help(emu: &[String]) -> String {
    Command::new(&emu[0])
        .args(&emu[1..])
        .arg("-help")
        .output()
        .map(|o| {
            let mut s = String::from_utf8_lossy(&o.stdout).into_owned();
            s.push_str(&String::from_utf8_lossy(&o.stderr));
            s
        })
        .unwrap_or_default()
}

/// Drive 8 emulation flags. By default we enable cycle-exact True Drive Emulation
/// (TDE): fastloaders and copy-protected games — a large slice of commercial
/// titles like Out Run — drive the 1541 hardware directly and only load under TDE.
/// Without it they autostart their boot file and then hang.
///
/// With `virtual_drive` we instead serve the disk through VICE's virtual
/// (host-filesystem) device: lower fidelity (fastloaders break), but the only mode
/// that works on a ROM-less VICE — e.g. a Linux package missing the non-free 1541
/// ROMs, where TDE can't run and every LOAD"*",8,1 fails with ?DEVICE NOT PRESENT.
/// The caller turns this on via C64_VIRTUAL_DRIVE or by auto-detecting that ROM-less
/// case (see `missing_tde_rom`).
///
/// Option spellings vary across builds and an unknown one makes VICE bail without
/// starting, so we probe the emulator's own `-help` for what it accepts.
fn drive_flags(help: &str, virtual_drive: bool) -> Vec<String> {
    if virtual_drive {
        if help.contains("-virtualdev8") {
            return vec!["-virtualdev8".into()];
        }
        if help.contains("-trapdevice8") {
            return vec!["-trapdevice8".into(), "+drive8truedrive".into()];
        }
        return Vec::new();
    }
    if help.contains("-drive8truedrive") {
        // True drive emulation (for fastloaders / copy protection), but let autostart
        // pull the program in quickly: -autostart-handle-tde turns TDE off just for the
        // autostart load (so it streams through the instant kernal trap) and back on
        // for the game. Normal games would otherwise load through the cycle-exact 1541
        // — correct but slow. -trapdevice8 keeps that fast loader available.
        let mut flags = vec!["-drive8truedrive".to_string()];
        if help.contains("-autostart-handle-tde") {
            flags.push("-trapdevice8".to_string());
            flags.push("-autostart-handle-tde".to_string());
        }
        flags
    } else if help.contains("-truedrive") {
        vec!["-truedrive".into()] // older VICE: global true-drive toggle
    } else {
        Vec::new()
    }
}

/// Authentic 1541 drive sound: VICE can emulate the mechanical whir and head-step
/// clatter the real floppy made while loading. Enabled by default; toggle per-run
/// with --drive-sound / --no-drive-sound, or persistently with C64_DRIVE_SOUND=0
/// (or false/no). C64_DRIVE_SOUND_VOLUME sets loudness on VICE's 0-4000 scale
/// (default 2000).
///
/// Two caveats the caller can't change here: the sound is produced by the emulated
/// 1541 mechanics, so it only plays under True Drive Emulation (not the ROM-less
/// virtual drive), and VICE mutes audio during warp fast-loading — so it's most
/// audible for in-game disk access and in --no-warp mode. Harmless when silent,
/// hence on by default. Option spellings are probed from `-help` like the rest.
fn drive_sound_flags(help: &str, cli: Option<bool>) -> Vec<String> {
    // Precedence: --drive-sound/--no-drive-sound, then C64_DRIVE_SOUND, then on.
    let on = cli.unwrap_or_else(|| match std::env::var("C64_DRIVE_SOUND") {
        Ok(v) => !matches!(v.trim(), "" | "0" | "false" | "no"),
        Err(_) => true,
    });
    if !on || !help.contains("-drivesound") {
        return Vec::new();
    }
    let mut flags = vec!["-drivesound".to_string()];
    if help.contains("-drivesoundvolume") {
        let vol = std::env::var("C64_DRIVE_SOUND_VOLUME")
            .ok()
            .filter(|s| s.trim().parse::<u32>().is_ok())
            .unwrap_or_else(|| "2000".to_string());
        flags.push("-drivesoundvolume".to_string());
        flags.push(vol);
    }
    flags
}

/// True when the chosen emulator is a native Linux VICE with no 1541 drive ROM on
/// its search path — the one case where True Drive Emulation can't start the drive
/// and `LOAD"*",8,1` fails with `?DEVICE NOT PRESENT ERROR`. The flatpak and the
/// Cloanto/wine build bundle their own ROMs, and on macOS the Homebrew cask does
/// too, so this only fires for a bare distro `vice` package missing the non-free
/// drive ROM (see install_vice_roms in setup-dependencies.sh).
fn missing_tde_rom(emu: &[String]) -> bool {
    #[cfg(target_os = "linux")]
    {
        let exe = Path::new(&emu[0])
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        // Only a directly-invoked native binary is at risk; flatpak/wine/etc. ship ROMs.
        (exe == "x64" || exe == "x64sc") && !tde_rom_available()
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = emu;
        false
    }
}

/// Scan VICE's ROM search directories for any `dos1541*` drive ROM.
#[cfg(target_os = "linux")]
fn tde_rom_available() -> bool {
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(d) = std::env::var_os("VICE_DATADIR") {
        dirs.push(Path::new(&d).join("DRIVES"));
    }
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(Path::new(&home).join(".local/share/vice/DRIVES"));
    }
    for base in ["/usr/lib/vice", "/usr/share/vice", "/usr/local/lib/vice", "/usr/local/share/vice"]
    {
        dirs.push(Path::new(base).join("DRIVES"));
    }
    dirs.iter().any(|d| {
        std::fs::read_dir(d)
            .map(|rd| {
                rd.flatten().any(|e| {
                    e.file_name().to_string_lossy().to_ascii_lowercase().starts_with("dos1541")
                })
            })
            .unwrap_or(false)
    })
}

/// Walk the library roots for image files whose name contains the query.
fn find_matches(query: &str) -> Vec<PathBuf> {
    let ql = query.to_lowercase();
    let mut out = Vec::new();
    for root in lib_dirs() {
        crate::core::walk_files(&root, &mut |p| {
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

/// True if a game controller (joystick / gamepad) appears to be connected.
/// Override with C64_JOYSTICK=1 (force on) or C64_JOYSTICK=0 (force off) when the
/// best-effort, platform-specific detection below guesses wrong.
pub fn joystick_present() -> bool {
    if let Ok(v) = std::env::var("C64_JOYSTICK") {
        return !matches!(v.trim(), "" | "0" | "false" | "no");
    }
    #[cfg(target_os = "linux")]
    {
        let any_entry = |dir: &str, pred: &dyn Fn(&str) -> bool| -> bool {
            std::fs::read_dir(dir)
                .map(|rd| rd.flatten().any(|e| pred(&e.file_name().to_string_lossy())))
                .unwrap_or(false)
        };
        // The legacy joystick API (joydev module) exposes each pad as /dev/input/jsN.
        if any_entry("/dev/input", &|n| n.starts_with("js") && n[2..].parse::<u32>().is_ok()) {
            return true;
        }
        // joydev may not be loaded, but udev still tags controllers in by-id/by-path
        // (e.g. usb-Sony_Controller-event-joystick, ...-event-gamepad) off the evdev
        // node, so look for those names too.
        ["/dev/input/by-id", "/dev/input/by-path"].iter().any(|dir| {
            any_entry(dir, &|n| {
                let n = n.to_ascii_lowercase();
                n.contains("joystick") || n.contains("gamepad")
            })
        })
    }
    #[cfg(target_os = "macos")]
    {
        // A real controller is PrimaryUsage 4 (Joystick) or 5 (Game Pad) on
        // PrimaryUsagePage 1 (Generic Desktop). Both checks matter: usage 4/5 on
        // other pages are trackpads, sensors, Touch ID, etc. ioreg prints the two
        // properties on adjacent lines (usage, then page), so test them pairwise.
        fn ioreg_val(line: &str, key_eq: &str) -> Option<i64> {
            if line.contains(key_eq) {
                line.rsplit('=').next()?.trim().parse().ok()
            } else {
                None
            }
        }
        Command::new("ioreg")
            .args(["-r", "-c", "IOHIDDevice"])
            .output()
            .map(|o| {
                let text = String::from_utf8_lossy(&o.stdout);
                let lines: Vec<&str> = text.lines().collect();
                lines.windows(2).any(|w| {
                    matches!(ioreg_val(w[0], "\"PrimaryUsage\" ="), Some(4) | Some(5))
                        && ioreg_val(w[1], "\"PrimaryUsagePage\" =") == Some(1)
                })
            })
            .unwrap_or(false)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        false
    }
}

/// One human-readable line per player for the current input scheme, for the
/// kiosk's pre-launch dialog and c64run's own banner.
pub fn controls_description(joystick: bool) -> Vec<String> {
    if joystick {
        vec![
            "Player 1   Joystick".to_string(),
            "Player 2   Keyboard — W A S D move, Space = fire".to_string(),
        ]
    } else {
        vec![
            "Player 1   Keyboard — W A S D move, Space = fire".to_string(),
            "Player 2   Keyboard — Arrow keys move, Right-Shift = fire".to_string(),
        ]
    }
}

/// Write a small VICE config defining the keyboard joystick keysets, and return
/// its path. Keyset 1 = WASD + Space (the primary keyboard player); Keyset 2 =
/// arrow keys + Right-Shift (a second player). Values are GDK keyvals — the format
/// the GTK build wants, and the letter/space ones match the SDL builds too. This
/// is the only way to map these keys: VICE has no command-line option to *define*
/// keyset keys, only to enable the feature. Returns None if the file can't be
/// written.
fn write_controls_config() -> Option<std::path::PathBuf> {
    // w=119 a=97 s=115 d=100 space=32 ; arrows 65361-65364 ; Shift_R=65506
    let body = "\
KeySetEnable=1
KeySet1North=119
KeySet1South=115
KeySet1West=97
KeySet1East=100
KeySet1Fire=32
KeySet2North=65362
KeySet2South=65364
KeySet2West=65361
KeySet2East=65363
KeySet2Fire=65506
";
    // x64sc reads the [C64SC] section, x64 reads [C64]; include both so either works.
    let ini = format!("[C64SC]\n{body}\n[C64]\n{body}");
    let path = crate::core::data_path("vice-controls.ini");
    std::fs::write(&path, ini).ok().map(|_| path)
}

/// VICE options wiring up the two control ports. The C64 reads single-player
/// games on port 2, so Player 1 = port 2 and Player 2 = port 1.
///
/// `-joydev1/2 <0-9>` selects each port's device: 2 = Keyset 1 (WASD), 3 = Keyset
/// 2 (arrows), 4 = the first host joystick (override with C64_JOYDEV). The keyset
/// keys themselves are defined in the `-config` file from [`write_controls_config`].
/// Either way the primary keyboard player gets WASD.
fn control_flags(joystick: bool) -> Vec<String> {
    let mut opts: Vec<String> = Vec::new();
    let cfg = write_controls_config();
    if let Some(path) = &cfg {
        opts.push("-config".to_string());
        opts.push(path.to_string_lossy().into_owned());
    }
    if joystick {
        let dev = std::env::var("C64_JOYDEV")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "4".to_string());
        // Player 1 = real joystick (port 2); Player 2 = Keyset 1 / WASD (port 1).
        opts.extend(["-joydev2".to_string(), dev, "-joydev1".to_string(), "2".to_string()]);
    } else if cfg.is_some() {
        // Player 1 = Keyset 1 / WASD (port 2); Player 2 = Keyset 2 / arrows (port 1).
        opts.extend(["-joydev2", "2", "-joydev1", "3"].map(String::from));
    } else {
        // Couldn't write the keyset config; fall back to the built-in Numpad.
        opts.extend(["-joydev2", "1"].map(String::from));
    }
    opts
}

pub fn main(argv: Vec<String>) -> ExitCode {
    let (mut warp, mut fullscreen, mut list_only, mut keyboard) = (true, true, false, false);
    let mut drive_sound: Option<bool> = None; // None = env/default; Some = CLI override
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
            "--drive-sound" => drive_sound = Some(true),
            "--no-drive-sound" => drive_sound = Some(false),
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
    let help = emu_help(&emu);

    // Drive mode: hardware True Drive Emulation by default, but a native Linux VICE
    // installed without the (non-free) 1541 drive ROM can't run it — every
    // LOAD"*",8,1 then fails with `?DEVICE NOT PRESENT ERROR`. Auto-fall back to the
    // ROM-less virtual drive in that case. C64_VIRTUAL_DRIVE overrides either way
    // (set it truthy to force virtual, or 0/false/no to force TDE).
    let virtual_drive = match std::env::var("C64_VIRTUAL_DRIVE") {
        Ok(v) => !matches!(v.trim(), "" | "0" | "false" | "no"),
        Err(_) => {
            let auto = missing_tde_rom(&emu);
            if auto && std::env::var_os("C64_QUIET").is_none() {
                eprintln!(
                    "note: no 1541 drive ROM found for VICE; using its virtual drive \
                     (in-game fastloaders may not work). Install the ROM, or set \
                     C64_VIRTUAL_DRIVE=0 to force True Drive Emulation."
                );
            }
            auto
        }
    };
    let mut opts: Vec<String> = drive_flags(&help, virtual_drive);
    opts.extend(drive_sound_flags(&help, drive_sound));
    if warp {
        opts.push("-autostart-warp".into()); // fast-forward loading, then normal speed
    }
    if fullscreen {
        opts.push("-VICIIfull".into()); // VIC-II (C64) fullscreen
    }
    // Controls: a connected joystick is Player 1, the keyboard is Player 2; with no
    // joystick both players are on the keyboard. -k forces keyboard-only.
    let joystick = !keyboard && joystick_present();
    opts.extend(control_flags(joystick));

    // Optional VICE diagnostics: C64_VICE_LOG=<path> (or =1 for vice.log in the data
    // dir) makes VICE write a verbose log of the launch, so a game that won't start
    // can be inspected. Propagates through the kiosk/menu, which exec c64run.
    if let Some(spec) = std::env::var_os("C64_VICE_LOG") {
        let spec = spec.to_string_lossy();
        let path = if spec == "1" || spec.eq_ignore_ascii_case("true") {
            crate::core::data_path("vice.log").to_string_lossy().into_owned()
        } else {
            spec.into_owned()
        };
        opts.push("-verbose".into());
        opts.push("-logfile".into());
        opts.push(path);
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
        for line in controls_description(joystick) {
            eprintln!("  {line}");
        }
    }

    // Replace this process with the emulator (no shell, no return path).
    let mut cmd = Command::new(&emu[0]);
    cmd.args(&emu[1..]).args(&opts).arg("-autostart").arg(&game);
    let err = cmd.exec(); // only returns on failure
    die(&format!("could not exec {}: {err}", emu[0]));
}
