// operations - Database operations for CRUD and utility functions.

use std::path::PathBuf;
use std::collections::HashMap;

use rusqlite::{Connection, OpenFlags, Result as SqlResult, params};
use serde_json;

use super::models::*;
use super::schema::*;
use crate::core;

/// Initialize the database connection.
pub fn initialize() -> SqlResult<Connection> {
    let path = db_path();
    std::fs::create_dir_all(path.parent().unwrap()).ok();
    
    let mut conn = Connection::open(path)?;
    
    // Enable WAL mode for better concurrency
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    conn.pragma_update(None, "temp_store", "MEMORY")?;
    conn.pragma_update(None, "cache_size", "-20000")?;
    
    // Create tables
    create_tables(&conn)?;
    check_version(&conn)?;
    
    Ok(conn)
}

/// Open a read-only connection.
pub fn open() -> SqlResult<Connection> {
    let path = db_path();
    // First ensure the parent directory exists
    std::fs::create_dir_all(path.parent().unwrap()).ok();
    
    // If the database file doesn't exist, we need to create it with tables
    if !path.exists() {
        // Call initialize which will create the file, tables, and return a connection
        // We'll use that connection directly (it's RW but that's OK for first init)
        return initialize();
    }
    
    // File exists, open read-only
    Connection::open_with_flags(&path, OpenFlags::SQLITE_OPEN_READ_ONLY)
}

/// Open a read-write connection.
pub fn open_rw() -> SqlResult<Connection> {
    initialize()
}

/// Check if database exists and is valid.
pub fn database_exists() -> bool {
    let path = db_path();
    if !path.exists() {
        return false;
    }
    
    if let Ok(conn) = open() {
        // Try to read version or check if tables exist
        conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='releases'",
            [],
            |row| row.get::<_, i64>(0)
        ).map(|count| count > 0).unwrap_or(false)
    } else {
        false
    }
}

// Database path helper
pub fn db_path() -> PathBuf {
    core::data_path("csdb_cache.sqlite")
}

