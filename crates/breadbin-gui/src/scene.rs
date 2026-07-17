// scene - CSDb Scene Database search view
//
// This module provides a GTK4-based UI for browsing and searching the CSDb
// (Commodore Scene Database). It shows the top-rated cached releases on load
// and switches to live search results as the user types.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gtk::glib;
use gtk::prelude::*;
use gtk::{Box, Label, ListBox, ListBoxRow, ScrolledWindow, SearchEntry, Spinner, Widget};
use adw::prelude::*;

use breadbin_core::core;
use breadbin_core::database::{DownloadLink, Release, ReleaseTypeFilter, SearchSort};
use breadbin_core::csdb;
use breadbin_core::diesel_db;
use breadbin_core::itch;
use breadbin_core::run;

use crate::config::Settings;
use crate::task::run_blocking;

/// How many top-rated releases to show when no search is active.
const TOP_LIMIT: usize = 20;
/// Maximum search results per query.
const SEARCH_LIMIT: usize = 50;

/// Disk/program image extensions VICE can autostart, best first.
const RUN_EXTS: &[&str] = &[".d64", ".g64", ".d81", ".d71", ".prg", ".crt", ".t64", ".tap"];
/// Further extensions VICE understands but cannot autostart directly.
const EXTRA_VICE_EXTS: &[&str] = &[".nib", ".p00", ".x64", ".sid"];

/// Cache directory for downloaded scene releases
fn scene_downloads_dir() -> PathBuf {
    core::user_data_dir().join("scene_downloads")
}

/// Cache directory for release screenshots
fn scene_shots_dir() -> PathBuf {
    core::user_data_dir().join("scene_shots")
}

/// Whether core::fetch can retrieve this URL (ureq is HTTP(S)-only,
/// but CSDb often lists ftp:// mirrors first).
fn is_fetchable_url(url: &str) -> bool {
    let url_lower = url.to_lowercase();
    url_lower.starts_with("http://") || url_lower.starts_with("https://")
}

/// Check if a download link is a VICE-compatible C64 file. Zip archives count:
/// fetch_and_prepare_release extracts the best runnable member from them.
fn is_vice_compatible_link(download: &DownloadLink) -> bool {
    let url_lower = download.url.to_lowercase();
    if RUN_EXTS
        .iter()
        .chain(EXTRA_VICE_EXTS)
        .chain(std::iter::once(&".zip"))
        .any(|ext| url_lower.ends_with(ext))
    {
        return true;
    }

    if let Some(ref file_type) = download.file_type {
        let file_type_lower = file_type.to_lowercase();
        // Common MIME types for C64 files
        if ["d64", "sid", "prg", "crt", "t64", "tap", "zip"]
            .iter()
            .any(|t| file_type_lower.contains(t))
        {
            return true;
        }
    }

    false
}

