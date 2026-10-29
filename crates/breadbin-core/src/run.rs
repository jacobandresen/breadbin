// run - boot a Commodore 64 game or demo in VICE as a child of the GUI.
//
// Uses C64 Forever's bundled VICE under wine when installed, else native VICE
// (x64sc / x64 on PATH, or the net.sf.VICE flatpak) with its own ROMs.
// VICE's -autostart is the emulator equivalent of typing  LOAD"*",8,1  then  RUN.

use std::path::{Path, PathBuf};
use std::process::Command;

/// Check for a flatpak app by looking at known install paths — no subprocess.
fn flatpak_installed(app_id: &str) -> bool {
    let home = std::env::var("HOME").unwrap_or_default();
    let roots = [
        format!("{home}/.local/share/flatpak/app"),
        "/var/lib/flatpak/app".to_string(),
    ];
    roots.iter().any(|r| Path::new(r).join(app_id).is_dir())
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
            // Each option is probed separately: some builds (e.g. VICE on Ubuntu)
            // ship -autostart-handle-tde but not -trapdevice8, and emitting an
            // unknown option makes VICE bail with "Unknown option '-trapdevice8'".
            if help.contains("-trapdevice8") {
                flags.push("-trapdevice8".to_string());
            }
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
/// drive ROM.
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
        if ["/dev/input/by-id", "/dev/input/by-path"].iter().any(|dir| {
            any_entry(dir, &|n| {
                let n = n.to_ascii_lowercase();
                n.contains("joystick") || n.contains("gamepad")
            })
        }) {
            return true;
        }
        // TheC64 joystick enumerates as a HID keyboard device, so it never creates
        // /dev/input/jsN and its by-id name carries "kbd" not "joystick". Detect it
        // through the evdev device name in /sys/class/input instead.
        controller_kind() == ControllerKind::TheC64
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
/// kiosk's pre-launch dialog.
pub fn controls_description(joystick: bool) -> Vec<String> {
    if joystick {
        vec![
            format!("Player 1   {}   {}", controller_name(), controller_icon()),
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
    let mut body = String::from(
        "\
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
",
    );
    // VICE 3.10's joystick subsystem opens a host pad but loads no default mapping
    // for it, so e.g. a DualSense is dead in-game (sticks/buttons bind to nothing).
    // Point the JoyMapFile resource at a per-device .vjm to actually wire it up.
    // C64_JOYMAP=<path> forces a specific map; otherwise we auto-supply the bundled
    // DualSense map when a PS5 pad is detected (see write_joymap). The map binds
    // host joystick 0, i.e. VICE's "Analog joystick 0" / -joydev 4.
    #[cfg(target_os = "linux")]
    if let Some(p) = joymap_path() {
        body.push_str(&format!("JoyMapFile={}\n", p.to_string_lossy()));
    }
    // x64sc reads the [C64SC] section, x64 reads [C64]; include both so either works.
    let ini = format!("[C64SC]\n{body}\n[C64]\n{body}");
    let path = crate::core::data_path("vice-controls.ini");
    std::fs::write(&path, ini).ok().map(|_| path)
}

/// Path to the VICE joystick mapping (.vjm) to load, or None to leave VICE on its
/// (mapping-less) default. C64_JOYMAP=<path> wins; otherwise, when a Sony DualSense
/// (PS5) pad or TheC64 joystick is connected, write and use the bundled map.
#[cfg(target_os = "linux")]
fn joymap_path() -> Option<std::path::PathBuf> {
    if let Some(p) = std::env::var_os("C64_JOYMAP").map(std::path::PathBuf::from) {
        return p.is_file().then_some(p);
    }
    if dualsense_present() {
        return write_dualsense_joymap();
    }
    if thec64_present() {
        return write_thec64_joymap();
    }
    None
}

/// Return a list of currently connected controller names for UI badges.
/// Only returns controllers that are actually present.
pub fn connected_controllers() -> Vec<String> {
    let mut controllers = Vec::new();
    
    #[cfg(target_os = "linux")]
    {
        // Check each controller type
        if thec64_present() {
            controllers.push("TheC64".to_string());
        }
        if dualsense_present() {
            controllers.push("PS5".to_string());
        }
        if switchpro_present() {
            controllers.push("Switch Pro".to_string());
        }
        // If no specific controllers found, check generic joystick
        if controllers.is_empty() && joystick_present() {
            controllers.push("Joystick".to_string());
        }
    }
    
    controllers
}

/// Lower-cased names of the connected input devices, gathered from udev's stable
/// by-id symlinks (which carry the model name over USB, e.g.
/// `usb-Sony_..._DualSense_...`) and from each evdev device's `name` (which carries
/// it over Bluetooth too, where by-id names don't), used to recognise specific
/// controllers by name.
#[cfg(target_os = "linux")]
fn input_device_names() -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir("/dev/input/by-id")
        .map(|rd| rd.flatten().map(|e| e.file_name().to_string_lossy().to_ascii_lowercase()).collect())
        .unwrap_or_default();
    if let Ok(rd) = std::fs::read_dir("/sys/class/input") {
        for e in rd.flatten() {
            if let Ok(n) = std::fs::read_to_string(e.path().join("name")) {
                names.push(n.trim().to_ascii_lowercase());
            }
        }
    }
    names
}

/// True if a Sony DualSense (PS5) controller is among the connected input devices.
/// The .vjm map below is keyed to this pad's exact axis/button layout, so we only
/// apply it when one is actually present — loading it for a different controller
/// would mis-map its inputs.
#[cfg(target_os = "linux")]
fn dualsense_present() -> bool {
    input_device_names().iter().any(|n| n.contains("dualsense"))
}

/// True if a TheC64 joystick is among the connected input devices.
/// TheC64 reports as e.g. "THEC64 Joystick" over USB.
#[cfg(target_os = "linux")]
fn thec64_present() -> bool {
    input_device_names().iter().any(|n| n.contains("thec64"))
}

/// True if a Nintendo Switch Pro controller is among the connected input devices.
#[cfg(target_os = "linux")]
fn switchpro_present() -> bool {
    let names = input_device_names();
    names.iter().any(|n| {
        n.contains("pro controller") || n.contains("switch pro") || n.contains("joy-con")
    })
}

/// Which game controller is plugged in, for the launch banner's label and icon. On
/// non-Linux hosts we can't read the device names, so everything is Generic.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ControllerKind {
    PlayStation,       // Sony DualSense (PS5)
    TheC64,            // Retro Games Ltd THEC64 / THEVIC20 joystick
    SwitchPro,        // Nintendo Switch Pro Controller
    Generic,
}

fn controller_kind() -> ControllerKind {
    #[cfg(target_os = "linux")]
    {
        let names = input_device_names();
        let has = |needle: &str| names.iter().any(|n| n.contains(needle));
        if has("dualsense") || has("playstation") {
            return ControllerKind::PlayStation;
        }
        // THEC64 / THEVIC20 joystick (Retro Games Ltd) reports e.g. "THEC64 Joystick".
        if has("thec64") {
            return ControllerKind::TheC64;
        }
        // Nintendo Switch Pro Controller reports as "Pro Controller" or similar.
        if has("pro controller") || has("switch pro") || has("joy-con") {
            return ControllerKind::SwitchPro;
        }
    }
    ControllerKind::Generic
}

/// A friendly name for the connected controller, for the controls banner: the
/// specific model when we recognise it, otherwise a plain "Joystick".
fn controller_name() -> String {
    match controller_kind() {
        ControllerKind::PlayStation => "PlayStation Controller",
        ControllerKind::TheC64 => "TheC64",
        ControllerKind::SwitchPro => "Switch Pro",
        ControllerKind::Generic => "Joystick",
    }
    .to_string()
}

/// A little controller icon for the controls report, themed by what's plugged in:
/// the THEC64 stick gets a sparkle-wreathed glowing halo; the DualSense gets a plain,
/// unglamorous pad. Plain Unicode so it renders in any label.
fn controller_icon() -> String {
    match controller_kind() {
        ControllerKind::TheC64 => "·✦· 🕹 ·✦·".to_string(),   // glowing halo
        ControllerKind::PlayStation => "🎮".to_string(),       // boring: plain, unadorned
        ControllerKind::SwitchPro => "🎮".to_string(),        // Nintendo controller icon
        ControllerKind::Generic => "🕹".to_string(),
    }
}

/// Write VICE's own DualSense mapping (data/joymaps/gtk3_joymap_ds5.vjm, embedded
/// verbatim) to the data dir and return its path. The native x64sc and the
/// net.sf.VICE flatpak are both GTK builds, so this gtk3 map fits either. The map
/// targets host joystick 0; pins are 1/2/4/8/16/32/64 = up/down/left/right/fires.
#[cfg(target_os = "linux")]
fn write_dualsense_joymap() -> Option<std::path::PathBuf> {
    const VJM: &str = "\
# VICE joystick mapping file
#
# A joystick map is read in as patch to the current map.
#
# File format:
# - comment lines start with '#'
# - keyword lines start with '!keyword'
# - normal line has 'joynum inputtype inputindex action'
#
# Keywords and their lines are:
# '!CLEAR'    clear all mappings
#
# inputtype:
# 0      axis
# 1      button
# 2      hat
#
# For buttons, inputindex is the zero-based index of the button.
# For hats: hat 0 has inputindex 0,1,2,3 respectively for up, down, left and
# right. Hat 1 has 5,6,7,8 etc.
# For axes, and action 1 (joystick) and 2 (keyboard): axis 0 has inputindex
# 0,1 respectively for positive and negative, axis 1 has 2,3 etc.
# For axes, and action 6 (pot axis): inputindex is the zero-based index of the
# axis.
#
# action [action_parameters]:
# 0                none
# 1 pin            joystick (pin: 1/2/4/8/16/32/64 = u/d/l/r/fire/fire2/fire3)
# 2 row col flags  keyboard (flags: 1=shift)
# 3                map
# 4                UI activate
# 5 action-name    UI function
# 6 pot            potentiometer (1=pot x, 2=pot y)
#

!CLEAR

# [054c:0ce6] \"Sony Interactive Entertainment DualSense Wireless Controller\" (8 axes, 13 buttons, 0 hats) Linux, GTK

0 0 0 1 8
0 0 1 1 4

0 0 2 1 2
0 0 3 1 1

0 0 4 0
0 0 5 0

0 0 3 6 1

0 0 4 6 2

0 0 10 0
0 0 11 0

0 0 12 1 8
0 0 13 1 4

0 0 14 1 2
0 0 15 1 1

0 1 0 1 16
0 1 1 1 64
0 1 2 1 128
0 1 3 1 32
0 1 4 1 256
0 1 5 1 512
0 1 6 0
0 1 7 0
0 1 8 1 1024
0 1 9 1 2048
0 1 10 5 pause-toggle
0 1 11 0
0 1 12 0
";
    let path = crate::core::data_path("vice-joymap-ds5.vjm");
    std::fs::write(&path, VJM).ok().map(|_| path)
}

/// Write VICE's TheC64 joystick mapping. TheC64 joystick (Retro Games Ltd)
/// enumerates as a HID keyboard device, but VICE can map its inputs to a
/// virtual joystick port. This .vjm maps TheC64's directional pad and fire
/// button to a single C64 joystick for host joystick 0 (VICE port = C64 port 2).
#[cfg(target_os = "linux")]
fn write_thec64_joymap() -> Option<std::path::PathBuf> {
    // TheC64 joystick typically reports as:
    //   button 0: up, button 1: down, button 2: left, button 3: right, button 4: fire
    // VICE C64 joystick pin mapping: 1=up, 2=down, 4=left, 8=right, 16=fire
    const VJM: &str = "\
# VICE joystick mapping file for TheC64 (Retro Games Ltd) joystick
#
# TheC64 enumerates as a HID keyboard device on Linux, so we explicitly
# map its button inputs to standard C64 joystick pins for host joystick 0.
#
# VJM format: joynum inputtype inputindex action [action_params]
#   inputtype: 0=axis, 1=button, 2=hat
#   action 1 pin: joystick (pin: 1/2/4/8/16 = up/down/left/right/fire)

!CLEAR

# TheC64 Joystick -> C64 Port 2 (VICE host joystick 0)
0 1 0 1 8    # Button 0 (Up)    -> pin 1 (up)
0 1 1 1 4    # Button 1 (Down)  -> pin 2 (down)
0 1 2 1 2    # Button 2 (Left)  -> pin 4 (left)
0 1 3 1 1    # Button 3 (Right) -> pin 8 (right)
0 1 4 1 16   # Button 4 (Fire)  -> pin 16 (fire)

# Disable additional buttons (TheC64 Maxi has extra buttons we ignore)
0 1 5 0
0 1 6 0
0 1 7 0
0 1 8 0
0 1 9 0
0 1 10 0
0 1 11 0
0 1 12 0
";
    let path = crate::core::data_path("vice-joymap-thec64.vjm");
    std::fs::write(&path, VJM).ok().map(|_| path)
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

/// Options for launching a game/demo in VICE. Carries the C64 Forever install so the
/// licensed ROMs are always used (see crate::roms).
#[derive(Clone)]
pub struct LaunchOpts {
    pub warp: bool,
    pub fullscreen: bool,
    pub keyboard: bool,
    pub drive_sound: Option<bool>,
    pub forever: crate::roms::Forever,
}

impl Default for LaunchOpts {
    fn default() -> Self {
        LaunchOpts {
            warp: true,
            fullscreen: true,
            keyboard: false,
            drive_sound: None,
            forever: crate::roms::Forever::RomDir(PathBuf::new()),
        }
    }
}

/// A native VICE command for the RomDir launch path (no Cloanto wine bundle).
fn native_vice_cmd() -> Vec<String> {
    for exe in ["x64sc", "x64"] {
        if which::which(exe).is_ok() {
            return vec![exe.to_string()];
        }
    }
    if which::which("flatpak").is_ok() && flatpak_installed("net.sf.VICE") {
        return ["flatpak", "run", "--command=x64sc", "net.sf.VICE"].map(String::from).to_vec();
    }
    vec!["x64sc".to_string()]
}

/// Point native VICE at extracted C64 Forever ROMs. UNVERIFIED filenames — confirm
/// against a real install before relying on this. Spellings probed from `-help`.
fn rom_args(dir: &Path, help: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (flag, file) in [("-kernal", "kernal"), ("-basic", "basic"), ("-chargen", "chargen")] {
        let p = dir.join(file);
        if help.contains(flag) && p.is_file() {
            out.push(flag.to_string());
            out.push(p.to_string_lossy().into_owned());
        }
    }
    out
}

/// Spawn VICE as a child process (NEVER exec) so the GUI stays alive while a game runs.
/// Returns the Child so the caller can watch for exit. The licensed ROMs come from the
/// user's C64 Forever install in `opts.forever`.
pub fn spawn(game: &Path, opts: &LaunchOpts) -> std::io::Result<std::process::Child> {
    let (emu, mut rom, wine_prefix): (Vec<String>, Vec<String>, Option<PathBuf>) =
        match &opts.forever {
            crate::roms::Forever::BundledVice { x64_exe, wine_prefix } => (
                vec!["wine".to_string(), x64_exe.to_string_lossy().into_owned()],
                Vec::new(),
                wine_prefix.clone(),
            ),
            crate::roms::Forever::RomDir(_) => (native_vice_cmd(), Vec::new(), None),
        };
    let help = emu_help(&emu);
    if let crate::roms::Forever::RomDir(dir) = &opts.forever {
        rom = rom_args(dir, &help);
    }

    // True Drive Emulation needs a 1541 ROM; a bare distro VICE without one fails
    // every LOAD with ?DEVICE NOT PRESENT, so fall back to the virtual drive there.
    // C64_VIRTUAL_DRIVE overrides (truthy forces virtual, 0/false/no forces TDE).
    let virtual_drive = match std::env::var("C64_VIRTUAL_DRIVE") {
        Ok(v) => !matches!(v.trim(), "" | "0" | "false" | "no"),
        Err(_) => missing_tde_rom(&emu),
    };
    let mut args: Vec<String> = drive_flags(&help, virtual_drive);
    args.extend(drive_sound_flags(&help, opts.drive_sound));
    if opts.warp {
        args.push("-autostart-warp".into());
    }
    if opts.fullscreen {
        if help.contains("-VICIIfull") {
            args.push("-VICIIfull".into());
        } else if help.contains("-fullscreen") {
            args.push("-fullscreen".into());
        }
    }
    if help.contains("-confirmexit") {
        args.push("+confirmexit".into()); // close window quits immediately, no dialog
    }
    let joystick = !opts.keyboard && joystick_present();
    args.extend(control_flags(joystick));
    args.extend(rom);

    // C64_VICE_LOG=<path> (or =1 for vice.log in the data dir) makes VICE write a
    // verbose log, so a game that won't start can be inspected.
    if let Some(spec) = std::env::var_os("C64_VICE_LOG") {
        let spec = spec.to_string_lossy();
        let path = if spec == "1" || spec.eq_ignore_ascii_case("true") {
            crate::core::data_path("vice.log").to_string_lossy().into_owned()
        } else {
            spec.into_owned()
        };
        args.extend(["-verbose".to_string(), "-logfile".to_string(), path]);
    }

    let mut cmd = Command::new(&emu[0]);
    cmd.args(&emu[1..]).args(&args).arg("-autostart").arg(game);
    // Force SDL's X11 backend on Linux so VICE doesn't land on a broken Wayland
    // renderer. Set on the child only: mutating our own env is unsound with GTK's threads.
    #[cfg(target_os = "linux")]
    if std::env::var_os("SDL_VIDEODRIVER").is_none() {
        cmd.env("SDL_VIDEODRIVER", "x11");
    }
    if let Some(pfx) = wine_prefix {
        cmd.env("WINEPREFIX", pfx).env("WINEDEBUG", "-all");
    }
    cmd.spawn()
}
