// csdb - CSDb (Commodore Scene Database) data collection and API integration.
//
// This module provides functions to fetch and process data from CSDb's webservice API.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::Semaphore;

use crate::core;
use crate::database::models::*;
use crate::diesel_db;

pub mod fetcher;
pub mod parser;

/// ID range for different entity types.
#[derive(Debug, Clone)]
pub struct IdRange {
    pub start: u32,
    pub end: u32,
}

impl IdRange {
    pub const fn new(start: u32, end: u32) -> Self {
        Self { start, end }
    }
    
    pub fn contains(&self, id: u32) -> bool {
        id >= self.start && id <= self.end
    }
    
    pub fn iter(&self) -> impl Iterator<Item = u32> + '_ {
        self.start..=self.end
    }
    
    pub fn len(&self) -> usize {
        (self.end - self.start + 1) as usize
    }
}

/// Known ID ranges for CSDb entities (as of 2024-2025).
/// These are approximate and will grow over time.
pub mod known_ranges {
    use super::IdRange;
    
    // Release ID ranges by type
    pub const DEMO_RANGE: IdRange = IdRange::new(1, 30000);
    pub const MUSIC_RANGE: IdRange = IdRange::new(1, 40000);
    pub const GAME_RANGE: IdRange = IdRange::new(1, 50000);
    
    // Global ranges
    pub const GROUP_RANGE: IdRange = IdRange::new(1, 5000);
    pub const SCENER_RANGE: IdRange = IdRange::new(1, 20000);
    pub const EVENT_RANGE: IdRange = IdRange::new(1, 2000);
    
    // Combined range for all releases
    pub const ALL_RELEASE_RANGE: IdRange = IdRange::new(1, 50000);
}

// CSDb API endpoints
pub const CSDB_BASE: &str = "https://csdb.dk";
pub const WEBSERVICE_BASE: &str = "https://csdb.dk/webservice/";

// Rate limiting constants
const MAX_CONCURRENT_REQUESTS: usize = 5;
const REQUEST_DELAY_MS: u64 = 100;

/// Rate limiter for CSDb API requests.
pub struct RateLimiter {
    semaphore: Arc<Semaphore>,
    delay: Duration,
    max_concurrent: usize,
    delay_ms: u64,
}

impl RateLimiter {
    /// Create a new rate limiter.
    pub fn new(max_concurrent: usize, delay_ms: u64) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(max_concurrent)),
            delay: Duration::from_millis(delay_ms),
            max_concurrent,
            delay_ms,
        }
    }
    
    /// Default rate limiter for CSDb.
    pub fn default() -> Self {
        Self::new(MAX_CONCURRENT_REQUESTS, REQUEST_DELAY_MS)
    }
    
    /// Acquire a permit to make a request.
    pub async fn acquire(&self) -> RateLimitPermit {
        let permit = self.semaphore.clone().acquire_owned().await.unwrap();
        if self.delay > Duration::ZERO {
            tokio::time::sleep(self.delay).await;
        }
        RateLimitPermit::new(permit)
    }
}

impl Clone for RateLimiter {
    fn clone(&self) -> Self {
        Self::new(self.max_concurrent, self.delay_ms)
    }
}

/// RAII guard for rate limiting.
pub struct RateLimitPermit {
    inner: tokio::sync::OwnedSemaphorePermit,
}

impl RateLimitPermit {
    fn new(permit: tokio::sync::OwnedSemaphorePermit) -> Self {
        Self { inner: permit }
    }
}

impl Drop for RateLimitPermit {
    fn drop(&mut self) {
        // Permit is automatically released when dropped
    }
}

/// Webservice endpoint builder.
pub fn webservice_url(ty: &str, id: u32, depth: u8) -> String {
    format!("{}?type={}&id={}&depth={}", WEBSERVICE_BASE, ty, id, depth)
}

/// Toplist URL builder.
pub fn toplist_url(ty: &str, subtype: &str) -> String {
    format!("{}?type={}&subtype={}", format!("{}/toplist.php", CSDB_BASE), ty, subtype)
}

/// Build the URL for a specific release toplist.
pub fn demo_toplist_url() -> String {
    toplist_url("release", "(1)") // C64 Demo
}

/// Build the URL for the music toplist.
pub fn music_toplist_url() -> String {
    toplist_url("release", "(7)") // C64 Music
}

/// Build the URL for the game toplist.
pub fn game_toplist_url() -> String {
    toplist_url("release", "(2)") // C64 Game
}

/// Collection configuration.
#[derive(Debug, Clone)]
pub struct CollectionConfig {
    /// Maximum number of concurrent requests
    pub max_concurrent: usize,
    /// Delay between requests in milliseconds
    pub delay_ms: u64,
    /// Timeout for individual requests in seconds
    pub timeout_secs: u64,
    /// User agent string
    pub user_agent: String,
    /// Whether to use persistent caching
    pub use_cache: bool,
}

impl Default for CollectionConfig {
    fn default() -> Self {
        Self {
            max_concurrent: MAX_CONCURRENT_REQUESTS,
            delay_ms: REQUEST_DELAY_MS,
            timeout_secs: 30,
            user_agent: "breadbin/1.0".to_string(),
            use_cache: true,
        }
    }
}

/// Data collection statistics.
#[derive(Debug, Clone, Default)]
pub struct CollectionStats {
    pub groups_fetched: usize,
    pub sceners_fetched: usize,
    pub events_fetched: usize,
    pub releases_fetched: usize,
    pub errors: Vec<String>,
}

