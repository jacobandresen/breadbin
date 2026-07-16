// diesel_db - Diesel ORM database layer for the CSDb cache.
//
// The public API works exclusively with the shared domain models in
// crate::database::models; the Diesel row structs below are private plumbing
// that mirrors the SQLite schema (crate::schema).

use diesel::prelude::*;
use diesel::r2d2::{ConnectionManager, Pool, PooledConnection};
use diesel::sqlite::SqliteConnection;
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};
use chrono::NaiveDateTime;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;

use crate::core;
use crate::database::models;
use crate::database::models::{
    ReleaseTypeFilter, SearchFacets, SearchQuery, SearchResult, SearchResultItem, SearchSort,
};

pub type QueryResult<T> = Result<T, String>;

/// Migrations compiled into the binary so the database self-initializes on first use.
pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");

// ============================================================================
// Connection Management
// ============================================================================

type DbPool = Pool<ConnectionManager<SqliteConnection>>;
type DbConnection = PooledConnection<ConnectionManager<SqliteConnection>>;

static POOL: OnceLock<DbPool> = OnceLock::new();

/// Path to the Diesel database file inside breadbin's data directory.
pub fn diesel_db_path() -> PathBuf {
    core::data_path("csdb_cache_diesel.sqlite")
}

/// Check if the Diesel database file exists.
pub fn diesel_database_exists() -> bool {
    diesel_db_path().exists()
}

/// Initialize the connection pool and run pending migrations.
pub fn initialize_pool() -> QueryResult<()> {
    POOL.get_or_init(|| {
        let path = diesel_db_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("Failed to create data directory");
        }

        let manager =
            ConnectionManager::<SqliteConnection>::new(path.to_string_lossy().to_string());
        let pool = Pool::builder()
            .max_size(1) // SQLite allows a single writer
            .build(manager)
            .expect("Failed to build connection pool");

        let mut conn = pool.get().expect("Failed to get connection for migrations");
        conn.run_pending_migrations(MIGRATIONS)
            .expect("Failed to run database migrations");

        pool
    });
    Ok(())
}

fn get_conn() -> QueryResult<DbConnection> {
    initialize_pool()?;
    POOL.get()
        .ok_or("Connection pool not initialized")?
        .get()
        .map_err(|e| format!("Failed to get connection: {}", e))
}

// ============================================================================
// Row Structs (private schema mirrors)
// ============================================================================

#[derive(Queryable, Insertable)]
#[diesel(table_name = crate::schema::_meta)]
struct MetaRow {
    key: Option<String>,
    value: String,
}

#[derive(Queryable, Insertable, AsChangeset)]
#[diesel(table_name = crate::schema::release_types)]
struct ReleaseTypeRow {
    id: Option<i32>,
    name: String,
    description: Option<String>,
}

#[derive(Queryable, Insertable, AsChangeset)]
#[diesel(table_name = crate::schema::groups)]
struct GroupRow {
    id: Option<i32>,
    name: String,
    short: Option<String>,
    base_country: Option<String>,
    founded_month: Option<i32>,
    founded_year: Option<i32>,
    website: Option<String>,
    rating: Option<f32>,
    grouptypes: Option<String>,
    slogans: Option<String>,
    updated_at: Option<NaiveDateTime>,
}

#[derive(Queryable, Insertable)]
#[diesel(table_name = crate::schema::sceners)]
struct ScenerRow {
    id: Option<i32>,
    country: Option<String>,
    trivia: Option<String>,
    handle_story: Option<String>,
    updated_at: Option<NaiveDateTime>,
}

#[derive(Queryable, Insertable, AsChangeset)]
#[diesel(table_name = crate::schema::events)]
struct EventRow {
    id: Option<i32>,
    name: String,
    event_type: Option<String>,
    aka: Option<String>,
    start_day: Option<i32>,
    start_month: Option<i32>,
    start_year: Option<i32>,
    end_day: Option<i32>,
    end_month: Option<i32>,
    end_year: Option<i32>,
    address: Option<String>,
    zip: Option<String>,
    city: Option<String>,
    country: Option<String>,
    website: Option<String>,
    updated_at: Option<NaiveDateTime>,
}

