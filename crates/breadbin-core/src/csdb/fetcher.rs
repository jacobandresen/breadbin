// fetcher - Async data collection engine for CSDb content.
//
// This module provides the main data fetching capabilities for collecting
// demos, tunes, groups, sceners, and events from CSDb with rate limiting,
// caching, and efficient parallel requests.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use tokio::fs;
use tokio::sync::Semaphore;

use crate::core;
use crate::database::models::*;

use super::{parser, RateLimiter, CollectionConfig, CollectionStats, IdRange, known_ranges};

/// HTTP client wrapper with timeout and user agent.
pub struct CsdbClient {
    rate_limiter: RateLimiter,
    timeout: Duration,
    user_agent: String,
}

impl CsdbClient {
    pub fn new(config: &CollectionConfig) -> Self {
        Self {
            rate_limiter: RateLimiter::new(config.max_concurrent, config.delay_ms),
            timeout: Duration::from_secs(config.timeout_secs),
            user_agent: config.user_agent.clone(),
        }
    }
    
    pub async fn fetch(&self, url: &str) -> Result<Vec<u8>, String> {
        let _permit = self.rate_limiter.acquire().await;
        
        let headers = [("User-Agent", self.user_agent.as_str())];
        
        match tokio::time::timeout(self.timeout, async {
            core::fetch(url, &headers)
        }).await {
            Ok(Ok(data)) => Ok(data),
            Ok(Err(e)) => Err(format!("Fetch error for {}: {}", url, e)),
            Err(_) => Err(format!("Timeout fetching {} after {} seconds", url, self.timeout.as_secs())),
        }
    }
}

/// Cache for storing fetched data to avoid redundant requests.
pub struct FetchCache {
    cache_dir: PathBuf,
    entries: HashMap<String, CacheEntry>,
}

#[derive(Debug, Clone)]
struct CacheEntry {
    data: Vec<u8>,
    timestamp: SystemTime,
    etag: Option<String>,
}

impl FetchCache {
    pub fn new(cache_dir: PathBuf) -> Self {
        Self {
            cache_dir,
            entries: HashMap::new(),
        }
    }
    
    pub async fn get(&mut self, url: &str) -> Option<Vec<u8>> {
        if let Some(entry) = self.entries.get(url) {
            // Check if cache is still valid (24 hours)
            if let Ok(elapsed) = entry.timestamp.elapsed() {
                if elapsed.as_secs() < 86400 { // 24 hours
                    return Some(entry.data.clone());
                }
            }
        }
        
        // Try to load from disk cache
        self.load_from_disk(url).await
    }
    
    pub async fn set(&mut self, url: &str, data: Vec<u8>) {
        let entry = CacheEntry {
            data: data.clone(),
            timestamp: SystemTime::now(),
            etag: None,
        };
        self.entries.insert(url.to_string(), entry);
        
        // Save to disk
        self.save_to_disk(url, &data).await;
    }
    
    async fn load_from_disk(&self, url: &str) -> Option<Vec<u8>> {
        let filename = self.cache_filename(url);
        if let Ok(data) = fs::read(&filename).await {
            // Check file age
            if let Ok(metadata) = fs::metadata(&filename).await {
                if let Ok(modified) = metadata.modified() {
                    if let Ok(elapsed) = modified.elapsed() {
                        if elapsed.as_secs() < 86400 {
                            return Some(data);
                        }
                    }
                }
            }
        }
        None
    }
    
    async fn save_to_disk(&self, url: &str, data: &[u8]) {
        let filename = self.cache_filename(url);
        if let Some(parent) = filename.parent() {
            if let Err(e) = fs::create_dir_all(parent).await {
                log::warn!("Failed to create cache directory {}: {}", parent.display(), e);
                return;
            }
        }
        if let Err(e) = fs::write(&filename, data).await {
            log::warn!("Failed to save cache for {}: {}", url, e);
        }
    }
    
    fn cache_filename(&self, url: &str) -> PathBuf {
        let hash = md5::compute(url);
        let hex_hash = format!("{:x}", hash);
        self.cache_dir.join(format!("{}.cache", hex_hash))
    }
}



