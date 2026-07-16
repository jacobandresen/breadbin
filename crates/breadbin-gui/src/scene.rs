// scene - CSDb Scene Database search view
//
// This module provides a GTK4-based UI for searching the CSDb (Commodore Scene Database).

use std::path::{Path, PathBuf};

extern crate zip;

use gtk::glib;
use gtk::prelude::*;
use gtk::{Box, Button, Entry, Label, ListBox, ListBoxRow, ProgressBar, ScrolledWindow, Widget};
use adw::prelude::*;

use breadbin_core::core;
use breadbin_core::database::{DownloadLink, Release, SearchResult, SearchResultItem, ReleaseTypeFilter, SearchSort};
use breadbin_core::diesel_db;
use breadbin_core::csdb;
use breadbin_core::itch;
use breadbin_core::run;

use crate::config::Settings;
use crate::task::run_blocking;

/// Cache directory for downloaded scene releases
fn scene_downloads_dir() -> PathBuf {
    core::user_data_dir().join("scene_downloads")
}

/// Disk/program image extensions VICE can autostart, best first.
const RUN_EXTS: &[&str] = &[".d64", ".g64", ".d81", ".d71", ".prg", ".crt", ".t64", ".tap"];

/// Find the best VICE-compatible download link from a release
/// First tries CSDb downloads, then falls back to crawling itch.io if available
fn find_best_vice_download(release: &Release) -> Option<String> {
    // First, try CSDb downloads
    if let Some(download) = release.downloads.iter()
        .filter(|d| is_vice_compatible_link(d))
        .find(|d| {
            let url_lower = d.url.to_lowercase();
            RUN_EXTS.iter().any(|ext| url_lower.ends_with(ext))
        })
    {
        return Some(download.url.clone());
    }
    
    // If no VICE-compatible CSDb download, try crawling itch.io (including related pages)
    if let Some(website) = &release.website {
        if itch::is_itchio_url(website) {
            match itch::find_all_downloads_from_itchio(website) {
                Ok(downloads) => {
                    // Return the first C64 file found on itch.io or related pages
                    for download_url in downloads {
                        let url_lower = download_url.to_lowercase();
                        if RUN_EXTS.iter().any(|ext| url_lower.ends_with(ext)) {
                            return Some(download_url);
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Failed to crawl {}: {}", website, e);
                }
            }
        }
    }
    
    None
}

/// Download and cache a release file, returning the local path
fn fetch_and_prepare_release(release: &Release) -> Result<PathBuf, String> {
    let dir = scene_downloads_dir();
    
    // Check if already cached
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for e in entries.flatten() {
            let p = e.path();
            let stem_matches = p
                .file_stem()
                .map(|s| s.to_string_lossy().starts_with(&format!("{}_", release.id)))
                .unwrap_or(false);
            if stem_matches && p.metadata().map(|m| m.len() > 0).unwrap_or(false) {
                return Ok(p);
            }
        }
    }
    
    // Find best download link (from CSDb or itch.io)
    let download_url = find_best_vice_download(release)
        .ok_or_else(|| format!("No VICE-compatible download found for release \"{}\"", release.name))?;
    
    // Download the file
    let data = core::fetch(&download_url, &[])
        .map_err(|e| format!("Failed to download {}: {}", download_url, e))?;
    
    // Create safe filename
    let safe: String = release
        .name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    
    // Determine extension from URL
    let ext = Path::new(&download_url)
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_lowercase())
        .unwrap_or_else(|| "bin".to_string());
    
    let filename = format!("{}_{}.{}", release.id, safe, ext);
    let dest = dir.join(filename);
    
    // Create directory if needed
    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Failed to create cache directory: {}", e))?;
    
    // Handle zip files
    if data.starts_with(b"PK") {
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(data))
            .map_err(|e| format!("bad zip: {}", e))?;
        let names: Vec<String> = zip.file_names().map(String::from).collect();
        let member = RUN_EXTS.iter().find_map(|ext| {
            let mut m: Vec<&String> = names.iter().filter(|n| n.to_lowercase().ends_with(ext)).collect();
            m.sort_by_key(|n| n.len());
            m.first().map(|s| (*s).clone())
        });
        
        let member_name = member.ok_or_else(|| format!("no runnable disk/program image inside the archive for \"{}\"", release.name))?;
        
        let mut member_data = Vec::new();
        use std::io::Read;
        zip.by_name(&member_name)
            .map_err(|e| format!("Failed to extract {}: {}", member_name, e))?
            .read_to_end(&mut member_data)
            .map_err(|e| format!("Failed to read {}: {}", member_name, e))?;
        
        std::fs::write(&dest, &member_data)
            .map_err(|e| format!("Failed to write {}: {}", dest.display(), e))?;
    } else {
        // Write the file directly
        std::fs::write(&dest, &data)
            .map_err(|e| format!("Failed to write {}: {}", dest.display(), e))?;
    }
    
    Ok(dest)
}

