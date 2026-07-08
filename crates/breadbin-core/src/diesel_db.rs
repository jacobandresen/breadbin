// Diesel ORM database module for Breadbin.
//
// This module provides type-safe database access using Diesel ORM.
// It coexists with the legacy rusqlite-based database module for gradual migration.

use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager, Pool, PooledConnection};
use diesel::sqlite::SqliteConnection;
use diesel::dsl::count_star;
use chrono::NaiveDateTime;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;

use crate::schema::*;

// ============================================================================
// Schema Import
// ============================================================================



// ============================================================================
// Models
// ============================================================================

// Database Metadata
#[derive(Debug, Clone, Queryable, Insertable)]
#[diesel(table_name = crate::schema::_meta)]
pub struct Meta {
    pub key: Option<String>,
    pub value: String,
}

// Release Types
#[derive(Debug, Clone, Queryable, Identifiable, Insertable, AsChangeset)]
#[diesel(table_name = crate::schema::release_types)]
pub struct ReleaseType {
    pub id: Option<i32>,
    pub name: String,
    pub description: Option<String>,
}

// Groups
#[derive(Debug, Clone, Queryable, Identifiable, Insertable, AsChangeset)]
#[diesel(table_name = crate::schema::groups)]
pub struct GroupRow {
    pub id: Option<i32>,
    pub name: String,
    pub short: Option<String>,
    pub base_country: Option<String>,
    pub founded_month: Option<i32>,
    pub founded_year: Option<i32>,
    pub website: Option<String>,
    pub rating: Option<f32>,
    pub grouptypes: Option<String>,
    pub slogans: Option<String>,
    pub updated_at: Option<NaiveDateTime>,
}

// Sceners
#[derive(Debug, Clone, Queryable, Identifiable, Insertable, AsChangeset)]
#[diesel(table_name = crate::schema::sceners)]
pub struct ScenerRow {
    pub id: Option<i32>,
    pub country: Option<String>,
    pub trivia: Option<String>,
    pub handle_story: Option<String>,
    pub updated_at: Option<NaiveDateTime>,
}

// Scener Handles
#[derive(Debug, Clone, Queryable, Identifiable, Insertable, AsChangeset)]
#[diesel(table_name = crate::schema::scener_handles)]
pub struct ScenerHandleRow {
    pub id: Option<i32>,
    pub scener_id: i32,
    pub handle: String,
    pub aka: Option<String>,
    pub currently_used: Option<bool>,
    pub handle_story: Option<String>,
}

// Events
#[derive(Debug, Clone, Queryable, Identifiable, Insertable, AsChangeset)]
#[diesel(table_name = crate::schema::events)]
pub struct EventRow {
    pub id: Option<i32>,
    pub name: String,
    pub event_type: Option<String>,
    pub aka: Option<String>,
    pub start_day: Option<i32>,
    pub start_month: Option<i32>,
    pub start_year: Option<i32>,
    pub end_day: Option<i32>,
    pub end_month: Option<i32>,
    pub end_year: Option<i32>,
    pub address: Option<String>,
    pub zip: Option<String>,
    pub city: Option<String>,
    pub country: Option<String>,
    pub website: Option<String>,
    pub updated_at: Option<NaiveDateTime>,
}

// Releases
#[derive(Debug, Clone, Queryable, Identifiable, Insertable, AsChangeset)]
#[diesel(table_name = crate::schema::releases)]
pub struct ReleaseRow {
    pub id: Option<i32>,
    pub name: String,
    pub release_type_id: i32,
    pub group_id: Option<i32>,
    pub rating: Option<f32>,
    pub release_day: Option<i32>,
    pub release_month: Option<i32>,
    pub release_year: Option<i32>,
    pub event_id: Option<i32>,
    pub compo: Option<String>,
    pub place: Option<i32>,
    pub screenshot_url: Option<String>,
    pub screenshot_urls: Option<String>,
    pub download_count: Option<i32>,
    pub website: Option<String>,
    pub created_at: Option<NaiveDateTime>,
    pub updated_at: Option<NaiveDateTime>,
}