/// Fetch strategy for data collection.
#[derive(Debug, Clone)]
pub enum FetchStrategy {
    /// Fetch everything in known ranges
    Full,
    /// Fetch only top-rated items from toplists
    ToplistsOnly,
    /// Fetch specific ID ranges
    SpecificRanges(Vec<IdRange>),
    /// Fetch only missing data (requires existing database)
    MissingOnly,
}

/// Data collection engine.
pub struct DataCollector {
    client: CsdbClient,
    cache: Option<FetchCache>,
    config: CollectionConfig,
    stats: CollectionStats,
}

impl DataCollector {
    pub fn new(config: CollectionConfig) -> Self {
        let cache = if config.use_cache {
            let cache_dir = core::cache_path("csdb");
            Some(FetchCache::new(cache_dir))
        } else {
            None
        };
        
        Self {
            client: CsdbClient::new(&config),
            cache,
            config,
            stats: CollectionStats::new(),
        }
    }
    
    /// Fetch a single release with full details.
    pub async fn fetch_release(&mut self, id: u32, depth: u8) -> Result<Release, String> {
        let url = super::webservice_url("release", id, depth);
        
        // Try cache first
        if let Some(ref mut cache) = self.cache {
            if let Some(data) = cache.get(&url).await {
                let xml = String::from_utf8_lossy(&data);
                return parser::parse_release(&xml, id);
            }
        }
        
        let data = self.client.fetch(&url).await?;
        
        // Update cache
        if let Some(ref mut cache) = self.cache {
            cache.set(&url, data.clone()).await;
        }
        
        let xml = String::from_utf8_lossy(&data);
        parser::parse_release(&xml, id)
    }
    
    /// Fetch a single group.
    pub async fn fetch_group(&mut self, id: u32) -> Result<Group, String> {
        let url = super::webservice_url("group", id, 1);
        
        if let Some(ref mut cache) = self.cache {
            if let Some(data) = cache.get(&url).await {
                let xml = String::from_utf8_lossy(&data);
                return parser::parse_group(&xml);
            }
        }
        
        let data = self.client.fetch(&url).await?;
        
        if let Some(ref mut cache) = self.cache {
            cache.set(&url, data.clone()).await;
        }
        
        let xml = String::from_utf8_lossy(&data);
        parser::parse_group(&xml)
    }
    
    /// Fetch a single scener.
    pub async fn fetch_scener(&mut self, id: u32) -> Result<Scener, String> {
        let url = super::webservice_url("scener", id, 1);
        
        if let Some(ref mut cache) = self.cache {
            if let Some(data) = cache.get(&url).await {
                let xml = String::from_utf8_lossy(&data);
                return parser::parse_scener(&xml);
            }
        }
        
        let data = self.client.fetch(&url).await?;
        
        if let Some(ref mut cache) = self.cache {
            cache.set(&url, data.clone()).await;
        }
        
        let xml = String::from_utf8_lossy(&data);
        parser::parse_scener(&xml)
    }
    
    /// Fetch a single event.
    pub async fn fetch_event(&mut self, id: u32) -> Result<Event, String> {
        let url = super::webservice_url("event", id, 1);
        
        if let Some(ref mut cache) = self.cache {
            if let Some(data) = cache.get(&url).await {
                let xml = String::from_utf8_lossy(&data);
                return parser::parse_event(&xml);
            }
        }
        
        let data = self.client.fetch(&url).await?;
        
        if let Some(ref mut cache) = self.cache {
            cache.set(&url, data.clone()).await;
        }
        
        let xml = String::from_utf8_lossy(&data);
        parser::parse_event(&xml)
    }
    