/// Find VICE-compatible download URLs for a release, best first: direct
/// disk/program images, then zip archives ranked by download count (the main
/// release is almost always the most-downloaded file, ahead of SID rips and
/// the like), then itch.io downloads if available.
fn find_vice_download_candidates(release: &Release) -> Vec<String> {
    let candidates: Vec<&DownloadLink> = release
        .downloads
        .iter()
        .filter(|d| is_fetchable_url(&d.url) && is_vice_compatible_link(d))
        .collect();

    let mut urls: Vec<String> = candidates
        .iter()
        .filter(|d| {
            let url_lower = d.url.to_lowercase();
            RUN_EXTS.iter().any(|ext| url_lower.ends_with(ext))
        })
        .map(|d| d.url.clone())
        .collect();

    let mut zips: Vec<&&DownloadLink> = candidates
        .iter()
        .filter(|d| d.url.to_lowercase().ends_with(".zip"))
        .collect();
    zips.sort_by(|a, b| b.downloads.cmp(&a.downloads));
    urls.extend(zips.into_iter().map(|d| d.url.clone()));

    // If no VICE-compatible CSDb download, try crawling itch.io (including related pages)
    if urls.is_empty() {
        if let Some(website) = &release.website {
            if itch::is_itchio_url(website) {
                match itch::find_all_downloads_from_itchio(website) {
                    Ok(downloads) => {
                        for download_url in downloads {
                            let url_lower = download_url.to_lowercase();
                            if RUN_EXTS.iter().any(|ext| url_lower.ends_with(ext)) {
                                urls.push(download_url);
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Failed to crawl {}: {}", website, e);
                    }
                }
            }
        }
    }

    urls
}

/// Download one candidate URL and reduce it to runnable payload bytes plus a
/// file extension. For zip archives this extracts the best runnable member
/// (erroring when the archive holds none); other files pass through with the
/// URL's extension.
fn fetch_runnable(download_url: &str) -> Result<(Vec<u8>, String), String> {
    let data = core::fetch(download_url, &[])
        .map_err(|e| format!("Failed to download {}: {}", download_url, e))?;

    if data.starts_with(b"PK") {
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(data))
            .map_err(|e| format!("bad zip: {}", e))?;
        let names: Vec<String> = zip.file_names().map(String::from).collect();
        let member = RUN_EXTS.iter().find_map(|ext| {
            let mut m: Vec<&String> = names.iter().filter(|n| n.to_lowercase().ends_with(ext)).collect();
            m.sort_by_key(|n| n.len());
            m.first().map(|s| (*s).clone())
        });

        let member_name = member
            .ok_or_else(|| "no runnable disk/program image inside the archive".to_string())?;

        let mut member_data = Vec::new();
        use std::io::Read;
        zip.by_name(&member_name)
            .map_err(|e| format!("Failed to extract {}: {}", member_name, e))?
            .read_to_end(&mut member_data)
            .map_err(|e| format!("Failed to read {}: {}", member_name, e))?;

        let ext = Path::new(&member_name)
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_lowercase())
            .unwrap_or_else(|| "prg".to_string());
        Ok((member_data, ext))
    } else {
        let ext = Path::new(download_url)
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_lowercase())
            .unwrap_or_else(|| "bin".to_string());
        Ok((data, ext))
    }
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

    // Try each candidate download link (from CSDb or itch.io) in turn: a
    // release often lists rip archives (SIDs, graphics) alongside the actual
    // demo, so a zip without a runnable member means "try the next one".
    let candidates = find_vice_download_candidates(release);
    if candidates.is_empty() {
        return Err(format!("No VICE-compatible download found for release \"{}\"", release.name));
    }

    let mut last_err = String::new();
    let mut found: Option<(Vec<u8>, String)> = None;
    for download_url in &candidates {
        match fetch_runnable(download_url) {
            Ok(payload_ext) => {
                found = Some(payload_ext);
                break;
            }
            Err(e) => {
                eprintln!("Skipping {}: {}", download_url, e);
                last_err = e;
            }
        }
    }
    let (payload, ext) = found.ok_or_else(|| {
        format!("no runnable disk/program image found for \"{}\" ({})", release.name, last_err)
    })?;

    // Create safe filename
    let safe: String = release
        .name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();

    std::fs::create_dir_all(&dir)
        .map_err(|e| format!("Failed to create cache directory: {}", e))?;

    let dest = dir.join(format!("{}_{}.{}", release.id, safe, ext));
    std::fs::write(&dest, &payload)
        .map_err(|e| format!("Failed to write {}: {}", dest.display(), e))?;

    Ok(dest)
}

/// Cached screenshot path for a release (downloading once), or None.
fn ensure_screenshot(release_id: u32, url: &str) -> Option<PathBuf> {
    let ext = Path::new(url)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .filter(|e| matches!(e.as_str(), "png" | "gif" | "jpg" | "jpeg"))
        .unwrap_or_else(|| "png".to_string());
    let cache = scene_shots_dir().join(format!("{release_id}.{ext}"));
    if cache.metadata().map(|m| m.len() > 0).unwrap_or(false) {
        return Some(cache);
    }
    let data = core::fetch(url, &[]).ok()?;
    if data.len() < 200 {
        return None;
    }
    std::fs::create_dir_all(scene_shots_dir()).ok()?;
    std::fs::write(&cache, data).ok()?;
    Some(cache)
}

