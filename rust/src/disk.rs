// c64disk - find and download C64 disk games from Internet Archive + TOSEC.
// Port of c64disk. Matching is exact canonical game-title equality for --missing,
// and a difflib-ratio fuzzy fzf picker for interactive queries (core::seq_ratio).

use std::collections::HashMap;
use std::io::{Cursor, Read, Write};
use std::path::Path;
use std::process::ExitCode;
use std::sync::OnceLock;

use regex::Regex;

use crate::{core, info};

const COLL: &str = "softwarelibrary_c64";
const SEARCH: &str = "https://archive.org/advancedsearch.php";
const SCRAPE: &str = "https://archive.org/services/search/v1/scrape";
const META: &str = "https://archive.org/metadata/";
const DL: &str = "https://archive.org/download/";
const TOSEC_ID: &str = "tosec-20161111-commodore-c64";
const TOSEC_ZIP: &str = "TOSEC.2016.11.11.Commodore.C64.AlphaBot.zip";
const UA: &str = "c64disk/1.0 (personal C64 collection tool; polite, low-volume)";
const EXT_PRIO: &[&str] = &[".d64", ".g64", ".d81", ".d71", ".t64", ".prg", ".crt"];
const STOP: &[&str] = &["the", "a", "of", "and", "or", "to", "in", "for"];

const HELP: &str = "\
c64disk - find and download C64 disk games from Internet Archive + TOSEC.

Usage:
  c64disk <title> [<title> ...]      search and download the best-matching disk image(s)
  c64disk --source all <title>       try every source (ia, then tosec)
  c64disk --missing                  disk games from the ranked list you don't own
  c64disk -n / --list <title>        dry run: show what would be downloaded
  c64disk --dest DIR                 download dir (default: <collection>/_IA_downloads)
  c64disk --refresh-index            rebuild the cached TOSEC listing
";

fn tosec_list_url() -> String {
    format!("{DL}{TOSEC_ID}/{TOSEC_ZIP}/")
}
fn tosec_index_path() -> std::path::PathBuf {
    core::data_path("tosec_index.tsv")
}
pub fn dest_default() -> std::path::PathBuf {
    core::c64_lib().join("_IA_downloads")
}

fn re(p: &str) -> Regex {
    Regex::new(p).expect("static regex")
}

fn get_text(url: &str) -> Result<String, String> {
    let bytes = core::fetch(url, &[("User-Agent", UA)])?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}
fn get_bytes(url: &str) -> Result<Vec<u8>, String> {
    core::fetch(url, &[("User-Agent", UA)])
}

fn is_disk(blob: &[u8]) -> bool {
    blob.len() >= 1024 && blob.iter().find(|b| !b.is_ascii_whitespace()) != Some(&b'<')
}

/// A download handle: ("ia", identifier) or ("tosec", url).
#[derive(Clone, Debug)]
pub struct Handle {
    pub src: &'static str,
    pub reff: String,
}

pub type Item = (Handle, String); // (handle, title)

// --- title helpers -----------------------------------------------------------

/// The game name only - strip '(year)(publisher)[flags]', subtitle and version.
pub fn game_title(t: &str) -> String {
    let t = core::split_before(t, r"\s*[\(\[]");
    let t = core::split_before(t, r":| - ");
    static VER: OnceLock<Regex> = OnceLock::new();
    let ver = VER.get_or_init(|| re(r"\bv\d+(\.\d+)*\b"));
    core::norm(&ver.replace_all(t, ""))
}

fn words_of(query: &str) -> Vec<String> {
    static WORD: OnceLock<Regex> = OnceLock::new();
    let word = WORD.get_or_init(|| re(r"[a-z0-9]+"));
    let lower = query.to_lowercase();
    let ws: Vec<String> = word
        .find_iter(&lower)
        .map(|m| m.as_str().to_string())
        .filter(|w| !STOP.contains(&w.as_str()))
        .collect();
    if ws.is_empty() {
        vec![lower]
    } else {
        ws
    }
}

pub fn disk_no(t: &str) -> i32 {
    static RX: OnceLock<Regex> = OnceLock::new();
    let rx = RX.get_or_init(|| re(r"disk\s*(\d+)"));
    rx.captures(&t.to_lowercase())
        .and_then(|c| c[1].parse().ok())
        .unwrap_or(1)
}