#[derive(Queryable, Insertable, AsChangeset)]
#[diesel(table_name = crate::schema::releases)]
struct ReleaseRow {
    id: Option<i32>,
    name: String,
    release_type_id: i32,
    group_id: Option<i32>,
    rating: Option<f32>,
    release_day: Option<i32>,
    release_month: Option<i32>,
    release_year: Option<i32>,
    event_id: Option<i32>,
    compo: Option<String>,
    place: Option<i32>,
    screenshot_url: Option<String>,
    screenshot_urls: Option<String>,
    download_count: Option<i32>,
    website: Option<String>,
    created_at: Option<NaiveDateTime>,
    updated_at: Option<NaiveDateTime>,
}

#[derive(Queryable, Insertable, AsChangeset)]
#[diesel(table_name = crate::schema::release_credits)]
struct ReleaseCreditRow {
    id: Option<i32>,
    release_id: i32,
    scener_id: i32,
    handle: String,
    credit_type: String,
    position: Option<i32>,
    functions: Option<String>,
}

#[derive(Queryable, Insertable, AsChangeset)]
#[diesel(table_name = crate::schema::release_downloads)]
struct DownloadLinkRow {
    id: Option<i32>,
    release_id: i32,
    url: String,
    counter_url: Option<String>,
    downloads: Option<i32>,
    status: Option<String>,
    is_internal: Option<bool>,
    file_type: Option<String>,
}

#[derive(Queryable, Insertable, AsChangeset)]
#[diesel(table_name = crate::schema::release_sids)]
struct SidFileRow {
    id: Option<i32>,
    release_id: i32,
    sid_id: Option<i32>,
    hvsc_path: Option<String>,
    name: String,
    author: Option<String>,
    released: Option<String>,
    load_addr: Option<i32>,
    init_addr: Option<i32>,
    play_addr: Option<i32>,
    num_songs: Option<i32>,
    default_song: Option<i32>,
    sid_model: Option<String>,
    clock_speed: Option<String>,
    data_size: Option<i32>,
}

#[derive(Queryable, Insertable, AsChangeset)]
#[diesel(table_name = crate::schema::ia_mappings)]
struct IAMappingRow {
    id: Option<i32>,
    release_id: i32,
    ia_identifier: String,
    ia_title: Option<String>,
    ia_collection: Option<String>,
    match_confidence: Option<f32>,
    last_checked: Option<NaiveDateTime>,
}

// ============================================================================
// Row <-> Domain Conversions
// ============================================================================

fn json_vec(v: &[String]) -> Option<String> {
    if v.is_empty() {
        None
    } else {
        serde_json::to_string(v).ok()
    }
}

fn positive(v: Option<i32>) -> Option<u32> {
    v.and_then(|n| if n > 0 { Some(n as u32) } else { None })
}

impl From<ReleaseRow> for models::Release {
    fn from(row: ReleaseRow) -> Self {
        models::Release {
            id: row.id.unwrap_or(0) as u32,
            name: row.name,
            release_type_id: row.release_type_id as u32,
            release_type: String::new(),
            group_id: positive(row.group_id),
            group: None,
            rating: row.rating.unwrap_or(0.0),
            release_day: positive(row.release_day),
            release_month: positive(row.release_month),
            release_year: positive(row.release_year),
            event_id: positive(row.event_id),
            event: None,
            compo: row.compo.filter(|s| !s.is_empty()),
            place: row.place.unwrap_or(0).max(0) as u32,
            screenshot_url: row.screenshot_url.filter(|s| !s.is_empty()),
            screenshot_urls: row
                .screenshot_urls
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default(),
            download_count: row.download_count.unwrap_or(0).max(0) as u32,
            website: row.website.filter(|s| !s.is_empty()),
            credits: Vec::new(),
            downloads: Vec::new(),
            sids: Vec::new(),
            ia_mappings: Vec::new(),
            created_at: row.created_at.map(|t| t.to_string()),
            updated_at: row.updated_at.map(|t| t.to_string()),
        }
    }
}

