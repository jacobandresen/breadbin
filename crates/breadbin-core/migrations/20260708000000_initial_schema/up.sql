-- Initial schema migration for Breadbin Diesel database
-- This creates all tables needed for the CSDb cache

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
    screenshot_urls TEXT,
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

-- Indexes for performance
CREATE INDEX IF NOT EXISTS idx_groups_name ON groups(name COLLATE NOCASE);
CREATE INDEX IF NOT EXISTS idx_groups_country ON groups(base_country COLLATE NOCASE);
CREATE INDEX IF NOT EXISTS idx_sceners_country ON sceners(country COLLATE NOCASE);
CREATE INDEX IF NOT EXISTS idx_events_name ON events(name COLLATE NOCASE);
CREATE INDEX IF NOT EXISTS idx_events_year ON events(start_year);
CREATE INDEX IF NOT EXISTS idx_events_country ON events(country COLLATE NOCASE);
CREATE INDEX IF NOT EXISTS idx_releases_name ON releases(name COLLATE NOCASE);
CREATE INDEX IF NOT EXISTS idx_releases_group ON releases(group_id);
CREATE INDEX IF NOT EXISTS idx_releases_event ON releases(event_id);
CREATE INDEX IF NOT EXISTS idx_releases_year ON releases(release_year);
CREATE INDEX IF NOT EXISTS idx_releases_rating ON releases(rating);
CREATE INDEX IF NOT EXISTS idx_releases_type ON releases(release_type_id);
CREATE INDEX IF NOT EXISTS idx_credits_type ON release_credits(credit_type);
CREATE INDEX IF NOT EXISTS idx_credits_scener ON release_credits(scener_id);
CREATE INDEX IF NOT EXISTS idx_credits_release ON release_credits(release_id);
CREATE INDEX IF NOT EXISTS idx_downloads_release ON release_downloads(release_id);
CREATE INDEX IF NOT EXISTS idx_sids_release ON release_sids(release_id);
CREATE INDEX IF NOT EXISTS idx_ia_mappings_release ON ia_mappings(release_id);