// Release Credits
#[derive(Debug, Clone, Queryable, Identifiable, Insertable, AsChangeset)]
#[diesel(table_name = crate::schema::release_credits)]
pub struct ReleaseCreditRow {
    pub id: Option<i32>,
    pub release_id: i32,
    pub scener_id: i32,
    pub handle: String,
    pub credit_type: String,
    pub position: Option<i32>,
    pub functions: Option<String>,
}

// Download Links
#[derive(Debug, Clone, Queryable, Identifiable, Insertable, AsChangeset)]
#[diesel(table_name = crate::schema::release_downloads)]
pub struct DownloadLinkRow {
    pub id: Option<i32>,
    pub release_id: i32,
    pub url: String,
    pub counter_url: Option<String>,
    pub downloads: Option<i32>,
    pub status: Option<String>,
    pub is_internal: Option<bool>,
    pub file_type: Option<String>,
}

// SID Files
#[derive(Debug, Clone, Queryable, Identifiable, Insertable, AsChangeset)]
#[diesel(table_name = crate::schema::release_sids)]
pub struct SidFileRow {
    pub id: Option<i32>,
    pub release_id: i32,
    pub sid_id: Option<i32>,
    pub hvsc_path: Option<String>,
    pub name: String,
    pub author: Option<String>,
    pub released: Option<String>,
    pub load_addr: Option<i32>,
    pub init_addr: Option<i32>,
    pub play_addr: Option<i32>,
    pub num_songs: Option<i32>,
    pub default_song: Option<i32>,
    pub sid_model: Option<String>,
    pub clock_speed: Option<String>,
    pub data_size: Option<i32>,
}

// IA Mappings
#[derive(Debug, Clone, Queryable, Identifiable, Insertable, AsChangeset)]
#[diesel(table_name = crate::schema::ia_mappings)]
pub struct IAMappingRow {
    pub id: Option<i32>,
    pub release_id: i32,
    pub ia_identifier: String,
    pub ia_title: Option<String>,
    pub ia_collection: Option<String>,
    pub match_confidence: Option<f32>,
    pub last_checked: Option<NaiveDateTime>,
}

// Statistics
#[derive(Debug, Clone)]
pub struct DatabaseStats {
    pub table_counts: HashMap<String, i64>,
    pub total_records: i64,
}

// Result types
pub type QueryResult<T> = Result<T, String>;

// ============================================================================
// Database Path and Connection Management
// ============================================================================

/// Database connection pool type alias.
pub type DbPool = Pool<ConnectionManager<SqliteConnection>>;

/// Global connection pool (lazy initialized).
static POOL: OnceLock<DbPool> = OnceLock::new();

/// Get the path to the Diesel database file.
pub fn diesel_db_path() -> PathBuf {
    // Use the crate directory as the base
    // This ensures the database is found where Diesel CLI created it
    PathBuf::from("csdb_cache_diesel.sqlite")
}

/// Initialize the connection pool.
pub fn initialize_pool() -> QueryResult<()> {
    use diesel::r2d2::Pool;
    
    POOL.get_or_init(|| {
        let path = diesel_db_path();
        
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                panic!("Failed to create directory: {}", e);
            }
        }
        
        let database_url = path
            .to_str()
            .expect("Failed to get database path");
        
        let manager = ConnectionManager::<SqliteConnection>::new(database_url.to_string());
        Pool::builder()
            .max_size(1) // SQLite limitation
            .build(manager)
            .expect("Failed to build connection pool")
    });
    
    Ok(())
}

/// Get a connection from the pool.
pub fn get_connection() -> QueryResult<PooledConnection<ConnectionManager<SqliteConnection>>> {
    POOL.get()
        .ok_or("Connection pool not initialized")?
        .get()
        .map_err(|e| format!("Failed to get connection: {}", e))
}