impl From<&models::Release> for ReleaseRow {
    fn from(r: &models::Release) -> Self {
        ReleaseRow {
            id: Some(r.id as i32),
            name: r.name.clone(),
            release_type_id: if r.release_type_id > 0 { r.release_type_id as i32 } else { 1 },
            group_id: r.group_id.map(|g| g as i32),
            rating: Some(r.rating),
            release_day: r.release_day.map(|d| d as i32),
            release_month: r.release_month.map(|m| m as i32),
            release_year: r.release_year.map(|y| y as i32),
            event_id: r.event_id.map(|e| e as i32),
            compo: r.compo.clone(),
            place: Some(r.place as i32),
            screenshot_url: r.screenshot_url.clone(),
            screenshot_urls: json_vec(&r.screenshot_urls),
            download_count: Some(r.download_count as i32),
            website: r.website.clone(),
            created_at: None,
            updated_at: None,
        }
    }
}

impl From<&models::Group> for GroupRow {
    fn from(g: &models::Group) -> Self {
        GroupRow {
            id: Some(g.id as i32),
            name: g.name.clone(),
            short: g.short.clone(),
            base_country: g.base_country.clone(),
            founded_month: g.founded_month.map(|m| m as i32),
            founded_year: g.founded_year.map(|y| y as i32),
            website: g.website.clone(),
            rating: Some(g.rating),
            grouptypes: json_vec(&g.group_types),
            slogans: json_vec(&g.slogans),
            updated_at: None,
        }
    }
}

impl From<&models::Event> for EventRow {
    fn from(e: &models::Event) -> Self {
        EventRow {
            id: Some(e.id as i32),
            name: e.name.clone(),
            event_type: e.event_type.clone(),
            aka: json_vec(&e.aka),
            start_day: e.start_day.map(|d| d as i32),
            start_month: e.start_month.map(|m| m as i32),
            start_year: e.start_year.map(|y| y as i32),
            end_day: e.end_day.map(|d| d as i32),
            end_month: e.end_month.map(|m| m as i32),
            end_year: e.end_year.map(|y| y as i32),
            address: e.address.clone(),
            zip: e.zip.clone(),
            city: e.city.clone(),
            country: e.country.clone(),
            website: e.website.clone(),
            updated_at: None,
        }
    }
}

impl From<DownloadLinkRow> for models::DownloadLink {
    fn from(row: DownloadLinkRow) -> Self {
        models::DownloadLink {
            id: row.id.map(|i| i as u64),
            release_id: row.release_id as u32,
            url: row.url,
            counter_url: row.counter_url.filter(|s| !s.is_empty()),
            downloads: row.downloads.unwrap_or(0).max(0) as u32,
            status: row.status.unwrap_or_default(),
            is_internal: row.is_internal.unwrap_or(false),
            file_type: row.file_type.filter(|s| !s.is_empty()),
        }
    }
}

// ============================================================================
// Release Operations
// ============================================================================