pub fn side_no(t: &str) -> i32 {
    static RX: OnceLock<Regex> = OnceLock::new();
    let rx = RX.get_or_init(|| re(r"side\s*([a-d1-4])"));
    match rx.captures(&t.to_lowercase()) {
        None => 0,
        Some(c) => {
            let g = c[1].chars().next().unwrap();
            if g.is_ascii_digit() {
                g.to_digit(10).unwrap() as i32
            } else {
                g as i32 - 'a' as i32 + 1
            }
        }
    }
}

fn release_key(t: &str) -> String {
    static R1: OnceLock<Regex> = OnceLock::new();
    static R2: OnceLock<Regex> = OnceLock::new();
    static R3: OnceLock<Regex> = OnceLock::new();
    static R4: OnceLock<Regex> = OnceLock::new();
    static R5: OnceLock<Regex> = OnceLock::new();
    let mut s = t.to_lowercase();
    s = R1.get_or_init(|| re(r"disk\s*\d+\s*(of\s*\d+)?")).replace_all(&s, "").into_owned();
    s = R2.get_or_init(|| re(r"side\s*[a-d0-9]\b")).replace_all(&s, "").into_owned();
    s = R3
        .get_or_init(|| re(r"\b(boot|game|player\s*disk|scenario|data\s*disk|doc)\b"))
        .replace_all(&s, "")
        .into_owned();
    s = R4.get_or_init(|| re(r"[\(\)\[\]]")).replace_all(&s, " ").into_owned();
    R5.get_or_init(|| re(r"\s+")).replace_all(s.trim(), " ").trim().to_string()
}

fn has_boot(members: &[Item]) -> bool {
    members
        .iter()
        .any(|(_, t)| t.to_lowercase().contains("boot") || (disk_no(t) == 1 && matches!(side_no(t), 0 | 1)))
}

fn bracket_count(t: &str) -> i64 {
    t.matches('[').count() as i64
}

/// Members of the best release (same source): bootable first, then most disks.
pub fn best_release(items: Vec<Item>) -> Vec<Item> {
    // group by release_key, preserving first-seen order (matches Python dict order)
    let mut order: Vec<String> = Vec::new();
    let mut groups: HashMap<String, Vec<Item>> = HashMap::new();
    for it in items {
        let key = release_key(&it.1);
        if !groups.contains_key(&key) {
            order.push(key.clone());
        }
        groups.entry(key).or_default().push(it);
    }
    let cov = |g: &[Item]| -> usize {
        g.iter().map(|(_, t)| disk_no(t)).collect::<std::collections::HashSet<_>>().len()
    };
    // key = (has_boot, cov, -sum brackets, -len(release_key)); first max wins
    let mut best_key: Option<(bool, usize, i64, i64)> = None;
    let mut best: Vec<Item> = Vec::new();
    for k in &order {
        let g = &groups[k];
        let key = (
            has_boot(g),
            cov(g),
            -g.iter().map(|(_, t)| bracket_count(t)).sum::<i64>(),
            -(k.chars().count() as i64),
        );
        if best_key.as_ref().map_or(true, |b| key > *b) {
            best_key = Some(key);
            best = g.clone();
        }
    }
    best.sort_by_key(|it| (disk_no(&it.1), side_no(&it.1)));
    best
}

fn coverage(members: &[Item]) -> (usize, i32) {
    static OF: OnceLock<Regex> = OnceLock::new();
    let of = OF.get_or_init(|| re(r"of\s*(\d+)"));
    let want = members
        .iter()
        .filter_map(|(_, t)| of.captures(&t.to_lowercase()).and_then(|c| c[1].parse::<i32>().ok()))
        .max()
        .unwrap_or(1);
    let got = members.iter().map(|(_, t)| disk_no(t)).collect::<std::collections::HashSet<_>>().len();
    (got, want)
}

// --- source: Internet Archive ------------------------------------------------
fn search_ia(query: &str) -> Result<Vec<Item>, String> {
    let q = format!("collection:({COLL}) AND title:({})", words_of(query).join(" OR "));
    let url = format!(
        "{SEARCH}?{}&fl[]=identifier&fl[]=title",
        core::urlencode(&[("q", &q), ("rows", "80"), ("output", "json")])
    );
    let json: serde_json::Value = serde_json::from_str(&get_text(&url)?).map_err(|e| e.to_string())?;
    let docs = json["response"]["docs"].as_array().cloned().unwrap_or_default();
    Ok(docs
        .iter()
        .map(|d| {
            let ident = d["identifier"].as_str().unwrap_or("").to_string();
            let title = d["title"].as_str().unwrap_or(&ident).to_string();
            (Handle { src: "ia", reff: ident }, title)
        })
        .collect())
}