/// Establish a direct connection.
pub fn establish_connection() -> QueryResult<SqliteConnection> {
    let path = diesel_db_path();
    
    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("Failed to create directory: {}", e))?;
    }
    
    let database_url = path
        .to_str()
        .ok_or("Failed to get database path")?;
    
    SqliteConnection::establish(database_url)
        .map_err(|e| format!("Failed to establish connection: {}", e))
}

/// Check if the Diesel database exists.
pub fn diesel_database_exists() -> bool {
    diesel_db_path().exists()
}

// ============================================================================
// Helper function to get a connection and execute a query
// ============================================================================

/// Get a connection from the pool, initializing if necessary.
fn get_conn() -> QueryResult<PooledConnection<ConnectionManager<SqliteConnection>>> {
    // Ensure pool is initialized
    initialize_pool()?;
    
    // Get a connection from the pool
    get_connection()
}

// ============================================================================
// Database Metadata Operations
// ============================================================================

/// Get a metadata value by key.
pub fn get_meta_value(meta_key: &str) -> QueryResult<Option<String>> {
    use crate::schema::_meta::dsl::*;
    
    let mut conn = get_conn()?;
    
    _meta
        .filter(key.eq(meta_key))
        .select(value)
        .first::<String>(&mut conn)
        .optional()
        .map(|v| v.map(|s| s.to_string()))
        .map_err(|e| format!("Failed to get meta value: {}", e))
}

/// Set a metadata value.
pub fn set_meta_value(meta_key: &str, meta_value: &str) -> QueryResult<()> {
    use crate::schema::_meta::dsl::*;
    
    let mut conn = get_conn()?;
    
    let meta = Meta {
        key: Some(meta_key.to_string()),
        value: meta_value.to_string(),
    };
    
    diesel::insert_into(_meta)
        .values(&meta)
        .on_conflict(key)
        .do_update()
        .set(value.eq(meta_value))
        .execute(&mut conn)
        .map_err(|e| format!("Failed to set meta value: {}", e))?;
    
    Ok(())
}

// ============================================================================
// Release Type Operations
// ============================================================================

/// Save a release type.
pub fn save_release_type(release_type: &ReleaseType) -> QueryResult<()> {
    use crate::schema::release_types::dsl::*;
    
    let mut conn = get_conn()?;
    
    diesel::insert_into(release_types)
        .values(release_type)
        .on_conflict(id)
        .do_update()
        .set(release_type)
        .execute(&mut conn)
        .map_err(|e| format!("Failed to save release type: {}", e))?;
    
    Ok(())
}

/// Get a release type by ID.
pub fn get_release_type(type_id: i32) -> QueryResult<Option<ReleaseType>> {
    use crate::schema::release_types::dsl::*;
    
    let mut conn = get_conn()?;
    
    release_types
        .filter(id.eq(Some(type_id)))
        .first::<ReleaseType>(&mut conn)
        .optional()
        .map_err(|e| format!("Failed to get release type: {}", e))
}

/// Get all release types.
pub fn get_all_release_types() -> QueryResult<Vec<ReleaseType>> {
    use crate::schema::release_types::dsl::*;
    
    let mut conn = get_conn()?;
    
    release_types
        .load::<ReleaseType>(&mut conn)
        .map_err(|e| format!("Failed to load release types: {}", e))
}

