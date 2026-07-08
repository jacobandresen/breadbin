// schema - Database schema definitions and migrations.

use rusqlite::Connection;
use rusqlite::Result as SqlResult;

// Database version - increment when schema changes
pub const DB_VERSION: u32 = 1;

/// Create all database tables.
pub fn create_tables(conn: &Connection) -> SqlResult<()> {
    conn.execute_batch(&r#"
        -- Database metadata
        CREATE TABLE IF NOT EXISTS _meta (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        
        -- Release types
        CREATE TABLE IF NOT EXISTS release_types (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            description TEXT
        );
        
        -- Groups
        CREATE TABLE IF NOT EXISTS groups (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            short TEXT,
            base_country TEXT,
            founded_month INTEGER,
            founded_year INTEGER,
            website TEXT,
            rating REAL DEFAULT 0,
            grouptypes TEXT,
            slogans TEXT,
            updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            UNIQUE(id)
        );
        
        -- Sceners
        CREATE TABLE IF NOT EXISTS sceners (
            id INTEGER PRIMARY KEY,
            country TEXT,
            trivia TEXT,
            handle_story TEXT,
            updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            UNIQUE(id)
        );
        
        -- Scener handles
        CREATE TABLE IF NOT EXISTS scener_handles (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            scener_id INTEGER NOT NULL,
            handle TEXT NOT NULL,
            aka TEXT,
            currently_used BOOLEAN DEFAULT true,
            handle_story TEXT,
            FOREIGN KEY (scener_id) REFERENCES sceners(id) ON DELETE CASCADE,
            UNIQUE(scener_id, handle)
        );
        
        -- Events
        CREATE TABLE IF NOT EXISTS events (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            event_type TEXT,
            aka TEXT,
            start_day INTEGER,
            start_month INTEGER,
            start_year INTEGER,
            end_day INTEGER,
            end_month INTEGER,
            end_year INTEGER,
            address TEXT,
            zip TEXT,
            city TEXT,
            country TEXT,
            website TEXT,
            updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            UNIQUE(id)
        );
        
        -- Releases
        CREATE TABLE IF NOT EXISTS releases (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            release_type_id INTEGER NOT NULL,
            group_id INTEGER,
            rating REAL DEFAULT 0,
            release_day INTEGER,
            release_month INTEGER,
            release_year INTEGER,
            event_id INTEGER,
            compo TEXT,
            place INTEGER DEFAULT 0,
            screenshot_url TEXT,
            download_count INTEGER DEFAULT 0,
            website TEXT,
            created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
            FOREIGN KEY (release_type_id) REFERENCES release_types(id),
            FOREIGN KEY (group_id) REFERENCES groups(id),
            FOREIGN KEY (event_id) REFERENCES events(id),
            UNIQUE(id)
        );
        
        -- Release credits
        CREATE TABLE IF NOT EXISTS release_credits (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            release_id INTEGER NOT NULL,
            scener_id INTEGER NOT NULL,
            handle TEXT NOT NULL,
            credit_type TEXT NOT NULL,
            position INTEGER DEFAULT 0,
            functions TEXT,
            FOREIGN KEY (release_id) REFERENCES releases(id) ON DELETE CASCADE,
            FOREIGN KEY (scener_id) REFERENCES sceners(id),
            UNIQUE(release_id, scener_id, credit_type, handle)
        );
        
        -- Download links
        CREATE TABLE IF NOT EXISTS release_downloads (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            release_id INTEGER NOT NULL,
            url TEXT NOT NULL,
            counter_url TEXT,
            downloads INTEGER DEFAULT 0,
            status TEXT DEFAULT 'Ok',
            is_internal BOOLEAN DEFAULT false,
            file_type TEXT,
            FOREIGN KEY (release_id) REFERENCES releases(id) ON DELETE CASCADE,
            UNIQUE(release_id, url)
        );
        
        -- SID files
        CREATE TABLE IF NOT EXISTS release_sids (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            release_id INTEGER NOT NULL,
            sid_id INTEGER,
            hvsc_path TEXT,
            name TEXT NOT NULL,
            author TEXT,
            released TEXT,
            load_addr INTEGER,
            init_addr INTEGER,
            play_addr INTEGER,
            num_songs INTEGER DEFAULT 1,
            default_song INTEGER DEFAULT 1,
            sid_model TEXT,
            clock_speed TEXT,
            data_size INTEGER,
            FOREIGN KEY (release_id) REFERENCES releases(id) ON DELETE CASCADE,
            UNIQUE(release_id, sid_id)
        );
        
        -- Internet Archive mappings
        CREATE TABLE IF NOT EXISTS ia_mappings (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            release_id INTEGER NOT NULL,
            ia_identifier TEXT NOT NULL,
            ia_title TEXT,
            ia_collection TEXT DEFAULT 'softwarelibrary_c64',
            match_confidence REAL DEFAULT 1.0,
            last_checked TIMESTAMP,
            FOREIGN KEY (release_id) REFERENCES releases(id) ON DELETE CASCADE,
            UNIQUE(release_id, ia_identifier)
        );
        
        -- Full-text search for releases
        CREATE VIRTUAL TABLE IF NOT EXISTS releases_fts USING fts5(
            name,
            group_name,
            event_name,
            compo,
            tokenize="unicode61 remove_diacritics 2"
        );
        
        -- Full-text search for sceners
        CREATE VIRTUAL TABLE IF NOT EXISTS sceners_fts USING fts5(
            handle,
            aka,
            trivia,
            handle_story,
            tokenize="unicode61 remove_diacritics 2"
        );
        
        -- Full-text search for groups
        CREATE VIRTUAL TABLE IF NOT EXISTS groups_fts USING fts5(
            name,
            short,
            slogans,
            tokenize="unicode61 remove_diacritics 2"
        );
        
        -- Full-text search for events
        CREATE VIRTUAL TABLE IF NOT EXISTS events_fts USING fts5(
            name,
            city,
            country,
            tokenize="unicode61 remove_diacritics 2"
        );
        
        -- Indexes for performance
        CREATE INDEX IF NOT EXISTS idx_releases_name ON releases(name COLLATE NOCASE);
        CREATE INDEX IF NOT EXISTS idx_releases_group ON releases(group_id);
        CREATE INDEX IF NOT EXISTS idx_releases_event ON releases(event_id);
        CREATE INDEX IF NOT EXISTS idx_releases_year ON releases(release_year);
        CREATE INDEX IF NOT EXISTS idx_releases_rating ON releases(rating);
        CREATE INDEX IF NOT EXISTS idx_releases_type ON releases(release_type_id);
        CREATE INDEX IF NOT EXISTS idx_sceners_country ON sceners(country COLLATE NOCASE);
        CREATE INDEX IF NOT EXISTS idx_events_name ON events(name COLLATE NOCASE);
        CREATE INDEX IF NOT EXISTS idx_events_year ON events(start_year);
        CREATE INDEX IF NOT EXISTS idx_events_country ON events(country COLLATE NOCASE);
        CREATE INDEX IF NOT EXISTS idx_credits_type ON release_credits(credit_type);
        CREATE INDEX IF NOT EXISTS idx_credits_scener ON release_credits(scener_id);
        CREATE INDEX IF NOT EXISTS idx_credits_release ON release_credits(release_id);
        CREATE INDEX IF NOT EXISTS idx_downloads_release ON release_downloads(release_id);
    "#)?;
    
    Ok(())
}

/// Initialize default data (release types).
pub fn initialize_default_data(conn: &Connection) -> SqlResult<()> {
    conn.execute_batch(&r#"
        INSERT OR IGNORE INTO release_types (id, name, description) VALUES
            (1, 'C64 Demo', 'Commodore 64 Demo'),
            (2, 'C64 Game', 'Commodore 64 Game'),
            (7, 'C64 Music', 'Commodore 64 Music/SID'),
            (3, 'C64 Crack', 'Cracked Game'),
            (4, 'C64 Trainer', 'Trainer for Cracked Game'),
            (5, 'C64 Intro', 'Intro (usually for a crack)'),
            (6, 'C64 Graphics', 'Graphics Release'),
            (8, 'C64 Music Collection', 'Collection of SID tunes'),
            (9, 'C64 Demo Collection', 'Collection of demos'),
            (10, 'C64 Diskmag', 'Disk Magazine'),
            (11, 'C64 Papermag', 'Paper Magazine'),
            (12, 'C64 Tool', 'Utility or Tool'),
            (13, 'C64 Game Preview', 'Preview of a game'),
            (14, 'C64 Invitation', 'Party Invitation'),
            (15, 'C64 One-File Demo', 'Single-file demo'),
            (16, 'C64 4K Intro', '4KB Intro'),
            (17, 'C64 8K Intro', '8KB Intro'),
            (18, 'C64 16K Intro', '16KB Intro'),
            (19, 'C64 32K Intro', '32KB Intro'),
            (20, 'C64 64K Intro', '64KB Intro');
    "#)?;
    
    Ok(())
}

/// Check and update database version.
pub fn check_version(conn: &Connection) -> SqlResult<()> {
    let current_version: u32 = conn.query_row(
        "SELECT value FROM _meta WHERE key = 'version'",
        [],
        |row| row.get(0)
    ).unwrap_or(0);
    
    if current_version < DB_VERSION {
        migrate_database(conn, current_version)?;
        conn.execute(
            "INSERT OR REPLACE INTO _meta (key, value) VALUES ('version', ?)",
            [DB_VERSION.to_string()]
        )?;
    } else if current_version == 0 {
        conn.execute(
            "INSERT OR REPLACE INTO _meta (key, value) VALUES ('version', ?)",
            [DB_VERSION.to_string()]
        )?;
        initialize_default_data(conn)?;
    }
    
    Ok(())
}

/// Migrate database from old version to new version.
pub fn migrate_database(conn: &Connection, from_version: u32) -> SqlResult<()> {
    // Implement migrations for future schema changes
    if from_version < 1 {
        // Migration from 0 to 1: recreate all tables
        conn.execute_batch(
            "DROP TABLE IF EXISTS releases_fts; 
             DROP TABLE IF EXISTS sceners_fts; 
             DROP TABLE IF EXISTS groups_fts; 
             DROP TABLE IF EXISTS events_fts;"
        )?;
        create_tables(conn)?;
        initialize_default_data(conn)?;
    }
    
    Ok(())
}