    /// Fetch multiple releases concurrently.
    pub async fn fetch_releases_concurrently(
        &self,
        ids: &[u32],
        depth: u8,
    ) -> Vec<Result<Release, String>> {
        let mut results = Vec::with_capacity(ids.len());
        let mut tasks = Vec::new();
        
        // Use semaphore to limit concurrency
        let semaphore = Arc::new(Semaphore::new(self.config.max_concurrent));
        
        for &id in ids {
            let semaphore = semaphore.clone();
            let client = self.client.clone();
            let config = self.config.clone();
            
            tasks.push(tokio::spawn(async move {
                let _permit = semaphore.acquire_owned().await.unwrap();
                let url = super::webservice_url("release", id, depth);
                
                match tokio::time::timeout(
                    Duration::from_secs(config.timeout_secs),
                    client.fetch(&url)
                ).await {
                    Ok(Ok(data)) => {
                        let xml = String::from_utf8_lossy(&data);
                        parser::parse_release(&xml, id)
                    }
                    Ok(Err(e)) => Err(e),
                    Err(_) => Err(format!("Timeout fetching release {}", id)),
                }
            }));
        }
        
        for task in tasks {
            results.push(task.await.unwrap());
        }
        
        results
    }
    
    /// Fetch releases in a range with pagination.
    pub async fn fetch_release_range(
        &self,
        range: &IdRange,
        depth: u8,
        batch_size: usize,
    ) -> Vec<Release> {
        let mut all_releases = Vec::new();
        let mut errors = Vec::new();
        
        for batch_start in (range.start..=range.end).step_by(batch_size) {
            let batch_end = std::cmp::min(batch_start + batch_size as u32 - 1, range.end);
            let batch_ids: Vec<u32> = (batch_start..=batch_end).collect();
            
            let results = self.fetch_releases_concurrently(&batch_ids, depth).await;
            
            for result in results {
                match result {
                    Ok(release) => all_releases.push(release),
                    Err(e) => {
                        errors.push(e.clone());
                        log::warn!("Failed to fetch release: {}", e);
                    }
                }
            }
            
            // Rate limiting delay between batches
            if self.config.delay_ms > 0 {
                tokio::time::sleep(Duration::from_millis(self.config.delay_ms)).await;
            }
        }
        
        if !errors.is_empty() {
            log::warn!("Encountered {} errors while fetching releases", errors.len());
        }
        
        all_releases
    }
    
    /// Fetch all demos (C64 Demo, C64 Intro, etc.) from known ranges.
    pub async fn fetch_all_demos(&self) -> Vec<Release> {
        // Use depth=2 to get full details including credits, screenshots, downloads
        self.fetch_release_range(&known_ranges::DEMO_RANGE, 2, 10).await
    }
    
    /// Fetch all music from known ranges.
    pub async fn fetch_all_music(&self) -> Vec<Release> {
        self.fetch_release_range(&known_ranges::MUSIC_RANGE, 2, 10).await
    }
    
    /// Fetch demo toplist.
    pub async fn fetch_demo_toplist(&self, limit: usize) -> Result<Vec<(u32, String, String, f32)>, String> {
        let url = super::demo_toplist_url();
        let html = self.client.fetch(&url).await?;
        let ranked = core::parse_toplist(&String::from_utf8_lossy(&html));
        Ok(ranked.into_iter().take(limit).collect())
    }
    
    /// Fetch music toplist.
    pub async fn fetch_music_toplist(&self, limit: usize) -> Result<Vec<(u32, String, String, f32)>, String> {
        let url = super::music_toplist_url();
        let html = self.client.fetch(&url).await?;
        let ranked = core::parse_toplist(&String::from_utf8_lossy(&html));
        Ok(ranked.into_iter().take(limit).collect())
    }
    
    /// Fetch top-rated releases and then get full details for each.
    pub async fn fetch_top_demos_with_details(&self, limit: usize) -> Vec<Release> {
        let toplist = match self.fetch_demo_toplist(limit).await {
            Ok(toplist) => toplist,
            Err(e) => {
                log::error!("Failed to fetch demo toplist: {}", e);
                return Vec::new();
            }
        };
        
        let ids: Vec<u32> = toplist.iter().map(|(id, _, _, _)| *id).collect();
        let results = self.fetch_releases_concurrently(&ids, 2).await;
        
        let mut demos = Vec::new();
        for (i, result) in results.into_iter().enumerate() {
            match result {
                Ok(release) => demos.push(release),
                Err(e) => {
                    if let Some((id, _, _, _)) = toplist.get(i) {
                        log::warn!("Failed to fetch details for demo {}: {}", id, e);
                    }
                }
            }
        }
        
        demos
    }
    