/// Initialize default release types.
pub fn initialize_default_release_types() -> QueryResult<()> {
    let default_types = vec![
        (1, "C64 Demo", Some("Commodore 64 Demo")),
        (2, "C64 Game", Some("Commodore 64 Game")),
        (7, "C64 Music", Some("Commodore 64 Music/SID")),
        (3, "C64 Crack", Some("Cracked Game")),
        (4, "C64 Trainer", Some("Trainer for Cracked Game")),
        (5, "C64 Intro", Some("Intro (usually for a crack)")),
        (6, "C64 Graphics", Some("Graphics Release")),
        (8, "C64 Music Collection", Some("Collection of SID tunes")),
        (9, "C64 Demo Collection", Some("Collection of demos")),
        (10, "C64 Diskmag", Some("Disk Magazine")),
        (11, "C64 Papermag", Some("Paper Magazine")),
        (12, "C64 Tool", Some("Utility or Tool")),
        (13, "C64 Game Preview", Some("Preview of a game")),
        (14, "C64 Invitation", Some("Party Invitation")),
        (15, "C64 One-File Demo", Some("Single-file demo")),
        (16, "C64 4K Intro", Some("4KB Intro")),
        (17, "C64 8K Intro", Some("8KB Intro")),
        (18, "C64 16K Intro", Some("16KB Intro")),
        (19, "C64 32K Intro", Some("32KB Intro")),
        (20, "C64 64K Intro", Some("64KB Intro")),
    ];
    
    for (id, name, description) in default_types {
        let release_type = ReleaseType {
            id: Some(id),
            name: name.to_string(),
            description: description.map(|s| s.to_string()),
        };
        save_release_type(&release_type)?;
    }
    
    Ok(())
}

// ============================================================================
// Group Operations
// ============================================================================

/// Save a group.
pub fn save_group(group: &GroupRow) -> QueryResult<()> {
    use crate::schema::groups::dsl::*;
    
    let mut conn = get_conn()?;
    
    diesel::insert_into(groups)
        .values(group)
        .on_conflict(id)
        .do_update()
        .set(group)
        .execute(&mut conn)
        .map_err(|e| format!("Failed to save group: {}", e))?;
    
    Ok(())
}

/// Get a group by ID.
pub fn get_group(group_id: i32) -> QueryResult<Option<GroupRow>> {
    use crate::schema::groups::dsl::*;
    
    let mut conn = get_conn()?;
    
    groups
        .filter(id.eq(Some(group_id)))
        .first::<GroupRow>(&mut conn)
        .optional()
        .map_err(|e| format!("Failed to get group: {}", e))
}

/// Get all groups.
pub fn get_all_groups() -> QueryResult<Vec<GroupRow>> {
    use crate::schema::groups::dsl::*;
    
    let mut conn = get_conn()?;
    
    groups
        .load::<GroupRow>(&mut conn)
        .map_err(|e| format!("Failed to load groups: {}", e))
}

// ============================================================================
// Scener Operations
// ============================================================================

/// Save a scener.
pub fn save_scener(scener: &ScenerRow) -> QueryResult<()> {
    use crate::schema::sceners::dsl::*;
    
    let mut conn = get_conn()?;
    
    diesel::insert_into(sceners)
        .values(scener)
        .on_conflict(id)
        .do_update()
        .set(scener)
        .execute(&mut conn)
        .map_err(|e| format!("Failed to save scener: {}", e))?;
    
    Ok(())
}

/// Get a scener by ID.
pub fn get_scener(scener_id: i32) -> QueryResult<Option<ScenerRow>> {
    use crate::schema::sceners::dsl::*;
    
    let mut conn = get_conn()?;
    
    sceners
        .filter(id.eq(Some(scener_id)))
        .first::<ScenerRow>(&mut conn)
        .optional()
        .map_err(|e| format!("Failed to get scener: {}", e))
}

/// Get all sceners.
pub fn get_all_sceners() -> QueryResult<Vec<ScenerRow>> {
    use crate::schema::sceners::dsl::*;
    
    let mut conn = get_conn()?;
    
    sceners
        .load::<ScenerRow>(&mut conn)
        .map_err(|e| format!("Failed to load sceners: {}", e))
}

// ============================================================================
// Event Operations
// ============================================================================

/// Save an event.
pub fn save_event(event: &EventRow) -> QueryResult<()> {
    use crate::schema::events::dsl::*;
    
    let mut conn = get_conn()?;
    
    diesel::insert_into(events)
        .values(event)
        .on_conflict(id)
        .do_update()
        .set(event)
        .execute(&mut conn)
        .map_err(|e| format!("Failed to save event: {}", e))?;
    
    Ok(())
}

