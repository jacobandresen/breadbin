// demos - the demoscene catalogue backend: ranked CSDb demos grouped by party, cached
// to demos_index.tsv, with screenshot caching and download-and-prepare for VICE. The
// terminal UI is gone; the GUI builds its screenshot grid on top of this.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::core;

// CSDb endpoints.
const TOPLIST: &str = "https://csdb.dk/toplist.php?type=release&subtype=(1)";
fn event_ws(id: u32) -> String {
    format!("https://csdb.dk/webservice/?type=event&id={id}&depth=1")
}

pub const DEFAULT_LIMIT: usize = 1000;
/// A party needs at least this many top demos to earn its own section.
const MIN_PER_PARTY: usize = 3;
/// Each party section is capped at its best N demos.
const TOP_PER_PARTY: usize = 50;

/// One ranked demo from CSDb.
#[derive(Clone)]
pub struct Demo {
    pub id: u32,
    pub name: String,
    pub group: String,
    pub rating: f32,
    pub party: String, // party-series name (year stripped), "" if released outside a party
    pub year: u32,
    pub place: u32, // compo placing; 1 = winner, 0 = unknown
    pub shot: String, // screenshot URL
    pub event_type: String, // e.g. "C64 Only Party"
    pub city: String,
    pub country: String,
    pub website: String,
}

pub fn demos_index_path() -> PathBuf {
    core::data_path("demos_index.tsv")
}
fn shots_dir() -> PathBuf {
    core::user_data_dir().join("demo_covers")
}
fn downloads_dir() -> PathBuf {
    core::user_data_dir().join("demos")
}

/// Pull (event id, party name, year, place, screenshot URL) out of a depth-1 release
/// webservice XML document.
fn parse_release_xml(xml: &str) -> (u32, String, u32, u32, String) {
    let released_at = core::between(xml, "<ReleasedAt>", "</ReleasedAt>");
    let event_id = released_at
        .and_then(|b| core::between(b, "<ID>", "</ID>"))
        .and_then(|i| i.trim().parse::<u32>().ok())
        .unwrap_or(0);
    let party = released_at
        .and_then(|b| core::between(b, "<Name>", "</Name>"))
        .map(core::html_unescape)
        .unwrap_or_default();
    let year = core::between(xml, "<ReleaseYear>", "</ReleaseYear>")
        .and_then(|y| y.trim().parse::<u32>().ok())
        .unwrap_or(0);
    let place = core::between(xml, "<Place>", "</Place>")
        .and_then(|p| p.trim().parse::<u32>().ok())
        .unwrap_or(0);
    let shot = core::between(xml, "<ScreenShot>", "</ScreenShot>")
        .map(|s| core::html_unescape(s.trim()))
        .unwrap_or_default();
    (event_id, party, year, place, shot)
}

/// Pull venue facts (party type, city, country, website) out of a depth-1 event XML.
fn parse_event_xml(xml: &str) -> (String, String, String, String) {
    let field = |tag_open: &str, tag_close: &str| {
        core::between(xml, tag_open, tag_close)
            .map(|s| core::html_unescape(s.trim()))
            .unwrap_or_default()
    };
    (
        field("<EventType>", "</EventType>"),
        field("<City>", "</City>"),
        field("<Country>", "</Country>"),
        field("<Website>", "</Website>"),
    )
}

/// Fetch the ranked top demos from CSDb, decorate the first `limit`, and write
/// demos_index.tsv. `progress(done, total)` is called as scanning proceeds.
pub fn build_index(limit: usize, progress: &mut dyn FnMut(u64, u64)) -> Result<(), String> {
    let body = core::fetch(TOPLIST, &[])?;
    let html = String::from_utf8_lossy(&body);
    let ranked = core::parse_toplist(&html);
    if ranked.is_empty() {
        return Err("could not parse the CSDb top demos list".to_string());
    }
    let take = ranked.len().min(limit);
    let mut out = String::new();
    let mut venues: HashMap<u32, (String, String, String, String)> = HashMap::new();
    for (n, (id, name, group, rating)) in ranked.iter().take(take).enumerate() {
        progress(n as u64 + 1, take as u64);
        let (event_id, party, year, place, shot) = match core::fetch(&core::release_ws(*id, 1), &[]) {
            Ok(b) => parse_release_xml(&String::from_utf8_lossy(&b)),
            Err(_) => (0, String::new(), 0, 0, String::new()),
        };
        let (event_type, city, country, website) = if event_id == 0 {
            (String::new(), String::new(), String::new(), String::new())
        } else {
            venues
                .entry(event_id)
                .or_insert_with(|| match core::fetch(&event_ws(event_id), &[]) {
                    Ok(b) => parse_event_xml(&String::from_utf8_lossy(&b)),
                    Err(_) => (String::new(), String::new(), String::new(), String::new()),
                })
                .clone()
        };
        let series = core::party_series(&party);
        out.push_str(&format!(
            "{id}\t{}\t{}\t{:.2}\t{}\t{year}\t{place}\t{}\t{}\t{}\t{}\t{}\n",
            core::clean(name),
            core::clean(group),
            rating,
            core::clean(&series),
            core::clean(&shot),
            core::clean(&event_type),
            core::clean(&city),
            core::clean(&country),
            core::clean(&website),
        ));
    }
    std::fs::write(demos_index_path(), out).map_err(|e| e.to_string())?;
    Ok(())
}