/// Check if a download link is a VICE-compatible C64 file
fn is_vice_compatible_link(download: &DownloadLink) -> bool {
    // List of C64 file extensions that VICE can handle
    let vice_extensions = [
        ".d64", ".d71", ".d81", ".t64", ".tap", ".prg", ".crt", 
        ".g64", ".nib", ".p00", ".x64", ".sid"
    ];
    
    let url_lower = download.url.to_lowercase();
    
    // Check URL extension
    for ext in &vice_extensions {
        if url_lower.ends_with(ext) {
            return true;
        }
    }
    
    // Check file_type field
    if let Some(ref file_type) = download.file_type {
        let file_type_lower = file_type.to_lowercase();
        // Common MIME types for C64 files
        if file_type_lower.contains("d64") 
            || file_type_lower.contains("sid") 
            || file_type_lower.contains("prg")
            || file_type_lower.contains("crt")
            || file_type_lower.contains("t64")
            || file_type_lower.contains("tap") {
            return true;
        }
        // Filter out non-VICE types
        if file_type_lower.contains("youtube") 
            || file_type_lower.contains("video")
            || file_type_lower.contains("html")
            || file_type_lower.contains("zip") {
            return false;
        }
    }
    
    false
}

/// Show a loading dialog for release preparation
fn show_loading_dialog(parent: &gtk::Window, release_name: &str) -> adw::Dialog {
    let dialog = adw::Dialog::new();
    dialog.set_title(&format!("Loading {}", release_name));
    dialog.set_content_width(400);

    let vbox = Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .margin_top(24)
        .margin_bottom(24)
        .margin_start(24)
        .margin_end(24)
        .build();

    let status_label = Label::builder()
        .label("Preparing release...")
        .xalign(0.0)
        .wrap(true)
        .build();
    status_label.add_css_class("caption");

    let progress_bar = ProgressBar::builder()
        .show_text(true)
        .hexpand(true)
        .build();

    vbox.append(&status_label);
    vbox.append(&progress_bar);
    dialog.set_child(Some(&vbox));
    dialog.present(Some(parent));

    dialog
}

/// Show result dialog for launch success/failure
fn show_result_dialog(parent: &gtk::Window, release_name: &str, success: bool, error_message: Option<String>) {
    let dialog = adw::Dialog::new();
    
    if success {
        dialog.set_title("Launch Successful");
        
        let vbox = Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(12)
            .margin_top(24)
            .margin_bottom(24)
            .margin_start(24)
            .margin_end(24)
            .build();
        
        let success_label = Label::new(Some(&format!("VICE is launching: {}", release_name)));
        success_label.set_wrap(true);
        vbox.append(&success_label);
        
        let close_btn = Button::with_label("Close");
        let dialog_clone = dialog.clone();
        close_btn.connect_clicked(move |_| {
            dialog_clone.close();
        });
        vbox.append(&close_btn);
        
        dialog.set_child(Some(&vbox));
    } else {
        dialog.set_title("Launch Failed");
        
        let vbox = Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(12)
            .margin_top(24)
            .margin_bottom(24)
            .margin_start(24)
            .margin_end(24)
            .build();
        
        let error_label = Label::new(Some(&format!(
            "Failed to launch {}: {}", 
            release_name,
            error_message.unwrap_or_else(|| "Unknown error".to_string())
        )));
        error_label.set_wrap(true);
        vbox.append(&error_label);
        
        let close_btn = Button::with_label("Close");
        let dialog_clone = dialog.clone();
        close_btn.connect_clicked(move |_| {
            dialog_clone.close();
        });
        vbox.append(&close_btn);
        
        dialog.set_child(Some(&vbox));
    }
    
    dialog.present(Some(parent));
}