fn download_ia(identifier: &str, _title: &str, dest: &Path, dry: bool) -> Vec<String> {
    let meta = match get_text(&format!("{META}{identifier}")) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("    !! metadata error: {e}");
            return vec![];
        }
    };
    let json: serde_json::Value = serde_json::from_str(&meta).unwrap_or(serde_json::Value::Null);
    let names: Vec<String> = json["files"]
        .as_array()
        .map(|fs| fs.iter().filter_map(|f| f["name"].as_str().map(String::from)).collect())
        .unwrap_or_default();

    // first extension in EXT_PRIO that any file matches; take all such names, sorted
    let fnames: Vec<String> = EXT_PRIO
        .iter()
        .find_map(|e| {
            let mut v: Vec<String> =
                names.iter().filter(|n| n.to_lowercase().ends_with(e)).cloned().collect();
            if v.is_empty() {
                None
            } else {
                v.sort();
                Some(v)
            }
        })
        .unwrap_or_default();

    let mut got = Vec::new();
    for fname in fnames {
        let out = dest.join(&fname);
        if dry {
            eprintln!("    would download: {fname}");
            got.push(fname);
            continue;
        }
        if out.metadata().map(|m| m.len() > 0).unwrap_or(false) {
            eprintln!("    exists, skip: {fname}");
            got.push(fname);
            continue;
        }
        let data = match get_bytes(&format!("{DL}{identifier}/{}", core::quote(&fname))) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("    !! download error: {e}");
                continue;
            }
        };
        if !is_disk(&data) {
            eprintln!("    !! not a disk image ({}B) - skipped: {fname}", data.len());
            continue;
        }
        let _ = std::fs::create_dir_all(dest);
        if std::fs::write(&out, &data).is_ok() {
            eprintln!("    saved: {fname} ({} KB)", data.len() / 1024);
            got.push(fname);
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    }
    got
}

