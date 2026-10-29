// itch - itch.io page crawler for finding C64 downloads.
//
// This module provides functionality to crawl itch.io pages and extract
// Commodore 64 download links (PRG, D64, etc.) that may not be available
// directly from CSDb.

use crate::core;

/// Known C64 file extensions that VICE can handle.
const C64_EXTENSIONS: &[&str] = &[
    ".d64", ".d71", ".d81", ".t64", ".tap", ".prg", ".crt", 
    ".g64", ".nib", ".p00", ".x64", ".sid"
];

/// Check if a URL extension is a C64 file type.
fn is_c64_file(url: &str) -> bool {
    let url_lower = url.to_lowercase();
    C64_EXTENSIONS.iter().any(|ext| url_lower.ends_with(ext))
}

/// Extract download links from an itch.io page HTML.
/// Looks for <a> tags with href attributes that point to downloadable files.
pub fn extract_download_links(html: &str, base_url: &str) -> Vec<String> {
    let mut links = Vec::new();
    let html_lower = html.to_lowercase();
    
    // Look for <a> tags with href attributes
    let mut rest = html;
    while let Some(a_start) = rest.find("<a ") {
        let after_a = &rest[a_start..];
        
        // Find href attribute
        if let Some(href_start) = after_a.to_lowercase().find("href=\"") {
            let href_content_start = a_start + href_start + 6; // 6 = len("href=\"")
            if href_content_start < rest.len() {
                let remaining = &rest[href_content_start..];
                if let Some(href_end) = remaining.find('"') {
                    let url = &rest[href_content_start..href_content_start + href_end];
                    
                    // Skip empty or javascript links
                    if !url.is_empty() && !url.to_lowercase().starts_with("javascript:") {
                        // Resolve relative URLs
                        let full_url = if url.starts_with("http") {
                            url.to_string()
                        } else if url.starts_with("/") {
                            format!("{}{}", base_url.trim_end_matches('/'), url)
                        } else {
                            format!("{}/{}", base_url.trim_end_matches('/'), url)
                        };
                        
                        // Check if it's a C64 file
                        if is_c64_file(&full_url) {
                            links.push(full_url);
                        }
                    }
                    rest = &rest[href_content_start + href_end + 1..];
                    continue;
                }
            }
        }
        
        // Move past this <a> tag
        rest = &rest[a_start + 1..];
    }
    
    // Also look for direct file links in the text (common on itch.io download sections)
    // Look for patterns like "circles.prg" or href="/files/circles.prg"
    for ext in C64_EXTENSIONS {
        let search_pattern = ext;
        let mut pos = 0;
        while let Some(found) = &html_lower[pos..].find(search_pattern) {
            let file_pos = pos + found;
            
            // Look backwards for a URL start
            let mut url_start = file_pos;
            while url_start > 0 {
                let c = html.as_bytes()[url_start - 1] as char;
                if c == '\"' || c == '\'' || c == '>' {
                    break;
                }
                if c.is_whitespace() {
                    url_start += 1;
                    break;
                }
                url_start -= 1;
            }
            
            // Look forwards for URL end
            let mut url_end = file_pos + search_pattern.len();
            while url_end < html.len() {
                let c = html.as_bytes()[url_end] as char;
                if c == '\"' || c == '\'' || c == '<' || c.is_whitespace() {
                    break;
                }
                url_end += 1;
            }
            
            let candidate = &html[url_start..url_end];
            
            // Skip if it's already found or contains HTML tags
            if !links.iter().any(|l| l.contains(candidate)) && !candidate.contains('<') {
                // Check if it looks like a file path (contains the extension at the end)
                if candidate.to_lowercase().ends_with(search_pattern) {
                    // Try to make it a full URL
                    let full_url = if candidate.starts_with("http") {
                        candidate.to_string()
                    } else if candidate.starts_with("/") {
                        format!("{}{}", base_url.trim_end_matches('/'), candidate)
                    } else {
                        // This might be a relative path or just a filename
                        // Skip for now as we can't be sure of the full URL
                        String::new()
                    };
                    
                    if !full_url.is_empty() && !links.contains(&full_url) {
                        links.push(full_url);
                    }
                }
            }
            
            pos = file_pos + search_pattern.len();
        }
    }
    
    links
}