/// Clear all data from the database.
pub fn clear_database(conn: &Connection) -> SqlResult<()> {
    conn.execute_batch(&r#"
        DELETE FROM groups;
        DELETE FROM sceners;
        DELETE FROM scener_handles;
        DELETE FROM events;
        DELETE FROM releases;
        DELETE FROM release_credits;
        DELETE FROM release_downloads;
        DELETE FROM release_sids;
        DELETE FROM ia_mappings;
        DELETE FROM releases_fts;
        DELETE FROM sceners_fts;
        DELETE FROM groups_fts;
        DELETE FROM events_fts;
        DELETE FROM _meta WHERE key != 'version';
    "#)?;
    Ok(())
}

/// Get database statistics.
pub fn get_stats(conn: &Connection) -> SqlResult<HashMap<String, i64>> {
    let mut stats = HashMap::new();
    
    let tables = [
        "groups", "sceners", "scener_handles", "events", "releases",
        "release_credits", "release_downloads", "release_sids", "ia_mappings"
    ];
    
    for table in &tables {
        let count: i64 = conn.query_row(
            &format!("SELECT COUNT(*) FROM {}", table),
            [],
            |row| row.get(0)
        )?;
        stats.insert(table.to_string(), count);
    }
    
    Ok(stats)
}

// ============================================================================
// Save Operations
// ============================================================================

/// Save a group to the database.
pub fn save_group(conn: &Connection, group: &Group) -> SqlResult<()> {
    conn.execute(
        r#"
        INSERT OR REPLACE INTO groups 
        (id, name, short, base_country, founded_month, founded_year, website, rating, grouptypes, slogans, updated_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, CURRENT_TIMESTAMP)
        "#,
        params![
            group.id as i64,
            &group.name,
            group.short.as_deref().unwrap_or(""),
            group.base_country.as_deref().unwrap_or(""),
            group.founded_month.map(|m| m as i64).unwrap_or(0),
            group.founded_year.map(|y| y as i64).unwrap_or(0),
            group.website.as_deref().unwrap_or(""),
            group.rating as f64,
            &serde_json::to_string(&group.group_types).unwrap_or_else(|_| "[]".to_string()),
            &serde_json::to_string(&group.slogans).unwrap_or_else(|_| "[]".to_string()),
        ],
    )?;
    Ok(())
}

/// Save a scener to the database.
pub fn save_scener(conn: &Connection, scener: &Scener) -> SqlResult<()> {
    conn.execute(
        r#"
        INSERT OR REPLACE INTO sceners 
        (id, country, trivia, handle_story, updated_at)
        VALUES (?, ?, ?, ?, CURRENT_TIMESTAMP)
        "#,
        params![
            scener.id as i64,
            scener.country.as_deref().unwrap_or(""),
            scener.trivia.as_deref().unwrap_or(""),
            scener.handle_story.as_deref().unwrap_or(""),
        ],
    )?;
    
    // Save handles
    for handle in &scener.handles {
        save_scener_handle(conn, handle)?;
    }
    
    Ok(())
}

/// Save a scener handle to the database.
pub fn save_scener_handle(conn: &Connection, handle: &ScenerHandle) -> SqlResult<()> {
    conn.execute(
        r#"
        INSERT OR REPLACE INTO scener_handles 
        (id, scener_id, handle, aka, currently_used, handle_story)
        VALUES (?, ?, ?, ?, ?, ?)
        "#,
        params![
            handle.id.map(|id| id as i64).unwrap_or(0),
            handle.scener_id as i64,
            &handle.handle,
            handle.aka.as_deref().unwrap_or(""),
            handle.currently_used as i64,
            handle.handle_story.as_deref().unwrap_or(""),
        ],
    )?;
    Ok(())
}

/// Save an event to the database.
pub fn save_event(conn: &Connection, event: &Event) -> SqlResult<()> {
    conn.execute(
        r#"
        INSERT OR REPLACE INTO events 
        (id, name, event_type, aka, start_day, start_month, start_year, end_day, end_month, end_year, 
         address, zip, city, country, website, updated_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, CURRENT_TIMESTAMP)
        "#,
        params![
            event.id as i64,
            &event.name,
            event.event_type.as_deref().unwrap_or(""),
            &serde_json::to_string(&event.aka).unwrap_or_else(|_| "[]".to_string()),
            event.start_day.map(|d| d as i64).unwrap_or(0),
            event.start_month.map(|m| m as i64).unwrap_or(0),
            event.start_year.map(|y| y as i64).unwrap_or(0),
            event.end_day.map(|d| d as i64).unwrap_or(0),
            event.end_month.map(|m| m as i64).unwrap_or(0),
            event.end_year.map(|y| y as i64).unwrap_or(0),
            event.address.as_deref().unwrap_or(""),
            event.zip.as_deref().unwrap_or(""),
            event.city.as_deref().unwrap_or(""),
            event.country.as_deref().unwrap_or(""),
            event.website.as_deref().unwrap_or(""),
        ],
    )?;
    Ok(())
}

/// Save a release to the database.
pub fn save_release(conn: &Connection, release: &Release) -> SqlResult<()> {
    let type_id = if release.release_type_id > 0 {
        release.release_type_id
    } else {
        1 // Default to Demo
    };
    
    conn.execute(
        r#"
        INSERT OR REPLACE INTO releases 
        (id, name, release_type_id, group_id, rating, release_day, release_month, release_year,
         event_id, compo, place, screenshot_url, download_count, website, created_at, updated_at)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, COALESCE(?, CURRENT_TIMESTAMP), CURRENT_TIMESTAMP)
        "#,
        params![
            release.id as i64,
            &release.name,
            type_id as i64,
            release.group_id.map(|g| g as i64).unwrap_or(0),
            release.rating as f64,
            release.release_day.map(|d| d as i64).unwrap_or(0),
            release.release_month.map(|m| m as i64).unwrap_or(0),
            release.release_year.map(|y| y as i64).unwrap_or(0),
            release.event_id.map(|e| e as i64).unwrap_or(0),
            release.compo.as_deref().unwrap_or(""),
            release.place as i64,
            release.screenshot_url.as_deref().unwrap_or(""),
            release.download_count as i64,
            release.website.as_deref().unwrap_or(""),
            release.created_at.as_deref().unwrap_or(""),
        ],
    )?;
    
    // Save FTS data
    let group_name = release.group.as_ref().map(|g| g.name.as_str()).unwrap_or("");
    let event_name = release.event.as_ref().map(|e| e.name.as_str()).unwrap_or("");
    let compo = release.compo.as_deref().unwrap_or("");
    
    conn.execute(
        r#"
        INSERT OR REPLACE INTO releases_fts (rowid, name, group_name, event_name, compo)
        VALUES (?, ?, ?, ?, ?)
        "#,
        params![
            release.id as i64,
            &release.name,
            group_name,
            event_name,
            compo,
        ],
    )?;
    
    // Save credits
    for credit in &release.credits {
        save_release_credit(conn, credit)?;
    }
    
    // Save downloads
    for download in &release.downloads {
        save_download_link(conn, download)?;
    }
    
    // Save SIDs
    for sid in &release.sids {
        save_sid_file(conn, sid)?;
    }
    
    // Save IA mappings
    for mapping in &release.ia_mappings {
        save_ia_mapping(conn, mapping)?;
    }
    
    Ok(())
}

