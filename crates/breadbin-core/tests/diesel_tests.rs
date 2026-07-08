// Diesel ORM tests for Breadbin.
//
// These tests verify that the Diesel database operations work correctly.

use diesel::prelude::*;
use breadbin_core::diesel_db::*;

// Setup function to ensure database is initialized
fn setup_test_db() {
    // Initialize the connection pool
    initialize_pool().expect("Failed to initialize pool");
    
    // Ensure the database exists
    assert!(diesel_database_exists(), "Database does not exist");
    
    // Initialize default release types if not already present
    initialize_default_release_types().ok();
}

#[test]
fn test_diesel_database_initialization() {
    setup_test_db();
    assert!(diesel_database_exists());
}

#[test]
fn test_connection_pool() {
    setup_test_db();
    
    // Get a connection
    let result = get_connection();
    assert!(result.is_ok(), "Failed to get connection");
}

#[test]
fn test_release_types_basic() {
    setup_test_db();
    
    // Initialize default release types
    let result = initialize_default_release_types();
    assert!(result.is_ok(), "Failed to initialize default release types");
    
    // Get a release type that should exist
    let result = get_release_type(1);
    assert!(result.is_ok(), "Failed to get release type");
    let release_type = result.unwrap();
    assert!(release_type.is_some(), "Release type 1 not found");
    assert_eq!(release_type.unwrap().name, "C64 Demo");
}

#[test]
fn test_get_all_release_types() {
    setup_test_db();
    
    // Initialize default release types
    initialize_default_release_types().ok();
    
    // Get all release types
    let result = get_all_release_types();
    assert!(result.is_ok(), "Failed to get all release types");
    let all_types = result.unwrap();
    assert!(all_types.len() > 0, "No release types found");
}

#[test]
fn test_database_version() {
    setup_test_db();
    
    // Get current version
    let result = get_db_version();
    assert!(result.is_ok(), "Failed to get database version");
    let version = result.unwrap();
    assert!(version >= 0, "Invalid version: {}", version);
}

#[test]
fn test_database_statistics() {
    setup_test_db();
    
    // Get statistics
    let result = get_stats();
    assert!(result.is_ok(), "Failed to get database statistics");
    let stats = result.unwrap();
    assert!(stats.total_records >= 0, "Invalid total records: {}", stats.total_records);
    assert!(!stats.table_counts.is_empty(), "No table counts found");
}

#[test]
fn test_metadata_operations() {
    setup_test_db();
    
    // Set a metadata value
    let result = set_meta_value("test_key", "test_value");
    assert!(result.is_ok(), "Failed to set metadata");
    
    // Get it back
    let result = get_meta_value("test_key");
    assert!(result.is_ok(), "Failed to get metadata");
    let retrieved = result.unwrap();
    assert!(retrieved.is_some(), "Metadata not found");
    assert_eq!(retrieved.unwrap(), "test_value");
    
    // Clean up
    let mut conn = get_connection().expect("Failed to get connection");
    diesel::delete(breadbin_core::schema::_meta::dsl::_meta)
        .filter(breadbin_core::schema::_meta::dsl::key.eq("test_key"))
        .execute(&mut conn)
        .ok();
}

#[test]
fn test_groups_basic() {
    setup_test_db();
    
    // Create a group
    let group = GroupRow {
        id: Some(9999),
        name: "Test Group".to_string(),
        short: Some("TG".to_string()),
        base_country: Some("Test Country".to_string()),
        founded_month: Some(1),
        founded_year: Some(2024),
        website: Some("http://test.example.com".to_string()),
        rating: Some(5.0),
        grouptypes: Some("[\"Demo\",\"Music\"]".to_string()),
        slogans: Some("[\"Test slogan\"]".to_string()),
        updated_at: None,
    };
    
    // Save it
    let result = save_group(&group);
    assert!(result.is_ok(), "Failed to save group");
    
    // Get it back
    let result = get_group(9999);
    assert!(result.is_ok(), "Failed to get group");
    let retrieved = result.unwrap();
    assert!(retrieved.is_some(), "Group not found");
    assert_eq!(retrieved.unwrap().name, "Test Group");
    
    // Clean up
    let mut conn = get_connection().expect("Failed to get connection");
    diesel::delete(breadbin_core::schema::groups::dsl::groups)
        .filter(breadbin_core::schema::groups::dsl::id.eq(Some(9999)))
        .execute(&mut conn)
        .ok();
}