/// Get an event by ID.
pub fn get_event(event_id: i32) -> QueryResult<Option<EventRow>> {
    use crate::schema::events::dsl::*;
    
    let mut conn = get_conn()?;
    
    events
        .filter(id.eq(Some(event_id)))
        .first::<EventRow>(&mut conn)
        .optional()
        .map_err(|e| format!("Failed to get event: {}", e))
}

/// Get all events.
pub fn get_all_events() -> QueryResult<Vec<EventRow>> {
    use crate::schema::events::dsl::*;
    
    let mut conn = get_conn()?;
    
    events
        .load::<EventRow>(&mut conn)
        .map_err(|e| format!("Failed to load events: {}", e))
}

// ============================================================================
// Release Operations
// ============================================================================

/// Save a release.
pub fn save_release(release: &ReleaseRow) -> QueryResult<()> {
    use crate::schema::releases::dsl::*;
    
    let mut conn = get_conn()?;
    
    diesel::insert_into(releases)
        .values(release)
        .on_conflict(id)
        .do_update()
        .set(release)
        .execute(&mut conn)
        .map_err(|e| format!("Failed to save release: {}", e))?;
    
    Ok(())
}

/// Get a release by ID.
pub fn get_release(release_id: i32) -> QueryResult<Option<ReleaseRow>> {
    use crate::schema::releases::dsl::*;
    
    let mut conn = get_conn()?;
    
    releases
        .filter(id.eq(Some(release_id)))
        .first::<ReleaseRow>(&mut conn)
        .optional()
        .map_err(|e| format!("Failed to get release: {}", e))
}

/// Get all releases.
pub fn get_all_releases() -> QueryResult<Vec<ReleaseRow>> {
    use crate::schema::releases::dsl::*;
    
    let mut conn = get_conn()?;
    
    releases
        .order(download_count.desc())
        .load::<ReleaseRow>(&mut conn)
        .map_err(|e| format!("Failed to load releases: {}", e))
}

/// Get top releases by download count.
pub fn get_top_releases(limit: i64) -> QueryResult<Vec<ReleaseRow>> {
    use crate::schema::releases::dsl::*;
    
    let mut conn = get_conn()?;
    
    releases
        .order(download_count.desc())
        .limit(limit)
        .load::<ReleaseRow>(&mut conn)
        .map_err(|e| format!("Failed to load top releases: {}", e))
}

// ============================================================================
// Scener Handle Operations
// ============================================================================

/// Save a scener handle.
pub fn save_scener_handle(scener_handle: &ScenerHandleRow) -> QueryResult<()> {
    use crate::schema::scener_handles::dsl::*;
    
    let mut conn = get_conn()?;
    
    // For scener_handles, use (scener_id, handle) as the conflict target
    // since id is auto-incrementing
    diesel::insert_into(scener_handles)
        .values(scener_handle)
        .on_conflict((scener_id, handle))
        .do_update()
        .set(scener_handle)
        .execute(&mut conn)
        .map_err(|e| format!("Failed to save scener handle: {}", e))?;
    
    Ok(())
}

/// Get scener handles by scener ID.
pub fn get_scener_handles(scener_id: i32) -> QueryResult<Vec<ScenerHandleRow>> {
    use crate::schema::scener_handles::dsl::*;
    
    let mut conn = get_conn()?;
    
    scener_handles
        .filter(scener_id.eq(scener_id))
        .load::<ScenerHandleRow>(&mut conn)
        .map_err(|e| format!("Failed to load scener handles: {}", e))
}

/// Get all scener handles.
pub fn get_all_scener_handles() -> QueryResult<Vec<ScenerHandleRow>> {
    use crate::schema::scener_handles::dsl::*;
    
    let mut conn = get_conn()?;
    
    scener_handles
        .load::<ScenerHandleRow>(&mut conn)
        .map_err(|e| format!("Failed to load all scener handles: {}", e))
}

// ============================================================================
// Release Credit Operations
// ============================================================================

