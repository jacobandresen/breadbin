// search - Full-text search and filtering implementation.
//
// This module provides advanced full-text search functionality for the CSDb database
// using SQLite FTS5 virtual tables for fast, relevance-ranked search results.

use rusqlite::{Connection, params, ToSql};

use super::models::*;
use super::schema;

/// Parse a search query string into a SearchQuery.
pub fn parse_search_query(input: &str) -> SearchQuery {
    let mut query = SearchQuery::default();
    
    // Tokenize input
    let tokens: Vec<&str> = input.split_whitespace().collect();
    let mut remaining = Vec::new();
    
    for token in &tokens {
        if token.starts_with("group:") {
            query.groups.push(token["group:".len()..].to_string());
        } else if token.starts_with("scener:") {
            query.sceners.push(token["scener:".len()..].to_string());
        } else if token.starts_with("event:") {
            query.events.push(token["event:".len()..].to_string());
        } else if token.starts_with("year:") {
            if let Ok(year) = token["year:".len()..].parse::<u32>() {
                query.years = Some(year..year + 1);
            }
        } else if token.starts_with("type:") {
            let type_name = &token["type:".len()..].to_lowercase();
            query.r#type = match type_name.as_str() {
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
        } else if token.starts_with("rating:") {
            if let Ok(rating) = token["rating:".len()..].parse::<f32>() {
                query.min_rating = Some(rating);
            }
        } else if token.starts_with("country:") {
            query.countries.push(token["country:".len()..].to_string());
        } else if token.starts_with("credit:") {
            query.credit_types.push(token["credit:".len()..].to_string());
        } else {
            remaining.push(*token);
        }
    }
    
    query.query = remaining.join(" ");
    query
}

/// Escape a string for FTS5 query to prevent injection and handle special characters.
fn escape_fts5_query(query: &str) -> String {
    query.chars()
        .map(|c| match c {
            '"' => "\\\"".to_string(),
            _ => c.to_string(),
        })
        .collect()
}

/// Build an FTS5 query from the search query string.
/// Supports phrase search with quotes, AND/OR operators, and prefix search.
fn build_fts5_query(query: &str) -> String {
    if query.is_empty() {
        return String::new();
    }
    
    // For now, use simple matching. Future enhancements could include:
    // - Phrase search: "exact phrase"
    // - AND/OR operators: +required -exclude
    // - Prefix search: prefix*
    // - Fuzzy search: term~2
    
    // Escape and quote the query for NEAR distance search
    // This provides better relevance ranking
    format!("\"{}\"", escape_fts5_query(query))
}