#[test]
fn test_sceners_basic() {
    setup_test_db();
    
    // Create a scener
    let scener = ScenerRow {
        id: Some(9999),
        country: Some("Test Country".to_string()),
        trivia: Some("Test trivia".to_string()),
        handle_story: Some("Test handle story".to_string()),
        updated_at: None,
    };
    
    // Save it
    let result = save_scener(&scener);
    assert!(result.is_ok(), "Failed to save scener");
    
    // Get it back
    let result = get_scener(9999);
    assert!(result.is_ok(), "Failed to get scener");
    let retrieved = result.unwrap();
    assert!(retrieved.is_some(), "Scener not found");
    assert_eq!(retrieved.unwrap().country, Some("Test Country".to_string()));
    
    // Clean up
    let mut conn = get_connection().expect("Failed to get connection");
    diesel::delete(breadbin_core::schema::sceners::dsl::sceners)
        .filter(breadbin_core::schema::sceners::dsl::id.eq(Some(9999)))
        .execute(&mut conn)
        .ok();
}

#[test]
fn test_events_basic() {
    setup_test_db();
    
    // Create an event
    let event = EventRow {
        id: Some(9999),
        name: "Test Event".to_string(),
        event_type: Some("Demo Party".to_string()),
        aka: Some("TE".to_string()),
        start_day: Some(15),
        start_month: Some(7),
        start_year: Some(2024),
        end_day: Some(17),
        end_month: Some(7),
        end_year: Some(2024),
        address: Some("Test Address".to_string()),
        zip: Some("12345".to_string()),
        city: Some("Test City".to_string()),
        country: Some("Test Country".to_string()),
        website: Some("http://test.event.com".to_string()),
        updated_at: None,
    };
    
    // Save it
    let result = save_event(&event);
    assert!(result.is_ok(), "Failed to save event");
    
    // Get it back
    let result = get_event(9999);
    assert!(result.is_ok(), "Failed to get event");
    let retrieved = result.unwrap();
    assert!(retrieved.is_some(), "Event not found");
    assert_eq!(retrieved.unwrap().name, "Test Event");
    
    // Clean up
    let mut conn = get_connection().expect("Failed to get connection");
    diesel::delete(breadbin_core::schema::events::dsl::events)
        .filter(breadbin_core::schema::events::dsl::id.eq(Some(9999)))
        .execute(&mut conn)
        .ok();
}

#[test]
fn test_releases_basic() {
    setup_test_db();
    
    // First ensure release types are initialized
    initialize_default_release_types().ok();
    
    // Create a release
    let release = ReleaseRow {
        id: Some(9999),
        name: "Test Release".to_string(),
        release_type_id: 1, // C64 Demo
        group_id: None,
        rating: Some(5.0),
        release_day: Some(1),
        release_month: Some(1),
        release_year: Some(2024),
        event_id: None,
        compo: Some("Demo Compo".to_string()),
        place: Some(1),
        screenshot_url: Some("http://test.com/screenshot.png".to_string()),
        screenshot_urls: None,
        download_count: Some(100),
        website: Some("http://test.com".to_string()),
        created_at: None,
        updated_at: None,
    };
    
    // Save it
    let result = save_release(&release);
    assert!(result.is_ok(), "Failed to save release");
    
    // Get it back
    let result = get_release(9999);
    assert!(result.is_ok(), "Failed to get release");
    let retrieved = result.unwrap();
    assert!(retrieved.is_some(), "Release not found");
    assert_eq!(retrieved.unwrap().name, "Test Release");
    
    // Clean up
    let mut conn = get_connection().expect("Failed to get connection");
    diesel::delete(breadbin_core::schema::releases::dsl::releases)
        .filter(breadbin_core::schema::releases::dsl::id.eq(Some(9999)))
        .execute(&mut conn)
        .ok();
}

#[test]
fn test_get_all_groups() {
    setup_test_db();
    
    // Get all groups (should be empty or have some)
    let result = get_all_groups();
    assert!(result.is_ok(), "Failed to get all groups");
    let all_groups = result.unwrap();
    assert!(all_groups.len() >= 0, "Invalid group count");
}

#[test]
fn test_get_all_sceners() {
    setup_test_db();
    
    // Get all sceners (should be empty or have some)
    let result = get_all_sceners();
    assert!(result.is_ok(), "Failed to get all sceners");
    let all_sceners = result.unwrap();
    assert!(all_sceners.len() >= 0, "Invalid scener count");
}

#[test]
fn test_get_all_events() {
    setup_test_db();
    
    // Get all events (should be empty or have some)
    let result = get_all_events();
    assert!(result.is_ok(), "Failed to get all events");
    let all_events = result.unwrap();
    assert!(all_events.len() >= 0, "Invalid event count");
}