/// Extract direct download links from itch.io page content.
/// Itch.io often has download links in specific formats like:
/// - <a href="/download/.../circles.prg">circles.prg</a>
/// - Text mentioning "circles.prg 3.9 kB"
pub fn find_itchio_downloads(page_html: &str, page_url: &str) -> Vec<String> {
    let mut downloads = Vec::new();
    
    // First try the general HTML parser
    downloads.extend(extract_download_links(page_html, page_url));
    
    // Itch.io specific: look for file listings in the download section
    // Itch.io often has patterns like: **filename.ext** size (markdown) or
    // <strong class="name">filename.ext</strong> <span class="file_size">3.9 kB</span> (HTML)
    let lines: Vec<&str> = page_html.lines().collect();
    for line in lines {
        let line_lower = line.to_lowercase();
        for ext in C64_EXTENSIONS {
            if line_lower.contains(ext) && (line_lower.contains("kb") || line_lower.contains("mb") || line_lower.contains("bytes")) {
                // Try to extract the filename from markdown **pattern
                if let Some(start) = line.find("**") {
                    if let Some(end) = line[start + 2..].find("**") {
                        let filename = &line[start + 2..start + 2 + end];
                        if filename.to_lowercase().ends_with(ext) {
                            let possible_url = format!("{}/download/{}", page_url.trim_end_matches('/'), filename);
                            if !downloads.contains(&possible_url) {
                                downloads.push(possible_url);
                            }
                        }
                    }
                }
                
                // Try to extract the filename from HTML <strong class="name"> pattern
                // Pattern: <strong ...>filename.ext</strong>
                if let Some(start) = line.to_lowercase().find("<strong") {
                    if let Some(tag_end) = line[start..].find('>') {
                        let content_start = start + tag_end + 1;
                        if let Some(content_end) = line[content_start..].find("</strong>") {
                            let filename = &line[content_start..content_start + content_end];
                            if filename.to_lowercase().ends_with(ext) {
                                let possible_url = format!("{}/download/{}", page_url.trim_end_matches('/'), filename);
                                if !downloads.contains(&possible_url) {
                                    downloads.push(possible_url);
                                }
                            }
                        }
                    }
                }
                
                // Also try to extract from title attribute: <strong title="filename.ext" ...>
                if let Some(title_start) = line.to_lowercase().find("title=\"") {
                    let after_title = &line[title_start + 7..]; // 7 = len("title=\"")
                    if let Some(title_end) = after_title.find('"') {
                        let filename = &after_title[..title_end];
                        if filename.to_lowercase().ends_with(ext) {
                            let possible_url = format!("{}/download/{}", page_url.trim_end_matches('/'), filename);
                            if !downloads.contains(&possible_url) {
                                downloads.push(possible_url);
                            }
                        }
                    }
                }
            }
        }
    }
    
    downloads
}

/// Test if a URL looks like an itch.io page URL.
pub fn is_itchio_url(url: &str) -> bool {
    url.contains("itch.io") && !url.contains("/download/")
}

/// Extract related game URLs from an itch.io page.
pub fn extract_related_game_urls(html: &str, base_url: &str) -> Vec<String> {
    let mut urls = Vec::new();
    
    // Look for "Related games" section links
    // Pattern: <a href="https://username.itch.io/game-name">
    let mut rest = html;
    while let Some(a_start) = rest.find("<a ") {
        let after_a = &rest[a_start..];
        
        if let Some(href_start) = after_a.to_lowercase().find("href=\"") {
            let href_content_start = a_start + href_start + 6;
            if href_content_start < rest.len() {
                let remaining = &rest[href_content_start..];
                if let Some(href_end) = remaining.find('"') {
                    let url = &rest[href_content_start..href_content_start + href_end];
                    
                    // Check if it's an itch.io game URL (not a download)
                    if url.contains("itch.io") && !url.contains("/download/") && !url.contains("javascript:") {
                        let full_url = if url.starts_with("http") {
                            url.to_string()
                        } else if url.starts_with("/") {
                            format!("{}{}", base_url.trim_end_matches('/'), url)
                        } else {
                            format!("{}/{}", base_url.trim_end_matches('/'), url)
                        };
                        
                        // Make sure it's a game page, not an asset
                        if full_url.contains("itch.io/") && !urls.contains(&full_url) {
                            urls.push(full_url);
                        }
                    }
                    rest = &rest[href_content_start + href_end + 1..];
                    continue;
                }
            }
        }
        
        rest = &rest[a_start + 1..];
    }
    
    urls
}

/// Find downloads on related itch.io pages.
/// This crawls the "Related games" section of an itch.io page and looks for downloads.
pub fn find_downloads_on_related_pages(page_html: &str, page_url: &str, max_depth: usize) -> Result<Vec<String>, String> {
    if max_depth == 0 {
        return Ok(Vec::new());
    }
    
    let mut all_downloads = Vec::new();
    
    // First, extract related game URLs
    let related_urls = extract_related_game_urls(page_html, page_url);
    
    // For each related page, try to find downloads
    for related_url in related_urls {
        // Skip if it's the same page
        if related_url == page_url {
            continue;
        }
        
        // Fetch the related page
        let html = match core::fetch(&related_url, &[]) {
            Ok(body) => String::from_utf8_lossy(&body).into_owned(),
            Err(e) => {
                eprintln!("Failed to fetch related page {}: {}", related_url, e);
                continue;
            }
        };
        
        // Extract downloads from this related page
        let downloads = find_itchio_downloads(&html, &related_url);
        all_downloads.extend(downloads);
        
        // Recursively check related pages of related pages (up to max_depth)
        // Note: We don't recurse to avoid excessive crawling for now
        // If needed, this can be enhanced later
    }
    
    Ok(all_downloads)
}

