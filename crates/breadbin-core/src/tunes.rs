// tunes - the SID-tune catalogue backend: ranked CSDb "C64 Music" releases grouped by
// party, cached to tunes_index.tsv, with on-demand .sid download. The GUI's player builds on
// this and crate::audio.

use std::path::PathBuf;

use crate::core;

// CSDb: subtype (7) of the release toplist is "C64 Music", ranked by rating.
const TOPLIST: &str = "https://csdb.dk/toplist.php?type=release&subtype=(7)";

pub const DEFAULT_LIMIT: usize = 600;
/// A party needs at least this many top tunes to earn its own section.
const MIN_PER_PARTY: usize = 2;
/// Each party section is capped at this many tunes.
const TOP_PER_PARTY: usize = 30;

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

fn is_sid(b: &[u8]) -> bool {
    b.len() > 0x7c && (&b[0..4] == b"PSID" || &b[0..4] == b"RSID")
}

fn sid_cache_path(id: u32) -> PathBuf {
    sids_dir().join(format!("{id}.sid"))
}

/// Download a tune's .sid (caching it under sids/) and return the bytes.
pub fn ensure_sid(t: &Tune) -> Result<Vec<u8>, String> {
    let cache = sid_cache_path(t.id);
    if let Ok(b) = std::fs::read(&cache) {
        if is_sid(&b) {
            return Ok(b);
        }
    }
    if t.sid_url.is_empty() {
        return Err("no SID download link for this tune".into());
    }
    let data = core::fetch(&t.sid_url, &[])?;
    if !is_sid(&data) {
        return Err("download was not a SID file".into());
    }
    std::fs::create_dir_all(sids_dir()).ok();
    std::fs::write(&cache, &data).ok();
    Ok(data)
}

// --- SID availability ---------------------------------------------------------

/// Whether a tune's SID can be played.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SidStatus {
    /// Downloaded and verified (cached locally).
    Ready,
    /// No link, a dead link, or a download that isn't a SID.
    Unavailable,
}

/// Failed checks are retried after this long, in case the host was only down.
const RECHECK_SECS: u64 = 7 * 24 * 3600;

fn unavailable_path() -> PathBuf {
    core::data_path("sid_unavailable.tsv")
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Tune ids whose SID failed to download recently (id<TAB>unix-time per line).
pub fn load_unavailable() -> std::collections::HashSet<u32> {
    let cutoff = now_secs().saturating_sub(RECHECK_SECS);
    std::fs::read_to_string(unavailable_path())
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.split_once('\t'))
        .filter(|(_, ts)| ts.parse::<u64>().is_ok_and(|t| t >= cutoff))
        .filter_map(|(id, _)| id.parse().ok())
        .collect()
}

fn record_unavailable(id: u32) {
    use std::io::Write;
    if let Ok(mut f) =
        std::fs::OpenOptions::new().create(true).append(true).open(unavailable_path())
    {
        let _ = writeln!(f, "{id}\t{}", now_secs());
    }
}

/// The status known without touching the network: Ready if the SID is cached,
/// Unavailable if it has no link or failed recently, None if it needs a check.
pub fn known_sid_status(
    t: &Tune,
    unavailable: &std::collections::HashSet<u32>,
) -> Option<SidStatus> {
    if std::fs::read(sid_cache_path(t.id)).is_ok_and(|b| is_sid(&b)) {
        Some(SidStatus::Ready)
    } else if t.sid_url.is_empty() || unavailable.contains(&t.id) {
        Some(SidStatus::Unavailable)
    } else {
        None
    }
}

/// Verify a SID by downloading it (into the cache, so playback is instant later).
/// Failures are remembered for [`RECHECK_SECS`].
pub fn verify_sid(t: &Tune) -> SidStatus {
    match ensure_sid(t) {
        Ok(_) => SidStatus::Ready,
        Err(_) => {
            record_unavailable(t.id);
            SidStatus::Unavailable
        }
    }
}

// --- Release details ------------------------------------------------------------

/// A musician credited on a tune.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Musician {
    pub handle: String,
    pub real_name: String,
    pub country: String,
}

/// The group a tune was released by.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GroupInfo {
    pub name: String,
    pub short: String,
    pub slogan: String,
    pub trivia: String,
    pub founded: Option<u32>,
    pub website: String,
}