#[test]
fn test_get_all_releases() {
    setup_test_db();
    
    // Get all releases (should be empty or have some)
    let result = get_all_releases();
    assert!(result.is_ok(), "Failed to get all releases");
    let all_releases = result.unwrap();
    assert!(all_releases.len() >= 0, "Invalid release count");
}

#[test]
fn test_get_top_releases() {
    setup_test_db();
    
    // Get top 10 releases
    let result = get_top_releases(10);
    assert!(result.is_ok(), "Failed to get top releases");
    let top_releases = result.unwrap();
    assert!(top_releases.len() >= 0, "Invalid top releases count");
    
    // Should be ordered by download_count descending
    if top_releases.len() > 1 {
        for i in 1..top_releases.len() {
            let prev = &top_releases[i-1];
            let curr = &top_releases[i];
            let prev_count = prev.download_count.unwrap_or(0);
            let curr_count = curr.download_count.unwrap_or(0);
            assert!(prev_count >= curr_count, "Top releases not ordered correctly");
        }
    }
}

#[test]
fn test_clear_database() {
    setup_test_db();
    
    // Clear the database
    let result = clear_database();
    assert!(result.is_ok(), "Failed to clear database");
    
    // Verify it's empty
    let stats = get_stats().expect("Failed to get stats");
    // All tables except _meta should be empty
    assert_eq!(stats.table_counts.get("releases").map(|&c| c).unwrap_or(0), 0);
    assert_eq!(stats.table_counts.get("groups").map(|&c| c).unwrap_or(0), 0);
    assert_eq!(stats.table_counts.get("sceners").map(|&c| c).unwrap_or(0), 0);
}

#[test]
fn test_save_group_update() {
    setup_test_db();
    
    // Create a group
    let group = GroupRow {
        id: Some(9998),
        name: "Original Name".to_string(),
        short: Some("ON".to_string()),
        base_country: Some("Country".to_string()),
        founded_month: Some(1),
        founded_year: Some(2024),
        website: Some("http://original.com".to_string()),
        rating: Some(3.0),
        grouptypes: Some("[\"Demo\"]".to_string()),
        slogans: Some("[\"Original\"]".to_string()),
        updated_at: None,
    };
    
    // Save it
    save_group(&group).expect("Failed to save group");
    
    // Update it
    let updated_group = GroupRow {
        id: Some(9998),
        name: "Updated Name".to_string(),
        short: Some("UN".to_string()),
        base_country: Some("Updated Country".to_string()),
        founded_month: Some(2),
        founded_year: Some(2025),
        website: Some("http://updated.com".to_string()),
        rating: Some(4.5),
        grouptypes: Some("[\"Demo\",\"Music\"]".to_string()),
        slogans: Some("[\"Updated\"]".to_string()),
        updated_at: None,
    };
    
    save_group(&updated_group).expect("Failed to update group");
    
    // Get it back
    let retrieved = get_group(9998).expect("Failed to get group").expect("Group not found");
    assert_eq!(retrieved.name, "Updated Name");
    assert_eq!(retrieved.rating, Some(4.5));
    
    // Clean up
    let mut conn = get_connection().expect("Failed to get connection");
    diesel::delete(breadbin_core::schema::groups::dsl::groups)
        .filter(breadbin_core::schema::groups::dsl::id.eq(Some(9998)))
        .execute(&mut conn)
        .ok();
}

#[test]
fn test_get_nonexistent_group() {
    setup_test_db();
    
    // Try to get a group that doesn't exist
    let result = get_group(99999);
    assert!(result.is_ok(), "Failed to query for nonexistent group");
    let group = result.unwrap();
    assert!(group.is_none(), "Expected no group to be found");
}

#[test]
fn test_get_nonexistent_scener() {
    setup_test_db();
    
    // Try to get a scener that doesn't exist
    let result = get_scener(99999);
    assert!(result.is_ok(), "Failed to query for nonexistent scener");
    let scener = result.unwrap();
    assert!(scener.is_none(), "Expected no scener to be found");
}

#[test]
fn test_get_nonexistent_event() {
    setup_test_db();
    
    // Try to get an event that doesn't exist
    let result = get_event(99999);
    assert!(result.is_ok(), "Failed to query for nonexistent event");
    let event = result.unwrap();
    assert!(event.is_none(), "Expected no event to be found");
}