/// Load a screenshot as a thumbnail-sized texture so rows keep a fixed height.
fn thumbnail_texture(path: &Path) -> Option<gdk::Texture> {
    let rgba = image::open(path).ok()?.thumbnail(128, 96).to_rgba8();
    let (w, h) = rgba.dimensions();
    let bytes = glib::Bytes::from_owned(rgba.into_raw());
    let tex = gdk::MemoryTexture::new(
        w as i32,
        h as i32,
        gdk::MemoryFormat::R8g8b8a8,
        &bytes,
        w as usize * 4,
    );
    Some(tex.upcast())
}

/// Show a small modal while a release is downloaded and prepared.
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

    let spinner = Spinner::new();
    spinner.start();
    spinner.set_halign(gtk::Align::Center);

    let status_label = Label::builder()
        .label(&format!("Downloading {}…", release_name))
        .wrap(true)
        .build();
    status_label.add_css_class("caption");

    vbox.append(&spinner);
    vbox.append(&status_label);
    dialog.set_child(Some(&vbox));
    dialog.present(Some(parent));

    dialog
}

/// Post a toast on the page's overlay, if one is reachable from the widget.
fn toast_from(widget: &impl IsA<gtk::Widget>, message: &str) {
    if let Some(overlay) = widget
        .ancestor(adw::ToastOverlay::static_type())
        .and_then(|w| w.downcast::<adw::ToastOverlay>().ok())
    {
        overlay.add_toast(adw::Toast::new(message));
    }
}

/// Handle launching a release: download with a loading dialog, then spawn VICE.
/// Success is reported with a toast, failure with an alert dialog.
fn handle_release_launch(row: &ListBoxRow, release: Release, launch_opts: run::LaunchOpts) {
    let Some(parent) = row.root().and_then(|r| r.downcast::<gtk::Window>().ok()) else {
        return;
    };

    let loading_dialog = show_loading_dialog(&parent, &release.name);
    let row = row.clone();

    glib::spawn_future_local(async move {
        let release_for_prepare = release.clone();
        let result = run_blocking(move || fetch_and_prepare_release(&release_for_prepare)).await;

        loading_dialog.close();

        let launch_result = result.and_then(|path| {
            run::spawn(&path, &launch_opts).map_err(|e| e.to_string())
        });

        match launch_result {
            Ok(_) => toast_from(&row, &format!("Launching {} in VICE", release.name)),
            Err(e) => {
                let dialog = adw::AlertDialog::new(
                    Some("Launch Failed"),
                    Some(&format!("Failed to launch {}: {}", release.name, e)),
                );
                dialog.add_response("close", "Close");
                dialog.present(Some(&parent));
            }
        }
    });
}