// --- source: TOSEC -----------------------------------------------------------
fn build_tosec_index() -> Result<(), String> {
    eprintln!("building TOSEC index (one-time ~170MB listing) ...");
    let page = get_text(&tosec_list_url())?;
    let href_re = re(r#"href="(//archive\.org/download/[^"]+\.zip)""#);
    let mut order: Vec<String> = Vec::new();
    let mut best: HashMap<String, String> = HashMap::new();
    for cap in href_re.captures_iter(&page) {
        let href = &cap[1];
        let is_d64 = href.contains("%5BD64%5D");
        if !href.contains("Games") || !(is_d64 || href.contains("%5BG64%5D")) {
            continue;
        }
        let last = href.split("%2F").last().unwrap_or("");
        let unq = core::html_unescape(&core::unquote(last));
        let title = unq.strip_suffix(".zip").unwrap_or(&unq).to_string();
        let url = format!("https:{href}");
        match best.get(&title) {
            Some(cur) if !(is_d64 && cur.contains("%5BG64%5D")) => {}
            Some(_) => {
                best.insert(title, url);
            }
            None => {
                order.push(title.clone());
                best.insert(title, url);
            }
        }
    }
    let mut keys: Vec<&String> = best.keys().collect();
    keys.sort();
    let mut out = String::new();
    for t in keys {
        out.push_str(&format!("{t}\t{}\n", best[t]));
    }
    std::fs::write(tosec_index_path(), out).map_err(|e| e.to_string())?;
    eprintln!("  TOSEC index: {} D64/G64 game disks cached", best.len());
    Ok(())
}

pub fn tosec_entries() -> Vec<(String, String)> {
    if !tosec_index_path().exists() {
        let _ = build_tosec_index();
    }
    let mut out = Vec::new();
    if let Ok(text) = std::fs::read_to_string(tosec_index_path()) {
        for line in text.lines() {
            if let Some((t, u)) = line.split_once('\t') {
                out.push((t.to_string(), u.to_string()));
            }
        }
    }
    out
}

fn search_tosec(query: &str) -> Result<Vec<Item>, String> {
    let ws = words_of(query);
    Ok(tosec_entries()
        .into_iter()
        .filter(|(t, _)| {
            let gt = game_title(t);
            ws.iter().any(|w| gt.contains(w))
        })
        .map(|(t, u)| (Handle { src: "tosec", reff: u }, t))
        .collect())
}

fn download_tosec(url: &str, title: &str, dest: &Path, dry: bool) -> Vec<String> {
    if dry {
        eprintln!("    would download: {title}.zip -> unpack");
        return vec![format!("{}.d64", title.replace('/', "-"))];
    }
    let data = match get_bytes(url) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("    !! bad TOSEC zip: {e}");
            return vec![];
        }
    };
    let mut zip = match zip::ZipArchive::new(Cursor::new(data)) {
        Ok(z) => z,
        Err(e) => {
            eprintln!("    !! bad TOSEC zip: {e}");
            return vec![];
        }
    };
    let names: Vec<String> = zip.file_names().map(String::from).collect();
    // first ext in EXT_PRIO with a match; among those, shortest name
    let member = EXT_PRIO.iter().find_map(|e| {
        let mut v: Vec<&String> = names.iter().filter(|n| n.to_lowercase().ends_with(e)).collect();
        v.sort_by_key(|n| n.len());
        v.first().map(|s| (*s).clone())
    });
    let Some(member) = member else {
        eprintln!("    (no disk image in TOSEC zip)");
        return vec![];
    };
    let ext = Path::new(&member).extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    let fname = format!("{title}{ext}").replace('/', "-");
    let out = dest.join(&fname);
    if out.metadata().map(|m| m.len() > 0).unwrap_or(false) {
        eprintln!("    exists, skip: {fname}");
        return vec![fname];
    }
    let mut blob = Vec::new();
    if zip.by_name(&member).and_then(|mut f| f.read_to_end(&mut blob).map_err(Into::into)).is_err() {
        eprintln!("    !! could not read zip member");
        return vec![];
    }
    if !is_disk(&blob) {
        eprintln!("    !! not a disk image - skipped: {fname}");
        return vec![];
    }
    let _ = std::fs::create_dir_all(dest);
    if std::fs::write(&out, &blob).is_ok() {
        eprintln!("    saved: {fname} ({} KB)", blob.len() / 1024);
        std::thread::sleep(std::time::Duration::from_secs(1));
        vec![fname]
    } else {
        vec![]
    }
}

pub fn download(handle: &Handle, title: &str, dest: &Path, dry: bool) -> Vec<String> {
    if handle.src == "tosec" {
        download_tosec(&handle.reff, title, dest, dry)
    } else {
        download_ia(&handle.reff, title, dest, dry)
    }
}

fn search_source(source: &str, query: &str) -> Result<Vec<Item>, String> {
    match source {
        "ia" => search_ia(query),
        "tosec" => search_tosec(query),
        _ => Err(format!("unknown source: {source}")),
    }
}