/// Save a full domain Release including its embedded group, event, credit
/// sceners, credits, downloads, SIDs and IA mappings, in one transaction.
pub fn save_release(release: &models::Release) -> QueryResult<()> {
    use crate::schema::{
        ia_mappings, release_credits, release_downloads, release_sids, releases, sceners,
    };

    let mut conn = get_conn()?;
    conn.transaction::<_, diesel::result::Error, _>(|conn| {
        if let Some(group) = release.group.as_deref().filter(|g| g.id > 0) {
            let row = GroupRow::from(group);
            diesel::insert_into(crate::schema::groups::table)
                .values(&row)
                .on_conflict(crate::schema::groups::id)
                .do_update()
                .set(&row)
                .execute(conn)?;
        }

        if let Some(event) = release.event.as_deref().filter(|e| e.id > 0) {
            let row = EventRow::from(event);
            diesel::insert_into(crate::schema::events::table)
                .values(&row)
                .on_conflict(crate::schema::events::id)
                .do_update()
                .set(&row)
                .execute(conn)?;
        }

        // Ensure sceners referenced in credits exist, without overwriting
        // fuller data already present.
        let mut scener_ids: Vec<u32> = release
            .credits
            .iter()
            .filter_map(|c| c.scener_id)
            .filter(|id| *id > 0)
            .collect();
        scener_ids.sort_unstable();
        scener_ids.dedup();
        for sid in scener_ids {
            diesel::insert_or_ignore_into(sceners::table)
                .values(&ScenerRow {
                    id: Some(sid as i32),
                    country: None,
                    trivia: None,
                    handle_story: None,
                    updated_at: None,
                })
                .execute(conn)?;
        }

        let row = ReleaseRow::from(release);
        diesel::insert_into(releases::table)
            .values(&row)
            .on_conflict(releases::id)
            .do_update()
            .set(&row)
            .execute(conn)?;

        for credit in &release.credits {
            let row = ReleaseCreditRow {
                id: None,
                release_id: release.id as i32,
                scener_id: credit.scener_id.unwrap_or(0) as i32,
                handle: credit.handle.clone(),
                credit_type: credit.credit_type.clone(),
                position: Some(credit.position as i32),
                functions: json_vec(&credit.functions),
            };
            diesel::insert_into(release_credits::table)
                .values(&row)
                .on_conflict((
                    release_credits::release_id,
                    release_credits::scener_id,
                    release_credits::credit_type,
                    release_credits::handle,
                ))
                .do_update()
                .set(&row)
                .execute(conn)?;
        }

        for download in &release.downloads {
            let row = DownloadLinkRow {
                id: None,
                release_id: release.id as i32,
                url: download.url.clone(),
                counter_url: download.counter_url.clone(),
                downloads: Some(download.downloads as i32),
                status: Some(download.status.clone()),
                is_internal: Some(download.is_internal),
                file_type: download.file_type.clone(),
            };
            diesel::insert_into(release_downloads::table)
                .values(&row)
                .on_conflict((release_downloads::release_id, release_downloads::url))
                .do_update()
                .set(&row)
                .execute(conn)?;
        }

        for sid in &release.sids {
            let row = SidFileRow {
                id: None,
                release_id: release.id as i32,
                sid_id: sid.sid_id.map(|s| s as i32),
                hvsc_path: sid.hvsc_path.clone(),
                name: sid.name.clone(),
                author: sid.author.clone(),
                released: sid.released.clone(),
                load_addr: sid.load_addr.map(|a| a as i32),
                init_addr: sid.init_addr.map(|a| a as i32),
                play_addr: sid.play_addr.map(|a| a as i32),
                num_songs: Some(sid.num_songs as i32),
                default_song: Some(sid.default_song as i32),
                sid_model: sid.sid_model.clone(),
                clock_speed: sid.clock_speed.clone(),
                data_size: sid.data_size.map(|s| s as i32),
            };
            diesel::insert_into(release_sids::table)
                .values(&row)
                .on_conflict((release_sids::release_id, release_sids::sid_id))
                .do_update()
                .set(&row)
                .execute(conn)?;
        }

        for mapping in &release.ia_mappings {
            let row = IAMappingRow {
                id: None,
                release_id: release.id as i32,
                ia_identifier: mapping.ia_identifier.clone(),
                ia_title: mapping.ia_title.clone(),
                ia_collection: Some(mapping.ia_collection.clone()),
                match_confidence: Some(mapping.match_confidence),
                last_checked: None,
            };
            diesel::insert_into(ia_mappings::table)
                .values(&row)
                .on_conflict((ia_mappings::release_id, ia_mappings::ia_identifier))
                .do_update()
                .set(&row)
                .execute(conn)?;
        }

        Ok(())
    })
    .map_err(|e| format!("Failed to save release {}: {}", release.id, e))
}

