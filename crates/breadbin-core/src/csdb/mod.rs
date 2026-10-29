// csdb - sync releases from CSDb (Commodore Scene Database) into the local
// Diesel-backed cache, and query that cache.

use std::time::Duration;

use crate::core;
use crate::database::models::*;
use crate::diesel_db;

pub mod parser;

const WEBSERVICE_BASE: &str = "https://csdb.dk/webservice/";
const TOPLIST_BASE: &str = "https://csdb.dk/toplist.php";
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