/// Save a release credit.
pub fn save_release_credit(conn: &Connection, credit: &ReleaseCredit) -> SqlResult<()> {
    conn.execute(
        r#"
        INSERT OR REPLACE INTO release_credits 
        (id, release_id, scener_id, handle, credit_type, position, functions)
        VALUES (?, ?, ?, ?, ?, ?, ?)
        "#,
        params![
            credit.id.map(|id| id as i64).unwrap_or(0),
            credit.release_id as i64,
            credit.scener_id.map(|s| s as i64).unwrap_or(0),
            &credit.handle,
            &credit.credit_type,
            credit.position as i64,
            &serde_json::to_string(&credit.functions).unwrap_or_else(|_| "[]".to_string()),
        ],
    )?;
    Ok(())
}

/// Save a download link.
pub fn save_download_link(conn: &Connection, download: &DownloadLink) -> SqlResult<()> {
    conn.execute(
        r#"
        INSERT OR REPLACE INTO release_downloads 
        (id, release_id, url, counter_url, downloads, status, is_internal, file_type)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?)
        "#,
        params![
            download.id.map(|id| id as i64).unwrap_or(0),
            download.release_id as i64,
            &download.url,
            download.counter_url.as_deref().unwrap_or(""),
            download.downloads as i64,
            &download.status,
            download.is_internal as i64,
            download.file_type.as_deref().unwrap_or(""),
        ],
    )?;
    Ok(())
}

/// Save a SID file.
pub fn save_sid_file(conn: &Connection, sid: &SidFile) -> SqlResult<()> {
    conn.execute(
        r#"
        INSERT OR REPLACE INTO release_sids 
        (id, release_id, sid_id, hvsc_path, name, author, released, load_addr, init_addr, 
         play_addr, num_songs, default_song, sid_model, clock_speed, data_size)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
        params![
            sid.id.map(|id| id as i64).unwrap_or(0),
            sid.release_id as i64,
            sid.sid_id.map(|s| s as i64).unwrap_or(0),
            sid.hvsc_path.as_deref().unwrap_or(""),
            &sid.name,
            sid.author.as_deref().unwrap_or(""),
            sid.released.as_deref().unwrap_or(""),
            sid.load_addr.map(|a| a as i64).unwrap_or(0),
            sid.init_addr.map(|a| a as i64).unwrap_or(0),
            sid.play_addr.map(|a| a as i64).unwrap_or(0),
            sid.num_songs as i64,
            sid.default_song as i64,
            sid.sid_model.as_deref().unwrap_or(""),
            sid.clock_speed.as_deref().unwrap_or(""),
            sid.data_size.map(|s| s as i64).unwrap_or(0),
        ],
    )?;
    Ok(())
}