// --- matching ----------------------------------------------------------------
/// fzf pick. None = not interactive / no fzf (caller lists instead).
fn pick_with_fzf(matches: &[Item]) -> Option<Vec<Item>> {
    use std::io::IsTerminal;
    if !std::io::stdin().is_terminal() || !core::command_exists("fzf") {
        return None;
    }
    let input: String = matches
        .iter()
        .enumerate()
        .map(|(idx, (_, t))| format!("{t}\t{idx}"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut child = match std::process::Command::new("fzf")
        .args(["--with-nth=1", "--delimiter=\t", "--reverse", "--prompt", "disk > "])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(_) => return None,
    };
    if let Some(stdin) = child.stdin.take() {
        let mut s = stdin;
        let _ = s.write_all(input.as_bytes());
    }
    let out = child.wait_with_output().ok()?;
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() {
        return Some(vec![]);
    }
    let idx: usize = s.rsplit('\t').next().and_then(|n| n.parse().ok())?;
    Some(vec![matches[idx].clone()])
}

/// [(handle, title)] for one source: exact canonical equality, else fuzzy fzf.
fn resolve(query: &str, strict: bool, source: &str) -> Vec<Item> {
    let qn = core::norm(query);
    if qn.is_empty() {
        return vec![];
    }
    let results = match search_source(source, query) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("  {source} search error for \"{query}\": {e}");
            return vec![];
        }
    };

    let exact: Vec<Item> = results.iter().filter(|(_, t)| game_title(t) == qn).cloned().collect();
    if !exact.is_empty() {
        return best_release(exact);
    }
    if strict {
        return vec![];
    }

    let mut scored: Vec<(f64, Item)> = results
        .iter()
        .map(|it| (core::seq_ratio(&qn, &game_title(&it.1)), it.clone()))
        .collect();
    // sorted by ratio desc, stable (Python sorted with reverse=True is stable)
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let pool: Vec<Item> = scored.into_iter().filter(|(r, _)| *r >= 0.45).take(20).map(|(_, it)| it).collect();
    if pool.is_empty() {
        return vec![];
    }
    let chosen = if pool.len() == 1 {
        Some(vec![pool[0].clone()])
    } else {
        pick_with_fzf(&pool)
    };
    let chosen = match chosen {
        None => {
            eprintln!("  \"{query}\": {} fuzzy matches in {source} - run on a terminal to pick:", pool.len());
            for (_, t) in pool.iter().take(15) {
                eprintln!("     {t}");
            }
            return vec![];
        }
        Some(c) if c.is_empty() => return vec![],
        Some(c) => c,
    };
    let cg = game_title(&chosen[0].1);
    best_release(results.into_iter().filter(|(_, t)| game_title(t) == cg).collect())
}

/// Try sources in order; never mix. Strict: keep the most disk-complete.
fn resolve_best(query: &str, strict: bool, sources: &[String]) -> Vec<Item> {
    let (mut best, mut best_cov): (Vec<Item>, i64) = (vec![], -1);
    for s in sources {
        let r = resolve(query, strict, s);
        if r.is_empty() {
            continue;
        }
        if !strict {
            return r;
        }
        let (cov, want) = coverage(&r);
        if cov as i64 > best_cov {
            best = r.clone();
            best_cov = cov as i64;
        }
        if cov as i32 >= want {
            return r;
        }
    }
    best
}

fn missing_titles() -> Vec<String> {
    let mut owned = std::collections::HashSet::new();
    if let Ok(text) = std::fs::read_to_string(core::data_path("c64_index.tsv")) {
        for line in text.lines() {
            let f: Vec<&str> = line.split('\t').collect();
            if f.len() >= 5 && f[1] == "local" {
                owned.insert(f[3].to_string());
            }
        }
    }
    let mut out = Vec::new();
    if let Ok(text) = std::fs::read_to_string(core::data_path("c64_popularity.tsv")) {
        for line in text.lines() {
            let f: Vec<&str> = line.split('\t').collect();
            if f.len() == 4 {
                let title = f[3];
                if !owned.contains(title) {
                    out.push(core::split_before(title, r":| - ").to_string());
                }
            }
        }
    }
    out
}

fn record_download(ident: &str, path: &str) {
    let dpath = core::data_path("downloaded.tsv");
    let mut rows: std::collections::BTreeMap<String, String> = std::collections::BTreeMap::new();
    if let Ok(text) = std::fs::read_to_string(&dpath) {
        for line in text.lines() {
            if let Some((k, v)) = line.split_once('\t') {
                rows.insert(k.to_string(), v.to_string());
            }
        }
    }
    rows.insert(ident.to_string(), path.to_string());
    let mut out = String::new();
    for (k, v) in &rows {
        out.push_str(&format!("{k}\t{v}\n"));
    }
    let _ = std::fs::write(&dpath, out);
}