/// Everything CSDb knows about a tune that is worth showing while it plays.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TuneInfo {
    pub name: String,
    pub year: Option<u32>,
    pub event: String,
    /// "City, Country" of the event.
    pub event_place: String,
    /// Compo name and placing, e.g. ("C64 Music", 16).
    pub compo: Option<(String, u32)>,
    pub rating: f32,
    pub screenshot_url: String,
    pub group: Option<GroupInfo>,
    pub musicians: Vec<Musician>,
    pub sid_model: String,
    pub clock: String,
    pub songs: u32,
    pub downloads: u32,
}

fn info_dir() -> PathBuf {
    core::user_data_dir().join("tune_info")
}

fn field(xml: &str, tag: &str) -> String {
    core::between(xml, &format!("<{tag}>"), &format!("</{tag}>"))
        .map(|v| core::html_unescape(v.trim()))
        .unwrap_or_default()
}

/// CSDb trivia uses BBCode; keep the text, drop the tags.
fn strip_bbcode(s: &str) -> String {
    static BBCODE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
    let re = BBCODE.get_or_init(|| regex::Regex::new(r"\[/?[a-zA-Z]+[^\]]*\]").unwrap());
    re.replace_all(s, "").trim().to_string()
}

/// Parse a depth-2 release webservice document into a [`TuneInfo`].
pub fn parse_info(xml: &str) -> TuneInfo {
    let num = |tag: &str, within: &str| field(within, tag).parse::<u32>().ok();
    let mut info = TuneInfo {
        name: field(xml, "Name"),
        year: num("ReleaseYear", xml),
        rating: field(xml, "Rating").parse().unwrap_or(0.0),
        screenshot_url: field(xml, "ScreenShot"),
        ..Default::default()
    };

    if let Some(ev) = core::between(xml, "<ReleasedAt>", "</ReleasedAt>") {
        info.event = field(ev, "Name");
        info.event_place = [field(ev, "City"), field(ev, "Country")]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(", ");
    }
    if let Some(a) = core::between(xml, "<Achievement>", "</Achievement>") {
        if let Some(place) = num("Place", a) {
            info.compo = Some((field(a, "Compo"), place));
        }
    }
    if let Some(by) = core::between(xml, "<ReleasedBy>", "</ReleasedBy>") {
        // Co-releases list several groups: name them all, detail the first.
        let groups: Vec<&str> = by.split("<Group>").skip(1).collect();
        let names: Vec<String> = groups.iter().map(|g| field(g, "Name")).collect();
        let g = groups.first().copied().unwrap_or(by);
        info.group = Some(GroupInfo {
            name: names.join(" & "),
            short: field(g, "Short"),
            slogan: field(g, "Slogan"),
            trivia: strip_bbcode(&field(g, "Trivia")),
            founded: num("FoundYear", g),
            website: field(g, "Website"),
        });
    }

    // Real names come from the HVSC "Author" line, e.g. "Sascha Zeidler (Linus)".
    let author = core::between(xml, "<UsedSIDs>", "</UsedSIDs>").map(|s| field(s, "Author"));
    let mut rest = xml;
    while let Some(open) = rest.find("<Credit>") {
        let after = &rest[open + 8..];
        let end = after.find("</Credit>").unwrap_or(after.len());
        let block = &after[..end];
        rest = &after[end..];
        if !block.contains("<CreditType>Music") {
            continue;
        }
        // The first <Handle> wraps the scener record; the handle name is the
        // first <Handle>…</Handle> pair with plain text inside.
        let mut hrest = block;
        let mut handle = String::new();
        while let Some(h) = core::between(hrest, "<Handle>", "</Handle>") {
            if !h.contains('<') && !h.trim().is_empty() {
                handle = core::html_unescape(h.trim());
                break;
            }
            hrest = &hrest[hrest.find("<Handle>").map_or(hrest.len(), |p| p + 8)..];
        }
        if handle.is_empty() {
            continue;
        }
        let real_name = author
            .as_deref()
            .filter(|a| a.contains(&format!("({handle})")))
            .map(|a| core::split_before(a, r" \(").trim().to_string())
            .unwrap_or_default();
        info.musicians.push(Musician { handle, real_name, country: field(block, "Country") });
    }

    if let Some(sid) = core::between(xml, "<SID>", "</SID>") {
        info.sid_model = field(sid, "SIDModel");
        info.clock = field(sid, "ClockSpeed");
        info.songs = num("NOSongs", sid).unwrap_or(0);
    }
    if let Some(dl) = core::between(xml, "<DownloadLinks>", "</DownloadLinks>") {
        info.downloads = num("Downloads", dl).unwrap_or(0);
    }
    info
}