    /// Fetch top-rated music and then get full details for each.
    pub async fn fetch_top_music_with_details(&self, limit: usize) -> Vec<Release> {
        let toplist = match self.fetch_music_toplist(limit).await {
            Ok(toplist) => toplist,
            Err(e) => {
                log::error!("Failed to fetch music toplist: {}", e);
                return Vec::new();
            }
        };
        
        let ids: Vec<u32> = toplist.iter().map(|(id, _, _, _)| *id).collect();
        let results = self.fetch_releases_concurrently(&ids, 2).await;
        
        let mut music = Vec::new();
        for (i, result) in results.into_iter().enumerate() {
            match result {
                Ok(release) => music.push(release),
                Err(e) => {
                    if let Some((id, _, _, _)) = toplist.get(i) {
                        log::warn!("Failed to fetch details for music {}: {}", id, e);
                    }
                }
            }
        }
        
        music
    }
    
    /// Collect all groups from releases.
    pub async fn collect_groups_from_releases(
        &self,
        releases: &[Release],
    ) -> Vec<Group> {
        let mut group_ids = HashSet::new();
        let mut groups = Vec::new();
        
        // Collect unique group IDs
        for release in releases {
            if let Some(id) = release.group_id {
                group_ids.insert(id);
            }
        }
        
        // Fetch all groups concurrently
        let group_ids: Vec<u32> = group_ids.into_iter().collect();
        let mut tasks = Vec::new();
        
        let semaphore = Arc::new(Semaphore::new(self.config.max_concurrent));
        
        for &id in &group_ids {
            let semaphore = semaphore.clone();
            let client = self.client.clone();
            let config = self.config.clone();
            
            tasks.push(tokio::spawn(async move {
                let _permit = semaphore.acquire_owned().await.unwrap();
                let url = super::webservice_url("group", id, 1);
                
                match tokio::time::timeout(
                    Duration::from_secs(config.timeout_secs),
                    client.fetch(&url)
                ).await {
                    Ok(Ok(data)) => {
                        let xml = String::from_utf8_lossy(&data);
                        parser::parse_group(&xml)
                    }
                    Ok(Err(e)) => Err(e),
                    Err(_) => Err(format!("Timeout fetching group {}", id)),
                }
            }));
        }
        
        for task in tasks {
            match task.await.unwrap() {
                Ok(group) => groups.push(group),
                Err(e) => log::warn!("Failed to fetch group: {}", e),
            }
        }
        
        groups
    }
    
    /// Collect all sceners from releases.
    pub async fn collect_sceners_from_releases(
        &self,
        releases: &[Release],
    ) -> Vec<Scener> {
        let mut scener_ids = HashSet::new();
        let mut sceners = Vec::new();
        
        // Collect unique scener IDs from credits
        for release in releases {
            for credit in &release.credits {
                if let Some(id) = credit.scener_id {
                    scener_ids.insert(id);
                }
            }
        }
        
        // Fetch all sceners concurrently
        let scener_ids: Vec<u32> = scener_ids.into_iter().collect();
        let mut tasks = Vec::new();
        
        let semaphore = Arc::new(Semaphore::new(self.config.max_concurrent));
        
        for &id in &scener_ids {
            let semaphore = semaphore.clone();
            let client = self.client.clone();
            let config = self.config.clone();
            
            tasks.push(tokio::spawn(async move {
                let _permit = semaphore.acquire_owned().await.unwrap();
                let url = super::webservice_url("scener", id, 1);
                
                match tokio::time::timeout(
                    Duration::from_secs(config.timeout_secs),
                    client.fetch(&url)
                ).await {
                    Ok(Ok(data)) => {
                        let xml = String::from_utf8_lossy(&data);
                        parser::parse_scener(&xml)
                    }
                    Ok(Err(e)) => Err(e),
                    Err(_) => Err(format!("Timeout fetching scener {}", id)),
                }
            }));
        }
        
        for task in tasks {
            match task.await.unwrap() {
                Ok(scener) => sceners.push(scener),
                Err(e) => log::warn!("Failed to fetch scener: {}", e),
            }
        }
        
        sceners
    }
    