/// Execute a basic search query and return matching releases.
/// Uses FTS5 full-text search for fast, relevance-ranked results.
pub fn execute_search_query(
    conn: &Connection,
    query: &SearchQuery,
) -> Result<SearchResult, String> {
    let mut sql = String::new();
    let mut conditions = Vec::new();
    let mut params: Vec<Box<dyn ToSql>> = Vec::new();
    
    // Build the main query using FTS5 if query text is provided
    if !query.query.is_empty() {
        let fts5_query = build_fts5_query(&query.query);
        sql.push_str("SELECT r.id, r.name, r.release_type_id, r.group_id, r.rating, r.release_year, ");
        sql.push_str("r.event_id, r.compo, r.place, r.screenshot_url, r.download_count,");
        sql.push_str(&format!("releases_fts.rank as score, '' as name_highlight "));
        sql.push_str("FROM releases r ");
        sql.push_str("JOIN releases_fts ON releases_fts.rowid = r.id ");
        sql.push_str(&format!("WHERE releases_fts.releases_fts MATCH ?{} ", params.len() + 1));
        params.push(Box::new(fts5_query) as Box<dyn ToSql>);
    } else {
        // No full-text search, just filter on releases table
        sql.push_str("SELECT r.id, r.name, r.release_type_id, r.group_id, r.rating, r.release_year, ");
        sql.push_str("r.event_id, r.compo, r.place, r.screenshot_url, r.download_count,");
        sql.push_str("1.0 as score, '' as name_highlight ");
        sql.push_str("FROM releases r WHERE 1=1");
    }
    
    // Add filters on the releases table
    if let Some(ref ty) = query.r#type {
        if ty.id() > 0 {
            conditions.push(format!("r.release_type_id = ?{}", params.len() + 1));
            params.push(Box::new(ty.id() as i64) as Box<dyn ToSql>);
        }
    }
    
    if let Some(ref range) = query.years {
        conditions.push(format!("r.release_year BETWEEN ?{} AND ?{}", params.len() + 1, params.len() + 2));
        params.push(Box::new(range.start as i64) as Box<dyn ToSql>);
        params.push(Box::new((range.end.saturating_sub(1)) as i64) as Box<dyn ToSql>);
    }
    
    if let Some(min) = query.min_rating {
        conditions.push(format!("r.rating >= ?{}", params.len() + 1));
        params.push(Box::new(min as f64) as Box<dyn ToSql>);
    }
    
    if let Some(max) = query.max_rating {
        conditions.push(format!("r.rating <= ?{}", params.len() + 1));
        params.push(Box::new(max as f64) as Box<dyn ToSql>);
    }
    
    // Add group filters
    for group in &query.groups {
        // Find group by name and filter by group_id
        // For now, we'll use a subquery approach
        conditions.push(format!("r.group_id IN (SELECT id FROM groups WHERE name COLLATE NOCASE LIKE ?{})", params.len() + 1));
        params.push(Box::new(format!("%{}%", group)) as Box<dyn ToSql>);
    }
    
    // Add scener filters (filter by credits)
    for scener in &query.sceners {
        conditions.push(format!("r.id IN (SELECT release_id FROM release_credits WHERE handle COLLATE NOCASE LIKE ?{})", params.len() + 1));
        params.push(Box::new(format!("%{}%", scener)) as Box<dyn ToSql>);
    }
    
    // Add event filters
    for event in &query.events {
        conditions.push(format!("r.event_id IN (SELECT id FROM events WHERE name COLLATE NOCASE LIKE ?{})", params.len() + 1));
        params.push(Box::new(format!("%{}%", event)) as Box<dyn ToSql>);
    }
    
    // Add country filters (filter by group country or event country)
    for country in &query.countries {
        conditions.push(format!("(r.group_id IN (SELECT id FROM groups WHERE base_country COLLATE NOCASE LIKE ?{}) OR r.event_id IN (SELECT id FROM events WHERE country COLLATE NOCASE LIKE ?{}))", params.len() + 1, params.len() + 2));
        params.push(Box::new(format!("%{}%", country)) as Box<dyn ToSql>);
        params.push(Box::new(format!("%{}%", country)) as Box<dyn ToSql>);
    }
    
    // Add credit type filters
    for credit_type in &query.credit_types {
        conditions.push(format!("r.id IN (SELECT release_id FROM release_credits WHERE credit_type COLLATE NOCASE LIKE ?{})", params.len() + 1));
        params.push(Box::new(format!("%{}%", credit_type)) as Box<dyn ToSql>);
    }
    
    // Add has_download filter
    if let Some(has_download) = query.has_download {
        if has_download {
            conditions.push(format!("r.id IN (SELECT release_id FROM release_downloads LIMIT 1)"));
        } else {
            conditions.push(format!("r.id NOT IN (SELECT release_id FROM release_downloads)"));
        }
    }
    
    if !conditions.is_empty() {
        sql.push_str(" AND ");
        sql.push_str(&conditions.join(" AND "));
    }
    
    // Add sorting - for FTS results, we want to sort by relevance score first, then by other criteria
    let order_by = match query.sort_by {
        SearchSort::Relevance => {
            if !query.query.is_empty() {
                "score DESC, r.rating DESC"
            } else {
                "r.rating DESC"
            }
        },
        SearchSort::Rating => "r.rating DESC, score DESC",
        SearchSort::Date => "r.release_year DESC, score DESC",
        SearchSort::Name => "r.name COLLATE NOCASE, score DESC",
        SearchSort::Downloads => "r.download_count DESC, score DESC",
    };
    
    sql.push_str(" ORDER BY ");
    sql.push_str(order_by);
    
    // Add pagination
    sql.push_str(&format!(" LIMIT ?{} OFFSET ?{}", params.len() + 1, params.len() + 2));
    params.push(Box::new(query.limit as i64) as Box<dyn ToSql>);
    params.push(Box::new(query.offset as i64) as Box<dyn ToSql>);
    
    // Execute query
    let param_refs: Vec<&dyn ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let mut stmt = conn.prepare(&sql).map_err(|e| format!("Failed to prepare query: {}", e))?;
    
    let rows = stmt.query_map(&*param_refs, |row| {
        let score: f64 = row.get(11)?;
        let highlight: String = row.get(12)?;
        let highlights = if !highlight.is_empty() {
            vec![highlight]
        } else {
            Vec::new()
        };
        
        Ok((Release {
            id: row.get::<_, u32>(0)?,
            name: row.get::<_, String>(1)?,
            release_type_id: row.get::<_, u32>(2)?,
            release_type: String::new(),
            group_id: row.get::<_, Option<u32>>(3)?,
            group: None,
            rating: row.get::<_, f32>(4)?,
            release_day: None,
            release_month: None,
            release_year: row.get::<_, Option<u32>>(5)?,
            event_id: row.get::<_, Option<u32>>(6)?,
            event: None,
            compo: row.get::<_, Option<String>>(7)?,
            place: row.get::<_, u32>(8)?,
            screenshot_url: row.get::<_, Option<String>>(9)?,
            screenshot_urls: Vec::new(),
            download_count: row.get::<_, u32>(10)?,
            website: None,
            credits: Vec::new(),
            downloads: Vec::new(),
            sids: Vec::new(),
            ia_mappings: Vec::new(),
            created_at: None,
            updated_at: None,
        }, score as f32, highlights))
    }).map_err(|e| format!("Failed to execute query: {}", e))?;
    
    let mut result_items = Vec::new();
    for row in rows {
        match row {
            Ok((release, score, highlights)) => {
                result_items.push(SearchResultItem {
                    release,
                    score,
                    highlights,
                });
            },
            Err(e) => return Err(format!("Row error: {}", e)),
        }
    }
    
    // Count total results
    let total = count_search_results(conn, query)?;
    
    // Build facets - skip for now to simplify
    // let facets = build_search_facets(conn, query)?;
    let facets = SearchFacets::default();
    
    Ok(SearchResult {
        total,
        results: result_items,
        facets,
    })
}