/// Handle launching a release with loading dialog
fn handle_release_launch(parent: &gtk::Window, release: Release, launch_opts: run::LaunchOpts) {
    // Show loading dialog
    let loading_dialog = show_loading_dialog(parent, &release.name);
    
    let release_for_prepare = release.clone();
    let release_for_display = release.clone();
    let opts = launch_opts.clone();
    let parent_clone = parent.clone();
    
    glib::spawn_future_local(async move {
        // Update status and progress
        if let Some(widget) = loading_dialog.child() {
            if let Some(vbox) = widget.downcast::<Box>().ok() {
                if let Some(label) = vbox.first_child().and_then(|c| c.downcast::<Label>().ok()) {
                    label.set_label("Downloading release...");
                }
                // Try to update progress bar if available
                if let Some(progress_bar) = vbox.last_child().and_then(|c| c.downcast::<ProgressBar>().ok()) {
                    progress_bar.set_fraction(0.3);
                    progress_bar.set_text(Some("30%"));
                }
            }
        }
        
        // Perform the download and preparation
        let result = run_blocking(move || fetch_and_prepare_release(&release_for_prepare)).await;
        
        // Close loading dialog
        loading_dialog.close();
        
        match result {
            Ok(path) => {
                match run::spawn(&path, &opts) {
                    Ok(_) => {
                        // Success - VICE launched
                        show_result_dialog(&parent_clone, &release_for_display.name, true, None);
                    }
                    Err(e) => {
                        // Show failure message
                        show_result_dialog(&parent_clone, &release_for_display.name, false, Some(e.to_string()));
                    }
                }
            }
            Err(e) => {
                // Show failure message for preparation
                show_result_dialog(&parent_clone, &release_for_display.name, false, Some(e));
            }
        }
    });
}

/// Main scene view widget container with settings
pub fn build_with_settings(settings: &Settings) -> Widget {
    // Get launch options once
    let launch_opts = settings.launch_opts();
    
    // Create the main container
    let vbox = Box::new(gtk::Orientation::Vertical, 12);
    vbox.set_hexpand(true);
    vbox.set_vexpand(true);
    vbox.set_widget_name("scene-view");
    
    // Create search entry
    let search_entry = Entry::builder()
        .placeholder_text("Search CSDb releases...")
        .hexpand(true)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();
    
    // Create a horizontal box for search entry and button
    let search_box = Box::new(gtk::Orientation::Horizontal, 6);
    search_box.set_hexpand(true);
    search_box.append(&search_entry);
    
    // Create search button
    let search_btn = Button::with_label("Search");
    search_box.append(&search_btn);
    
    // Create results list
    let results_list = ListBox::new();
    results_list.set_selection_mode(gtk::SelectionMode::None);
    results_list.add_css_class("boxed-list");
    
    let scrolled = ScrolledWindow::builder()
        .child(&results_list)
        .hexpand(true)
        .vexpand(true)
        .propagate_natural_height(true)
        .build();
    
    // Add widgets to main box
    vbox.append(&search_box);
    vbox.append(&scrolled);
    
    // Create a closure for performing searches
    let results_list_clone = results_list.clone();
    let launch_opts_search = launch_opts.clone();
    let perform_search = move |query: String| {
        let results_list = results_list_clone.clone();
        let launch_opts = launch_opts_search.clone();
        glib::spawn_future_local(async move {
            match execute_search(&query).await {
                Ok(result) => {
                    display_results(&results_list, result, launch_opts);
                }
                Err(e) => {
                    eprintln!("Search failed: {}", e);
                    display_error(&results_list, &e);
                }
            }
        });
    };
    
    // Connect search button click
    let perform_search_btn = perform_search.clone();
    let search_entry_btn = search_entry.clone();
    search_btn.connect_clicked(move |_| {
        let query = search_entry_btn.text().to_string();
        perform_search_btn(query);
    });
    
    // Connect Enter key in search entry
    let perform_search_entry = perform_search.clone();
    search_entry.connect_activate(move |entry| {
        let query = entry.text().to_string();
        perform_search_entry(query);
    });
    
    // Add top releases section
    let top_releases_box = create_top_releases_section(launch_opts.clone());
    vbox.append(&top_releases_box);
    
    // Add separator
    let separator = Box::new(gtk::Orientation::Horizontal, 0);
    separator.set_hexpand(true);
    separator.add_css_class("separator");
    vbox.append(&separator);
    
    vbox.upcast::<Widget>()
}

/// Create the top releases section
fn create_top_releases_section(launch_opts: Option<run::LaunchOpts>) -> Box {
    let vbox = Box::new(gtk::Orientation::Vertical, 12);
    vbox.set_margin_top(12);
    vbox.set_margin_start(12);
    vbox.set_margin_end(12);
    
    // Title
    let title = Label::new(Some("Top 3 CSDb Releases"));
    title.set_halign(gtk::Align::Start);
    title.add_css_class("heading");
    vbox.append(&title);
    
    // Container for top releases - use ListBox for consistency
    let top_list = ListBox::new();
    top_list.set_selection_mode(gtk::SelectionMode::None);
    top_list.add_css_class("boxed-list");
    
    // Load top releases
    load_top_releases(&top_list, launch_opts);
    
    vbox.append(&top_list);
    vbox
}