/// Release details for a tune, from the on-disk cache or CSDb.
pub fn fetch_info(id: u32) -> Result<TuneInfo, String> {
    let cache = info_dir().join(format!("{id}.xml"));
    let xml = match std::fs::read_to_string(&cache) {
        Ok(x) if x.contains("<Release>") => x,
        _ => {
            let x = String::from_utf8_lossy(&core::fetch(&core::release_ws(id, 2), &[])?)
                .into_owned();
            if !x.contains("<Release>") {
                return Err(format!("CSDb has no release {id}"));
            }
            std::fs::create_dir_all(info_dir()).ok();
            std::fs::write(&cache, &x).ok();
            x
        }
    };
    Ok(parse_info(&xml))
}

/// The release screenshot, downloaded once into the info cache.
pub fn ensure_screenshot(id: u32, url: &str) -> Option<PathBuf> {
    if url.is_empty() {
        return None;
    }
    let ext = url.rsplit('.').next().filter(|e| e.len() <= 4).unwrap_or("png");
    let path = info_dir().join(format!("{id}.{ext}"));
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > 0) {
        return Some(path);
    }
    let data = core::fetch(url, &[]).ok()?;
    std::fs::create_dir_all(info_dir()).ok();
    std::fs::write(&path, data).ok()?;
    Some(path)
}

// ============================================================================
// Diesel Database Integration
// ============================================================================

use crate::database::models::{DownloadLink, Release, ReleaseCredit};
use crate::diesel_db;

/// Load tunes from the Diesel database instead of the TSV file.
/// Includes releases of type 7 (C64 Music) and 8 (C64 Music Collection).
/// The SID URL comes from the release's first .sid download link and the
/// composer from its first Music credit; either stays empty if absent.
pub fn load_tunes_from_db() -> Vec<Tune> {
    match diesel_db::list_releases(&[7, 8]) {
        Ok(listings) => listings
            .into_iter()
            .map(|l| {
                let sid_url = l
                    .release
                    .downloads
                    .iter()
                    .find(|d| d.url.to_lowercase().ends_with(".sid"))
                    .map(|d| d.url.clone())
                    .unwrap_or_default();
                let composer = l
                    .release
                    .credits
                    .iter()
                    .find(|c| c.credit_type.starts_with("Music"))
                    .map(|c| c.handle.clone())
                    .unwrap_or_default();
                Tune {
                    id: l.release.id,
                    name: l.release.name,
                    composer,
                    group: l.group_name,
                    party: l.event_name,
                    rating: l.release.rating,
                    year: l.release.release_year.unwrap_or(0),
                    sid_url,
                }
            })
            .collect(),
        Err(e) => {
            eprintln!("Failed to load tunes from Diesel DB: {}", e);
            Vec::new()
        }
    }
}

/// Load tunes from the Diesel database and the TSV index combined: DB rows
/// win, but missing fields (SID URL, composer, group, party) are filled from
/// the TSV, and TSV-only tunes are appended.
pub fn load_tunes_merged() -> Vec<Tune> {
    let mut tunes = load_tunes_from_db();
    let mut from_tsv: std::collections::HashMap<u32, Tune> =
        load_tunes().into_iter().map(|t| (t.id, t)).collect();
    for t in tunes.iter_mut() {
        if let Some(f) = from_tsv.remove(&t.id) {
            if t.sid_url.is_empty() {
                t.sid_url = f.sid_url;
            }
            if t.composer.is_empty() {
                t.composer = f.composer;
            }
            if t.group.is_empty() {
                t.group = f.group;
            }
            if t.party.is_empty() {
                t.party = f.party;
            }
        }
    }
    tunes.extend(from_tsv.into_values());
    tunes
}