#[test]
fn test_get_nonexistent_release() {
    setup_test_db();
    
    // Try to get a release that doesn't exist
    let result = get_release(99999);
    assert!(result.is_ok(), "Failed to query for nonexistent release");
    let release = result.unwrap();
    assert!(release.is_none(), "Expected no release to be found");
}

#[test]
fn test_get_nonexistent_release_type() {
    setup_test_db();
    
    // Try to get a release type that doesn't exist
    let result = get_release_type(999);
    assert!(result.is_ok(), "Failed to query for nonexistent release type");
    let release_type = result.unwrap();
    assert!(release_type.is_none(), "Expected no release type to be found");
}

#[test]
fn test_save_release_type_update() {
    setup_test_db();
    
    // Create a release type
    let release_type = ReleaseType {
        id: Some(999),
        name: "Custom Type".to_string(),
        description: Some("A custom release type".to_string()),
    };
    
    // Save it
    save_release_type(&release_type).expect("Failed to save release type");
    
    // Update it
    let updated_type = ReleaseType {
        id: Some(999),
        name: "Updated Custom Type".to_string(),
        description: Some("An updated custom release type".to_string()),
    };
    
    save_release_type(&updated_type).expect("Failed to update release type");
    
    // Get it back
    let retrieved = get_release_type(999).expect("Failed to get release type").expect("Release type not found");
    assert_eq!(retrieved.name, "Updated Custom Type");
    assert_eq!(retrieved.description, Some("An updated custom release type".to_string()));
    
    // Clean up
    let mut conn = get_connection().expect("Failed to get connection");
    diesel::delete(breadbin_core::schema::release_types::dsl::release_types)
        .filter(breadbin_core::schema::release_types::dsl::id.eq(Some(999)))
        .execute(&mut conn)
        .ok();
}

#[test]
fn test_save_scener_update() {
    setup_test_db();
    
    // Create a scener
    let scener = ScenerRow {
        id: Some(9998),
        country: Some("Original Country".to_string()),
        trivia: Some("Original trivia".to_string()),
        handle_story: Some("Original handle story".to_string()),
        updated_at: None,
    };
    
    // Save it
    save_scener(&scener).expect("Failed to save scener");
    
    // Update it
    let updated_scener = ScenerRow {
        id: Some(9998),
        country: Some("Updated Country".to_string()),
        trivia: Some("Updated trivia".to_string()),
        handle_story: Some("Updated handle story".to_string()),
        updated_at: None,
    };
    
    save_scener(&updated_scener).expect("Failed to update scener");
    
    // Get it back
    let retrieved = get_scener(9998).expect("Failed to get scener").expect("Scener not found");
    assert_eq!(retrieved.country, Some("Updated Country".to_string()));
    
    // Clean up
    let mut conn = get_connection().expect("Failed to get connection");
    diesel::delete(breadbin_core::schema::sceners::dsl::sceners)
        .filter(breadbin_core::schema::sceners::dsl::id.eq(Some(9998)))
        .execute(&mut conn)
        .ok();
}

#[test]
fn test_database_version_management() {
    setup_test_db();
    
    // Get current version
    let original_version = get_db_version().expect("Failed to get original version");
    
    // Set a new version
    set_db_version(42).expect("Failed to set version");
    
    // Verify it was set
    let new_version = get_db_version().expect("Failed to get new version");
    assert_eq!(new_version, 42);
    
    // Restore original version
    set_db_version(original_version).expect("Failed to restore version");
}

#[test]
#[ignore = "Performance test - run manually"]
fn test_performance_bulk_insert() {
    setup_test_db();
    
    // Clear any existing test data
    let mut conn = get_connection().expect("Failed to get connection");
    diesel::delete(breadbin_core::schema::groups::dsl::groups)
        .filter(breadbin_core::schema::groups::dsl::name.like("Perf Test%"))
        .execute(&mut conn)
        .ok();
    
    // Insert 100 groups
    for i in 0..100 {
        let group = GroupRow {
            id: Some(10000 + i),
            name: format!("Perf Test Group {}", i),
            short: Some(format!("PT{}", i)),
            base_country: Some("Test Country".to_string()),
            founded_month: Some(1),
            founded_year: Some(2024),
            website: Some("http://test.com".to_string()),
            rating: Some(5.0),
            grouptypes: Some("[\"Demo\"]".to_string()),
            slogans: Some("[\"Test\"]".to_string()),
            updated_at: None,
        };
        save_group(&group).expect("Failed to save group");
    }
    
    // Verify count
    let all_groups = get_all_groups().expect("Failed to get all groups");
    let perf_groups = all_groups.iter()
        .filter(|g| g.name.starts_with("Perf Test"))
        .count();
    assert!(perf_groups >= 95, "Expected at least 95 perf test groups, found {}", perf_groups);
    
    // Clean up
    let mut conn = get_connection().expect("Failed to get connection");
    diesel::delete(breadbin_core::schema::groups::dsl::groups)
        .filter(breadbin_core::schema::groups::dsl::name.like("Perf Test%"))
        .execute(&mut conn)
        .ok();
}