/// Get the top N releases by rating, with their download links.
pub fn get_top_releases(limit: usize) -> QueryResult<Vec<models::Release>> {
    use crate::schema::{release_downloads, releases};

    let mut conn = get_conn()?;

    let rows = releases::table
        .order(releases::rating.desc())
        .limit(limit as i64)
        .load::<ReleaseRow>(&mut conn)
        .map_err(|e| format!("Failed to load top releases: {}", e))?;

    rows.into_iter()
        .map(|row| {
            let rid = row.id.unwrap_or(0);
            let mut release = models::Release::from(row);
            release.downloads = release_downloads::table
                .filter(release_downloads::release_id.eq(rid))
                .load::<DownloadLinkRow>(&mut conn)
                .map_err(|e| format!("Failed to load download links: {}", e))?
                .into_iter()
                .map(Into::into)
                .collect();
            Ok(release)
        })
        .collect()
}

/// A release joined with its group and event names, for flat list views.
pub struct ReleaseListing {
    pub release: models::Release,
    pub group_name: String,
    pub event_name: String,
}

/// List releases of the given types (all types if empty), joined with group
/// and event names, ordered by rating.
pub fn list_releases(type_ids: &[u32]) -> QueryResult<Vec<ReleaseListing>> {
    use crate::schema::{events, groups, releases};

    let mut conn = get_conn()?;

    let mut q = releases::table
        .left_join(groups::table)
        .left_join(events::table)
        .into_boxed();
    if !type_ids.is_empty() {
        let ids: Vec<i32> = type_ids.iter().map(|t| *t as i32).collect();
        q = q.filter(releases::release_type_id.eq_any(ids));
    }

    let rows = q
        .order(releases::rating.desc())
        .select((
            releases::all_columns,
            groups::name.nullable(),
            events::name.nullable(),
        ))
        .load::<(ReleaseRow, Option<String>, Option<String>)>(&mut conn)
        .map_err(|e| format!("Failed to list releases: {}", e))?;

    Ok(rows
        .into_iter()
        .map(|(row, group_name, event_name)| ReleaseListing {
            release: row.into(),
            group_name: group_name.unwrap_or_default(),
            event_name: event_name.unwrap_or_default(),
        })
        .collect())
}

/// Check whether the database exists and contains at least one release.
pub fn has_data() -> bool {
    diesel_database_exists()
        && get_conn()
            .and_then(|mut conn| count_table(&mut conn, "releases"))
            .map(|c| c > 0)
            .unwrap_or(false)
}

// ============================================================================
// Release Types
// ============================================================================

/// Seed the release_types table with CSDb's C64 release types.
pub fn initialize_default_release_types() -> QueryResult<()> {
    use crate::schema::release_types;

    let default_types = [
        (1, "C64 Demo", "Commodore 64 Demo"),
        (2, "C64 Game", "Commodore 64 Game"),
        (3, "C64 Crack", "Cracked Game"),
        (4, "C64 Trainer", "Trainer for Cracked Game"),
        (5, "C64 Intro", "Intro (usually for a crack)"),
        (6, "C64 Graphics", "Graphics Release"),
        (7, "C64 Music", "Commodore 64 Music/SID"),
        (8, "C64 Music Collection", "Collection of SID tunes"),
        (9, "C64 Demo Collection", "Collection of demos"),
        (10, "C64 Diskmag", "Disk Magazine"),
        (11, "C64 Papermag", "Paper Magazine"),
        (12, "C64 Tool", "Utility or Tool"),
        (13, "C64 Game Preview", "Preview of a game"),
        (14, "C64 Invitation", "Party Invitation"),
        (15, "C64 One-File Demo", "Single-file demo"),
        (16, "C64 4K Intro", "4KB Intro"),
        (17, "C64 8K Intro", "8KB Intro"),
        (18, "C64 16K Intro", "16KB Intro"),
        (19, "C64 32K Intro", "32KB Intro"),
        (20, "C64 64K Intro", "64KB Intro"),
    ];

    let mut conn = get_conn()?;
    for (id, name, description) in default_types {
        let row = ReleaseTypeRow {
            id: Some(id),
            name: name.to_string(),
            description: Some(description.to_string()),
        };
        diesel::insert_into(release_types::table)
            .values(&row)
            .on_conflict(release_types::id)
            .do_update()
            .set(&row)
            .execute(&mut conn)
            .map_err(|e| format!("Failed to save release type {}: {}", id, e))?;
    }
    Ok(())
}

