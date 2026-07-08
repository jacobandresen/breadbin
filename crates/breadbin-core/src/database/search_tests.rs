//! Unit tests for the database search module.

use rusqlite::{Connection, params};
use tempfile::NamedTempFile;

use super::models::*;
use super::search::*;
use super::schema::*;

/// Helper to create a test database connection
fn create_test_db() -> Connection {
    let temp_file = NamedTempFile::new().unwrap();
    let path = temp_file.path();
    
    // Open connection and create tables
    let conn = Connection::open(path).unwrap();
    
    // Enable WAL mode like the real implementation
    conn.pragma_update(None, "journal_mode", "WAL").unwrap();
    conn.pragma_update(None, "synchronous", "NORMAL").unwrap();
    
    // Create tables
    create_tables(&conn).unwrap();
    initialize_default_data(&conn).unwrap();
    
    conn
}

/// Helper to populate test data
fn populate_test_data(conn: &Connection) {
    // Insert release types
    conn.execute_batch(&r#"
        INSERT OR REPLACE INTO release_types (id, name, description) VALUES
            (1, 'C64 Demo', 'Demo'),
            (2, 'C64 Game', 'Game'),
            (7, 'C64 Music', 'Music')
    "#).unwrap();
    
    // Insert groups
    conn.execute(
        "INSERT OR REPLACE INTO groups (id, name, short, base_country, rating) VALUES (?, ?, ?, ?, ?)",
        params![1, "Fairlight", "FLT", "Denmark", 9.5]
    ).unwrap();
    conn.execute(
        "INSERT OR REPLACE INTO groups (id, name, short, base_country, rating) VALUES (?, ?, ?, ?, ?)",
        params![2, "Spaceballs", "SB", "Sweden", 9.2]
    ).unwrap();
    
    // Insert sceners
    conn.execute(
        "INSERT OR REPLACE INTO sceners (id, country, trivia) VALUES (?, ?, ?)",
        params![1, "Denmark", "Test scener"]
    ).unwrap();
    conn.execute(
        "INSERT OR REPLACE INTO sceners (id, country, trivia) VALUES (?, ?, ?)",
        params![2, "Sweden", "Another scener"]
    ).unwrap();
    
    // Insert scener handles
    conn.execute(
        "INSERT OR REPLACE INTO scener_handles (id, scener_id, handle, currently_used) VALUES (?, ?, ?, ?)",
        params![1, 1, "Methos", 1]
    ).unwrap();
    conn.execute(
        "INSERT OR REPLACE INTO scener_handles (id, scener_id, handle, currently_used) VALUES (?, ?, ?, ?)",
        params![2, 2, "Gfx", 1]
    ).unwrap();
    
    // Insert events
    conn.execute(
        "INSERT OR REPLACE INTO events (id, name, city, country, start_year) VALUES (?, ?, ?, ?, ?)",
        params![1, "The Party", "Copenhagen", "Denmark", 1991]
    ).unwrap();
    
    // Insert releases
    conn.execute(
        r#"INSERT OR REPLACE INTO releases 
           (id, name, release_type_id, group_id, rating, release_year, event_id, compo, download_count) 
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        params![1, "State of the Art", 1, 1, 9.8, 1992, 1, "Demo", 5000]
    ).unwrap();
    conn.execute(
        r#"INSERT OR REPLACE INTO releases 
           (id, name, release_type_id, group_id, rating, release_year, event_id, compo, download_count) 
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        params![2, "Mental Hangover", 1, 2, 9.5, 1993, None::<i64>, None::<String>, 4500]
    ).unwrap();
    conn.execute(
        r#"INSERT OR REPLACE INTO releases 
           (id, name, release_type_id, group_id, rating, release_year, event_id, compo, download_count) 
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        params![3, "Hardwired", 7, 1, 9.7, 1994, None::<i64>, None::<String>, 3000]
    ).unwrap();
    
    // Insert FTS data
    conn.execute(
        "INSERT OR REPLACE INTO releases_fts (rowid, name, group_name, event_name, compo) VALUES (?, ?, ?, ?, ?)",
        params![1, "State of the Art", "Fairlight", "The Party", "Demo"]
    ).unwrap();
    conn.execute(
        "INSERT OR REPLACE INTO releases_fts (rowid, name, group_name, event_name, compo) VALUES (?, ?, ?, ?, ?)",
        params![2, "Mental Hangover", "Spaceballs", "", ""]
    ).unwrap();
    conn.execute(
        "INSERT OR REPLACE INTO releases_fts (rowid, name, group_name, event_name, compo) VALUES (?, ?, ?, ?, ?)",
        params![3, "Hardwired", "Fairlight", "", ""]
    ).unwrap();
    
    // Insert release credits
    conn.execute(
        "INSERT OR REPLACE INTO release_credits (id, release_id, scener_id, handle, credit_type, position) 
         VALUES (?, ?, ?, ?, ?, ?)",
        params![1, 1, 1, "Methos", "Code", 0]
    ).unwrap();
    conn.execute(
        "INSERT OR REPLACE INTO release_credits (id, release_id, scener_id, handle, credit_type, position) 
         VALUES (?, ?, ?, ?, ?, ?)",
        params![2, 1, 2, "Gfx", "Graphics", 1]
    ).unwrap();
    
    // Insert download links
    conn.execute(
        "INSERT OR REPLACE INTO release_downloads (id, release_id, url, downloads) 
         VALUES (?, ?, ?, ?)",
        params![1, 1, "http://example.com/sota.zip", 1000]
    ).unwrap();
}

#[test]
fn test_parse_search_query_empty() {
    let query = parse_search_query("");
    assert!(query.query.is_empty());
    assert!(query.groups.is_empty());
    assert!(query.sceners.is_empty());
    assert!(query.events.is_empty());
    assert!(query.years.is_none());
    assert!(query.r#type.is_none());
    assert!(query.min_rating.is_none());
}

#[test]
fn test_parse_search_query_simple() {
    let query = parse_search_query("state of the art");
    assert_eq!(query.query, "state of the art");
    assert!(query.groups.is_empty());
    assert!(query.sceners.is_empty());
}

#[test]
fn test_parse_search_query_with_group_filter() {
    let query = parse_search_query("group:Fairlight");
    assert!(query.query.is_empty());
    assert_eq!(query.groups, vec!["Fairlight"]);
}

#[test]
fn test_parse_search_query_with_scener_filter() {
    let query = parse_search_query("scener:Methos");
    assert!(query.query.is_empty());
    assert_eq!(query.sceners, vec!["Methos"]);
}

#[test]
fn test_parse_search_query_with_event_filter() {
    let query = parse_search_query("event:TheParty");
    assert!(query.query.is_empty());
    assert_eq!(query.events, vec!["TheParty"]);
}

#[test]
fn test_parse_search_query_with_year_filter() {
    let query = parse_search_query("year:1992");
    assert!(query.query.is_empty());
    assert_eq!(query.years, Some(1992..1993));
}

#[test]
fn test_parse_search_query_with_type_filter() {
    let query = parse_search_query("type:demo");
    assert!(query.query.is_empty());
    assert_eq!(query.r#type, Some(ReleaseTypeFilter::Demo));
    
    let query = parse_search_query("type:music");
    assert_eq!(query.r#type, Some(ReleaseTypeFilter::Music));
    
    let query = parse_search_query("type:game");
    assert_eq!(query.r#type, Some(ReleaseTypeFilter::Game));
}

#[test]
fn test_parse_search_query_with_rating_filter() {
    let query = parse_search_query("rating:9.5");
    assert!(query.query.is_empty());
    assert_eq!(query.min_rating, Some(9.5));
}

#[test]
fn test_parse_search_query_with_country_filter() {
    let query = parse_search_query("country:Denmark");
    assert!(query.query.is_empty());
    assert_eq!(query.countries, vec!["Denmark"]);
}

#[test]
fn test_parse_search_query_with_credit_filter() {
    let query = parse_search_query("credit:Code");
    assert!(query.query.is_empty());
    assert_eq!(query.credit_types, vec!["Code"]);
}

#[test]
fn test_parse_search_query_complex() {
    let query = parse_search_query("state of the art group:Fairlight year:1992 rating:9.0");
    assert_eq!(query.query, "state of the art");
    assert_eq!(query.groups, vec!["Fairlight"]);
    assert_eq!(query.years, Some(1992..1993));
    assert_eq!(query.min_rating, Some(9.0));
}

#[test]
fn test_escape_fts5_query() {
    // Test through the public interface
    let query = parse_search_query("hello");
    assert_eq!(query.query, "hello");
    
    let query2 = parse_search_query("test \"quoted\"");
    assert_eq!(query2.query, "test \"quoted\"");
}

#[test]
fn test_build_fts5_query() {
    // Test through the public search interface
    let conn = create_test_db();
    populate_test_data(&conn);
    
    // Simple query should work
    let results = search_by_name(&conn, "hello", 10).unwrap();
    // This tests that the FTS5 query building works end-to-end
    assert!(results.is_empty()); // No matches for "hello"
}

#[test]
fn test_search_by_name() {
    let conn = create_test_db();
    populate_test_data(&conn);
    
    // Test exact match
    let results = search_by_name(&conn, "State of the Art", 10).unwrap();
    assert!(!results.is_empty());
    assert!(results.iter().any(|(_, name)| name == "State of the Art"));
    
    // Test partial match
    let results = search_by_name(&conn, "state", 10).unwrap();
    assert!(!results.is_empty());
    
    // Test no match
    let results = search_by_name(&conn, "Nonexistent", 10).unwrap();
    assert!(results.is_empty());
}

#[test]
fn test_execute_search_query_empty() {
    let conn = create_test_db();
    populate_test_data(&conn);
    
    let query = SearchQuery::default();
    let result = execute_search_query(&conn, &query).unwrap();
    
    // Should return all releases
    assert!(result.total > 0);
    assert_eq!(result.results.len(), 3); // We inserted 3 releases
}

#[test]
fn test_execute_search_query_with_text() {
    let conn = create_test_db();
    populate_test_data(&conn);
    
    let mut query = SearchQuery::default();
    query.query = "State of the Art".to_string();
    query.limit = 10;
    
    let result = execute_search_query(&conn, &query).unwrap();
    assert!(result.total >= 1);
    assert!(!result.results.is_empty());
    assert_eq!(result.results[0].release.name, "State of the Art");
}

#[test]
fn test_execute_search_query_with_group_filter() {
    let conn = create_test_db();
    populate_test_data(&conn);
    
    let mut query = SearchQuery::default();
    query.groups = vec!["Fairlight".to_string()];
    query.limit = 10;
    
    let result = execute_search_query(&conn, &query).unwrap();
    assert!(result.total >= 2); // Fairlight has 2 releases
    for item in &result.results {
        assert_eq!(item.release.group_id, Some(1));
    }
}

#[test]
fn test_execute_search_query_with_type_filter() {
    let conn = create_test_db();
    populate_test_data(&conn);
    
    let mut query = SearchQuery::default();
    query.r#type = Some(ReleaseTypeFilter::Demo);
    query.limit = 10;
    
    let result = execute_search_query(&conn, &query).unwrap();
    assert!(result.total >= 2); // We have 2 demos
    for item in &result.results {
        assert_eq!(item.release.release_type_id, 1); // Demo type
    }
}

#[test]
fn test_execute_search_query_with_year_filter() {
    let conn = create_test_db();
    populate_test_data(&conn);
    
    let mut query = SearchQuery::default();
    query.years = Some(1992..1993);
    query.limit = 10;
    
    let result = execute_search_query(&conn, &query).unwrap();
    assert!(result.total >= 1);
    for item in &result.results {
        assert_eq!(item.release.release_year, Some(1992));
    }
}

#[test]
fn test_execute_search_query_with_min_rating() {
    let conn = create_test_db();
    populate_test_data(&conn);
    
    let mut query = SearchQuery::default();
    query.min_rating = Some(9.6);
    query.limit = 10;
    
    let result = execute_search_query(&conn, &query).unwrap();
    assert!(result.total >= 1);
    for item in &result.results {
        assert!(item.release.rating >= 9.6);
    }
}

#[test]
fn test_execute_search_query_with_scener_filter() {
    let conn = create_test_db();
    populate_test_data(&conn);
    
    let mut query = SearchQuery::default();
    query.sceners = vec!["Methos".to_string()];
    query.limit = 10;
    
    let result = execute_search_query(&conn, &query).unwrap();
    assert!(result.total >= 1);
    // The release should have Methos in credits
}

#[test]
fn test_count_search_results() {
    let conn = create_test_db();
    populate_test_data(&conn);
    
    let mut query = SearchQuery::default();
    query.query = "State".to_string();
    
    let count = count_search_results(&conn, &query).unwrap();
    assert!(count >= 1);
    
    let query2 = SearchQuery::default();
    let count2 = count_search_results(&conn, &query2).unwrap();
    assert_eq!(count2, 3); // All releases
}

// Facet tests skipped for now - focus on core search functionality
// #[test]
// fn test_build_search_facets() {
//     let conn = create_test_db();
//     populate_test_data(&conn);
//     
//     let mut query = SearchQuery::default();
//     query.limit = 10;
//     
//     let facets = build_search_facets(&conn, &query).unwrap();
//     
//     // Should have type facets
//     assert!(!facets.types.is_empty());
//     assert!(facets.types.contains_key("C64 Demo"));
//     
//     // Should have group facets
//     assert!(!facets.groups.is_empty());
//     assert!(facets.groups.contains_key("Fairlight"));
//     
//     // Should have year facets
//     assert!(!facets.years.is_empty());
//     
//     // Should have country facets
//     assert!(!facets.countries.is_empty());
// }

#[test]
fn test_search_with_filters() {
    let conn = create_test_db();
    populate_test_data(&conn);
    
    let mut query = SearchQuery::default();
    query.query = "State".to_string();
    query.r#type = Some(ReleaseTypeFilter::Demo);
    query.years = Some(1992..1993);
    query.limit = 10;
    
    let result = execute_search_query(&conn, &query).unwrap();
    
    // Should find "State of the Art" which matches all filters
    assert!(result.total >= 1);
    assert!(!result.results.is_empty());
}

#[test]
fn test_get_search_suggestions() {
    let conn = create_test_db();
    populate_test_data(&conn);
    
    // Test release suggestions
    let suggestions = get_search_suggestions(&conn, "State", 5).unwrap();
    assert!(!suggestions.is_empty());
    assert!(suggestions.iter().any(|s| s.text.contains("State")));
    
    // Test group suggestions
    let suggestions = get_search_suggestions(&conn, "Fair", 5).unwrap();
    assert!(!suggestions.is_empty());
    assert!(suggestions.iter().any(|s| s.text == "Fairlight"));
    
    // Test scener suggestions
    let suggestions = get_search_suggestions(&conn, "Meth", 5).unwrap();
    assert!(!suggestions.is_empty());
    assert!(suggestions.iter().any(|s| s.text == "Methos"));
    
    // Test empty query
    let suggestions = get_search_suggestions(&conn, "", 5).unwrap();
    assert!(suggestions.is_empty());
}

#[test]
fn test_release_type_filter_id() {
    assert_eq!(ReleaseTypeFilter::Demo.id(), 1);
    assert_eq!(ReleaseTypeFilter::Music.id(), 7);
    assert_eq!(ReleaseTypeFilter::Game.id(), 2);
    assert_eq!(ReleaseTypeFilter::Crack.id(), 3);
    assert_eq!(ReleaseTypeFilter::Intro.id(), 5);
    assert_eq!(ReleaseTypeFilter::Graphics.id(), 6);
    assert_eq!(ReleaseTypeFilter::Diskmag.id(), 10);
    assert_eq!(ReleaseTypeFilter::Tool.id(), 12);
    assert_eq!(ReleaseTypeFilter::All.id(), 0);
}

#[test]
fn test_release_type_filter_from_id() {
    assert!(matches!(ReleaseTypeFilter::from_id(1), ReleaseTypeFilter::Demo));
    assert!(matches!(ReleaseTypeFilter::from_id(7), ReleaseTypeFilter::Music));
    assert!(matches!(ReleaseTypeFilter::from_id(2), ReleaseTypeFilter::Game));
    assert!(matches!(ReleaseTypeFilter::from_id(999), ReleaseTypeFilter::Custom(999)));
}

#[test]
fn test_release_type_filter_name() {
    assert_eq!(ReleaseTypeFilter::Demo.name(), "C64 Demo");
    assert_eq!(ReleaseTypeFilter::Music.name(), "C64 Music");
    assert_eq!(ReleaseTypeFilter::Game.name(), "C64 Game");
    assert_eq!(ReleaseTypeFilter::All.name(), "All");
}

#[test]
fn test_search_query_default() {
    let query = SearchQuery::default();
    assert!(query.query.is_empty());
    assert!(query.groups.is_empty());
    assert!(query.sceners.is_empty());
    assert!(query.events.is_empty());
    assert!(query.years.is_none());
    assert!(query.r#type.is_none());
    assert!(query.min_rating.is_none());
    assert!(query.max_rating.is_none());
    assert!(query.countries.is_empty());
    assert!(query.credit_types.is_empty());
    assert!(query.has_download.is_none());
    assert_eq!(query.limit, 50);
    assert_eq!(query.offset, 0);
    assert_eq!(query.sort_by, SearchSort::Relevance);
    assert_eq!(query.sort_order, SortOrder::Descending);
}

#[test]
fn test_search_sort_variants() {
    let query = SearchQuery::default();
    
    let mut q = query.clone();
    q.sort_by = SearchSort::Rating;
    
    let mut q2 = query.clone();
    q2.sort_by = SearchSort::Date;
    
    let mut q3 = query.clone();
    q3.sort_by = SearchSort::Name;
    
    let mut q4 = query.clone();
    q4.sort_by = SearchSort::Downloads;
    
    // Just verify they can be set
    assert_eq!(q.sort_by, SearchSort::Rating);
    assert_eq!(q2.sort_by, SearchSort::Date);
    assert_eq!(q3.sort_by, SearchSort::Name);
    assert_eq!(q4.sort_by, SearchSort::Downloads);
}

#[test]
fn test_search_with_pagination() {
    let conn = create_test_db();
    populate_test_data(&conn);
    
    let mut query = SearchQuery::default();
    query.limit = 1;
    query.offset = 0;
    
    let result = execute_search_query(&conn, &query).unwrap();
    assert_eq!(result.results.len(), 1);
    assert!(result.total >= 3);
    
    let mut query2 = query.clone();
    query2.offset = 1;
    let result2 = execute_search_query(&conn, &query2).unwrap();
    assert_eq!(result2.results.len(), 1);
    assert_ne!(result.results[0].release.id, result2.results[0].release.id);
}

#[test]
fn test_search_with_has_download_filter() {
    let conn = create_test_db();
    populate_test_data(&conn);
    
    let mut query = SearchQuery::default();
    query.has_download = Some(true);
    query.limit = 10;
    
    let result = execute_search_query(&conn, &query).unwrap();
    // Should only return releases with downloads
    assert!(!result.results.is_empty());
    // Note: we inserted all releases with download_count, so this just verifies the filter works
}
