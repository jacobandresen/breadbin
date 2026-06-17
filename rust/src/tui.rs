// tui - shared data model and actions for the two terminal UIs (c64menu and
// c64kiosk). In the Python toolkit these helpers lived in c64menu and were
// imported into c64kiosk via importlib; here they collapse into one module:
// loading the ranked index, grouping by genre, resolving covers, recording and
// replaying the play history, downloading-on-demand, and launching games.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::process::{Command, Stdio};

use crate::{core, cover, disk, run};

/// Genre bucket for rows that carry no genre.
pub const GENRE_OTHER: &str = "Other";
/// Synthetic top section in the kiosk: the most recently played games.
pub const LATEST_GENRE: &str = "latest played";

/// One ranked game from c64_index.tsv:
/// display, status, target, title, query, identifier, genre, downloads.
#[derive(Clone)]
pub struct Row {
    pub display: String,
    pub status: String,
    pub target: String,
    pub title: String,
    pub query: String,
    pub ident: String,
    pub genre: String,
    /// Internet Archive download count — popularity, used to order the kiosk.
    /// 0 when absent (an index built before this column existed).
    pub downloads: i64,
}

impl Row {
    pub fn is_local(&self) -> bool {
        self.status == "local"
    }
    pub fn genre_or_other(&self) -> &str {
        if self.genre.is_empty() {
            GENRE_OTHER
        } else {
            &self.genre
        }
    }
}

pub fn index_path() -> PathBuf {
    core::data_path("c64_index.tsv")
}
pub fn played_path() -> PathBuf {
    core::data_path("played.tsv")
}

/// Load every ranked game from c64_index.tsv (rows with < 5 fields are skipped,
/// mirroring c64menu.load_rows).
pub fn load_rows() -> Vec<Row> {
    let mut rows = Vec::new();
    let Ok(text) = std::fs::read_to_string(index_path()) else {
        return rows;
    };
    for line in text.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() >= 5 {
            rows.push(Row {
                display: f[0].to_string(),
                status: f[1].to_string(),
                target: f[2].to_string(),
                title: f[3].to_string(),
                query: f[4].to_string(),
                ident: f.get(5).copied().unwrap_or("").to_string(),
                genre: f.get(6).copied().unwrap_or("").to_string(),
                downloads: f.get(7).and_then(|s| s.parse().ok()).unwrap_or(0),
            });
        }
    }
    rows
}

/// Canonical cover key for a row, matching the catalogue's canon so the libretro
/// boxart lookup lines up (c64disk.game_title applied to the row's query).
pub fn canon_of(row: &Row) -> String {
    disk::game_title(&row.query)
}

/// Cover image path for a row: libretro boxart first, else the Internet Archive
/// item image. Both are cached on disk; this may fetch on first use.
pub fn cover_for(row: &Row, cidx: &HashMap<String, String>) -> Option<PathBuf> {
    if let Some(p) = cover::ensure_cover(&canon_of(row), cidx) {
        return Some(p);
    }
    cover::ia_cover(&row.ident)
}

/// Group rows by genre, preserving popularity order; genres appear in first-seen
/// order. Returns (genre, indices-into-rows).
pub fn group_by_genre(rows: &[Row]) -> Vec<(String, Vec<usize>)> {
    let mut order: Vec<String> = Vec::new();
    let mut map: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, r) in rows.iter().enumerate() {
        let g = r.genre_or_other().to_string();
        if !map.contains_key(&g) {
            order.push(g.clone());
        }
        map.entry(g).or_default().push(i);
    }
    order
        .into_iter()
        .map(|g| {
            let v = map.remove(&g).unwrap();
            (g, v)
        })
        .collect()
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Append a play event (timestamp<TAB>display) so the kiosk's "latest played"
/// row can surface it. Keyed by the row's display string.
pub fn record_play(row: &Row) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(played_path())
    {
        let _ = writeln!(f, "{}\t{}", now_secs(), row.display);
    }
}

/// Display keys of recently-played games, newest first, de-duplicated.
pub fn recent_plays(limit: Option<usize>) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(played_path()) else {
        return Vec::new();
    };
    let mut seen: HashSet<String> = HashSet::new();
    let mut order: Vec<String> = Vec::new();
    for line in text.lines().rev() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 2 || seen.contains(parts[1]) {
            continue;
        }
        seen.insert(parts[1].to_string());
        order.push(parts[1].to_string());
        if let Some(l) = limit {
            if order.len() >= l {
                break;
            }
        }
    }
    order
}

