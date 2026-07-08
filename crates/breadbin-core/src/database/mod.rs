// database - SQLite database abstraction for enhanced CSDb data storage and search.
//
// This module provides a comprehensive database for storing demoscene data
// including releases, groups, sceners, events, and their relationships.
// It enables fast, offline-available search and filtering.

pub mod models;
pub mod schema_legacy;
pub mod search;
pub mod operations;

pub use schema_legacy as schema;

#[cfg(test)]
mod search_tests;

use std::path::PathBuf;

use rusqlite::Connection;

use crate::core;

// Database file name
pub fn db_path() -> PathBuf {
    core::data_path("csdb_cache.sqlite")
}

/// Initialize the database, creating tables if they don't exist.
pub fn initialize() -> Result<Connection, String> {
    operations::initialize()
        .map_err(|e| format!("Failed to initialize database: {}", e))
}

/// Open a read-only connection to the database.
pub fn open() -> Result<Connection, String> {
    operations::open()
        .map_err(|e| format!("Failed to open database: {}", e))
}

/// Open a read-write connection to the database.
pub fn open_rw() -> Result<Connection, String> {
    operations::open_rw()
        .map_err(|e| format!("Failed to open database for writing: {}", e))
}

/// Check if database exists and is valid.
pub fn database_exists() -> bool {
    operations::database_exists()
}

// Re-export commonly used types
pub use models::*;
pub use search::*;
pub use operations::*;