// ============================================================================
// Metadata and Versioning
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
        .map_err(|e| format!("Failed to get meta value: {}", e))
}

/// Set a metadata value.
pub fn set_meta_value(meta_key: &str, meta_value: &str) -> QueryResult<()> {
    use crate::schema::_meta::dsl::*;

    let mut conn = get_conn()?;
    diesel::insert_into(_meta)
        .values(&MetaRow {
            key: Some(meta_key.to_string()),
            value: meta_value.to_string(),
        })
        .on_conflict(key)
        .do_update()
        .set(value.eq(meta_value))
        .execute(&mut conn)
        .map_err(|e| format!("Failed to set meta value: {}", e))?;
    Ok(())
}

/// Get the database schema version (0 if unset).
pub fn get_db_version() -> QueryResult<u32> {
    Ok(get_meta_value("version")?
        .and_then(|v| v.parse().ok())
        .unwrap_or(0))
}

/// Set the database schema version.
pub fn set_db_version(version: u32) -> QueryResult<()> {
    set_meta_value("version", &version.to_string())
}

// ============================================================================
// Maintenance
// ============================================================================

const TABLES: [&str; 11] = [
    "_meta",
    "release_types",
    "groups",
    "sceners",
    "scener_handles",
    "events",
    "releases",
    "release_credits",
    "release_downloads",
    "release_sids",
    "ia_mappings",
];

#[derive(QueryableByName)]
struct CountRow {
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    n: i64,
}

fn count_table(conn: &mut SqliteConnection, table: &str) -> QueryResult<i64> {
    debug_assert!(TABLES.contains(&table));
    diesel::sql_query(format!("SELECT COUNT(*) AS n FROM {}", table))
        .get_result::<CountRow>(conn)
        .map(|r| r.n)
        .map_err(|e| format!("Failed to count {}: {}", table, e))
}

/// Per-table record counts and the overall total.
#[derive(Debug, Clone)]
pub struct DatabaseStats {
    pub table_counts: HashMap<String, i64>,
    pub total_records: i64,
}

/// Get database statistics.
pub fn get_stats() -> QueryResult<DatabaseStats> {
    let mut conn = get_conn()?;
    let mut table_counts = HashMap::new();
    let mut total_records = 0i64;

    for table in TABLES {
        let count = count_table(&mut conn, table)?;
        table_counts.insert(table.to_string(), count);
        total_records += count;
    }

    Ok(DatabaseStats { table_counts, total_records })
}

/// Clear all data from the database (except the version metadata).
pub fn clear_database() -> QueryResult<()> {
    let mut conn = get_conn()?;
    for table in TABLES.iter().filter(|t| **t != "_meta") {
        diesel::sql_query(format!("DELETE FROM {}", table))
            .execute(&mut conn)
            .map_err(|e| format!("Failed to clear {}: {}", table, e))?;
    }
    diesel::sql_query("DELETE FROM _meta WHERE key != 'version'")
        .execute(&mut conn)
        .map_err(|e| e.to_string())?;
    Ok(())
}

// ============================================================================
// Search
//
// Diesel query-builder port of the legacy FTS5 search. Text matching uses
// LIKE (case-insensitive in SQLite for ASCII); relevance ordering falls back
// to rating.
// ============================================================================