/// Load top 3 releases into the container
fn load_top_releases(container: &ListBox, launch_opts: Option<run::LaunchOpts>) {
    // Clear existing content
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
    
    match csdb::get_top_3_releases() {
        Ok(releases) => {
            if releases.is_empty() {
                let no_data = Label::new(Some("No CSDb releases cached. Run 'breadbin sync' to fetch data from csdb.dk."));
                no_data.set_halign(gtk::Align::Start);
                no_data.add_css_class("dim-label");
                container.append(&no_data);
            } else {
                // Show top 3 releases from CSDb
                for (i, release) in releases.iter().take(3).enumerate() {
                    let row = create_top_release_row(release, i + 1, launch_opts.clone());
                    container.append(&row);
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to load top releases: {}", e);
            let error_label = Label::new(Some("Failed to load top releases."));
            error_label.set_halign(gtk::Align::Start);
            error_label.add_css_class("error-label");
            container.append(&error_label);
        }
    }
}

/// Create a row for a top release
fn create_top_release_row(release: &Release, rank: usize, launch_opts: Option<run::LaunchOpts>) -> ListBoxRow {
    let release_clone = release.clone();
    let launch_opts_clone = launch_opts.clone();
    
    let main_box = Box::new(gtk::Orientation::Vertical, 6);
    main_box.set_halign(gtk::Align::Fill);
    main_box.set_margin_top(6);
    main_box.set_margin_bottom(6);
    
    // Header row with rank, name, and details
    let header_box = Box::new(gtk::Orientation::Horizontal, 12);
    header_box.set_halign(gtk::Align::Fill);
    
    // Rank badge
    let rank_label = Label::new(Some(&format!("#{}", rank)));
    rank_label.set_halign(gtk::Align::Center);
    rank_label.set_valign(gtk::Align::Center);
    rank_label.set_width_chars(3);
    rank_label.add_css_class("badge");
    
    // Left side: name and details
    let vbox = Box::new(gtk::Orientation::Vertical, 4);
    vbox.set_hexpand(true);
    vbox.set_halign(gtk::Align::Start);
    
    // Title
    let title = Label::new(Some(&release.name));
    title.set_halign(gtk::Align::Start);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.add_css_class("heading");
    vbox.append(&title);
    
    // Details line
    let mut details_parts = Vec::new();
    
    // Rating
    details_parts.push(format!("★ {:.1}", release.rating));
    
    // Year
    if let Some(year) = release.release_year {
        details_parts.push(format!("{}", year));
    }
    
    // Type
    let type_filter = ReleaseTypeFilter::from_id(release.release_type_id);
    let type_name = type_filter.name();
    details_parts.push(format!("{}", type_name));
    
    // Count VICE-compatible downloads
    let vice_downloads = release.downloads.iter().filter(|d| is_vice_compatible_link(d)).count();
    if vice_downloads > 0 {
        details_parts.push(format!("{} download(s)", vice_downloads));
    }
    
    let details = Label::new(Some(&details_parts.join(" • ")));
    details.set_halign(gtk::Align::Start);
    details.add_css_class("dim-label");
    vbox.append(&details);
    
    header_box.append(&rank_label);
    header_box.append(&vbox);
    main_box.append(&header_box);
    
    // Screenshot row (if available)
    if let Some(screenshot_url) = &release.screenshot_url {
        // Try to load screenshot - for now just show a placeholder with the URL
        // In a real implementation, this would download and cache the screenshot
        if !screenshot_url.is_empty() {
            let screenshot_box = Box::new(gtk::Orientation::Horizontal, 0);
            screenshot_box.set_halign(gtk::Align::Start);
            
            // Extract filename from URL for display
            let filename = Path::new(screenshot_url)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("Screenshot");
            let placeholder = Label::new(Some(&format!("📷 {}", filename)));
            placeholder.add_css_class("dim-label");
            placeholder.set_halign(gtk::Align::Start);
            screenshot_box.append(&placeholder);
            
            main_box.append(&screenshot_box);
        }
    }
    
    // Create the row
    let row = ListBoxRow::builder().activatable(true).build();
    row.set_child(Some(&main_box));
    
    // Connect activate signal for click handling
    row.connect_activate(move |row_widget| {
        if let Some(opts) = launch_opts_clone.clone() {
            let release_for_launch = release_clone.clone();
            
            // Find the parent window for the dialog
            if let Some(parent_window) = row_widget.root().and_then(|r| r.downcast::<gtk::Window>().ok()) {
                handle_release_launch(&parent_window, release_for_launch, opts);
            }
        }
    });
    
    row
}

/// Execute a search query
async fn execute_search(query: &str) -> Result<SearchResult, String> {
    let mut search_query = diesel_db::parse_search_query(query);
    search_query.limit = 50;
    search_query.offset = 0;
    search_query.sort_by = SearchSort::Relevance;

    diesel_db::execute_search_query(&search_query)
}

/// Display search results in the list
fn display_results(results_list: &ListBox, result: SearchResult, launch_opts: Option<run::LaunchOpts>) {
    // Clear existing results
    while let Some(child) = results_list.first_child() {
        results_list.remove(&child);
    }
    
    if result.results.is_empty() {
        let no_results = Label::new(Some("No results found"));
        no_results.add_css_class("dim-label");
        no_results.set_halign(gtk::Align::Center);
        no_results.set_margin_top(24);
        no_results.set_margin_bottom(24);
        results_list.append(&no_results);
        return;
    }
    
    // Display each result
    for item in &result.results {
        let row = create_result_row(item, launch_opts.clone());
        results_list.append(&row);
    }
}

/// Create a row widget for a search result
fn create_result_row(item: &SearchResultItem, launch_opts: Option<run::LaunchOpts>) -> ListBoxRow {
    let release = &item.release;
    let release_clone = release.clone();
    let launch_opts_clone = launch_opts.clone();
    
    // Create the content box
    let hbox = Box::new(gtk::Orientation::Horizontal, 12);
    hbox.set_halign(gtk::Align::Fill);
    hbox.set_margin_top(6);
    hbox.set_margin_bottom(6);
    hbox.set_margin_start(12);
    hbox.set_margin_end(12);
    
    // Left side: name and details
    let vbox = Box::new(gtk::Orientation::Vertical, 4);
    vbox.set_hexpand(true);
    vbox.set_halign(gtk::Align::Start);
    
    // Title
    let title = Label::new(Some(&release.name));
    title.set_halign(gtk::Align::Start);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.add_css_class("heading");
    vbox.append(&title);
    
    // Details line
    let mut details_parts = Vec::new();
    
    // Type
    let type_filter = ReleaseTypeFilter::from_id(release.release_type_id);
    let type_name = type_filter.name();
    details_parts.push(format!("{}", type_name));
    
    // Year
    if let Some(year) = release.release_year {
        details_parts.push(format!("{}", year));
    }
    
    // Group
    if let Some(group_id) = release.group_id {
        details_parts.push(format!("Group #{}", group_id));
    }
    
    // Rating
    details_parts.push(format!("★ {:.1}", release.rating));
    
    let details = Label::new(Some(&details_parts.join(" • ")));
    details.set_halign(gtk::Align::Start);
    details.add_css_class("dim-label");
    vbox.append(&details);
    
    // Right side: score
    let score_label = Label::new(Some(&format!("{:.0}%", item.score * 100.0)));
    score_label.set_halign(gtk::Align::End);
    score_label.add_css_class("dim-label");
    
    hbox.append(&vbox);
    hbox.append(&score_label);
    
    // Create the row
    let row = ListBoxRow::builder().activatable(true).build();
    row.set_child(Some(&hbox));
    
    // Connect activate signal for click handling
    row.connect_activate(move |row_widget| {
        if let Some(opts) = launch_opts_clone.clone() {
            let release_for_launch = release_clone.clone();
            
            // Find the parent window for the dialog
            if let Some(parent_window) = row_widget.root().and_then(|r| r.downcast::<gtk::Window>().ok()) {
                handle_release_launch(&parent_window, release_for_launch, opts);
            }
        }
    });
    
    row
}

/// Display an error message
fn display_error(results_list: &ListBox, error: &str) {
    while let Some(child) = results_list.first_child() {
        results_list.remove(&child);
    }
    
    let error_label = Label::new(Some(&format!("Error: {}", error)));
    error_label.set_halign(gtk::Align::Center);
    error_label.add_css_class("error-label");
    error_label.set_margin_top(24);
    error_label.set_margin_bottom(24);
    results_list.append(&error_label);
}

/// Simple synchronous search function for CLI usage
pub fn search(query: &str, limit: usize) -> Result<Vec<(u32, String)>, String> {
    breadbin_core::csdb::search_csdb(query, limit)
}

/// Check if CSDb data is available
pub fn has_data() -> bool {
    breadbin_core::csdb::has_csdb_data()
}

/// Initialize/sync CSDb database
pub async fn sync_database() -> Result<breadbin_core::csdb::CollectionStats, String> {
    breadbin_core::csdb::initialize_csdb_database().await
}