// breadbin - Commodore 64 game library: find · fetch · launch.
//
// A single multi-call binary. It behaves as the `breadbin` umbrella when invoked
// by that name, and as the individual tool (c64run, c64menu, …) when invoked
// through a symlink of that name — busybox-style argv[0] dispatch. The umbrella's
// first argument selects the subcommand (default: kiosk).

mod build_index;
mod core;
mod cover;
mod disk;
mod get;
mod info;
mod run;
mod tosec;
mod tui;
mod menu;
mod kiosk;
mod demos;
mod sid;
mod tunes;

use std::process::ExitCode;

const USAGE: &str = "\
breadbin - Commodore 64 game library (find · fetch · launch)

  breadbin kiosk             grid of game cards (cover + details); play/fetch on Enter (default)
  breadbin menu              browse ranked games; play owned, fetch the rest
  breadbin play  <name>      launch a game straight into the emulator (LOAD\"*\",8,1 : RUN)
  breadbin info  <name>      show GameBase64 details for a game (year, genre, author, ...)
  breadbin get   <name>      download a tape from the Ultimate Tape Archive
  breadbin disk  <name>      download a disk from the Internet Archive / TOSEC
  breadbin tosec             browse the whole TOSEC catalogue; download + play on pick
  breadbin demos             browse the best C64 demoscene demos by party; run on pick
  breadbin tunes             play the best C64 SID music by composer; visualiser on play
  breadbin install-links     create the c64run/c64menu/... symlinks next to this binary

On first run breadbin downloads and builds everything it needs (game catalogue,
GameBase64 details, cover art) into its data directory ($BREADBIN_USER_DATA, or
~/.breadbin by default). No bundled data files are required.

Add --help to any subcommand for its own options, e.g.  breadbin disk --help
";

/// Map an umbrella subcommand alias to a canonical tool name.
fn resolve_subcommand(cmd: &str) -> Option<&'static str> {
    Some(match cmd {
        "menu" | "pick" | "browse" => "c64menu",
        "kiosk" | "grid" | "cards" => "c64kiosk",
        "play" | "run" | "launch" => "c64run",
        "info" | "details" => "c64info",
        "get" | "tape" => "c64get",
        "disk" => "c64disk",
        "tosec" | "browse-all" => "c64tosec",
        "demos" | "demoscene" | "scene" => "c64demos",
        "tunes" | "music" | "sid" | "jukebox" => "c64tunes",
        "index" | "build-index" => "build_index",
        _ => return None,
    })
}

/// Dispatch to a tool by its canonical name with the given args.
fn run_tool(tool: &str, args: Vec<String>) -> ExitCode {
    match tool {
        "c64run" => run::main(args),
        "c64info" => info::main(args),
        "c64disk" => disk::main(args),
        "c64get" => get::main(args),
        "c64tosec" => tosec::main(args),
        "build_index" => build_index::main(args),
        "c64menu" => menu::main(args),
        "c64kiosk" => kiosk::main(args),
        "c64demos" => demos::main(args),
        "c64tunes" => tunes::main(args),
        _ => {
            eprintln!("breadbin: no such tool: {tool}");
            ExitCode::from(2)
        }
    }
}

/// The standalone tool names that resolve to this binary via argv[0] dispatch.
const TOOL_NAMES: &[&str] = &[
    "c64run", "c64menu", "c64kiosk", "c64info", "c64get", "c64disk", "c64tosec", "c64demos",
    "c64tunes",
];