/// Parse a search query string with `key:value` filter tokens into a SearchQuery.
pub fn parse_search_query(input: &str) -> SearchQuery {
    let mut query = SearchQuery::default();
    let mut remaining = Vec::new();

    for token in input.split_whitespace() {
        if let Some(v) = token.strip_prefix("group:") {
            query.groups.push(v.to_string());
        } else if let Some(v) = token.strip_prefix("scener:") {
            query.sceners.push(v.to_string());
        } else if let Some(v) = token.strip_prefix("event:") {
            query.events.push(v.to_string());
        } else if let Some(v) = token.strip_prefix("year:") {
            if let Ok(year) = v.parse::<u32>() {
                query.years = Some(year..year + 1);
            }
        } else if let Some(v) = token.strip_prefix("type:") {
            query.r#type = match v.to_lowercase().as_str() {
                "demo" | "demos" => Some(ReleaseTypeFilter::Demo),
                "music" | "tune" | "tunes" | "sid" | "sids" => Some(ReleaseTypeFilter::Music),
                "game" | "games" => Some(ReleaseTypeFilter::Game),
                "crack" | "cracks" => Some(ReleaseTypeFilter::Crack),
                "intro" | "intros" => Some(ReleaseTypeFilter::Intro),
                "graphics" => Some(ReleaseTypeFilter::Graphics),
                "diskmag" | "diskmags" => Some(ReleaseTypeFilter::Diskmag),
                "tool" | "tools" => Some(ReleaseTypeFilter::Tool),
                _ => None,
            };
        } else if let Some(v) = token.strip_prefix("rating:") {
            if let Ok(rating) = v.parse::<f32>() {
                query.min_rating = Some(rating);
            }
        } else if let Some(v) = token.strip_prefix("country:") {
            query.countries.push(v.to_string());
        } else if let Some(v) = token.strip_prefix("credit:") {
            query.credit_types.push(v.to_string());
        } else {
            remaining.push(token);
        }
    }

    query.query = remaining.join(" ");
    query
}

type BoxedReleaseQuery<'a> = crate::schema::releases::BoxedQuery<'a, diesel::sqlite::Sqlite>;

/// Apply all SearchQuery filters onto a boxed releases query.
fn apply_search_filters<'a>(
    mut q: BoxedReleaseQuery<'a>,
    query: &SearchQuery,
) -> BoxedReleaseQuery<'a> {
    use crate::schema::{events, groups, release_credits, release_downloads, releases};

    if !query.query.is_empty() {
        q = q.filter(releases::name.like(format!("%{}%", query.query)));
    }

    if let Some(ref ty) = query.r#type {
        if ty.id() > 0 {
            q = q.filter(releases::release_type_id.eq(ty.id() as i32));
        }
    }

    if let Some(ref range) = query.years {
        q = q.filter(releases::release_year.between(
            range.start as i32,
            range.end.saturating_sub(1) as i32,
        ));
    }

    if let Some(min) = query.min_rating {
        q = q.filter(releases::rating.ge(min));
    }
    if let Some(max) = query.max_rating {
        q = q.filter(releases::rating.le(max));
    }

    for group in &query.groups {
        q = q.filter(
            releases::group_id.eq_any(
                groups::table
                    .filter(groups::name.like(format!("%{}%", group)))
                    .select(groups::id),
            ),
        );
    }

    for scener in &query.sceners {
        q = q.filter(
            releases::id.eq_any(
                release_credits::table
                    .filter(release_credits::handle.like(format!("%{}%", scener)))
                    .select(release_credits::release_id.nullable()),
            ),
        );
    }

    for event in &query.events {
        q = q.filter(
            releases::event_id.eq_any(
                events::table
                    .filter(events::name.like(format!("%{}%", event)))
                    .select(events::id),
            ),
        );
    }

    for country in &query.countries {
        q = q.filter(
            releases::group_id
                .eq_any(
                    groups::table
                        .filter(groups::base_country.like(format!("%{}%", country)))
                        .select(groups::id),
                )
                .or(releases::event_id.eq_any(
                    events::table
                        .filter(events::country.like(format!("%{}%", country)))
                        .select(events::id),
                )),
        );
    }

    for credit_type in &query.credit_types {
        q = q.filter(
            releases::id.eq_any(
                release_credits::table
                    .filter(release_credits::credit_type.like(format!("%{}%", credit_type)))
                    .select(release_credits::release_id.nullable()),
            ),
        );
    }

    if let Some(has_download) = query.has_download {
        let with_downloads =
            release_downloads::table.select(release_downloads::release_id.nullable());
        if has_download {
            q = q.filter(releases::id.eq_any(with_downloads));
        } else {
            q = q.filter(diesel::dsl::not(releases::id.eq_any(with_downloads)));
        }
    }

    q
}

