// disk - find and download C64 disk games from the Internet Archive and TOSEC,
// and build the ranked IA catalogue (ia_index.tsv). Queries are matched by
// difflib-style fuzzy ratio (core::seq_ratio).

use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::path::Path;
use std::sync::OnceLock;

use regex::Regex;

use crate::{core, gb64};

const COLL: &str = "softwarelibrary_c64";
const SEARCH: &str = "https://archive.org/advancedsearch.php";
const SCRAPE: &str = "https://archive.org/services/search/v1/scrape";
const META: &str = "https://archive.org/metadata/";
const DL: &str = "https://archive.org/download/";
const TOSEC_ID: &str = "tosec-20161111-commodore-c64";
const TOSEC_ZIP: &str = "TOSEC.2016.11.11.Commodore.C64.AlphaBot.zip";
const UA: &str = "breadbin/1.0 (personal C64 collection tool; polite, low-volume)";
const EXT_PRIO: &[&str] = &[".d64", ".g64", ".d81", ".d71", ".t64", ".prg", ".crt"];
const STOP: &[&str] = &["the", "a", "of", "and", "or", "to", "in", "for"];

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
    // group by release_key, preserving first-seen order
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
/// [(handle, title)] for one source: exact canonical equality, else the best
/// fuzzy match.
fn resolve(query: &str, source: &str) -> Vec<Item> {
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

    // highest ratio wins; ties keep search order
    let Some((ratio, chosen)) = results
        .iter()
        .map(|it| (core::seq_ratio(&qn, &game_title(&it.1)), it))
        .fold(None, |best: Option<(f64, &Item)>, (r, it)| match best {
            Some((br, _)) if br >= r => best,
            _ => Some((r, it)),
        })
    else {
        return vec![];
    };
    if ratio < 0.45 {
        return vec![];
    }
    let cg = game_title(&chosen.1);
    best_release(results.into_iter().filter(|(_, t)| game_title(t) == cg).collect())
}

/// Try sources in order; the first that yields a match wins (sources are never mixed).
fn resolve_best(query: &str, sources: &[String]) -> Vec<Item> {
    sources.iter().map(|s| resolve(query, s)).find(|r| !r.is_empty()).unwrap_or_default()
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
fn gb64_meta() -> Result<HashMap<String, (i64, String)>, String> {
    let con = gb64::connect()?;
    let mut out: HashMap<String, (i64, String)> = HashMap::new();
    let sql = "SELECT g.Name, g.Rating, pg.ParentGenre FROM Games g \
               LEFT JOIN Genres ge ON ge.GE_Id = g.GE_Id \
               LEFT JOIN PGenres pg ON pg.PG_Id = ge.PG_Id";
    let mut stmt = con.prepare(sql).map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| {
            let name = gb64::decode_text(r.get_ref(0)?).unwrap_or_default();
            let rating = r.get_ref(1).ok().and_then(|v| match v {
                rusqlite::types::ValueRef::Integer(i) => Some(i),
                _ => None,
            });
            let genre = gb64::decode_text(r.get_ref(2)?);
            Ok((name, rating, genre))
        })
        .map_err(|e| e.to_string())?;
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
    Ok(out)
}

pub fn build_ia_index() -> Result<(), String> {
    eprintln!("loading GameBase64 ratings + genres ...");
    let meta = gb64_meta()?;
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
            Err(e) => return Err(format!("IA scrape error: {e}")),
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
    rows.sort_by_key(|r| std::cmp::Reverse((r.1, r.2)));
    let mut out = String::new();
    for (c, r, dl, genre, ident, disp) in &rows {
        out.push_str(&format!("{c}\t{r}\t{dl}\t{genre}\t{ident}\t{disp}\n"));
    }
    let ia_path = core::data_path("ia_index.tsv");
    std::fs::write(&ia_path, out).map_err(|e| format!("cannot write {}: {e}", ia_path.display()))?;
    eprintln!("scanned {seen} IA items; {} games on IA with details -> {}", rows.len(), ia_path.display());
    Ok(())
}

/// Download an exact Internet Archive item and return the boot disk path, recording
/// it in downloaded.tsv. None on failure. Used by the GUI instead of re-exec'ing the
/// `disk --id` subcommand.
pub fn download_by_id(id: &str, dest: &Path) -> Option<std::path::PathBuf> {
    let got = download_ia(id, id, dest, false);
    let boot = dest.join(got.first()?);
    record_download(id, &boot.to_string_lossy());
    Some(boot)
}

/// Resolve a query against the given sources and download the best matching release,
/// returning the saved disk-image paths. Library entry point for the GUI.
pub fn download_query(query: &str, sources: &[String], dest: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    for (handle, ti) in resolve_best(query, sources) {
        for f in download(&handle, &ti, dest, false) {
            out.push(dest.join(f));
        }
    }
    out
}