/// Create a row widget for a release. `rank` adds a "#N" badge (top list).
fn create_release_row(release: &Release, rank: Option<usize>) -> ListBoxRow {
    let hbox = Box::new(gtk::Orientation::Horizontal, 12);
    hbox.set_margin_top(6);
    hbox.set_margin_bottom(6);
    hbox.set_margin_start(12);
    hbox.set_margin_end(12);

    if let Some(rank) = rank {
        let rank_label = Label::new(Some(&format!("#{}", rank)));
        rank_label.set_valign(gtk::Align::Center);
        rank_label.set_width_chars(3);
        rank_label.add_css_class("badge");
        hbox.append(&rank_label);
    }

    // Screenshot thumbnail, loaded (and cached) in the background
    if let Some(url) = release.screenshot_url.clone() {
        let picture = gtk::Picture::builder()
            .width_request(96)
            .height_request(72)
            .content_fit(gtk::ContentFit::Cover)
            .build();
        picture.add_css_class("cover-picture");
        hbox.append(&picture);

        let release_id = release.id;
        let pic_weak = picture.downgrade();
        glib::spawn_future_local(async move {
            let path = run_blocking(move || ensure_screenshot(release_id, &url)).await;
            let Some(pic) = pic_weak.upgrade() else { return };
            if let Some(texture) = path.as_deref().and_then(thumbnail_texture) {
                pic.set_paintable(Some(&texture));
            }
        });
    }

    // Name and details
    let vbox = Box::new(gtk::Orientation::Vertical, 4);
    vbox.set_hexpand(true);
    vbox.set_valign(gtk::Align::Center);

    let title = Label::new(Some(&release.name));
    title.set_halign(gtk::Align::Start);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.add_css_class("heading");
    vbox.append(&title);

    let mut details_parts = vec![ReleaseTypeFilter::from_id(release.release_type_id).name().to_string()];
    if let Some(year) = release.release_year {
        details_parts.push(year.to_string());
    }
    if let Some(group) = &release.group {
        details_parts.push(group.name.clone());
    }
    if release.rating > 0.0 {
        details_parts.push(format!("★ {:.1}", release.rating));
    }
    let vice_downloads = release.downloads.iter().filter(|d| is_vice_compatible_link(d)).count();
    if vice_downloads > 0 {
        details_parts.push(format!(
            "{} download{}",
            vice_downloads,
            if vice_downloads == 1 { "" } else { "s" }
        ));
    } else {
        details_parts.push("no VICE download".to_string());
    }

    let details = Label::new(Some(&details_parts.join(" • ")));
    details.set_halign(gtk::Align::Start);
    details.set_ellipsize(gtk::pango::EllipsizeMode::End);
    details.add_css_class("dim-label");
    vbox.append(&details);

    hbox.append(&vbox);

    let row = ListBoxRow::builder().activatable(true).build();
    row.set_child(Some(&hbox));
    row
}

/// Replace the list contents with a message label.
fn show_message(results_list: &ListBox, message: &str, css_class: &str) {
    let label = Label::new(Some(message));
    label.set_wrap(true);
    label.add_css_class(css_class);
    label.set_margin_top(24);
    label.set_margin_bottom(24);
    results_list.append(&label);
}