/// Count total results for a search query.
pub fn count_search_results(query: &SearchQuery) -> QueryResult<u64> {
    use crate::schema::releases;

    let mut conn = get_conn()?;
    apply_search_filters(releases::table.into_boxed(), query)
        .count()
        .get_result::<i64>(&mut conn)
        .map(|c| c as u64)
        .map_err(|e| format!("Failed to count results: {}", e))
}

/// Execute a search query and return matching releases.
pub fn execute_search_query(query: &SearchQuery) -> QueryResult<SearchResult> {
    use crate::schema::releases;

    let mut conn = get_conn()?;

    let mut q = apply_search_filters(releases::table.into_boxed(), query);
    q = match query.sort_by {
        SearchSort::Relevance | SearchSort::Rating => q.order(releases::rating.desc()),
        SearchSort::Date => q.order(releases::release_year.desc()),
        SearchSort::Name => q.order(releases::name.asc()),
        SearchSort::Downloads => q.order(releases::download_count.desc()),
    };

    let results = q
        .limit(query.limit as i64)
        .offset(query.offset as i64)
        .load::<ReleaseRow>(&mut conn)
        .map_err(|e| format!("Failed to execute search: {}", e))?
        .into_iter()
        .map(|row| SearchResultItem {
            release: row.into(),
            score: 1.0,
            highlights: Vec::new(),
        })
        .collect();
    drop(conn);

    Ok(SearchResult {
        total: count_search_results(query)?,
        results,
        facets: SearchFacets::default(),
    })
}

/// Simple search by name, returning (id, name) pairs.
pub fn search_by_name(name_query: &str, limit: usize) -> QueryResult<Vec<(u32, String)>> {
    let mut query = parse_search_query(name_query);
    query.limit = limit;
    query.offset = 0;

    Ok(execute_search_query(&query)?
        .results
        .into_iter()
        .map(|item| (item.release.id, item.release.name))
        .collect())
}

#[cfg(test)]
mod parse_tests {
    use super::*;

    #[test]
    fn parse_empty() {
        let query = parse_search_query("");
        assert!(query.query.is_empty());
        assert!(query.groups.is_empty());
        assert!(query.years.is_none());
        assert!(query.r#type.is_none());
    }

    #[test]
    fn parse_simple() {
        let query = parse_search_query("state of the art");
        assert_eq!(query.query, "state of the art");
    }

    #[test]
    fn parse_filters() {
        let query = parse_search_query(
            "group:Fairlight scener:Methos event:X year:1992 type:demo rating:9.0 country:Denmark credit:Code",
        );
        assert_eq!(query.groups, vec!["Fairlight"]);
        assert_eq!(query.sceners, vec!["Methos"]);
        assert_eq!(query.events, vec!["X"]);
        assert_eq!(query.years, Some(1992..1993));
        assert_eq!(query.r#type, Some(ReleaseTypeFilter::Demo));
        assert_eq!(query.min_rating, Some(9.0));
        assert_eq!(query.countries, vec!["Denmark"]);
        assert_eq!(query.credit_types, vec!["Code"]);
    }

    #[test]
    fn parse_mixed() {
        let query = parse_search_query("state of the art group:Fairlight year:1992 rating:9.0");
        assert_eq!(query.query, "state of the art");
        assert_eq!(query.groups, vec!["Fairlight"]);
        assert_eq!(query.years, Some(1992..1993));
        assert_eq!(query.min_rating, Some(9.0));
    }
}