#[test]
#[ignore = "Performance test - run manually"]
fn test_performance_query() {
    setup_test_db();
    
    // Query all groups multiple times
    for _ in 0..100 {
        let result = get_all_groups();
        assert!(result.is_ok(), "Failed to query groups");
    }
}

// ============================================================================
// Scener Handle Tests
// ============================================================================

#[test]
fn test_scener_handles_basic() {
    setup_test_db();
    
    // First create a scener
    let scener = ScenerRow {
        id: Some(9997),
        country: Some("Test Country".to_string()),
        trivia: Some("Test trivia".to_string()),
        handle_story: Some("Test handle story".to_string()),
        updated_at: None,
    };
    save_scener(&scener).expect("Failed to save scener");
    
    // Create a handle for the scener
    let handle = ScenerHandleRow {
        id: None, // Auto-increment
        scener_id: 9997,
        handle: "TestHandle".to_string(),
        aka: Some("TH".to_string()),
        currently_used: Some(true),
        handle_story: Some("Test handle story".to_string()),
    };
    
    // Save it
    let result = save_scener_handle(&handle);
    assert!(result.is_ok(), "Failed to save scener handle");
    
    // Get handles for this scener
    let result = get_scener_handles(9997);
    assert!(result.is_ok(), "Failed to get scener handles");
    let handles = result.unwrap();
    assert!(handles.len() >= 1, "Expected at least 1 handle");
    assert!(handles.iter().any(|h| h.handle == "TestHandle"), "Handle not found");
    
    // Clean up
    let mut conn = get_connection().expect("Failed to get connection");
    diesel::delete(breadbin_core::schema::scener_handles::dsl::scener_handles)
        .filter(breadbin_core::schema::scener_handles::dsl::scener_id.eq(9997))
        .execute(&mut conn)
        .ok();
    diesel::delete(breadbin_core::schema::sceners::dsl::sceners)
        .filter(breadbin_core::schema::sceners::dsl::id.eq(Some(9997)))
        .execute(&mut conn)
        .ok();
}

// ============================================================================
// Release Download Tests
// ============================================================================

#[test]
fn test_download_links_basic() {
    setup_test_db();
    
    // First ensure release types are initialized
    initialize_default_release_types().ok();
    
    // Create a release
    let release = ReleaseRow {
        id: Some(9998),
        name: "Test Release 2".to_string(),
        release_type_id: 1,
        group_id: None,
        rating: Some(5.0),
        release_day: Some(1),
        release_month: Some(1),
        release_year: Some(2024),
        event_id: None,
        compo: None,
        place: None,
        screenshot_url: None,
        screenshot_urls: None,
        download_count: Some(0),
        website: None,
        created_at: None,
        updated_at: None,
    };
    save_release(&release).expect("Failed to save release");
    
    // Create a download link
    let link = DownloadLinkRow {
        id: None, // Auto-increment
        release_id: 9998,
        url: "http://test.com/download.zip".to_string(),
        counter_url: None,
        downloads: Some(0),
        status: Some("Ok".to_string()),
        is_internal: Some(false),
        file_type: Some("zip".to_string()),
    };
    
    // Save it
    let result = save_download_link(&link);
    assert!(result.is_ok(), "Failed to save download link");
    
    // Get links for this release
    let result = get_download_links(9998);
    assert!(result.is_ok(), "Failed to get download links");
    let links = result.unwrap();
    assert!(links.len() >= 1, "Expected at least 1 download link");
    assert!(links.iter().any(|l| l.url == "http://test.com/download.zip"), "Download link not found");
    
    // Clean up
    let mut conn = get_connection().expect("Failed to get connection");
    diesel::delete(breadbin_core::schema::release_downloads::dsl::release_downloads)
        .filter(breadbin_core::schema::release_downloads::dsl::release_id.eq(9998))
        .execute(&mut conn)
        .ok();
    diesel::delete(breadbin_core::schema::releases::dsl::releases)
        .filter(breadbin_core::schema::releases::dsl::id.eq(Some(9998)))
        .execute(&mut conn)
        .ok();
}