/// Save a release credit.
pub fn save_release_credit(credit_row: &ReleaseCreditRow) -> QueryResult<()> {
    use crate::schema::release_credits::dsl::*;
    
    let mut conn = get_conn()?;
    
    // For release_credits, use (release_id, scener_id, credit_type, handle) as the conflict target
    // since id is auto-incrementing
    diesel::insert_into(release_credits)
        .values(credit_row)
        .on_conflict((release_id, scener_id, credit_type, handle))
        .do_update()
        .set(credit_row)
        .execute(&mut conn)
        .map_err(|e| format!("Failed to save release credit: {}", e))?;
    
    Ok(())
}

/// Get release credits by release ID.
pub fn get_release_credits(release_id: i32) -> QueryResult<Vec<ReleaseCreditRow>> {
    use crate::schema::release_credits::dsl::*;
    
    let mut conn = get_conn()?;
    
    release_credits
        .filter(release_id.eq(release_id))
        .load::<ReleaseCreditRow>(&mut conn)
        .map_err(|e| format!("Failed to load release credits: {}", e))
}

/// Get all release credits.
pub fn get_all_release_credits() -> QueryResult<Vec<ReleaseCreditRow>> {
    use crate::schema::release_credits::dsl::*;
    
    let mut conn = get_conn()?;
    
    release_credits
        .load::<ReleaseCreditRow>(&mut conn)
        .map_err(|e| format!("Failed to load all release credits: {}", e))
}

// ============================================================================
// Download Link Operations
// ============================================================================

/// Save a download link.
pub fn save_download_link(link_row: &DownloadLinkRow) -> QueryResult<()> {
    use crate::schema::release_downloads::dsl::*;
    
    let mut conn = get_conn()?;
    
    // For release_downloads, use (release_id, url) as the conflict target
    // since id is auto-incrementing
    diesel::insert_into(release_downloads)
        .values(link_row)
        .on_conflict((release_id, url))
        .do_update()
        .set(link_row)
        .execute(&mut conn)
        .map_err(|e| format!("Failed to save download link: {}", e))?;
    
    Ok(())
}

/// Get download links by release ID.
pub fn get_download_links(release_id: i32) -> QueryResult<Vec<DownloadLinkRow>> {
    use crate::schema::release_downloads::dsl::*;
    
    let mut conn = get_conn()?;
    
    release_downloads
        .filter(release_id.eq(release_id))
        .load::<DownloadLinkRow>(&mut conn)
        .map_err(|e| format!("Failed to load download links: {}", e))
}

/// Get all download links.
pub fn get_all_download_links() -> QueryResult<Vec<DownloadLinkRow>> {
    use crate::schema::release_downloads::dsl::*;
    
    let mut conn = get_conn()?;
    
    release_downloads
        .load::<DownloadLinkRow>(&mut conn)
        .map_err(|e| format!("Failed to load all download links: {}", e))
}

// ============================================================================
// SID File Operations
// ============================================================================

/// Save a SID file.
pub fn save_sid_file(sid_row: &SidFileRow) -> QueryResult<()> {
    use crate::schema::release_sids::dsl::*;
    
    let mut conn = get_conn()?;
    
    // For release_sids, use (release_id, sid_id) as the conflict target
    // since id is auto-incrementing
    diesel::insert_into(release_sids)
        .values(sid_row)
        .on_conflict((release_id, sid_id))
        .do_update()
        .set(sid_row)
        .execute(&mut conn)
        .map_err(|e| format!("Failed to save SID file: {}", e))?;
    
    Ok(())
}

/// Get SID files by release ID.
pub fn get_sid_files(release_id: i32) -> QueryResult<Vec<SidFileRow>> {
    use crate::schema::release_sids::dsl::*;
    
    let mut conn = get_conn()?;
    
    release_sids
        .filter(release_id.eq(release_id))
        .load::<SidFileRow>(&mut conn)
        .map_err(|e| format!("Failed to load SID files: {}", e))
}

/// Get all SID files.
pub fn get_all_sid_files() -> QueryResult<Vec<SidFileRow>> {
    use crate::schema::release_sids::dsl::*;
    
    let mut conn = get_conn()?;
    
    release_sids
        .load::<SidFileRow>(&mut conn)
        .map_err(|e| format!("Failed to load all SID files: {}", e))
}