/// Save an IA mapping.
pub fn save_ia_mapping(conn: &Connection, mapping: &IAMapping) -> SqlResult<()> {
    conn.execute(
        r#"
        INSERT OR REPLACE INTO ia_mappings 
        (id, release_id, ia_identifier, ia_title, ia_collection, match_confidence, last_checked)
        VALUES (?, ?, ?, ?, ?, ?, ?)
        "#,
        params![
            mapping.id.map(|id| id as i64).unwrap_or(0),
            mapping.release_id as i64,
            &mapping.ia_identifier,
            mapping.ia_title.as_deref().unwrap_or(""),
            &mapping.ia_collection,
            mapping.match_confidence as f64,
            mapping.last_checked.as_deref().unwrap_or(""),
        ],
    )?;
    Ok(())
}

/// Get the top N releases by rating from the database.
pub fn get_top_releases(conn: &Connection, limit: usize) -> SqlResult<Vec<Release>> {
    // First, get the releases
    let mut stmt = conn.prepare(
        r#"
        SELECT id, name, release_type_id, group_id, rating, release_day, release_month, 
               release_year, event_id, compo, place, screenshot_url, download_count, 
               website, created_at, updated_at
        FROM releases 
        ORDER BY rating DESC 
        LIMIT ?
        "#,
    )?;

    let mut rows = stmt.query([limit])?;
    let mut releases = Vec::new();

    while let Some(row) = rows.next()? {
        let release_id: u32 = row.get::<_, u32>("id")?;
        
        // Get downloads for this release
        let mut download_stmt = conn.prepare(
            "SELECT id, release_id, url, counter_url, downloads, status, is_internal, file_type FROM release_downloads WHERE release_id = ?"
        )?;
        let mut download_rows = download_stmt.query([release_id])?;
        let mut downloads = Vec::new();
        while let Some(dl_row) = download_rows.next()? {
            downloads.push(DownloadLink {
                id: dl_row.get::<_, Option<u64>>("id")?,
                release_id: dl_row.get::<_, u32>("release_id")?,
                url: dl_row.get::<_, String>("url")?,
                counter_url: dl_row.get::<_, Option<String>>("counter_url")?,
                downloads: dl_row.get::<_, u32>("downloads")?,
                status: dl_row.get::<_, String>("status")?,
                is_internal: dl_row.get::<_, bool>("is_internal")?,
                file_type: dl_row.get::<_, Option<String>>("file_type")?,
            });
        }
        
        let release = Release {
            id: release_id,
            name: row.get::<_, String>("name")?,
            release_type_id: row.get::<_, u32>("release_type_id")?,
            release_type: String::new(),
            group_id: row.get::<_, Option<u32>>("group_id")?,
            group: None,
            rating: row.get::<_, f32>("rating")?,
            release_day: row.get::<_, Option<u32>>("release_day")?,
            release_month: row.get::<_, Option<u32>>("release_month")?,
            release_year: row.get::<_, Option<u32>>("release_year")?,
            event_id: row.get::<_, Option<u32>>("event_id")?,
            event: None,
            compo: row.get::<_, Option<String>>("compo")?,
            place: row.get::<_, u32>("place")?,
            screenshot_url: row.get::<_, Option<String>>("screenshot_url")?,
            screenshot_urls: Vec::new(),
            download_count: row.get::<_, u32>("download_count")?,
            website: row.get::<_, Option<String>>("website")?,
            credits: Vec::new(),
            downloads,
            sids: Vec::new(),
            ia_mappings: Vec::new(),
            created_at: row.get::<_, Option<String>>("created_at")?,
            updated_at: row.get::<_, Option<String>>("updated_at")?,
        };
        releases.push(release);
    }

    Ok(releases)
}