/// Count total results for a search query.
pub fn count_search_results(
    conn: &Connection,
    query: &SearchQuery,
) -> Result<u64, String> {
    let mut sql = String::new();
    let mut conditions = Vec::new();
    let mut params: Vec<Box<dyn ToSql>> = Vec::new();
    
    // Build the main query using FTS5 if query text is provided
    if !query.query.is_empty() {
        let fts5_query = build_fts5_query(&query.query);
        sql.push_str("SELECT COUNT(*) FROM releases r JOIN releases_fts ON releases_fts.rowid = r.id WHERE releases_fts.releases_fts MATCH ?1");
        params.push(Box::new(fts5_query) as Box<dyn ToSql>);
    } else {
        sql.push_str("SELECT COUNT(*) FROM releases r WHERE 1=1");
    }
    
    // Add filters on the releases table
    if let Some(ref ty) = query.r#type {
        if ty.id() > 0 {
            conditions.push(format!("r.release_type_id = ?{}", params.len() + 1));
            params.push(Box::new(ty.id() as i64) as Box<dyn ToSql>);
        }
    }
    
    if let Some(ref range) = query.years {
        conditions.push(format!("r.release_year BETWEEN ?{} AND ?{}", params.len() + 1, params.len() + 2));
        params.push(Box::new(range.start as i64) as Box<dyn ToSql>);
        params.push(Box::new((range.end.saturating_sub(1)) as i64) as Box<dyn ToSql>);
    }
    
    if let Some(min) = query.min_rating {
        conditions.push(format!("r.rating >= ?{}", params.len() + 1));
        params.push(Box::new(min as f64) as Box<dyn ToSql>);
    }
    
    if let Some(max) = query.max_rating {
        conditions.push(format!("r.rating <= ?{}", params.len() + 1));
        params.push(Box::new(max as f64) as Box<dyn ToSql>);
    }
    
    // Add group filters
    for group in &query.groups {
        conditions.push(format!("r.group_id IN (SELECT id FROM groups WHERE name COLLATE NOCASE LIKE ?{})", params.len() + 1));
        params.push(Box::new(format!("%{}%", group)) as Box<dyn ToSql>);
    }
    
    // Add scener filters (filter by credits)
    for scener in &query.sceners {
        conditions.push(format!("r.id IN (SELECT release_id FROM release_credits WHERE handle COLLATE NOCASE LIKE ?{})", params.len() + 1));
        params.push(Box::new(format!("%{}%", scener)) as Box<dyn ToSql>);
    }
    
    // Add event filters
    for event in &query.events {
        conditions.push(format!("r.event_id IN (SELECT id FROM events WHERE name COLLATE NOCASE LIKE ?{})", params.len() + 1));
        params.push(Box::new(format!("%{}%", event)) as Box<dyn ToSql>);
    }
    
    // Add country filters (filter by group country or event country)
    for country in &query.countries {
        conditions.push(format!("(r.group_id IN (SELECT id FROM groups WHERE base_country COLLATE NOCASE LIKE ?{}) OR r.event_id IN (SELECT id FROM events WHERE country COLLATE NOCASE LIKE ?{}))", params.len() + 1, params.len() + 2));
        params.push(Box::new(format!("%{}%", country)) as Box<dyn ToSql>);
        params.push(Box::new(format!("%{}%", country)) as Box<dyn ToSql>);
    }
    
    // Add credit type filters
    for credit_type in &query.credit_types {
        conditions.push(format!("r.id IN (SELECT release_id FROM release_credits WHERE credit_type COLLATE NOCASE LIKE ?{})", params.len() + 1));
        params.push(Box::new(format!("%{}%", credit_type)) as Box<dyn ToSql>);
    }
    
    // Add has_download filter
    if let Some(has_download) = query.has_download {
        if has_download {
            conditions.push(format!("r.id IN (SELECT release_id FROM release_downloads LIMIT 1)"));
        } else {
            conditions.push(format!("r.id NOT IN (SELECT release_id FROM release_downloads)"));
        }
    }
    
    if !conditions.is_empty() {
        sql.push_str(" AND ");
        sql.push_str(&conditions.join(" AND "));
    }
    
    let param_refs: Vec<&dyn ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let count: i64 = conn.query_row(&sql, &*param_refs, |row| row.get(0))
        .map_err(|e| format!("Failed to count results: {}", e))?;
    
    Ok(count as u64)
}

