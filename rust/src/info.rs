// c64info - GameBase64 (GB64) details for a C64 game, from a local SQLite copy
// (~17MB, ~30k games). The DB is downloaded + cached once, then offline. Port of
// c64info; search()/best_match()/record() are also used by the menu's detail card.

use std::io::Read;
use std::process::ExitCode;

use regex::Regex;
use rusqlite::{Connection, OpenFlags};

use crate::core;

const DB_URL: &str = "https://www.twinbirds.com/gamebase64browser/GBC_v19.sqlitedb.gz";
const UA: &str = "breadbin-c64info/1.0 (personal C64 collection tool; polite, low-volume)";

const HELP: &str = "\
c64info - show GameBase64 (GB64) details for a C64 game.

Usage:
  c64info <title words>          print GB64 details for the best match
  c64info -a / --all <title>     list every matching game (don't show details)
  c64info --refresh-db           re-download the GB64 SQLite database
";

const DETAIL_SQL: &str = "
SELECT g.Name, y.Year, pu.Publisher, de.Developer,
       pg.ParentGenre, ge.Genre, pr.Programmer, mu.Musician,
       la.Language, g.PlayersFrom, g.PlayersTo, g.PlayersSim,
       g.Rating, g.Classic, g.Comment, g.MemoText,
       g.WebLink_Name, g.WebLink_URL, g.Filename
FROM Games g
LEFT JOIN Years y        ON y.YE_Id  = g.YE_Id
LEFT JOIN Publishers pu  ON pu.PU_Id = g.PU_Id
LEFT JOIN Developers de  ON de.DE_Id = g.DE_Id
LEFT JOIN Genres ge      ON ge.GE_Id = g.GE_Id
LEFT JOIN PGenres pg     ON pg.PG_Id = ge.PG_Id
LEFT JOIN Programmers pr ON pr.PR_Id = g.PR_Id
LEFT JOIN Musicians mu   ON mu.MU_Id = g.MU_Id
LEFT JOIN Languages la   ON la.LA_Id = g.LA_Id
WHERE g.GA_Id = ?;
";

fn db_path() -> std::path::PathBuf {
    core::data_path("gb64.sqlitedb")
}

fn die(msg: &str) -> ! {
    eprintln!("c64info: {msg}");
    std::process::exit(1);
}

/// Download + cache the GB64 SQLite DB (gzip) if missing.
fn ensure_db(refresh: bool) -> Result<(), String> {
    let db = db_path();
    if refresh {
        let _ = std::fs::remove_file(&db);
    }
    if std::fs::metadata(&db).map(|m| m.len() > 0).unwrap_or(false) {
        return Ok(());
    }
    eprintln!("downloading GameBase64 SQLite database (~6MB) ...");
    let data = core::fetch(DB_URL, &[("User-Agent", UA)])?;
    // The .gz may arrive still gzipped, or already decompressed: ureq transparently
    // decodes Content-Encoding: gzip/x-gzip (the server tags the file that way), so
    // detect the real payload by its magic bytes rather than always gunzipping.
    let blob = if data.starts_with(&[0x1f, 0x8b]) {
        let mut out = Vec::new();
        flate2::read::GzDecoder::new(&data[..])
            .read_to_end(&mut out)
            .map_err(|e| e.to_string())?;
        out
    } else if data.starts_with(b"SQLite format 3\0") {
        data
    } else {
        return Err(format!(
            "unexpected response from {DB_URL} ({} bytes, not gzip or a SQLite database)",
            data.len()
        ));
    };
    std::fs::write(&db, &blob).map_err(|e| e.to_string())?;
    eprintln!("  cached {}MB -> {}", blob.len() / (1024 * 1024), db.display());
    Ok(())
}

/// Connect read-only to the (lazily-downloaded) GB64 DB. Used by c64disk too.
pub fn connect() -> Connection {
    open_db(false)
}

/// Decode a SQLite TEXT/INTEGER column leniently (UTF-8 then Windows-1252).
/// Public so c64disk can read GB64 names that may be cp1252.
pub fn decode_text(v: rusqlite::types::ValueRef) -> Option<String> {
    text(v)
}

/// Connect to the GB64 DB. (Decoding is handled per-column in `text()`.)
fn open_db(refresh: bool) -> Connection {
    if let Err(e) = ensure_db(refresh) {
        die(&format!("could not obtain DB: {e}"));
    }
    Connection::open_with_flags(db_path(), OpenFlags::SQLITE_OPEN_READ_ONLY)
        .unwrap_or_else(|e| die(&format!("could not open DB: {e}")))
}

/// Decode a TEXT/INTEGER column leniently: UTF-8 if valid, else Windows-1252
/// (GB64 text comes from a .mdb origin), integers/reals stringified.
fn text(v: rusqlite::types::ValueRef) -> Option<String> {
    use rusqlite::types::ValueRef::*;
    match v {
        Null => None,
        Integer(i) => Some(i.to_string()),
        Real(r) => Some(r.to_string()),
        Text(b) | Blob(b) => Some(match std::str::from_utf8(b) {
            Ok(s) => s.to_string(),
            Err(_) => encoding_rs::WINDOWS_1252.decode(b).0.into_owned(),
        }),
    }
}

fn int(v: rusqlite::types::ValueRef) -> Option<i64> {
    use rusqlite::types::ValueRef::*;
    match v {
        Integer(i) => Some(i),
        Text(b) => std::str::from_utf8(b).ok().and_then(|s| s.trim().parse().ok()),
        _ => None,
    }
}

/// (id, name) pairs. Returns (all matches best-first, the canonical-exact subset).
fn search(con: &Connection, query: &str) -> (Vec<(i64, String)>, Vec<(i64, String)>) {
    let qn = core::norm(query);

    // primary: substring LIKE; fallback: word-by-word AND
    let mut rows: Vec<(i64, String, i64)> = query_like(con, &[format!("%{query}%")]);
    if rows.is_empty() {
        let words: Vec<String> = Regex::new(r"[A-Za-z0-9]+")
            .unwrap()
            .find_iter(query)
            .map(|m| m.as_str().to_string())
            .filter(|w| w.len() > 1)
            .collect();
        if !words.is_empty() {
            let pats: Vec<String> = words.iter().map(|w| format!("%{w}%")).collect();
            rows = query_like(con, &pats);
        }
    }

    let rank = |r: &(i64, String, i64)| -r.2; // higher GB64 rating first
    let is_exact =
        |r: &(i64, String, i64)| core::norm(core::split_before(&r.1, r"[\(\[]")) == qn;

    let mut exact: Vec<(i64, String, i64)> = rows.iter().filter(|r| is_exact(r)).cloned().collect();
    exact.sort_by_key(rank); // stable: preserves DB order within equal rating
    let exact_ids: std::collections::HashSet<i64> = exact.iter().map(|r| r.0).collect();
    let mut rest: Vec<(i64, String, i64)> =
        rows.into_iter().filter(|r| !exact_ids.contains(&r.0)).collect();
    rest.sort_by(|a, b| rank(a).cmp(&rank(b)).then_with(|| a.1.cmp(&b.1)));

    let pairs = |rs: &[(i64, String, i64)]| rs.iter().map(|r| (r.0, r.1.clone())).collect::<Vec<_>>();
    let exact_pairs = pairs(&exact);
    let mut all = exact;
    all.extend(rest);
    (pairs(&all), exact_pairs)
}

/// Run "SELECT GA_Id,Name,Rating FROM Games WHERE Name LIKE ? AND ... COLLATE NOCASE".
fn query_like(con: &Connection, patterns: &[String]) -> Vec<(i64, String, i64)> {
    let clause = vec!["Name LIKE ?"; patterns.len()].join(" AND ");
    let sql = format!("SELECT GA_Id, Name, Rating FROM Games WHERE {clause} COLLATE NOCASE");
    let mut stmt = match con.prepare(&sql) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let params = rusqlite::params_from_iter(patterns.iter());
    let mapped = stmt.query_map(params, |row| {
        let id: i64 = row.get(0)?;
        let name = text(row.get_ref(1)?).unwrap_or_default();
        let rating = int(row.get_ref(2)?).unwrap_or(0);
        Ok((id, name, rating))
    });
    match mapped {
        Ok(it) => it.filter_map(Result::ok).collect(),
        Err(_) => Vec::new(),
    }
}

/// Resolve multiple matches. A single match is returned; otherwise (interactive
/// fzf picking is not ported) we list the candidates and exit, like Python's
/// non-fzf path.
fn pick(matches: &[(i64, String)]) -> i64 {
    if matches.len() == 1 {
        return matches[0].0;
    }
    eprintln!("{} matches - narrow it, or use --all:", matches.len());
    for (_, n) in matches.iter().take(25) {
        eprintln!("  {n}");
    }
    std::process::exit(1);
}

/// GA_Id of the single best match for `query`, or None. Used by the TUIs'
/// detail card (mirrors c64info.best_match).
pub fn best_match(con: &Connection, query: &str) -> Option<i64> {
    let (matches, exact) = search(con, query);
    if !exact.is_empty() {
        Some(exact[0].0)
    } else {
        matches.first().map(|m| m.0)
    }
}

#[derive(Clone)]
pub struct InfoRecord {
    pub name: String,
    pub year: Option<String>,
    pub rows: Vec<(String, String)>,
    pub note: Option<String>,
}

/// Treat GB64 sentinel values ("", "(None)", "(Unknown)", -1) as absent.
fn clean(v: Option<String>) -> Option<String> {
    match v {
        None => None,
        Some(s) if matches!(s.as_str(), "" | "(None)" | "(Unknown)" | "-1") => None,
        Some(s) => Some(s),
    }
}
fn clean_int(v: Option<i64>) -> Option<i64> {
    v.filter(|&n| n != -1)
}
/// Python truthiness for an int sentinel-cleaned value: present and non-zero.
fn truthy(v: Option<i64>) -> bool {
    matches!(v, Some(n) if n != 0)
}

/// Structured GB64 details for one game.
pub fn record(con: &Connection, gid: i64) -> InfoRecord {
    // Decode every column inside the closure: GB64 text can be cp1252, which the
    // rusqlite Value/String converters reject, so we must go through ValueRef.
    let cells: Vec<Option<String>> = con
        .query_row(DETAIL_SQL, [gid], |r| {
            (0..19).map(|i| Ok(text(r.get_ref(i)?))).collect()
        })
        .unwrap_or_else(|e| die(&format!("query failed: {e}")));
    let name = cells[0].clone().unwrap_or_default();
    let t = |i: usize| clean(cells[i].clone());
    let n = |i: usize| clean_int(cells[i].as_deref().and_then(|s| s.parse::<i64>().ok()));

    let (pf, pt, psim) = (n(9), n(10), n(11));
    let players = if truthy(pf) || truthy(pt) {
        let or = |a: Option<i64>, b: Option<i64>| if truthy(a) { a } else { b };
        let mut s = if pf == pt || !truthy(pt) {
            or(pf, pt).unwrap_or(0).to_string()
        } else {
            format!("{}-{}", pf.unwrap_or(0), pt.unwrap_or(0))
        };
        if truthy(psim) {
            s.push_str(" (simultaneous)");
        }
        Some(s)
    } else {
        None
    };

    let genre = {
        let parts: Vec<String> = [t(4), t(5)].into_iter().flatten().collect();
        if parts.is_empty() {
            None
        } else {
            Some(parts.join(" / "))
        }
    };
    let rating = n(12);
    let link = t(17).map(|url| {
        let name = t(16).unwrap_or_default();
        format!("{name} {url}").trim().to_string()
    });

    let candidates: Vec<(&str, Option<String>)> = vec![
        ("Publisher", t(2)),
        ("Developer", t(3)),
        ("Genre", genre),
        ("Programmer", t(6)),
        ("Music", t(7)),
        ("Language", t(8)),
        ("Players", players),
        ("Rating", rating.map(|r| format!("{r}/5"))),
        ("Classic", truthy(n(13)).then(|| "yes".to_string())),
        ("GB64 file", t(18)),
        ("Link", link),
    ];
    let rows: Vec<(String, String)> = candidates
        .into_iter()
        .filter_map(|(k, v)| v.map(|v| (k.to_string(), v)))
        .collect();

    let mut note = clean(t(14).or_else(|| t(15)));
    if let Some(text) = note.take() {
        let collapsed = Regex::new(r"\s+").unwrap().replace_all(text.trim(), " ").into_owned();
        note = Some(truncate_note(&collapsed, 500));
    }

    InfoRecord {
        name,
        year: t(1),
        rows,
        note,
    }
}

fn truncate_note(s: &str, limit: usize) -> String {
    if s.chars().count() <= limit {
        return s.to_string();
    }
    let cut: String = s.chars().take(limit).collect();
    let head = cut.rsplit_once(' ').map(|(h, _)| h).unwrap_or(&cut);
    format!("{head} ...")
}

fn show(con: &Connection, gid: i64) {
    let r = record(con, gid);
    let mut head = format!("\n  {}", r.name);
    if let Some(y) = &r.year {
        head.push_str(&format!("  ({y})"));
    }
    println!("{head}");
    println!("  {}", "-".repeat(std::cmp::max(r.name.len() + 8, 24)));
    for (label, val) in &r.rows {
        println!("    {label:<11} {val}");
    }
    if let Some(note) = &r.note {
        println!("\n    {note}");
    }
    println!();
}

pub fn main(argv: Vec<String>) -> ExitCode {
    use std::io::IsTerminal;
    let (mut refresh, mut list_all, mut queries) = (false, false, Vec::<String>::new());
    for a in &argv {
        match a.as_str() {
            "--refresh-db" => refresh = true,
            "-a" | "--all" => list_all = true,
            "-h" | "--help" => {
                print!("{HELP}");
                return ExitCode::SUCCESS;
            }
            s if s.starts_with('-') => die(&format!("unknown option: {s}")),
            s => queries.push(s.to_string()),
        }
    }

    if queries.is_empty() && !refresh {
        die("give a game title (see --help)");
    }
    let con = open_db(refresh);
    if queries.is_empty() {
        return ExitCode::SUCCESS;
    }
    let query = queries.join(" ");
    let (matches, exact) = search(&con, &query);
    if matches.is_empty() {
        die(&format!("no GB64 game matching: {query}"));
    }
    if list_all {
        for (_, n) in &matches {
            println!("{n}");
        }
        return ExitCode::SUCCESS;
    }
    // one clear exact title -> just show it; several exact -> best-rated unless on a tty
    let gid = if exact.len() == 1 || (!exact.is_empty() && !std::io::stdin().is_terminal()) {
        if exact.len() > 1 {
            eprintln!(
                "note: {} GB64 entries titled like \"{query}\"; showing the top-rated (use --all to see them).",
                exact.len()
            );
        }
        exact[0].0
    } else {
        pick(&matches)
    };
    show(&con, gid);
    ExitCode::SUCCESS
}
