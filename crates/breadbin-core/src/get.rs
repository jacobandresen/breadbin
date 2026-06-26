// c64get - find and download C64 games from the Ultimate Tape Archive (UTA).
// One folder per game, each holding .tap image(s). Downloads the .tap files into
// the collection so `c64menu --refresh` picks them up. Port of c64get.

use std::collections::BTreeSet;
use std::io::Write;
use std::path::Path;
use std::process::ExitCode;
use std::sync::OnceLock;

use regex::Regex;

use crate::core;

const BASE: &str = "https://uta.pokefinder.org/Ultimate_Tape_Archive/";
const UA: &str = "c64get/1.0 (personal C64 collection tool; polite, low-volume)";
const TAP_MAGIC: &[u8] = b"C64-TAPE-RAW";

const HELP: &str = "\
c64get - find and download C64 games from the Ultimate Tape Archive (UTA).

Usage:
  c64get <title> [<title> ...]   search UTA for each title and download the .tap(s)
  c64get --missing               download missing ranked games found in UTA
  c64get -n / --list <title>     dry run: show what would be downloaded, don't fetch
  c64get --dest DIR              download directory (default: <collection>/_UTA_downloads)
  c64get --refresh-index         re-download the UTA directory listing first
";

fn idx_cache() -> std::path::PathBuf {
    core::data_path("uta_index.html")
}
fn dest_default() -> std::path::PathBuf {
    core::c64_lib().join("_UTA_downloads")
}

fn get_text(url: &str) -> Result<String, String> {
    Ok(String::from_utf8_lossy(&core::fetch(url, &[("User-Agent", UA)])?).into_owned())
}

/// (norm_title, display_name, href) for every UTA game folder.
fn load_index(refresh: bool) -> Vec<(String, String, String)> {
    if refresh || !idx_cache().exists() {
        eprintln!("Fetching UTA directory listing ...");
        if let Ok(html) = get_text(BASE) {
            let _ = std::fs::write(idx_cache(), html);
        }
    }
    let txt = std::fs::read_to_string(idx_cache()).unwrap_or_default();

    static FOLDER: OnceLock<Regex> = OnceLock::new();
    let folder = FOLDER.get_or_init(|| Regex::new(r#"href="([^"/]+%5[bB][0-9]+%5[dD])/?""#).unwrap());
    let folders: BTreeSet<String> =
        folder.captures_iter(&txt).map(|c| c[1].to_string()).collect();

    folders
        .into_iter()
        .map(|f| {
            let name = core::html_unescape(&core::unquote(&f));
            let spaced = name.replace('_', " ");
            let title = core::split_before(&spaced, r"\s*\((?:\d{4}|19xx)");
            (core::norm(title), name, f)
        })
        .collect()
}

fn find_exact<'a>(fmap: &'a [(String, String, String)], key: &str) -> Vec<(&'a str, &'a str)> {
    fmap.iter().filter(|(nk, _, _)| nk == key).map(|(_, d, h)| (d.as_str(), h.as_str())).collect()
}

fn find_substr<'a>(fmap: &'a [(String, String, String)], q: &str) -> Vec<(&'a str, &'a str)> {
    let qn = core::norm(q);
    if qn.is_empty() {
        return vec![];
    }
    fmap.iter().filter(|(nk, _, _)| nk.contains(&qn)).map(|(_, d, h)| (d.as_str(), h.as_str())).collect()
}

/// fzf multi-select. None = non-interactive (caller lists instead).
fn pick_with_fzf<'a>(matches: &[(&'a str, &'a str)]) -> Option<Vec<(&'a str, &'a str)>> {
    use std::io::IsTerminal;
    if !std::io::stdin().is_terminal() || !core::command_exists("fzf") {
        return None;
    }
    let input = matches.iter().map(|(d, _)| *d).collect::<Vec<_>>().join("\n");
    let mut child = std::process::Command::new("fzf")
        .args(["--multi", "--reverse", "--prompt", "UTA pick (TAB=multi) > "])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .ok()?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(input.as_bytes());
    }
    let out = child.wait_with_output().ok()?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    let chosen: BTreeSet<&str> = stdout.lines().filter(|l| !l.is_empty()).collect();
    // map chosen display lines back to (display, href)
    Some(matches.iter().filter(|(d, _)| chosen.contains(*d)).cloned().collect())
}