// ============================================================================
// IA Mapping Operations
// ============================================================================

/// Save an IA mapping.
pub fn save_ia_mapping(mapping_row: &IAMappingRow) -> QueryResult<()> {
    use crate::schema::ia_mappings::dsl::*;
    
    let mut conn = get_conn()?;
    
    // For ia_mappings, use (release_id, ia_identifier) as the conflict target
    // since id is auto-incrementing
    diesel::insert_into(ia_mappings)
        .values(mapping_row)
        .on_conflict((release_id, ia_identifier))
        .do_update()
        .set(mapping_row)
        .execute(&mut conn)
        .map_err(|e| format!("Failed to save IA mapping: {}", e))?;
    
    Ok(())
}

/// Get IA mappings by release ID.
pub fn get_ia_mappings(release_id: i32) -> QueryResult<Vec<IAMappingRow>> {
    use crate::schema::ia_mappings::dsl::*;
    
    let mut conn = get_conn()?;
    
    ia_mappings
        .filter(release_id.eq(release_id))
        .load::<IAMappingRow>(&mut conn)
        .map_err(|e| format!("Failed to load IA mappings: {}", e))
}

/// Get all IA mappings.
pub fn get_all_ia_mappings() -> QueryResult<Vec<IAMappingRow>> {
    use crate::schema::ia_mappings::dsl::*;
    
    let mut conn = get_conn()?;
    
    ia_mappings
        .load::<IAMappingRow>(&mut conn)
        .map_err(|e| format!("Failed to load all IA mappings: {}", e))
}

// ============================================================================
// Database Management
// ============================================================================

/// Get database statistics.
pub fn get_stats() -> QueryResult<DatabaseStats> {
    let mut table_counts = HashMap::new();
    let mut total_records = 0i64;
    
    let tables = [
        "_meta", "release_types", "groups", "sceners", "scener_handles",
        "events", "releases", "release_credits", "release_downloads",
        "release_sids", "ia_mappings"
    ];
    
    for table in &tables {
        // Use execute and parse the result differently
        let count = count_records(table)?;
        table_counts.insert(table.to_string(), count);
        total_records += count;
    }
    
    Ok(DatabaseStats {
        table_counts,
        total_records,
    })
}

/// Clear all data from the database (except version metadata).
pub fn clear_database() -> QueryResult<()> {
    let mut conn = get_conn()?;
    
    diesel::sql_query("DELETE FROM scener_handles").execute(&mut conn).map_err(|e| e.to_string())?;
    diesel::sql_query("DELETE FROM release_credits").execute(&mut conn).map_err(|e| e.to_string())?;
    diesel::sql_query("DELETE FROM release_downloads").execute(&mut conn).map_err(|e| e.to_string())?;
    diesel::sql_query("DELETE FROM release_sids").execute(&mut conn).map_err(|e| e.to_string())?;
    diesel::sql_query("DELETE FROM ia_mappings").execute(&mut conn).map_err(|e| e.to_string())?;
    diesel::sql_query("DELETE FROM releases").execute(&mut conn).map_err(|e| e.to_string())?;
    diesel::sql_query("DELETE FROM sceners").execute(&mut conn).map_err(|e| e.to_string())?;
    diesel::sql_query("DELETE FROM groups").execute(&mut conn).map_err(|e| e.to_string())?;
    diesel::sql_query("DELETE FROM events").execute(&mut conn).map_err(|e| e.to_string())?;
    diesel::sql_query("DELETE FROM release_types").execute(&mut conn).map_err(|e| e.to_string())?;
    diesel::sql_query("DELETE FROM _meta WHERE key != 'version'").execute(&mut conn).map_err(|e| e.to_string())?;
    
    Ok(())
}