// Facet building disabled for now - SQL query complexity needs refactoring
// #[allow(dead_code)]
// pub fn build_search_facets(
//     conn: &Connection,
//     query: &SearchQuery,
// ) -> Result<SearchFacets, String> {
//     // Implementation deferred
//     Ok(SearchFacets::default())
// }

/// Get search suggestions based on the query.
/// Returns suggestions for completing the query based on existing data.
pub fn get_search_suggestions(
    conn: &Connection,
    query: &str,
    limit: usize,
) -> Result<Vec<SearchSuggestion>, String> {
    let mut suggestions = Vec::new();
    
    if query.is_empty() {
        return Ok(suggestions);
    }
    
    // Search for release name suggestions
    let pattern = format!("%{}%", query);
    let mut stmt = conn.prepare(
        "SELECT name, name, 'release' FROM releases 
         WHERE name COLLATE NOCASE LIKE ? 
         ORDER BY rating DESC LIMIT ?"
    ).map_err(|e| format!("Failed to prepare suggestion query: {}", e))?;
    
    let rows = stmt.query_map(params![&pattern, limit as i64], |row| {
        Ok(SearchSuggestion {
            text: row.get::<_, String>(0)?,
            display: row.get::<_, String>(1)?,
            category: row.get::<_, String>(2)?,
        })
    }).map_err(|e| format!("Failed to execute suggestion query: {}", e))?;
    
    for row in rows {
        match row {
            Ok(s) => suggestions.push(s),
            Err(e) => eprintln!("Error getting suggestion: {}", e),
        }
    }
    
    // Search for group name suggestions
    let mut stmt = conn.prepare(
        "SELECT name, name, 'group' FROM groups 
         WHERE name COLLATE NOCASE LIKE ? 
         ORDER BY rating DESC LIMIT ?"
    ).map_err(|e| format!("Failed to prepare suggestion query: {}", e))?;
    
    let rows = stmt.query_map(params![&pattern, limit as i64], |row| {
        Ok(SearchSuggestion {
            text: row.get::<_, String>(0)?,
            display: row.get::<_, String>(1)?,
            category: row.get::<_, String>(2)?,
        })
    }).map_err(|e| format!("Failed to execute suggestion query: {}", e))?;
    
    for row in rows {
        match row {
            Ok(s) => suggestions.push(s),
            Err(e) => eprintln!("Error getting suggestion: {}", e),
        }
    }
    
    // Search for scener/handle suggestions
    let mut stmt = conn.prepare(
        "SELECT handle, handle, 'scener' FROM scener_handles 
         WHERE handle COLLATE NOCASE LIKE ? 
         ORDER BY currently_used DESC, handle ASC LIMIT ?"
    ).map_err(|e| format!("Failed to prepare suggestion query: {}", e))?;
    
    let rows = stmt.query_map(params![&pattern, limit as i64], |row| {
        Ok(SearchSuggestion {
            text: row.get::<_, String>(0)?,
            display: row.get::<_, String>(1)?,
            category: row.get::<_, String>(2)?,
        })
    }).map_err(|e| format!("Failed to execute suggestion query: {}", e))?;
    
    for row in rows {
        match row {
            Ok(s) => suggestions.push(s),
            Err(e) => eprintln!("Error getting suggestion: {}", e),
        }
    }
    
    // Search for event suggestions
    let mut stmt = conn.prepare(
        "SELECT name, name, 'event' FROM events 
         WHERE name COLLATE NOCASE LIKE ? 
         ORDER BY start_year DESC LIMIT ?"
    ).map_err(|e| format!("Failed to prepare suggestion query: {}", e))?;
    
    let rows = stmt.query_map(params![&pattern, limit as i64], |row| {
        Ok(SearchSuggestion {
            text: row.get::<_, String>(0)?,
            display: row.get::<_, String>(1)?,
            category: row.get::<_, String>(2)?,
        })
    }).map_err(|e| format!("Failed to execute suggestion query: {}", e))?;
    
    for row in rows {
        match row {
            Ok(s) => suggestions.push(s),
            Err(e) => eprintln!("Error getting suggestion: {}", e),
        }
    }
    
    // Deduplicate and limit results
    suggestions.sort_by(|a, b| a.display.cmp(&b.display));
    suggestions.dedup_by(|a, b| a.text == b.text);
    suggestions.truncate(limit);
    
    Ok(suggestions)
}

/// Execute a simple search by name only (for backward compatibility with old search).
/// This is a simpler interface that doesn't use the full SearchQuery struct.
pub fn search_by_name(
    conn: &Connection,
    name_query: &str,
    limit: usize,
) -> Result<Vec<(u32, String)>, String> {
    let query = parse_search_query(name_query);
    let mut result_query = query.clone();
    result_query.limit = limit;
    result_query.offset = 0;
    
    let result = execute_search_query(conn, &result_query)?;
    
    Ok(result.results
        .into_iter()
        .map(|item| (item.release.id, item.release.name))
        .collect())
}