    /// Collect all events from releases.
    pub async fn collect_events_from_releases(
        &self,
        releases: &[Release],
    ) -> Vec<Event> {
        let mut event_ids = HashSet::new();
        let mut events = Vec::new();
        
        // Collect unique event IDs
        for release in releases {
            if let Some(id) = release.event_id {
                event_ids.insert(id);
            }
        }
        
        // Fetch all events concurrently
        let event_ids: Vec<u32> = event_ids.into_iter().collect();
        let mut tasks = Vec::new();
        
        let semaphore = Arc::new(Semaphore::new(self.config.max_concurrent));
        
        for &id in &event_ids {
            let semaphore = semaphore.clone();
            let client = self.client.clone();
            let config = self.config.clone();
            
            tasks.push(tokio::spawn(async move {
                let _permit = semaphore.acquire_owned().await.unwrap();
                let url = super::webservice_url("event", id, 1);
                
                match tokio::time::timeout(
                    Duration::from_secs(config.timeout_secs),
                    client.fetch(&url)
                ).await {
                    Ok(Ok(data)) => {
                        let xml = String::from_utf8_lossy(&data);
                        parser::parse_event(&xml)
                    }
                    Ok(Err(e)) => Err(e),
                    Err(_) => Err(format!("Timeout fetching event {}", id)),
                }
            }));
        }
        
        for task in tasks {
            match task.await.unwrap() {
                Ok(event) => events.push(event),
                Err(e) => log::warn!("Failed to fetch event: {}", e),
            }
        }
        
        events
    }
    
    /// Main data collection method - collects everything.
    pub async fn collect_all(
        &mut self,
        demo_limit: usize,
        music_limit: usize,
    ) -> Result<CollectionStats, String> {
        // Fetch top demos
        let demos = self.fetch_top_demos_with_details(demo_limit).await;
        
        // Fetch top music
        let music = self.fetch_top_music_with_details(music_limit).await;
        
        // Collect all releases
        let all_releases: Vec<Release> = demos.into_iter()
            .chain(music.into_iter())
            .collect();
        
        // Collect related entities
        let groups = self.collect_groups_from_releases(&all_releases).await;
        let sceners = self.collect_sceners_from_releases(&all_releases).await;
        let events = self.collect_events_from_releases(&all_releases).await;
        
        // Update stats
        self.stats.groups_fetched = groups.len();
        self.stats.sceners_fetched = sceners.len();
        self.stats.events_fetched = events.len();
        self.stats.releases_fetched = all_releases.len();
        
        Ok(self.stats.clone())
    }
    
    /// Get current collection statistics.
    pub fn stats(&self) -> &CollectionStats {
        &self.stats
    }
    
    /// Add an error to the collection statistics.
    pub fn add_error(&mut self, error: String) {
        self.stats.add_error(error);
    }
}

impl Clone for CsdbClient {
    fn clone(&self) -> Self {
        Self {
            rate_limiter: self.rate_limiter.clone(),
            timeout: self.timeout,
            user_agent: self.user_agent.clone(),
        }
    }
}

impl Clone for FetchCache {
    fn clone(&self) -> Self {
        Self {
            cache_dir: self.cache_dir.clone(),
            entries: HashMap::new(), // Don't clone cache entries
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_id_range() {
        let range = IdRange::new(10, 20);
        assert!(range.contains(15));
        assert!(!range.contains(5));
        assert!(!range.contains(25));
        assert_eq!(range.len(), 11);
    }
    
    #[test]
    fn test_id_range_iter() {
        let range = IdRange::new(1, 5);
        let ids: Vec<u32> = range.iter().collect();
        assert_eq!(ids, vec![1, 2, 3, 4, 5]);
    }
    
    #[tokio::test]
    async fn test_fetch_single_release() {
        let config = CollectionConfig::default();
        let mut collector = DataCollector::new(config);
        
        // Test with a known demo ID (this is a real demo on CSDb)
        // Note: This test requires network access and may fail if CSDb is down
        let result = collector.fetch_release(250000, 1).await;
        
        // We don't assert on the result as it depends on network availability
        // Just ensure it doesn't panic
        let _ = result;
    }
}
