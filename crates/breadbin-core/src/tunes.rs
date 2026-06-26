// tunes - the SID-tune catalogue backend: ranked CSDb "C64 Music" releases grouped by
// party, cached to tunes_index.tsv, with on-demand .sid download. The terminal UI and
// visualisers are gone; the GUI builds its own player on top of this + crate::audio.

use std::path::PathBuf;

use crate::core;

// CSDb: subtype (7) of the release toplist is "C64 Music", ranked by rating.
const TOPLIST: &str = "https://csdb.dk/toplist.php?type=release&subtype=(7)";

pub const DEFAULT_LIMIT: usize = 600;
/// A party needs at least this many top tunes to earn its own section.
const MIN_PER_PARTY: usize = 2;
/// Each party section is capped at this many tunes.
const TOP_PER_PARTY: usize = 30;

/// How long radio lingers on one tune (seconds of playback) before picking the next.
pub const RADIO_SECS: u64 = 90;

/// One ranked tune from CSDb.
#[derive(Clone)]
pub struct Tune {
    pub id: u32,
    pub name: String,
    pub composer: String,
    pub group: String,
    pub party: String, // party-series the tune was released at, "" if none
    pub rating: f32,
    pub year: u32,
    pub sid_url: String,
}

pub fn tunes_index_path() -> PathBuf {
    core::data_path("tunes_index.tsv")
}
fn sids_dir() -> PathBuf {
    core::user_data_dir().join("sids")
}

/// Pull (composer, party, year, sid download URL) out of a depth-2 release webservice
/// XML document.
fn parse_release_xml(xml: &str) -> (String, String, u32, String) {
    let year = core::between(xml, "<ReleaseYear>", "</ReleaseYear>")
        .and_then(|y| y.trim().parse::<u32>().ok())
        .unwrap_or(0);

    let party = core::between(xml, "<ReleasedAt>", "</ReleasedAt>")
        .and_then(|b| core::between(b, "<Name>", "</Name>"))
        .map(|n| core::party_series(&core::html_unescape(n)))
        .unwrap_or_default();

    let mut sid_url = String::new();
    let mut rest = xml;
    while let Some(link) = core::between(rest, "<Link>", "</Link>") {
        let url = core::html_unescape(link.trim());
        if url.starts_with("http") && url.to_lowercase().ends_with(".sid") {
            sid_url = url;
            break;
        }
        let adv = rest.find("</Link>").map(|p| p + 7).unwrap_or(rest.len());
        rest = &rest[adv..];
    }

    let mut composer = String::new();
    let mut rest = xml;
    while let Some(open) = rest.find("<Credit>") {
        let after = &rest[open + 8..];
        let block_end = after.find("</Credit>").unwrap_or(after.len());
        let block = &after[..block_end];
        if block.contains("<CreditType>Music") {
            let mut hrest = block;
            while let Some(h) = core::between(hrest, "<Handle>", "</Handle>") {
                let trimmed = h.trim();
                if !trimmed.is_empty() && !trimmed.contains('<') {
                    composer = core::html_unescape(trimmed);
                    break;
                }
                let adv = hrest.find("<Handle>").map(|p| p + 8).unwrap_or(hrest.len());
                hrest = &hrest[adv..];
            }
            if !composer.is_empty() {
                break;
            }
        }
        rest = &after[block_end..];
    }
    (composer, party, year, sid_url)
}

/// Fetch the ranked top tunes from CSDb, decorate the first `limit`, and write
/// tunes_index.tsv. `progress(done, total)` is called as scanning proceeds.
pub fn build_index(limit: usize, progress: &mut dyn FnMut(u64, u64)) -> Result<(), String> {
    let body = core::fetch(TOPLIST, &[])?;
    let html = String::from_utf8_lossy(&body);
    let ranked = core::parse_toplist(&html);
    if ranked.is_empty() {
        return Err("could not parse the CSDb top music list".to_string());
    }
    let take = ranked.len().min(limit);
    let mut out = String::new();
    for (n, (id, name, group, rating)) in ranked.iter().take(take).enumerate() {
        progress(n as u64 + 1, take as u64);
        let (composer, party, year, sid_url) = match core::fetch(&core::release_ws(*id, 2), &[]) {
            Ok(b) => parse_release_xml(&String::from_utf8_lossy(&b)),
            Err(_) => (String::new(), String::new(), 0, String::new()),
        };
        if sid_url.is_empty() {
            continue;
        }
        out.push_str(&format!(
            "{id}\t{}\t{}\t{}\t{:.2}\t{year}\t{}\t{}\n",
            core::clean(name),
            core::clean(&composer),
            core::clean(group),
            rating,
            core::clean(&sid_url),
            core::clean(&party),
        ));
    }
    std::fs::write(tunes_index_path(), out).map_err(|e| e.to_string())?;
    Ok(())
}

/// Load tunes_index.tsv into Tune records.
pub fn load_tunes() -> Vec<Tune> {
    let mut v = Vec::new();
    let Ok(text) = std::fs::read_to_string(tunes_index_path()) else {
        return v;
    };
    for line in text.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() < 8 {
            continue;
        }
        v.push(Tune {
            id: f[0].parse().unwrap_or(0),
            name: f[1].to_string(),
            composer: f[2].to_string(),
            group: f[3].to_string(),
            rating: f[4].parse().unwrap_or(0.0),
            year: f[5].parse().unwrap_or(0),
            sid_url: f[6].to_string(),
            party: f[7].to_string(),
        });
    }
    v
}

/// Group tunes by party, highest-rated section first. See [`core::group_by_party`].
pub fn group_by_party(all: &[Tune]) -> Vec<(String, Vec<usize>)> {
    core::group_by_party(all, |t| t.party.as_str(), |t| t.rating, MIN_PER_PARTY, TOP_PER_PARTY, true)
}

/// Download a tune's .sid (caching it under sids/) and return the bytes.
pub fn ensure_sid(t: &Tune) -> Result<Vec<u8>, String> {
    let cache = sids_dir().join(format!("{}.sid", t.id));
    if let Ok(b) = std::fs::read(&cache) {
        if b.len() > 0x7c && (&b[0..4] == b"PSID" || &b[0..4] == b"RSID") {
            return Ok(b);
        }
    }
    if t.sid_url.is_empty() {
        return Err("no SID download link for this tune".into());
    }
    let data = core::fetch(&t.sid_url, &[])?;
    if data.len() < 0x7c || (&data[0..4] != b"PSID" && &data[0..4] != b"RSID") {
        return Err("download was not a SID file".into());
    }
    std::fs::create_dir_all(sids_dir()).ok();
    std::fs::write(&cache, &data).ok();
    Ok(data)
}

/// Tiny xorshift64 RNG for radio mode (random tune + visual picks).
pub struct Rng(u64);

impl Rng {
    pub fn new() -> Rng {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9e3779b97f4a7c15);
        Rng(seed | 1)
    }
    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    /// A value in 0..n (n must be > 0).
    pub fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }
}

impl Default for Rng {
    fn default() -> Self {
        Rng::new()
    }
}