/// Comprehensive download finder for itch.io pages.
/// Looks for downloads on the main page, then checks related pages as fallback.
pub fn find_all_downloads_from_itchio(website_url: &str) -> Result<Vec<String>, String> {
    if !website_url.contains("itch.io") {
        return Ok(Vec::new());
    }
    
    // Fetch the main itch.io page
    let html = match core::fetch(website_url, &[]) {
        Ok(body) => String::from_utf8_lossy(&body).into_owned(),
        Err(e) => return Err(format!("Failed to fetch {}: {}", website_url, e)),
    };
    
    // First, try to find downloads on the main page
    let mut downloads = find_itchio_downloads(&html, website_url);
    
    // If no downloads found, try related pages
    if downloads.is_empty() {
        downloads = find_downloads_on_related_pages(&html, website_url, 1)?;
    }
    
    Ok(downloads)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_c64_file() {
        assert!(is_c64_file("game.prg"));
        assert!(is_c64_file("https://example.com/game.d64"));
        assert!(is_c64_file("/path/to/demo.sid"));
        assert!(!is_c64_file("game.zip"));
        assert!(!is_c64_file("https://example.com/game.exe"));
    }

    #[test]
    fn test_extract_prg_from_html() {
        let html = r#"
        <html>
        <body>
        <h1>Circles</h1>
        <p>Download: <a href="circles.prg">circles.prg</a> (3.9 kB)</p>
        <p>Mirror: <a href="https://example.com/mirror/circles.d64">D64 version</a></p>
        <p>Not a C64 file: <a href="game.zip">game.zip</a></p>
        </body>
        </html>
        "#;
        
        let downloads = extract_download_links(html, "https://example.com/game");
        assert!(downloads.iter().any(|d| d.contains("circles.prg")));
        assert!(downloads.iter().any(|d| d.contains("circles.d64")));
        assert!(!downloads.iter().any(|d| d.contains("game.zip")));
    }

    #[test]
    fn test_itchio_markdown_pattern() {
        let html = r#"## Download

**circles.prg** 3.9 kB

Some text here.
"#;
        
        let downloads = find_itchio_downloads(html, "https://jaammees.itch.io/circles");
        assert!(!downloads.is_empty());
        assert!(downloads.iter().any(|d| d.contains("circles.prg")));
    }

    #[test]
    fn test_itchio_markdown_pattern_mb_bytes() {
        // Test that lines with "mb" or "bytes" but no extension are not processed
        let html = r#"## Download

**readme.txt** 1.5 MB
**circles.prg** 3.9 kB
**docs.pdf** 250 bytes

Some text here.
"#;
        
        let downloads = find_itchio_downloads(html, "https://jaammees.itch.io/circles");
        assert!(!downloads.is_empty());
        // Should find circles.prg but not readme.txt or docs.pdf
        assert!(downloads.iter().any(|d| d.contains("circles.prg")));
        assert!(!downloads.iter().any(|d| d.contains("readme.txt")));
        assert!(!downloads.iter().any(|d| d.contains("docs.pdf")));
    }

    #[test]
    fn test_itchio_html_strong_pattern() {
        // Test extraction from HTML <strong> tags (real itch.io page pattern)
        let html = r#"<div class="upload_name"><strong title="circles.prg" class="name">circles.prg</strong> <span class="file_size"><span>3.9 kB</span></span></div>"#;
        
        let downloads = find_itchio_downloads(html, "https://jaammees.itch.io/circles");
        assert!(!downloads.is_empty());
        assert!(downloads.iter().any(|d| d.contains("circles.prg")));
        // Should construct a download URL
        assert!(downloads.iter().any(|d| d.contains("/download/circles.prg")));
    }

    #[test]
    fn test_itchio_html_strong_simple() {
        // Test extraction from simple <strong>filename.ext</strong> pattern
        let html = r#"<strong>demo.d64</strong> <span>1.2 MB</span>"#;
        
        let downloads = find_itchio_downloads(html, "https://example.itch.io/game");
        assert!(!downloads.is_empty());
        assert!(downloads.iter().any(|d| d.contains("demo.d64")));
    }
}
