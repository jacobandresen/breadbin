// cover - box-art lookup from the libretro-thumbnails Commodore 64 set
// (Named_Boxarts), plus the Internet Archive item-image fallback. Port of
// bb-cover's index/fetch logic and c64menu.ia_cover; the fzf preview CLI is not
// ported (the ratatui TUIs render covers directly).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;

use regex::Regex;

use crate::core;

const TREE: &str =
    "https://api.github.com/repos/libretro-thumbnails/Commodore_-_64/git/trees/master?recursive=1";
const RAW: &str = "https://raw.githubusercontent.com/libretro-thumbnails/Commodore_-_64/master/Named_Boxarts/";

pub fn cidx_path() -> PathBuf {
    core::data_path("covers_index.tsv")
}
pub fn cdir() -> PathBuf {
    // Use user data directory for cached covers
    crate::core::user_data_dir().join("covers")
}

fn region_rank(name: &str) -> u8 {
    let n = name.to_lowercase();
    if n.contains("(world") {
        0
    } else if n.contains("(europe") {
        1
    } else if n.contains("(usa") {
        2
    } else {
        3
    }
}

/// Fetch the libretro tree and write covers_index.tsv (canon<TAB>filename),
/// keeping the best region per canonical title.
pub fn build_cover_index() -> Result<(), String> {
    let body = core::fetch(TREE, &[])?;
    let json: serde_json::Value =
        serde_json::from_slice(&body).map_err(|e| e.to_string())?;
    let tree = json["tree"].as_array().ok_or("tree: not an array")?;

    let mut best: HashMap<String, (u8, String)> = HashMap::new();
    for p in tree {
        let path = p["path"].as_str().unwrap_or("");
        if !path.starts_with("Named_Boxarts/") || !path.ends_with(".png") {
            continue;
        }
        let fname = &path["Named_Boxarts/".len()..];
        let stem = &fname[..fname.len() - 4]; // drop ".png"
        let canon = core::norm(core::split_before(stem, r"\s*\("));
        let rank = region_rank(fname);
        if !canon.is_empty()
            && best.get(&canon).map_or(true, |(r, _)| rank < *r)
        {
            best.insert(canon, (rank, fname.to_string()));
        }
    }

    let mut out = String::new();
    for (c, (_, fname)) in &best {
        out.push_str(&format!("{c}\t{fname}\n"));
    }
    std::fs::write(cidx_path(), out).map_err(|e| e.to_string())
}

/// canon -> cover filename map, building the index on first use.
pub fn load_index() -> HashMap<String, String> {
    if !cidx_path().exists() {
        let _ = build_cover_index();
    }
    let mut map = HashMap::new();
    if let Ok(text) = std::fs::read_to_string(cidx_path()) {
        for line in text.lines() {
            if let Some((c, fname)) = line.split_once('\t') {
                map.insert(c.to_string(), fname.to_string());
            }
        }
    }
    map
}

fn cached(path: &PathBuf) -> bool {
    std::fs::metadata(path).map(|m| m.len() > 0).unwrap_or(false)
}

/// Cached cover path for a game (downloading once), or None.
pub fn ensure_cover(canon: &str, idx: &HashMap<String, String>) -> Option<PathBuf> {
    let fname = idx.get(canon)?;
    let cache = cdir().join(format!("{canon}.png"));
    if !cached(&cache) {
        let data = core::fetch(&format!("{RAW}{}", core::quote(fname)), &[]).ok()?;
        std::fs::create_dir_all(cdir()).ok()?;
        std::fs::write(&cache, data).ok()?;
    }
    Some(cache)
}

/// Fallback cover: the Internet Archive item's own image (services/img), cached.
pub fn ia_cover(ident: &str) -> Option<PathBuf> {
    if ident.is_empty() {
        return None;
    }
    static SANITIZE: OnceLock<Regex> = OnceLock::new();
    let sanitize = SANITIZE.get_or_init(|| Regex::new(r"[^A-Za-z0-9_.-]").unwrap());
    let safe = sanitize.replace_all(ident, "_");
    let cache = cdir().join(format!("ia_{safe}.jpg"));
    if cached(&cache) {
        return Some(cache);
    }
    let data = core::fetch(&format!("https://archive.org/services/img/{ident}"), &[]).ok()?;
    if data.len() < 1000 {
        return None; // generic placeholder icon -> treat as none
    }
    std::fs::create_dir_all(cdir()).ok()?;
    std::fs::write(&cache, data).ok()?;
    Some(cache)
}