/// Get database version.
pub fn get_db_version() -> QueryResult<u32> {
    use crate::schema::_meta::dsl::*;
    
    let mut conn = get_conn()?;
    
    _meta
        .filter(key.eq("version"))
        .select(value)
        .first::<String>(&mut conn)
        .map(|v| v.parse().unwrap_or(0))
        .optional()
        .map(|v| v.unwrap_or(0))
        .map_err(|e| format!("Failed to get database version: {}", e))
}

/// Set database version.
pub fn set_db_version(version: u32) -> QueryResult<()> {
    use crate::schema::_meta::dsl::*;
    
    let mut conn = get_conn()?;
    
    let meta = Meta {
        key: Some("version".to_string()),
        value: version.to_string(),
    };
    
    diesel::insert_into(_meta)
        .values(&meta)
        .on_conflict(key)
        .do_update()
        .set(crate::schema::_meta::dsl::value.eq(version.to_string()))
        .execute(&mut conn)
        .map_err(|e| format!("Failed to set database version: {}", e))?;
    
    Ok(())
}



/// Count records in a table.
pub fn count_records(table_name: &str) -> QueryResult<i64> {
    let mut conn = get_conn()?;
    
    // Use raw SQL to count records - this is the most reliable approach
    match table_name {
        "_meta" => {
            use crate::schema::_meta::dsl::*;
            _meta
                .select(diesel::dsl::count_star())
                .first::<i64>(&mut conn)
                .map_err(|e| format!("Failed to count {}: {}", table_name, e))
        }
        "release_types" => {
            use crate::schema::release_types::dsl::*;
            release_types
                .select(diesel::dsl::count_star())
                .first::<i64>(&mut conn)
                .map_err(|e| format!("Failed to count {}: {}", table_name, e))
        }
        "groups" => {
            use crate::schema::groups::dsl::*;
            groups
                .select(diesel::dsl::count_star())
                .first::<i64>(&mut conn)
                .map_err(|e| format!("Failed to count {}: {}", table_name, e))
        }
        "sceners" => {
            use crate::schema::sceners::dsl::*;
            sceners
                .select(diesel::dsl::count_star())
                .first::<i64>(&mut conn)
                .map_err(|e| format!("Failed to count {}: {}", table_name, e))
        }
        "scener_handles" => {
            use crate::schema::scener_handles::dsl::*;
            scener_handles
                .select(diesel::dsl::count_star())
                .first::<i64>(&mut conn)
                .map_err(|e| format!("Failed to count {}: {}", table_name, e))
        }
        "events" => {
            use crate::schema::events::dsl::*;
            events
                .select(diesel::dsl::count_star())
                .first::<i64>(&mut conn)
                .map_err(|e| format!("Failed to count {}: {}", table_name, e))
        }
        "releases" => {
            use crate::schema::releases::dsl::*;
            releases
                .select(diesel::dsl::count_star())
                .first::<i64>(&mut conn)
                .map_err(|e| format!("Failed to count {}: {}", table_name, e))
        }
        "release_credits" => {
            use crate::schema::release_credits::dsl::*;
            release_credits
                .select(diesel::dsl::count_star())
                .first::<i64>(&mut conn)
                .map_err(|e| format!("Failed to count {}: {}", table_name, e))
        }
        "release_downloads" => {
            use crate::schema::release_downloads::dsl::*;
            release_downloads
                .select(diesel::dsl::count_star())
                .first::<i64>(&mut conn)
                .map_err(|e| format!("Failed to count {}: {}", table_name, e))
        }
        "release_sids" => {
            use crate::schema::release_sids::dsl::*;
            release_sids
                .select(diesel::dsl::count_star())
                .first::<i64>(&mut conn)
                .map_err(|e| format!("Failed to count {}: {}", table_name, e))
        }
        "ia_mappings" => {
            use crate::schema::ia_mappings::dsl::*;
            ia_mappings
                .select(diesel::dsl::count_star())
                .first::<i64>(&mut conn)
                .map_err(|e| format!("Failed to count {}: {}", table_name, e))
        }
        _ => Err(format!("Unknown table: {}", table_name))
    }
}