/// Canon -> (rating 0-5, parent genre) from the GB64 DB.
fn gb64_meta() -> HashMap<String, (i64, String)> {
    let con = info::connect();
    let mut out: HashMap<String, (i64, String)> = HashMap::new();
    let sql = "SELECT g.Name, g.Rating, pg.ParentGenre FROM Games g \
               LEFT JOIN Genres ge ON ge.GE_Id = g.GE_Id \
               LEFT JOIN PGenres pg ON pg.PG_Id = ge.PG_Id";
    let mut stmt = con.prepare(sql).expect("gb64 query");
    let rows = stmt
        .query_map([], |r| {
            let name = info::decode_text(r.get_ref(0)?).unwrap_or_default();
            let rating = r.get_ref(1).ok().and_then(|v| match v {
                rusqlite::types::ValueRef::Integer(i) => Some(i),
                _ => None,
            });
            let genre = info::decode_text(r.get_ref(2)?);
            Ok((name, rating, genre))
        })
        .expect("gb64 rows");
    for row in rows.flatten() {
        let (name, rating, genre) = row;
        if name.is_empty() {
            continue;
        }
        let c = game_title(&name);
        if c.is_empty() {
            continue;
        }
        let r = match rating {
            Some(v) if v > 0 => v,
            _ => 0,
        };
        let g = match genre {
            Some(s) if s != "(None)" && s != "(Unknown)" && !s.is_empty() => s,
            _ => String::new(),
        };
        out.entry(c)
            .and_modify(|cur| {
                cur.0 = cur.0.max(r);
                if cur.1.is_empty() {
                    cur.1 = g.clone();
                }
            })
            .or_insert((r, g));
    }
    out
}

fn build_ia_index() -> ExitCode {
    eprintln!("loading GameBase64 ratings + genres ...");
    let meta = gb64_meta();
    eprintln!("games with details: {}  ·  scanning IA collection ({COLL}) ...", meta.len());
    // canon -> (downloads, title, identifier), most-downloaded wins
    let mut best: HashMap<String, (i64, String, String)> = HashMap::new();
    let mut seen = 0u64;
    let mut cursor: Option<String> = None;
    let ws = re(r"\s+");
    let mut bar: Option<core::Progress> = None;
    loop {
        let mut params = vec![
            ("q", format!("collection:{COLL}")),
            ("fields", "title,identifier,downloads".to_string()),
            ("count", "10000".to_string()),
        ];
        if let Some(c) = &cursor {
            params.push(("cursor", c.clone()));
        }
        let pairs: Vec<(&str, &str)> = params.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let url = format!("{SCRAPE}?{}", core::urlencode(&pairs));
        let json: serde_json::Value = match get_text(&url).and_then(|t| serde_json::from_str(&t).map_err(|e| e.to_string())) {
            Ok(j) => j,
            Err(e) => {
                eprintln!("  IA scrape error: {e}");
                return ExitCode::from(1);
            }
        };
        let items = json["items"].as_array().cloned().unwrap_or_default();
        if items.is_empty() {
            break;
        }
        // The scrape API reports the collection size up front, so the bar shows a
        // real percentage; updated once per page (pages are up to 10k items).
        let total = json["total"].as_u64().unwrap_or(0);
        let bar = bar.get_or_insert_with(|| core::Progress::new("  scanning IA", total));
        for it in &items {
            seen += 1;
            let ident = it["identifier"].as_str().unwrap_or("").to_string();
            let title = it["title"].as_str().filter(|s| !s.is_empty()).unwrap_or(&ident).to_string();
            let c = game_title(&title);
            if meta.contains_key(&c) {
                let before = core::split_before(&title, r"[\(\[]");
                let disp = ws.replace_all(before.trim(), " ").trim().to_string();
                let dls = it["downloads"].as_i64().unwrap_or(0);
                if best.get(&c).map_or(true, |b| dls > b.0) {
                    best.insert(c, (dls, disp, ident));
                }
            }
        }
        bar.set(seen);
        cursor = json["cursor"].as_str().map(String::from);
        if cursor.is_none() {
            break;
        }
    }
    if let Some(bar) = &bar {
        bar.finish();
    }
    let mut rows: Vec<(String, i64, i64, String, String, String)> = best
        .into_iter()
        .map(|(c, (dl, disp, ident))| {
            let (rating, genre) = meta.get(&c).cloned().unwrap_or((0, String::new()));
            (c, rating, dl, genre, ident, disp)
        })
        .collect();
    // rating, then IA downloads, descending
    rows.sort_by(|a, b| (b.1, b.2).cmp(&(a.1, a.2)));
    let mut out = String::new();
    for (c, r, dl, genre, ident, disp) in &rows {
        out.push_str(&format!("{c}\t{r}\t{dl}\t{genre}\t{ident}\t{disp}\n"));
    }
    let ia_path = core::data_path("ia_index.tsv");
    if std::fs::write(&ia_path, out).is_err() {
        eprintln!("c64disk: cannot write {}", ia_path.display());
        return ExitCode::from(1);
    }
    eprintln!("scanned {seen} IA items; {} games on IA with details -> {}", rows.len(), ia_path.display());
    ExitCode::SUCCESS
}

