// Integration tests for the Diesel database layer, exercising the public
// domain-model API. The database lives in a per-process temp directory so
// tests never touch the user's real data.

use std::sync::Once;

use breadbin_core::database::models::{DownloadLink, Release, ReleaseCredit, SearchQuery};
use breadbin_core::diesel_db::*;

static INIT: Once = Once::new();

fn setup() {
    INIT.call_once(|| {
        let dir = std::env::temp_dir().join(format!("breadbin-diesel-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("BREADBIN_USER_DATA", &dir);
        std::env::remove_var("BREADBIN_HOME");
        initialize_pool().expect("Failed to initialize pool");
        initialize_default_release_types().expect("Failed to seed release types");
        seed_releases();
    });
}

fn release(id: u32, name: &str, type_id: u32, rating: f32, year: u32) -> Release {
    Release {
        id,
        name: name.to_string(),
        release_type_id: type_id,
        rating,
        release_year: Some(year),
        ..Default::default()
    }
}

fn seed_releases() {
    let mut sota = release(1, "State of the Art", 1, 9.8, 1992);
    sota.credits.push(ReleaseCredit {
        id: None,
        release_id: 1,
        scener_id: Some(1),
        handle: "Methos".to_string(),
        credit_type: "Code".to_string(),
        position: 0,
        functions: vec!["Code".to_string()],
    });
    sota.downloads.push(DownloadLink {
        id: None,
        release_id: 1,
        url: "http://example.com/sota.zip".to_string(),
        counter_url: None,
        downloads: 1000,
        status: "Ok".to_string(),
        is_internal: false,
        file_type: Some("zip".to_string()),
    });
    save_release(&sota).unwrap();
    save_release(&release(2, "Mental Hangover", 1, 9.5, 1993)).unwrap();
    save_release(&release(3, "Hardwired Tune", 7, 9.7, 1994)).unwrap();
}

#[test]
fn test_database_initializes_with_migrations() {
    setup();
    assert!(diesel_database_exists());
    assert!(has_data());
}

#[test]
fn test_save_release_roundtrip_with_children() {
    setup();
    let top = get_top_releases(10).unwrap();
    let sota = top.iter().find(|r| r.id == 1).expect("release 1 saved");
    assert_eq!(sota.name, "State of the Art");
    assert_eq!(sota.release_type_id, 1);
    assert_eq!(sota.rating, 9.8);
    assert_eq!(sota.release_year, Some(1992));
    assert_eq!(sota.downloads.len(), 1);
    assert_eq!(sota.downloads[0].url, "http://example.com/sota.zip");
}

#[test]
fn test_save_release_is_upsert() {
    setup();
    let mut r = release(42, "Upsert Me", 1, 5.0, 1990);
    save_release(&r).unwrap();
    r.rating = 8.0;
    save_release(&r).unwrap();

    let saved = get_top_releases(100)
        .unwrap()
        .into_iter()
        .find(|r| r.id == 42)
        .unwrap();
    assert_eq!(saved.rating, 8.0);
}

#[test]
fn test_get_top_releases_ordered_by_rating() {
    setup();
    let top = get_top_releases(2).unwrap();
    assert_eq!(top.len(), 2);
    assert!(top[0].rating >= top[1].rating);
}

#[test]
fn test_list_releases_filters_by_type() {
    setup();
    let music = list_releases(&[7, 8]).unwrap();
    assert!(music.iter().any(|l| l.release.id == 3));
    assert!(music.iter().all(|l| [7, 8].contains(&l.release.release_type_id)));
}

#[test]
fn test_search_by_name_matches_partial() {
    setup();
    let results = search_by_name("state", 10).unwrap();
    assert!(results.iter().any(|(_, name)| name == "State of the Art"));

    let none = search_by_name("nonexistent-xyzzy", 10).unwrap();
    assert!(none.is_empty());
}

#[test]
fn test_execute_search_query_filters() {
    setup();

    let mut query = SearchQuery::default();
    query.years = Some(1992..1993);
    let result = execute_search_query(&query).unwrap();
    assert!(result.results.iter().all(|i| i.release.release_year == Some(1992)));
    assert!(result.results.iter().any(|i| i.release.id == 1));

    let mut query = SearchQuery::default();
    query.min_rating = Some(9.6);
    let result = execute_search_query(&query).unwrap();
    assert!(result.total >= 2);
    assert!(result.results.iter().all(|i| i.release.rating >= 9.6));

    let mut query = SearchQuery::default();
    query.sceners = vec!["Methos".to_string()];
    let result = execute_search_query(&query).unwrap();
    assert!(result.results.iter().any(|i| i.release.id == 1));
}

#[test]
fn test_search_pagination() {
    setup();
    let mut query = SearchQuery::default();
    query.limit = 1;
    let page1 = execute_search_query(&query).unwrap();
    assert_eq!(page1.results.len(), 1);
    assert!(page1.total >= 3);

    query.offset = 1;
    let page2 = execute_search_query(&query).unwrap();
    assert_ne!(page1.results[0].release.id, page2.results[0].release.id);
}

#[test]
fn test_meta_and_version() {
    setup();
    set_meta_value("test_key", "test_value").unwrap();
    assert_eq!(get_meta_value("test_key").unwrap().as_deref(), Some("test_value"));
    assert_eq!(get_meta_value("missing_key").unwrap(), None);

    set_db_version(3).unwrap();
    assert_eq!(get_db_version().unwrap(), 3);
}

#[test]
fn test_get_stats() {
    setup();
    let stats = get_stats().unwrap();
    assert!(stats.total_records > 0);
    assert!(*stats.table_counts.get("releases").unwrap() >= 3);
    assert!(*stats.table_counts.get("release_types").unwrap() >= 20);
}