/// Load demos_index.tsv into Demo records.
pub fn load_demos() -> Vec<Demo> {
    let mut v = Vec::new();
    let Ok(text) = std::fs::read_to_string(demos_index_path()) else {
        return v;
    };
    for line in text.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() < 8 {
            continue;
        }
        let field = |n: usize| f.get(n).map(|s| s.to_string()).unwrap_or_default();
        v.push(Demo {
            id: f[0].parse().unwrap_or(0),
            name: f[1].to_string(),
            group: f[2].to_string(),
            rating: f[3].parse().unwrap_or(0.0),
            party: f[4].to_string(),
            year: f[5].parse().unwrap_or(0),
            place: f[6].parse().unwrap_or(0),
            shot: f[7].to_string(),
            event_type: field(8),
            city: field(9),
            country: field(10),
            website: field(11),
        });
    }
    v
}

/// Group demos by party, biggest sections first. See [`core::group_by_party`].
pub fn group_by_party(all: &[Demo]) -> Vec<(String, Vec<usize>)> {
    core::group_by_party(all, |d| d.party.as_str(), |d| d.rating, MIN_PER_PARTY, TOP_PER_PARTY, false)
}

/// Cached screenshot path for a demo (downloading once), or None.
pub fn ensure_shot(d: &Demo) -> Option<PathBuf> {
    if d.shot.is_empty() {
        return None;
    }
    let ext = Path::new(&d.shot)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .filter(|e| matches!(e.as_str(), "png" | "gif" | "jpg" | "jpeg"))
        .unwrap_or_else(|| "png".to_string());
    let cache = shots_dir().join(format!("{}.{ext}", d.id));
    if cache.metadata().map(|m| m.len() > 0).unwrap_or(false) {
        return Some(cache);
    }
    let data = core::fetch(&d.shot, &[]).ok()?;
    if data.len() < 200 {
        return None;
    }
    std::fs::create_dir_all(shots_dir()).ok()?;
    std::fs::write(&cache, data).ok()?;
    Some(cache)
}

// Disk/program image extensions VICE can autostart, best first.
const RUN_EXTS: &[&str] = &[".d64", ".g64", ".d81", ".d71", ".prg", ".crt", ".t64", ".tap"];

/// Download a demo and return a local file VICE can run, caching it under demos/.
pub fn fetch_and_prepare(d: &Demo) -> Result<PathBuf, String> {
    let dir = downloads_dir();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for e in entries.flatten() {
            let p = e.path();
            let stem_matches = p
                .file_stem()
                .map(|s| s.to_string_lossy().starts_with(&format!("{}_", d.id)))
                .unwrap_or(false);
            if stem_matches && p.metadata().map(|m| m.len() > 0).unwrap_or(false) {
                return Ok(p);
            }
        }
    }
    let body = core::fetch(&core::release_ws(d.id, 2), &[])?;
    let xml = String::from_utf8_lossy(&body);
    let mut links: Vec<String> = Vec::new();
    let mut rest = xml.as_ref();
    while let Some(link) = core::between(rest, "<Link>", "</Link>") {
        links.push(core::html_unescape(link.trim()));
        let adv = rest.find("</Link>").map(|p| p + 7).unwrap_or(rest.len());
        rest = &rest[adv..];
    }
    let link = links
        .iter()
        .find(|l| l.starts_with("http"))
        .ok_or("no downloadable (http) link for this demo on CSDb")?;

    let data = core::fetch(link, &[])?;
    let safe: String = d
        .name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    if data.starts_with(b"PK") {
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(data))
            .map_err(|e| format!("bad zip: {e}"))?;
        let names: Vec<String> = zip.file_names().map(String::from).collect();
        let member = RUN_EXTS.iter().find_map(|ext| {
            let mut m: Vec<&String> = names.iter().filter(|n| n.to_lowercase().ends_with(ext)).collect();
            m.sort_by_key(|n| n.len());
            m.first().map(|s| (*s).clone())
        });
        let member = member.ok_or("no runnable disk/program image inside the demo archive")?;
        let ext = Path::new(&member)
            .extension()
            .map(|e| format!(".{}", e.to_string_lossy()))
            .unwrap_or_default();
        let mut blob = Vec::new();
        use std::io::Read;
        zip.by_name(&member)
            .map_err(|e| e.to_string())?
            .read_to_end(&mut blob)
            .map_err(|e| e.to_string())?;
        let out = dir.join(format!("{}_{safe}{ext}", d.id));
        std::fs::write(&out, &blob).map_err(|e| e.to_string())?;
        Ok(out)
    } else {
        let ext = Path::new(link)
            .extension()
            .map(|e| format!(".{}", e.to_string_lossy().to_lowercase()))
            .filter(|e| RUN_EXTS.contains(&e.as_str()))
            .ok_or("the demo download is not a recognised C64 image")?;
        let out = dir.join(format!("{}_{safe}{ext}", d.id));
        std::fs::write(&out, &data).map_err(|e| e.to_string())?;
        Ok(out)
    }
}