/// Create the standalone `c64*` symlinks next to the installed binary (or in the
/// directory given as an argument), so `c64run`, `c64menu`, … resolve to this
/// multi-call binary. Run once after `cargo install breadbin`.
fn install_links(dir_arg: Option<&str>) -> ExitCode {
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("breadbin: cannot find own path: {e}");
            return ExitCode::from(1);
        }
    };
    let dir = match dir_arg {
        Some(d) => std::path::PathBuf::from(d),
        None => exe.parent().map(|p| p.to_path_buf()).unwrap_or_default(),
    };
    let mut failed = false;
    for name in TOOL_NAMES {
        let link = dir.join(name);
        let _ = std::fs::remove_file(&link); // replace a stale link
        match std::os::unix::fs::symlink(&exe, &link) {
            Ok(()) => println!("linked {} -> {}", link.display(), exe.display()),
            Err(e) => {
                eprintln!("breadbin: could not link {}: {e}", link.display());
                failed = true;
            }
        }
    }
    if failed {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

fn main() -> ExitCode {
    // Ensure user data directory exists before any operation
    core::ensure_user_data_dir();
    let mut argv = std::env::args();
    let arg0 = argv.next().unwrap_or_default();
    let args: Vec<String> = argv.collect();

    // argv[0] basename: a c64* name means we were invoked as that standalone tool.
    let prog = std::path::Path::new(&arg0)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();

    if prog.starts_with("c64") {
        return run_tool(&prog, args);
    }

    // hidden dev aid: `breadbin _norm` reads lines from stdin, prints norm() each.
    if args.first().map(String::as_str) == Some("_norm") {
        use std::io::BufRead;
        let stdin = std::io::stdin();
        for line in stdin.lock().lines().map_while(Result::ok) {
            println!("{}", core::norm(&line));
        }
        return ExitCode::SUCCESS;
    }

    // hidden dev aid: exercise the cover module.
    //   _cover index-count        -> number of entries in covers_index.tsv
    //   _cover ensure <canon>     -> cached cover path (no fetch if present)
    //   _cover ia <identifier>    -> IA item-image cover path
    if args.first().map(String::as_str) == Some("_cover") {
        match args.get(1).map(String::as_str) {
            Some("index-count") => println!("{}", cover::load_index().len()),
            Some("ensure") => {
                let idx = cover::load_index();
                let canon = args.get(2).cloned().unwrap_or_default();
                match cover::ensure_cover(&canon, &idx) {
                    Some(p) => println!("{}", p.display()),
                    None => println!("(none)"),
                }
            }
            Some("ia") => match cover::ia_cover(args.get(2).map(String::as_str).unwrap_or("")) {
                Some(p) => println!("{}", p.display()),
                None => println!("(none)"),
            },
            _ => eprintln!("usage: _cover index-count | ensure <canon> | ia <ident>"),
        }
        return ExitCode::SUCCESS;
    }

    // hidden dev aid: exercise c64disk's pure matching helpers for parity tests.
    if args.first().map(String::as_str) == Some("_disk") {
        match args.get(1).map(String::as_str) {
            Some("title") => println!("{}", disk::game_title(args.get(2).map(String::as_str).unwrap_or(""))),
            Some("ratio") => println!(
                "{:.6}",
                core::seq_ratio(args.get(2).map(String::as_str).unwrap_or(""), args.get(3).map(String::as_str).unwrap_or(""))
            ),
            _ => eprintln!("usage: _disk title <s> | ratio <a> <b>"),
        }
        return ExitCode::SUCCESS;
    }

    // hidden dev aid: render a .sid headlessly to validate the SID engine.
    //   _sid <file.sid> [song]  -> render ~2s and report peak/rms/non-silence
    if args.first().map(String::as_str) == Some("_sid") {
        let path = args.get(1).cloned().unwrap_or_default();
        let song: u16 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1);
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("_sid: cannot read {path}: {e}");
                return ExitCode::from(1);
            }
        };
        match sid::Player::new(&bytes, song, 44100) {
            Ok(mut p) => {
                println!("name={:?} author={:?} songs={}", p.name, p.author, p.songs);
                let mut buf = vec![0i16; 44100 * 2];
                let mut peak = 0i32;
                let mut sumsq = 0f64;
                let mut nonzero = 0usize;
                for _ in 0..2 {
                    let vis = p.render(&mut buf);
                    for &s in &buf {
                        let a = (s as i32).abs();
                        if a > peak { peak = a; }
                        if s != 0 { nonzero += 1; }
                        sumsq += (s as f64) * (s as f64);
                    }
                    let f: Vec<String> = (0..3).map(|v| format!("{:.3}", vis.voice_freq(v))).collect();
                    println!("frame={} vol={} voiceFreq=[{}]", vis.frame, vis.volume(), f.join(", "));
                }
                let rms = (sumsq / (buf.len() as f64 * 2.0)).sqrt();
                println!("peak={peak} rms={rms:.1} nonzero={nonzero}/{}", buf.len() * 2);
                return ExitCode::SUCCESS;
            }
            Err(e) => {
                eprintln!("_sid: {e}");
                return ExitCode::from(1);
            }
        }
    }

    // Otherwise we're the `breadbin` umbrella: first arg selects the subcommand.
    let cmd = args.first().map(String::as_str).unwrap_or("kiosk");
    if matches!(cmd, "-h" | "--help" | "help") {
        print!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    if cmd == "install-links" {
        return install_links(args.get(1).map(String::as_str));
    }
    match resolve_subcommand(cmd) {
        Some(tool) => run_tool(tool, args.get(1..).unwrap_or_default().to_vec()),
        None => {
            eprintln!("breadbin: unknown command \"{cmd}\" (try: breadbin --help)");
            ExitCode::from(2)
        }
    }
}