/// Save a Tune to the Diesel database as a type 7 (C64 Music) release, with
/// its SID URL as a download link and the composer as a Music credit. The
/// party is not persisted (CSDb event ids aren't known here).
pub fn save_tune_to_db(tune: &Tune) -> Result<(), String> {
    diesel_db::initialize_default_release_types()?;
    let downloads = if tune.sid_url.is_empty() {
        Vec::new()
    } else {
        vec![DownloadLink {
            id: None,
            release_id: tune.id,
            url: tune.sid_url.clone(),
            counter_url: None,
            downloads: 0,
            status: String::new(),
            is_internal: false,
            file_type: Some("sid".to_string()),
        }]
    };
    let credits = if tune.composer.is_empty() {
        Vec::new()
    } else {
        vec![ReleaseCredit {
            id: None,
            release_id: tune.id,
            scener_id: None,
            handle: tune.composer.clone(),
            credit_type: "Music".to_string(),
            position: 0,
            functions: Vec::new(),
        }]
    };
    diesel_db::save_release(&Release {
        id: tune.id,
        name: tune.name.clone(),
        release_type_id: 7, // C64 Music
        rating: tune.rating,
        release_year: (tune.year > 0).then_some(tune.year),
        downloads,
        credits,
        ..Default::default()
    })
}

/// Build index and save to the Diesel database, also writing tunes_index.tsv
/// in the same pass.
pub fn build_index_to_db(limit: usize, progress: &mut dyn FnMut(u64, u64)) -> Result<(), String> {
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

        let tune = Tune {
            id: *id,
            name: core::clean(name),
            composer: core::clean(&composer),
            group: core::clean(group),
            rating: *rating,
            year,
            sid_url: core::clean(&sid_url),
            party: core::clean(&party),
        };

        if let Err(e) = save_tune_to_db(&tune) {
            eprintln!("Failed to save tune {}: {}", id, e);
        }

        out.push_str(&format!(
            "{}\t{}\t{}\t{}\t{:.2}\t{}\t{}\t{}\n",
            tune.id, tune.name, tune.composer, tune.group, tune.rating, tune.year, tune.sid_url,
            tune.party,
        ));
    }

    std::fs::write(tunes_index_path(), out).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const XML: &str = "<Release><ID>1</ID><Name>Sheds</Name><ReleaseYear>2023</ReleaseYear>\
<ReleasedAt><Event><ID>3</ID><Name>X'2023</Name><City>Someren</City><Country>Netherlands</Country></Event></ReleasedAt>\
<Achievement><Compo>C64 Music</Compo><Place>16</Place></Achievement><Rating>9.64</Rating>\
<ScreenShot>https://csdb.dk/gfx/1.png</ScreenShot>\
<ReleasedBy><Group><ID>9</ID><Name>MultiStyle Labs</Name><Short>MSL</Short>\
<Trivia>Friends [url]http://x[/url]</Trivia><FoundYear>2000</FoundYear><Slogan>we ar moor</Slogan></Group>\
<Group><ID>10</ID><Name>Pretzel Logic</Name><Slogan>other</Slogan></Group></ReleasedBy>\
<Credits><Credit><CreditType>Music</CreditType><Handle><ID>5</ID><Handle>Linus</Handle>\
<Scener><ID>2</ID><Country>Germany</Country></Scener></Handle></Credit></Credits>\
<UsedSIDs><SID><ID>7</ID><Author>Sascha Zeidler (Linus)</Author><NOSongs>1</NOSongs>\
<SIDModel>8580</SIDModel><ClockSpeed>PAL</ClockSpeed></SID></UsedSIDs>\
<DownloadLinks><DownloadLink><Link>x.sid</Link><Downloads>184</Downloads></DownloadLink></DownloadLinks></Release>";

    #[test]
    fn parses_release_details() {
        let i = parse_info(XML);
        assert_eq!(i.name, "Sheds");
        assert_eq!(i.year, Some(2023));
        assert_eq!(i.event, "X'2023");
        assert_eq!(i.event_place, "Someren, Netherlands");
        assert_eq!(i.compo, Some(("C64 Music".to_string(), 16)));
        let g = i.group.unwrap();
        assert_eq!(
            (g.name.as_str(), g.short.as_str(), g.founded, g.slogan.as_str()),
            ("MultiStyle Labs & Pretzel Logic", "MSL", Some(2000), "we ar moor")
        );
        assert_eq!(g.trivia, "Friends http://x");
        assert_eq!(
            i.musicians,
            vec![Musician {
                handle: "Linus".into(),
                real_name: "Sascha Zeidler".into(),
                country: "Germany".into()
            }]
        );
        assert_eq!((i.sid_model.as_str(), i.clock.as_str(), i.songs, i.downloads), ("8580", "PAL", 1, 184));
    }
}