/// Download every .tap in a UTA game folder. Returns saved paths.
fn download_folder(href: &str, dest: &Path, dry: bool) -> Vec<std::path::PathBuf> {
    let folder_url = format!("{BASE}{href}/");
    let page = match get_text(&folder_url) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("    !! folder error: {e}");
            return vec![];
        }
    };
    static TAP: OnceLock<Regex> = OnceLock::new();
    let tap = TAP.get_or_init(|| Regex::new(r#"(?i)href="([^"/?]+\.tap)""#).unwrap());
    let taps: BTreeSet<String> = tap.captures_iter(&page).map(|c| c[1].to_string()).collect();
    if taps.is_empty() {
        eprintln!("    (no .tap in folder - skipped)");
        return vec![];
    }
    let mut saved = Vec::new();
    for t in taps {
        let url = format!("{folder_url}{t}");
        let name = core::html_unescape(&core::unquote(&t));
        let out = dest.join(&name);
        if dry {
            eprintln!("    would download: {name}");
            continue;
        }
        if out.metadata().map(|m| m.len() > 0).unwrap_or(false) {
            eprintln!("    exists, skip: {name}");
            saved.push(out);
            continue;
        }
        let data = match core::fetch(&url, &[("User-Agent", UA)]) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("    !! download error: {e}");
                continue;
            }
        };
        if !data.starts_with(TAP_MAGIC) {
            eprintln!("    !! not a TAP (got {}B, bad signature) - skipped: {name}", data.len());
            continue;
        }
        let _ = std::fs::create_dir_all(dest);
        if std::fs::write(&out, &data).is_ok() {
            eprintln!("    saved: {name} ({} KB)", data.len() / 1024);
            saved.push(out);
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    }
    saved
}

fn missing_titles() -> Vec<String> {
    let mut out = Vec::new();
    if let Ok(text) = std::fs::read_to_string(core::data_path("c64_index.tsv")) {
        for line in text.lines() {
            let f: Vec<&str> = line.split('\t').collect();
            if f.len() >= 5 && f[1] == "missing" {
                out.push(f[4].to_string());
            }
        }
    }
    out
}

pub fn main(argv: Vec<String>) -> ExitCode {
    let (mut dry, mut refresh, mut use_missing) = (false, false, false);
    let mut dest = dest_default();
    let mut queries: Vec<String> = Vec::new();

    let mut i = 0;
    while i < argv.len() {
        match argv[i].as_str() {
            "-n" | "--list" => dry = true,
            "--missing" => use_missing = true,
            "--refresh-index" => refresh = true,
            "--dest" => {
                i += 1;
                if let Some(d) = argv.get(i) {
                    dest = std::path::PathBuf::from(d);
                }
            }
            "-h" | "--help" => {
                print!("{HELP}");
                return ExitCode::SUCCESS;
            }
            s if s.starts_with('-') => {
                eprintln!("unknown option: {s}");
                return ExitCode::from(2);
            }
            s => queries.push(s.to_string()),
        }
        i += 1;
    }

    if !use_missing && queries.is_empty() {
        eprintln!("nothing to do: give a title, or --missing (see --help)");
        return ExitCode::from(2);
    }

    let fmap = load_index(refresh);
    eprintln!("UTA index: {} games\n", fmap.len());

    // build the target folder set (display -> href), insertion order via Vec+seen
    let mut targets: Vec<(String, String)> = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let add = |d: &str, h: &str, targets: &mut Vec<(String, String)>, seen: &mut std::collections::HashSet<String>| {
        if seen.insert(d.to_string()) {
            targets.push((d.to_string(), h.to_string()));
        }
    };

    if use_missing {
        let before = targets.len();
        for t in missing_titles() {
            for (d, h) in find_exact(&fmap, &core::title_key(&t)) {
                add(d, h, &mut targets, &mut seen);
            }
        }
        eprintln!("missing top-100 found in UTA: {}", targets.len() - before);
    }
    for q in &queries {
        let mut m = find_exact(&fmap, &core::norm(q));
        if m.is_empty() {
            m = find_substr(&fmap, q);
        }
        if m.is_empty() {
            eprintln!("\"{q}\": no match in UTA");
            continue;
        }
        if m.len() == 1 {
            add(m[0].0, m[0].1, &mut targets, &mut seen);
        } else {
            match pick_with_fzf(&m) {
                None => {
                    eprintln!("\"{q}\": {} matches - narrow it or run on a terminal:", m.len());
                    for (d, _) in m.iter().take(20) {
                        eprintln!("    {d}");
                    }
                }
                Some(chosen) => {
                    for (d, h) in chosen {
                        add(d, h, &mut targets, &mut seen);
                    }
                }
            }
        }
    }

    if targets.is_empty() {
        eprintln!("\nnothing matched to download.");
        return ExitCode::from(1);
    }

    targets.sort();
    eprintln!("\n{}processing {} game(s) -> {}\n", if dry { "(dry run) " } else { "" }, targets.len(), dest.display());
    let mut total = 0;
    for (d, h) in &targets {
        eprintln!("* {d}");
        total += download_folder(h, &dest, dry).len();
    }
    eprintln!("\n{} {total} .tap file(s).", if dry { "would download" } else { "downloaded" });
    if total > 0 && !dry {
        eprintln!("run  c64menu --refresh  to add them to the picker.");
    }
    ExitCode::SUCCESS
}
