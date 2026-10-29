// gb64 - the GameBase64 database (~30k games), a SQLite file downloaded and
// cached once in the user data dir. Supplies ratings, genres and curated flags.

use std::io::Read;

use rusqlite::{Connection, OpenFlags};

use crate::core;

const DB_URL: &str = "https://www.twinbirds.com/gamebase64browser/GBC_v19.sqlitedb.gz";
const UA: &str = "breadbin/1.0 (personal C64 collection tool; polite, low-volume)";

fn db_path() -> std::path::PathBuf {
    core::data_path("gb64.sqlitedb")
}

/// Download + cache the GB64 SQLite DB (gzip) if missing.
fn ensure_db() -> Result<(), String> {
    let db = db_path();
    if std::fs::metadata(&db).map(|m| m.len() > 0).unwrap_or(false) {
        return Ok(());
    }
    eprintln!("downloading GameBase64 SQLite database (~6MB) ...");
    let data = core::fetch(DB_URL, &[("User-Agent", UA)])?;
    // The .gz may arrive still gzipped, or already decompressed: ureq transparently
    // decodes Content-Encoding: gzip/x-gzip (the server tags the file that way), so
    // detect the real payload by its magic bytes rather than always gunzipping.
    let blob = if data.starts_with(&[0x1f, 0x8b]) {
        let mut out = Vec::new();
        flate2::read::GzDecoder::new(&data[..])
            .read_to_end(&mut out)
            .map_err(|e| e.to_string())?;
        out
    } else if data.starts_with(b"SQLite format 3\0") {
        data
    } else {
        return Err(format!(
            "unexpected response from {DB_URL} ({} bytes, not gzip or a SQLite database)",
            data.len()
        ));
    };
    std::fs::write(&db, &blob).map_err(|e| e.to_string())?;
    eprintln!("  cached {}MB -> {}", blob.len() / (1024 * 1024), db.display());
    Ok(())
}

/// Connect read-only to the GB64 DB, downloading it first if needed.
pub fn connect() -> Result<Connection, String> {
    ensure_db().map_err(|e| format!("could not obtain GB64 database: {e}"))?;
    Connection::open_with_flags(db_path(), OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| format!("could not open GB64 database: {e}"))
}

/// Decode a TEXT/INTEGER column leniently: UTF-8 if valid, else Windows-1252
/// (GB64 text comes from a .mdb origin), integers/reals stringified.
pub fn decode_text(v: rusqlite::types::ValueRef) -> Option<String> {
    use rusqlite::types::ValueRef::*;
    match v {
        Null => None,
        Integer(i) => Some(i.to_string()),
        Real(r) => Some(r.to_string()),
        Text(b) | Blob(b) => Some(match std::str::from_utf8(b) {
            Ok(s) => s.to_string(),
            Err(_) => encoding_rs::WINDOWS_1252.decode(b).0.into_owned(),
        }),
    }
}