/// Re-pull the ranked IA catalogue, then rebuild c64_index.tsv. Spawns the same
/// binary as `disk --ia-index` and `index`, mirroring c64menu.refresh(). This is
/// also the first-run bootstrap that fills a fresh user data directory.
pub fn refresh() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let st = Command::new(&exe)
        .arg("disk")
        .arg("--ia-index")
        .status()
        .map_err(|e| e.to_string())?;
    if !st.success() {
        return Err("could not build the IA catalogue".to_string());
    }
    Command::new(&exe).arg("index").status().map_err(|e| e.to_string())?;
    Ok(())
}

/// Resolve a local disk path for a row, downloading from the Internet Archive if
/// needed. With `quiet`, child output is suppressed (the kiosk owns the screen).
/// Returns None if the download produced no matching disk.
pub fn resolve(row: &Row, quiet: bool) -> Option<PathBuf> {
    if row.is_local() {
        return Some(PathBuf::from(&row.target));
    }
    let exe = std::env::current_exe().ok()?;
    let mut path = String::new();
    if !row.ident.is_empty() {
        // exact IA item: c64disk prints the boot path on stdout.
        let mut cmd = Command::new(&exe);
        cmd.arg("disk").arg("--id").arg(&row.ident).stdout(Stdio::piped());
        if quiet {
            cmd.stderr(Stdio::null());
        }
        if let Ok(out) = cmd.output() {
            let s = String::from_utf8_lossy(&out.stdout);
            path = s
                .lines()
                .filter(|l| !l.trim().is_empty())
                .last()
                .unwrap_or("")
                .to_string();
        }
    } else {
        let mut cmd = Command::new(&exe);
        cmd.arg("disk").arg("--source").arg("ia").arg(&row.query);
        if quiet {
            cmd.stdout(Stdio::null()).stderr(Stdio::null());
        }
        let _ = cmd.status();
    }
    // refresh local/available state
    let mut bcmd = Command::new(&exe);
    bcmd.arg("index");
    if quiet {
        bcmd.stdout(Stdio::null()).stderr(Stdio::null());
    } else {
        bcmd.stdout(Stdio::null());
    }
    let _ = bcmd.status();

    let p = PathBuf::from(&path);
    if !path.is_empty() && p.exists() {
        return Some(p);
    }
    // fallback: match by title against the rebuilt index
    for f in load_rows() {
        if f.title == row.title && f.is_local() {
            return Some(PathBuf::from(f.target));
        }
    }
    None
}

/// Launch a game in-place: spawn the emulator and wait for it to exit, keeping
/// the caller on its alternate screen (used by the kiosk). C64_QUIET suppresses
/// c64run's LOAD"*",8,1 banner. Returns Ok(()) when the emulator ran cleanly, or
/// Err(message) carrying the emulator's own diagnostics (e.g. "no VICE found" or
/// "?DEVICE NOT PRESENT") so the caller can show *why* a game wouldn't start
/// rather than silently bouncing back to the grid. stderr is captured for that;
/// stdout (the emulator's own chatter) is discarded.
pub fn launch_inplace(target: &str, runopts: &[String]) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut args: Vec<String> = vec!["play".to_string()];
    args.extend(runopts.iter().cloned());
    args.push(target.to_string());
    let out = Command::new(&exe)
        .args(&args)
        .env("C64_QUIET", "1")
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        return Ok(());
    }
    // Surface the tail of stderr (the actual error). VICE often prints the offending
    // option on one line and "error parsing command line option" on the next, so keep
    // the last few non-empty lines rather than just one. Fall back to the exit status
    // when the emulator said nothing.
    let err = String::from_utf8_lossy(&out.stderr);
    let mut tail: Vec<&str> = err.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let n = tail.len();
    tail = tail.split_off(n.saturating_sub(3));
    if tail.is_empty() {
        Err(format!("emulator exited with {}", out.status))
    } else {
        Err(tail.join("\n"))
    }
}

/// Boot a game by replacing this process with the emulator (used by c64menu once
/// the picker has exited). Records the play first. Only returns on failure.
pub fn play_exec(target: &str, runopts: &[String], row: Option<&Row>) -> std::process::ExitCode {
    if let Some(r) = row {
        record_play(r);
    }
    let mut args: Vec<String> = runopts.to_vec();
    args.push(target.to_string());
    run::main(args) // execs VICE; only returns on the list-only path / error
}