pub fn main(argv: Vec<String>) -> ExitCode {
    let (mut dry, mut use_missing) = (false, false);
    let mut dest = dest_default();
    let mut sources: Vec<String> = vec!["ia".to_string()];
    let mut queries: Vec<String> = Vec::new();
    let mut ident: Option<String> = None;

    let mut i = 0;
    while i < argv.len() {
        let a = argv[i].as_str();
        match a {
            "-n" | "--list" => dry = true,
            "--missing" => use_missing = true,
            "--ia-index" => return build_ia_index(),
            "--id" => {
                i += 1;
                ident = argv.get(i).cloned();
            }
            "--dest" => {
                i += 1;
                if let Some(d) = argv.get(i) {
                    dest = std::path::PathBuf::from(d);
                }
            }
            "--source" => {
                i += 1;
                sources = match argv.get(i).map(String::as_str) {
                    Some("all") => vec!["ia".into(), "tosec".into()],
                    Some(s) => vec![s.to_string()],
                    None => sources,
                };
            }
            "--refresh-index" => {
                let _ = std::fs::remove_file(tosec_index_path());
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

    let bad: Vec<&String> = sources.iter().filter(|s| !matches!(s.as_str(), "ia" | "tosec")).collect();
    if !bad.is_empty() {
        eprintln!("unknown source(s): {} (use ia, tosec, or all)", bad.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", "));
        return ExitCode::from(2);
    }

    if let Some(id) = ident {
        eprintln!("downloading IA item {id} -> {}", dest.display());
        let got = download_ia(&id, &id, &dest, dry);
        if !got.is_empty() && !dry {
            let boot = dest.join(&got[0]);
            record_download(&id, &boot.to_string_lossy());
            println!("{}", boot.display());
        }
        eprintln!("\n{} {} disk image(s).", if dry { "would download" } else { "downloaded" }, got.len());
        return if got.is_empty() { ExitCode::from(1) } else { ExitCode::SUCCESS };
    }

    if !use_missing && queries.is_empty() {
        eprintln!("nothing to do: give a title, or --missing (see --help)");
        return ExitCode::from(2);
    }

    let mut games: Vec<(String, Vec<Item>)> = Vec::new();
    if use_missing {
        for t in missing_titles() {
            let r = resolve_best(&t, true, &sources);
            if !r.is_empty() {
                games.push((t, r));
            }
        }
        eprintln!("missing top-100 disk games found ({}): {}", sources.join("+"), games.len());
    }
    for q in &queries {
        let r = resolve_best(q, false, &sources);
        if !r.is_empty() {
            games.push((q.clone(), r));
        }
    }

    if games.is_empty() {
        eprintln!("nothing matched to download.");
        return ExitCode::from(1);
    }

    eprintln!("\n{}processing {} game(s) -> {}\n", if dry { "(dry run) " } else { "" }, games.len(), dest.display());
    let of = re(r"of\s*(\d+)");
    let mut total = 0;
    for (label, members) in &games {
        eprintln!("* {label}  [{}]", members[0].0.src);
        let mut files = Vec::new();
        for (handle, ti) in members {
            files.extend(download(handle, ti, &dest, dry));
        }
        total += files.len();
        let fs: Vec<String> = files.iter().map(|f| f.replace('_', " ")).collect();
        let want = fs
            .iter()
            .filter_map(|f| of.captures(&f.to_lowercase()).and_then(|c| c[1].parse::<i32>().ok()))
            .max()
            .unwrap_or(1);
        let got = {
            let d = fs.iter().map(|f| disk_no(f)).collect::<std::collections::HashSet<_>>().len();
            if d == 0 {
                files.len()
            } else {
                d
            }
        };
        let note = if got as i32 >= want {
            format!("{got}/{want} disk(s)")
        } else {
            format!("{got}/{want} disk(s)  ⚠ incomplete - only {got} of {want} found")
        };
        eprintln!("    -> {note}");
    }
    eprintln!("\n{} {total} disk image(s) for {} game(s).", if dry { "would download" } else { "downloaded" }, games.len());
    if total > 0 && !dry {
        eprintln!("run  c64menu --refresh  to add them to the picker.");
    }
    ExitCode::SUCCESS
}
