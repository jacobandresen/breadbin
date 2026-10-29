// csdb - sync releases from CSDb (Commodore Scene Database) into the local
// Diesel-backed cache, and query that cache.

use std::time::Duration;

use crate::core;
use crate::database::models::*;
use crate::diesel_db;

pub mod parser;

const WEBSERVICE_BASE: &str = "https://csdb.dk/webservice/";
const TOPLIST_BASE: &str = "https://csdb.dk/toplist.php";
const SEARCH_URL: &str = "https://csdb.dk/search/";
/// Pause between webservice requests, to stay polite.
const REQUEST_DELAY: Duration = Duration::from_millis(100);

/// Sync statistics.
#[derive(Debug, Clone, Default)]
pub struct CollectionStats {
    pub releases_fetched: usize,
    pub errors: Vec<String>,
}

/// Fetch a single release with full details.
pub fn fetch_release(id: u32, depth: u8) -> Result<Release, String> {
    let url = format!("{WEBSERVICE_BASE}?type=release&id={id}&depth={depth}");
    let xml = core::fetch(&url, &[])?;
    parser::parse_release(&String::from_utf8_lossy(&xml), id)
}

/// Release ids of the top `limit` entries of a release toplist (`subtype` is
/// CSDb's release type, e.g. 1 = C64 Demo, 7 = C64 Music).
fn toplist_ids(subtype: u32, limit: usize) -> Result<Vec<u32>, String> {
    let url = format!("{TOPLIST_BASE}?type=release&subtype=({subtype})");
    let html = core::fetch(&url, &[])?;
    let ranked = core::parse_toplist(&String::from_utf8_lossy(&html));
    Ok(ranked.into_iter().take(limit).map(|(id, ..)| id).collect())
}

/// Fetch the given releases from CSDb and store them in the local database.
pub fn sync_releases(release_ids: &[u32]) -> Result<CollectionStats, String> {
    let mut stats = CollectionStats::default();

    // Releases reference release_types by id; seed them before saving.
    diesel_db::initialize_default_release_types()?;

    for (i, &id) in release_ids.iter().enumerate() {
        if i > 0 {
            std::thread::sleep(REQUEST_DELAY);
        }
        match fetch_release(id, 2) {
            Ok(release) => match diesel_db::save_release(&release) {
                Ok(()) => stats.releases_fetched += 1,
                Err(e) => stats.errors.push(format!("Failed to save release {id}: {e}")),
            },
            Err(e) => stats.errors.push(format!("Failed to fetch release {id}: {e}")),
        }
    }
    Ok(stats)
}

/// Sync the top `count` demos and top `count` music releases to the local database.
pub fn sync_top_releases(count: usize) -> Result<CollectionStats, String> {
    let mut ids = toplist_ids(1, count)?;
    ids.extend(toplist_ids(7, count)?);
    ids.sort_unstable();
    ids.dedup();
    sync_releases(&ids)
}

/// Release ids matching `query` on csdb.dk, exact titles first (empty if none).
/// A query with a single hit redirects straight to that release's page.
pub fn search_page_ids(query: &str) -> Result<Vec<u32>, String> {
    let url = format!(
        "{SEARCH_URL}?{}",
        core::urlencode(&[("seinsel", "releases"), ("search", query)])
    );
    let resp = ureq::get(&url)
        .set("User-Agent", "breadbin/1.0")
        .call()
        .map_err(|e| e.to_string())?;
    let landed = resp.get_url().to_string();
    let html = resp.into_string().map_err(|e| e.to_string())?;

    if let Some(id) = landed.split("/release/?id=").nth(1).and_then(|r| {
        r.split(|c: char| !c.is_ascii_digit()).next().and_then(|n| n.parse().ok())
    }) {
        return Ok(vec![id]);
    }
    Ok(parse_search_page(&html, query))
}

/// Release ids from a csdb.dk search results page, exact titles first.
fn parse_search_page(html: &str, query: &str) -> Vec<u32> {
    // Hits are the first <ol> after "N release matches"; the sidebars hold
    // toplists full of other release links, so look nowhere else.
    let Some(list) = html
        .find(" matches:")
        .and_then(|at| core::between(&html[at..], "<ol>", "</ol>"))
    else {
        return Vec::new();
    };
    let mut hits: Vec<(u32, String)> = Vec::new();
    for chunk in list.split("<a href=\"/release/?id=").skip(1) {
        let Some((id, rest)) = chunk.split_once("\">") else { continue };
        let Ok(id) = id.parse::<u32>() else { continue };
        let name = core::html_unescape(rest.split("</a>").next().unwrap_or(""));
        if !hits.iter().any(|(h, _)| *h == id) {
            hits.push((id, name));
        }
    }
    // csdb.dk lists hits newest first; put exact titles, then prefixes, first.
    let q = query.trim().to_lowercase();
    hits.sort_by_key(|(_, name)| {
        let n = name.to_lowercase();
        if n == q { 0 } else if n.starts_with(&q) { 1 } else { 2 }
    });
    hits.into_iter().map(|(id, _)| id).collect()
}

/// Fetch a release in full and cache it locally, so later searches find it offline.
pub fn fetch_and_cache(id: u32) -> Result<Release, String> {
    diesel_db::initialize_default_release_types()?;
    let release = fetch_release(id, 2)?;
    // Releases of types outside the seeded set can't be cached; still return them.
    let _ = diesel_db::save_release(&release);
    Ok(release)
}

/// Search csdb.dk itself and fetch (and cache) the first `limit` hits, in
/// csdb.dk's relevance order.
pub fn search_online(query: &str, limit: usize) -> Result<Vec<Release>, String> {
    let mut releases = Vec::new();
    for (i, id) in search_page_ids(query)?.into_iter().take(limit).enumerate() {
        if i > 0 {
            std::thread::sleep(REQUEST_DELAY);
        }
        match fetch_and_cache(id) {
            Ok(r) => releases.push(r),
            Err(e) => eprintln!("CSDb release {id}: {e}"),
        }
    }
    Ok(releases)
}

/// Search the local CSDb cache by release name.
pub fn search_csdb(query: &str, limit: usize) -> Result<Vec<(u32, String)>, String> {
    diesel_db::search_by_name(query, limit)
}

/// True if the local CSDb cache has data.
pub fn has_csdb_data() -> bool {
    diesel_db::has_data()
}

/// The top `limit` releases by rating from the local CSDb cache.
pub fn get_top_releases(limit: usize) -> Result<Vec<Release>, String> {
    diesel_db::get_top_releases(limit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_page_ignores_sidebars_and_ranks_exact_titles_first() {
        let html = r#"<b>3 release</b> matches:<br><ol>
<li><a href="/release/download.php?id=1"><img></a><a href="/release/?id=205788">Hedge Hacker Of Disgrace</a> (C64 Demo)
<li><a href="/release/?id=197722">Hedge of Disgrace</a>
<li><a href="/release/?id=72550">Edge of Disgrace</a>
</ol><table>Top Demos 2 <a href="/release/?id=232976">Next Level</a></table>"#;
        assert_eq!(parse_search_page(html, "Edge of Disgrace"), vec![72550, 205788, 197722]);
        assert!(parse_search_page("<p>No matches</p>", "x").is_empty());
    }
}