impl CollectionStats {
    pub fn new() -> Self {
        Self::default()
    }
    
    pub fn add_error(&mut self, error: String) {
        self.errors.push(error);
    }
}

/// Main data collection entry point.
pub async fn collect_all_data(config: &CollectionConfig) -> Result<CollectionStats, String> {
    let rate_limiter = RateLimiter::new(config.max_concurrent, config.delay_ms);
    let stats = CollectionStats::new();
    
    // For now, we'll implement a simplified version
    // A full implementation would:
    // 1. Fetch all groups
    // 2. Fetch all sceners
    // 3. Fetch all events
    // 4. Fetch all releases with full metadata
    
    Ok(stats)
}

/// Fetch a single release with full details.
pub async fn fetch_release(id: u32, depth: u8) -> Result<Release, String> {
    let url = webservice_url("release", id, depth);
    let xml = core::fetch(&url, &[])?;
    parser::parse_release(&String::from_utf8_lossy(&xml), id)
}

/// Fetch a group by ID.
pub async fn fetch_group(id: u32) -> Result<Group, String> {
    let url = webservice_url("group", id, 1);
    let xml = core::fetch(&url, &[])?;
    parser::parse_group(&String::from_utf8_lossy(&xml))
}

/// Fetch a scener by ID.
pub async fn fetch_scener(id: u32) -> Result<Scener, String> {
    let url = webservice_url("scener", id, 1);
    let xml = core::fetch(&url, &[])?;
    parser::parse_scener(&String::from_utf8_lossy(&xml))
}

/// Fetch an event by ID.
pub async fn fetch_event(id: u32) -> Result<Event, String> {
    let url = webservice_url("event", id, 1);
    let xml = core::fetch(&url, &[])?;
    parser::parse_event(&String::from_utf8_lossy(&xml))
}

/// Fetch the demo toplist.
pub async fn fetch_demo_toplist(limit: usize) -> Result<Vec<(u32, String, String, f32)>, String> {
    let url = demo_toplist_url();
    let html = core::fetch(&url, &[])?;
    let ranked = core::parse_toplist(&String::from_utf8_lossy(&html));
    Ok(ranked.into_iter().take(limit).collect())
}

/// Fetch the music toplist.
pub async fn fetch_music_toplist(limit: usize) -> Result<Vec<(u32, String, String, f32)>, String> {
    let url = music_toplist_url();
    let html = core::fetch(&url, &[])?;
    let ranked = core::parse_toplist(&String::from_utf8_lossy(&html));
    Ok(ranked.into_iter().take(limit).collect())
}

/// Sync CSDb data to the local database.
/// Fetches releases from CSDb and stores them in the SQLite database for fast searching.
pub async fn sync_csdb_to_database(
    config: &CollectionConfig,
    release_ids: Vec<u32>,
) -> Result<CollectionStats, String> {
    let rate_limiter = RateLimiter::new(config.max_concurrent, config.delay_ms);
    let mut stats = CollectionStats::new();
    let mut errors = Vec::new();

    // Releases reference release_types by id; seed them before saving.
    diesel_db::initialize_default_release_types()?;

    // Fetch and save each release
    for id in release_ids {
        let _permit = rate_limiter.acquire().await;

        match fetch_release(id, 2).await {
            Ok(release) => {
                if let Err(e) = diesel_db::save_release(&release) {
                    errors.push(format!("Failed to save release {}: {}", id, e));
                } else {
                    stats.releases_fetched += 1;
                }
            }
            Err(e) => {
                errors.push(format!("Failed to fetch release {}: {}", id, e));
            }
        }
    }

    stats.errors = errors;
    Ok(stats)
}

/// Sync top-rated releases from CSDb to the local database.
/// This is a good starting point for building a searchable collection.
pub async fn sync_top_releases(
    config: &CollectionConfig,
    count: usize,
) -> Result<CollectionStats, String> {
    // Fetch top demos
    let demos = fetch_demo_toplist(count).await?;
    let demo_ids: Vec<u32> = demos.iter().map(|(id, _, _, _)| *id).collect();
    
    // Fetch top music
    let music = fetch_music_toplist(count).await?;
    let music_ids: Vec<u32> = music.iter().map(|(id, _, _, _)| *id).collect();
    
    // Combine and deduplicate
    let mut all_ids = demo_ids;
    all_ids.extend(music_ids);
    all_ids.sort();
    all_ids.dedup();
    
    // Sync to database
    sync_csdb_to_database(config, all_ids).await
}

/// Initialize the CSDb database with a basic set of data.
/// Fetches top releases and stores them for searching.
pub async fn initialize_csdb_database() -> Result<CollectionStats, String> {
    let config = CollectionConfig::default();
    sync_top_releases(&config, 100).await
}

/// Convenience function for searching CSDb from breadbin-core.
/// This provides a simple interface for CLI usage.
pub fn search_csdb(query: &str, limit: usize) -> Result<Vec<(u32, String)>, String> {
    diesel_db::search_by_name(query, limit)
}

/// Check if CSDb database exists and has data.
pub fn has_csdb_data() -> bool {
    diesel_db::has_data()
}

/// Get the top N releases by rating from the CSDb database.
pub fn get_top_releases(limit: usize) -> Result<Vec<Release>, String> {
    diesel_db::get_top_releases(limit)
}

/// Get the top 3 releases by rating from the CSDb database.
pub fn get_top_3_releases() -> Result<Vec<crate::database::Release>, String> {
    get_top_releases(3)
}

// Re-export commonly used items
pub use fetcher::*;
pub use parser::*;