/// Main scene view widget container with settings
pub fn build_with_settings(settings: &Settings) -> Widget {
    let launch_opts = settings.launch_opts();

    let vbox = Box::new(gtk::Orientation::Vertical, 12);
    vbox.set_hexpand(true);
    vbox.set_vexpand(true);
    vbox.set_widget_name("scene-view");

    // Search entry with an activity spinner
    let search_entry = SearchEntry::builder()
        .placeholder_text("Search CSDb releases (e.g. comaland, type:demo year:1994)…")
        .hexpand(true)
        .build();
    search_entry.set_search_delay(300);

    let spinner = Spinner::new();
    spinner.set_valign(gtk::Align::Center);

    let search_box = Box::new(gtk::Orientation::Horizontal, 6);
    search_box.set_margin_top(12);
    search_box.set_margin_start(12);
    search_box.set_margin_end(12);
    search_box.append(&search_entry);
    search_box.append(&spinner);

    // Heading that describes the current list (top releases or search results)
    let heading = Label::new(None);
    heading.set_halign(gtk::Align::Start);
    heading.set_margin_start(12);
    heading.set_margin_end(12);
    heading.add_css_class("heading");

    let results_list = ListBox::new();
    results_list.set_selection_mode(gtk::SelectionMode::None);
    results_list.add_css_class("boxed-list");

    // The releases currently shown, indexed like the list rows.
    let current_releases: Rc<RefCell<Vec<Release>>> = Rc::new(RefCell::new(Vec::new()));
    {
        let current_releases = current_releases.clone();
        let launch_opts = launch_opts.clone();
        results_list.connect_row_activated(move |_, row| {
            let Some(release) = current_releases
                .borrow()
                .get(row.index().max(0) as usize)
                .cloned()
            else {
                return;
            };
            match &launch_opts {
                Some(opts) => handle_release_launch(row, release, opts.clone()),
                None => toast_from(row, "C64 Forever not detected — configure it in Preferences"),
            }
        });
    }
    results_list.set_margin_start(12);
    results_list.set_margin_end(12);
    results_list.set_margin_bottom(12);
    results_list.set_valign(gtk::Align::Start);

    let scrolled = ScrolledWindow::builder()
        .child(&results_list)
        .hexpand(true)
        .vexpand(true)
        .build();

    vbox.append(&search_box);
    vbox.append(&heading);
    vbox.append(&scrolled);

    let overlay = adw::ToastOverlay::new();
    overlay.set_child(Some(&vbox));

    // Run a query off the main thread and repopulate the list. An empty query
    // shows the top-rated cached releases; generation guards drop stale results.
    let generation = Rc::new(Cell::new(0u64));
    let run_query = {
        let results_list = results_list.downgrade();
        let heading = heading.downgrade();
        let spinner = spinner.downgrade();
        let current_releases = current_releases.clone();
        move |query: String| {
            let generation = generation.clone();
            let this_gen = generation.get() + 1;
            generation.set(this_gen);

            if let Some(s) = spinner.upgrade() {
                s.start();
            }

            let results_list = results_list.clone();
            let heading = heading.clone();
            let spinner = spinner.clone();
            let current_releases = current_releases.clone();
            glib::spawn_future_local(async move {
                let is_browse = query.is_empty();
                let outcome: Result<(String, Vec<Release>), String> = if is_browse {
                    run_blocking(|| csdb::get_top_releases(TOP_LIMIT))
                        .await
                        .map(|releases| {
                            (format!("Top {} CSDb Releases", releases.len()), releases)
                        })
                } else {
                    let query_for_db = query.clone();
                    run_blocking(move || {
                        let mut search_query = diesel_db::parse_search_query(&query_for_db);
                        search_query.limit = SEARCH_LIMIT;
                        search_query.offset = 0;
                        search_query.sort_by = SearchSort::Relevance;
                        diesel_db::execute_search_query(&search_query)
                    })
                    .await
                    .map(|result| {
                        let releases: Vec<Release> =
                            result.results.into_iter().map(|item| item.release).collect();
                        let title = if result.total as usize > releases.len() {
                            format!("Showing {} of {} results for “{}”", releases.len(), result.total, query)
                        } else {
                            format!("{} result(s) for “{}”", result.total, query)
                        };
                        (title, releases)
                    })
                };

                // A newer query superseded this one while we were loading
                if generation.get() != this_gen {
                    return;
                }
                if let Some(s) = spinner.upgrade() {
                    s.stop();
                }
                let (Some(results_list), Some(heading)) =
                    (results_list.upgrade(), heading.upgrade())
                else {
                    return;
                };

                while let Some(child) = results_list.first_child() {
                    results_list.remove(&child);
                }

                match outcome {
                    Ok((title, releases)) => {
                        heading.set_label(&title);
                        if releases.is_empty() {
                            if is_browse {
                                heading.set_label("CSDb");
                                show_message(
                                    &results_list,
                                    "No CSDb releases cached yet. Use the menu → “Sync CSDb data” to fetch the top releases from csdb.dk.",
                                    "dim-label",
                                );
                            } else {
                                show_message(&results_list, "No results found", "dim-label");
                            }
                        } else {
                            for (i, release) in releases.iter().enumerate() {
                                let rank = if is_browse { Some(i + 1) } else { None };
                                results_list.append(&create_release_row(release, rank));
                            }
                        }
                        current_releases.replace(releases);
                    }
                    Err(e) => {
                        eprintln!("CSDb query failed: {}", e);
                        heading.set_label("CSDb");
                        show_message(&results_list, &format!("Error: {}", e), "error-label");
                        current_releases.replace(Vec::new());
                    }
                }
            });
        }
    };

    // Live search as the user types (debounced by search_delay), Enter included
    let run_query_changed = run_query.clone();
    search_entry.connect_search_changed(move |entry| {
        run_query_changed(entry.text().trim().to_string());
    });
    let run_query_activate = run_query.clone();
    search_entry.connect_activate(move |entry| {
        run_query_activate(entry.text().trim().to_string());
    });

    // Initial content: top-rated cached releases
    run_query(String::new());

    overlay.upcast::<Widget>()
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
