// kiosk - the Games tab: one horizontal strip per genre, expandable to full grid.

use std::path::PathBuf;

use adw::prelude::*;

use breadbin_core::{cover, games, run};
use std::rc::Rc;
use std::cell::RefCell;

use crate::config::Settings;
use crate::task::run_blocking;

extern crate async_channel;

const STRIP_MAX: usize = 8;

// ── Cover texture ─────────────────────────────────────────────────────────────

/// Badges drawn onto a game's cover.
#[derive(Clone, Copy, Default)]
struct Badges {
    joystick: bool,
    top_rated: bool,
}

/// The GB64 canon sets that decide each game's [`Badges`].
struct BadgeSets {
    joystick: std::collections::HashSet<String>,
    top_rated: std::collections::HashSet<String>,
}

impl BadgeSets {
    fn load() -> Self {
        BadgeSets { joystick: games::joystick_canons(), top_rated: games::top_rated_canons() }
    }

    fn for_row(&self, row: &games::Row) -> Badges {
        let canon = games::canon_of(row);
        Badges { joystick: self.joystick.contains(&canon), top_rated: self.top_rated.contains(&canon) }
    }
}

fn texture_from_path(path: &std::path::Path, badges: Badges) -> Option<gdk::Texture> {
    let mut rgba = image::open(path).ok()?.to_rgba8();
    if badges.top_rated {
        breadbin_core::cover::draw_rating_badge(&mut rgba);
    }
    if badges.joystick {
        breadbin_core::cover::draw_joystick_badge(&mut rgba);
    }
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

// ── Card widget ───────────────────────────────────────────────────────────────

struct Card {
    root: gtk::Box,
    picture: gtk::Picture,
}

impl Card {
    fn new(title: &str, is_local: bool, hexpand: bool) -> Self {
        let picture = gtk::Picture::builder()
            .width_request(110)
            .height_request(138)
            .content_fit(gtk::ContentFit::Contain)
            .build();
        picture.add_css_class("cover-picture");

        let label = gtk::Label::builder()
            .label(title)
            .max_width_chars(14)
            .wrap(true)
            .lines(2)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .xalign(0.5)
            .build();
        label.add_css_class("cover-title");

        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(6)  // Increased spacing for better title readability
            .hexpand(hexpand)
            .build();
        root.add_css_class("cover-card");
        root.append(&picture);
        root.append(&label);

        if !is_local {
            let dl_btn = gtk::Label::builder()
                .label("⬇ Download")
                .build();
            dl_btn.add_css_class("caption");
            root.append(&dl_btn);
        }

        Card { root, picture }
    }
}

// ── Section header ────────────────────────────────────────────────────────────

thread_local! {
    static CHIP_CLASSES: std::cell::RefCell<std::collections::HashSet<String>> =
        Default::default();
}

/// CSS class painting a chip in the given colour, registered display-wide the
/// first time each colour is seen (the palette is small, so this stays tiny).
fn chip_color_class(r: u8, g: u8, b: u8) -> String {
    let class = format!("chip-{r:02x}{g:02x}{b:02x}");
    CHIP_CLASSES.with(|seen| {
        if seen.borrow_mut().insert(class.clone()) {
            if let Some(display) = gdk::Display::default() {
                let provider = gtk::CssProvider::new();
                provider.load_from_data(&format!(
                    ".{class} {{ background-color: #{r:02x}{g:02x}{b:02x}; border-radius: 2px; }}"
                ));
                gtk::style_context_add_provider_for_display(
                    &display,
                    &provider,
                    gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
                );
            }
        }
    });
    class
}

/// A full-width section header with a C64 colour-bar chip on the left.
pub fn section_header_widget(title: &str) -> gtk::Box {
    let [r, g, b] = breadbin_core::core::palette::bar_for(title);
    let chip = gtk::Box::builder()
        .width_request(8)
        .height_request(32)
        .build();
    chip.add_css_class("section-chip");
    chip.add_css_class(&chip_color_class(r, g, b));

    let label = gtk::Label::builder()
        .label(title.to_uppercase())
        .xalign(0.0)
        .build();
    label.add_css_class("section-header");

    let sep = gtk::Separator::builder()
        .orientation(gtk::Orientation::Horizontal)
        .hexpand(true)
        .valign(gtk::Align::Center)
        .build();
    sep.add_css_class("section-sep");

    let row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(10)
        .margin_top(8)
        .margin_bottom(4)
        .build();
    row.append(&chip);
    row.append(&label);
    row.append(&sep);
    row
}

// ── Detail dialog ─────────────────────────────────────────────────────────────

fn show_detail(
    parent: &gtk::Window,
    row: games::Row,
    cover_path: Option<PathBuf>,
    badges: Badges,
    launch_opts: Option<run::LaunchOpts>,
    on_refresh: Rc<dyn Fn()>,
) {
    let dialog = adw::Dialog::new();
    dialog.set_title(&row.title);
    dialog.set_content_width(400);

    let vbox = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .margin_top(24)
        .margin_bottom(24)
        .margin_start(24)
        .margin_end(24)
        .build();

    if let Some(path) = &cover_path {
        if let Some(texture) = texture_from_path(path, badges) {
            let pic = gtk::Picture::builder()
                .paintable(&texture)
                .content_fit(gtk::ContentFit::Contain)
                .height_request(300)
                .build();
            vbox.append(&pic);
        }
    }

    let title_lbl = gtk::Label::builder()
        .label(&row.title)
        .wrap(true)
        .xalign(0.0)
        .build();
    title_lbl.add_css_class("title-2");
    vbox.append(&title_lbl);

    if !row.genre.is_empty() {
        let genre_lbl = gtk::Label::builder()
            .label(format!("Genre: {}", row.genre))
            .xalign(0.0)
            .build();
        genre_lbl.add_css_class("caption");
        vbox.append(&genre_lbl);
    }

    let btn_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .halign(gtk::Align::End)
        .build();

    // Hide / Unhide button
    let is_hidden = games::load_hidden().contains(&row.display);
    let hide_btn = gtk::Button::builder()
        .label(if is_hidden { "Unhide" } else { "Hide" })
        .build();
    hide_btn.add_css_class("destructive-action");
    let row_display = row.display.clone();
    let dialog_for_hide = dialog.clone();
    let on_refresh2 = on_refresh.clone();
    hide_btn.connect_clicked(move |_| {
        let currently_hidden = games::load_hidden().contains(&row_display);
        games::set_hidden(&row_display, !currently_hidden);
        dialog_for_hide.close();
        on_refresh2();
    });
    btn_box.append(&hide_btn);

    if !row.is_local() {
        let dl_btn = gtk::Button::builder().label("Download").build();
        let row2 = row.clone();
        let dialog2 = dialog.clone();
        dl_btn.connect_clicked(move |_| {
            let r = row2.clone();
            glib::spawn_future_local(async move {
                let _ = run_blocking(move || games::resolve(&r)).await;
            });
            dialog2.close();
        });
        btn_box.append(&dl_btn);
    }

    let play_btn = gtk::Button::builder().label("Play").build();
    play_btn.add_css_class("suggested-action");
    if let Some(opts) = launch_opts {
        let row3 = row.clone();
        let dialog3 = dialog.clone();
        let parent_weak = parent.downgrade();
        play_btn.connect_clicked(move |_| {
            let controls = run::controls_description(run::joystick_present());
            let msg = controls.join("\n");
            let alert = adw::AlertDialog::new(Some("Controls"), Some(&msg));
            alert.add_response("cancel", "Cancel");
            alert.add_response("play", "Play");
            alert.set_response_appearance("play", adw::ResponseAppearance::Suggested);
            let row4 = row3.clone();
            let opts2 = breadbin_core::run::LaunchOpts {
                warp: opts.warp,
                fullscreen: opts.fullscreen,
                keyboard: opts.keyboard,
                drive_sound: opts.drive_sound,
                forever: opts.forever.clone(),
            };
            let dialog4 = dialog3.clone();
            let pw = parent_weak.clone();
            alert.connect_response(None, move |_, resp| {
                if resp == "play" {
                    let r = row4.clone();
                    let o = breadbin_core::run::LaunchOpts {
                        warp: opts2.warp,
                        fullscreen: opts2.fullscreen,
                        keyboard: opts2.keyboard,
                        drive_sound: opts2.drive_sound,
                        forever: opts2.forever.clone(),
                    };
                    glib::spawn_future_local(async move {
                        let r2 = r.clone();
                        let path = run_blocking(move || games::resolve(&r)).await;
                        if let Some(p) = path {
                            games::record_play(&r2);
                            let _ = run::spawn(&p, &o);
                        }
                    });
                    dialog4.close();
                }
            });
            if let Some(w) = pw.upgrade() {
                alert.present(Some(&w));
            }
        });
    } else {
        play_btn.set_sensitive(false);
        play_btn.set_tooltip_text(Some("C64 Forever not detected — configure in Preferences"));
    }
    btn_box.append(&play_btn);
    vbox.append(&btn_box);

    let scroll = gtk::ScrolledWindow::builder()
        .child(&vbox)
        .propagate_natural_height(true)
        .build();

    dialog.set_child(Some(&scroll));
    dialog.present(Some(parent));
}

// ── Card builder (shared between strip and grid) ──────────────────────────────

fn build_game_card(
    row: &games::Row,
    cidx: std::collections::HashMap<String, String>,
    badges: Badges,
    launch_opts: Option<run::LaunchOpts>,
    hexpand: bool,
    cover_done: Option<async_channel::Sender<()>>,
    on_refresh: Rc<dyn Fn()>,
) -> gtk::Box {
    let is_local = row.is_local();
    let card = Card::new(&row.title, is_local, hexpand);

    let cidx2 = cidx.clone();
    let row_clone = row.clone();
    let pic = card.picture.clone();
    glib::spawn_future_local(async move {
        let cover_path =
            run_blocking(move || games::cover_for(&row_clone, &cidx2)).await;
        if let Some(path) = &cover_path {
            if let Some(texture) = texture_from_path(path, badges) {
                pic.set_paintable(Some(&texture));
            }
        }
        if let Some(tx) = cover_done {
            tx.send(()).await.ok();
        }
    });

    let gesture = gtk::GestureClick::new();
    let row_for_click = row.clone();
    let cidx3 = cidx.clone();
    let launch_for_click = launch_opts.clone();
    let on_refresh3 = on_refresh.clone();
    gesture.connect_released(move |g, _, _, _| {
        let Some(widget) = g.widget() else { return };
        let Some(root) = widget.root() else { return };
        let Some(win) = root.downcast_ref::<gtk::Window>() else { return };

        let row_snap = row_for_click.clone();
        let cidx_c = cidx3.clone();
        let win_c = win.clone();
        let launch = launch_for_click.clone();
        let on_refresh4 = on_refresh3.clone();
        glib::spawn_future_local(async move {
            let row_snap2 = row_snap.clone();
            let cidx_c2 = cidx_c.clone();
            let cover_path =
                run_blocking(move || games::cover_for(&row_snap2, &cidx_c2)).await;
            show_detail(&win_c, row_snap, cover_path, badges, launch, on_refresh4);
        });
    });
    card.root.add_controller(gesture);

    card.root
}

// ── Kiosk genre section ───────────────────────────────────────────────────────

fn build_genre_section(
    section_name: &str,
    row_indices: &[usize],
    rows: &[games::Row],
    cidx: &std::collections::HashMap<String, String>,
    badge_sets: &BadgeSets,
    launch_opts: &Option<run::LaunchOpts>,
    on_refresh: Rc<dyn Fn()>,
) -> gtk::Box {
    let section_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .build();

    // Header row with optional expand button
    let header_row = section_header_widget(section_name);
    let has_overflow = row_indices.len() > STRIP_MAX;
    let toggle_btn = gtk::Button::builder()
        .label(format!("Show All ({})", row_indices.len()))
        .visible(has_overflow)
        .build();
    toggle_btn.add_css_class("pill");
    header_row.append(&toggle_btn);
    section_box.append(&header_row);

    // Stack: "strip" (horizontal scroll row) vs "grid" (full FlowBox)
    let stack = gtk::Stack::builder()
        .transition_type(gtk::StackTransitionType::SlideUpDown)
        .transition_duration(180)
        .vhomogeneous(false)
        .build();

    // ── Strip page ────────────────────────────────────────────────────────────
    let strip_scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vscrollbar_policy(gtk::PolicyType::Never)
        .build();
    let strip_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(5)
        .hexpand(true)
        .build();

    for &idx in row_indices.iter().take(STRIP_MAX) {
        let row = &rows[idx];
        // Cards expand to fill the strip when fewer than STRIP_MAX items
        let hexpand = row_indices.len() <= STRIP_MAX;
        let card_widget = build_game_card(row, cidx.clone(), badge_sets.for_row(row), launch_opts.clone(), hexpand, None, on_refresh.clone());
        strip_box.append(&card_widget);
    }

    strip_scroll.set_child(Some(&strip_box));
    stack.add_named(&strip_scroll, Some("strip"));

    // ── Grid page: first strip row (identical to strip) + overflow FlowBox ───
    let grid_vbox = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(8)
        .build();

    // First row mirrors the strip exactly (same card count, same sizing).
    let grid_first_row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(5)
        .hexpand(true)
        .build();

    let loading_header = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(4)
        .margin_top(4)
        .margin_bottom(4)
        .halign(gtk::Align::Center)
        .build();

    let overflow_count = row_indices.len().saturating_sub(STRIP_MAX);
    let loading_label = gtk::Label::builder()
        .label(format!("Loading covers — 0 / {overflow_count}"))
        .build();
    loading_label.add_css_class("caption");

    let loading_bar = gtk::ProgressBar::builder()
        .width_request(320)
        .show_text(false)
        .build();

    loading_header.append(&loading_label);
    loading_header.append(&loading_bar);
    loading_header.set_visible(false);

    let flow = gtk::FlowBox::builder()
        .column_spacing(5)
        .row_spacing(5)
        .homogeneous(false)
        .min_children_per_line(STRIP_MAX as u32)
        .max_children_per_line(STRIP_MAX as u32)
        .selection_mode(gtk::SelectionMode::None)
        .build();

    grid_vbox.append(&grid_first_row);
    grid_vbox.append(&loading_header);
    grid_vbox.append(&flow);
    stack.add_named(&grid_vbox, Some("grid"));
    stack.set_visible_child_name("strip");
    section_box.append(&stack);

    // Snapshot what the grid needs; owned data so the closure is 'static.
    let grid_rows: Vec<(games::Row, Badges)> = row_indices
        .iter()
        .map(|&idx| (rows[idx].clone(), badge_sets.for_row(&rows[idx])))
        .collect();
    let cidx_for_grid = cidx.clone();
    let launch_for_grid = launch_opts.clone();
    let on_refresh_for_grid = on_refresh.clone();

    // Wire up the toggle button — populate grid on first expand
    let stack_weak = stack.downgrade();
    let first_row_weak = grid_first_row.downgrade();
    let flow_weak = flow.downgrade();
    let loading_header_weak = loading_header.downgrade();
    let loading_label_weak = loading_label.downgrade();
    let loading_bar_weak = loading_bar.downgrade();
    let total = row_indices.len();
    let row_count = row_indices.len(); // Capture the value for the closure
    let grid_populated = std::rc::Rc::new(std::cell::Cell::new(false));
    toggle_btn.connect_clicked(move |btn| {
        let Some(s) = stack_weak.upgrade() else { return };
        if s.visible_child_name().as_deref() == Some("strip") {
            if !grid_populated.get() {
                grid_populated.set(true);

                // First row: same STRIP_MAX cards, same hexpand as strip.
                // Use same hexpand logic as strip for consistent sizing.
                let hexpand_first_row = row_count <= STRIP_MAX;
                if let Some(first_row) = first_row_weak.upgrade() {
                    for (row, badges) in grid_rows.iter().take(STRIP_MAX) {
                        let card = build_game_card(
                            row,
                            cidx_for_grid.clone(),
                            *badges,
                            launch_for_grid.clone(),
                            hexpand_first_row,
                            None,
                            on_refresh_for_grid.clone(),
                        );
                        first_row.append(&card);
                    }
                }

                // Overflow items go into the FlowBox, tracked by the progress bar.
                let overflow = total.saturating_sub(STRIP_MAX);
                if overflow > 0 {
                    if let Some(flow) = flow_weak.upgrade() {
                        let (tx, rx) = async_channel::bounded::<()>(overflow);

                        if let Some(hdr) = loading_header_weak.upgrade() {
                            hdr.set_visible(true);
                        }
                        if let Some(lbl) = loading_label_weak.upgrade() {
                            lbl.set_label(&format!("Loading covers — 0 / {overflow}"));
                        }
                        if let Some(bar) = loading_bar_weak.upgrade() {
                            bar.set_fraction(0.0);
                        }

                        let lbl_weak2 = loading_label_weak.clone();
                        let bar_weak2 = loading_bar_weak.clone();
                        let hdr_weak2 = loading_header_weak.clone();
                        glib::spawn_future_local(async move {
                            let mut done = 0usize;
                            while rx.recv().await.is_ok() {
                                done += 1;
                                let frac = done as f64 / overflow as f64;
                                if let Some(b) = bar_weak2.upgrade() {
                                    b.set_fraction(frac);
                                }
                                if let Some(l) = lbl_weak2.upgrade() {
                                    l.set_label(&format!("Loading covers — {done} / {overflow}"));
                                }
                                if done >= overflow {
                                    if let Some(h) = hdr_weak2.upgrade() {
                                        h.set_visible(false);
                                    }
                                    break;
                                }
                            }
                        });

                        for (row, badges) in grid_rows.iter().skip(STRIP_MAX) {
                            let card = build_game_card(
                                row,
                                cidx_for_grid.clone(),
                                *badges,
                                launch_for_grid.clone(),
                                false,
                                Some(tx.clone()),
                                on_refresh_for_grid.clone(),
                            );
                            let fb_child = gtk::FlowBoxChild::new();
                            fb_child.set_child(Some(&card));
                            flow.append(&fb_child);
                        }
                    }
                }
            }
            s.set_visible_child_name("grid");
            btn.set_label("Show Less");
        } else {
            s.set_visible_child_name("strip");
            btn.set_label(&format!("Show All ({})", total));
        }
    });

    section_box
}

// ── Main build function ───────────────────────────────────────────────────────

fn populate(outer: &gtk::Box, launch_opts: Option<run::LaunchOpts>, on_refresh: Rc<dyn Fn()>) {
    // Show a spinner while loading
    while let Some(child) = outer.first_child() {
        outer.remove(&child);
    }
    let spinner = gtk::Spinner::new();
    spinner.start();
    spinner.set_margin_top(60);
    spinner.set_halign(gtk::Align::Center);
    outer.append(&spinner);

    let outer_weak = outer.downgrade();
    glib::spawn_future_local(async move {
        let (rows, cidx, sections, badge_sets) =
            run_blocking(|| {
                let all_rows = games::load_rows();
                let hidden = games::load_hidden();
                let rows: Vec<games::Row> = all_rows
                    .into_iter()
                    .filter(|r| !hidden.contains(&r.display))
                    .collect();
                let cidx = cover::load_index();
                let sections = games::sections(&rows);
                (rows, cidx, sections, BadgeSets::load())
            })
            .await;

        let Some(outer) = outer_weak.upgrade() else {
            return;
        };

        while let Some(child) = outer.first_child() {
            outer.remove(&child);
        }

        if rows.is_empty() {
            let status = adw::StatusPage::builder()
                .icon_name("applications-games-symbolic")
                .title("No games found")
                .description("Use Refresh catalogue to download the game list.")
                .build();
            outer.append(&status);
            return;
        }

        let scrolled = gtk::ScrolledWindow::builder()
            .vexpand(true)
            .hexpand(true)
            .build();

        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(8)
            .margin_top(10)
            .margin_bottom(10)
            .margin_start(10)
            .margin_end(10)
            .build();

        for (section_name, row_indices) in &sections {
            let section = build_genre_section(
                section_name,
                row_indices,
                &rows,
                &cidx,
                &badge_sets,
                &launch_opts,
                on_refresh.clone(),
            );
            content.append(&section);
        }

        scrolled.set_child(Some(&content));
        outer.append(&scrolled);
    });
}

/// Returns the kiosk widget and a refresh callback (to rebuild after hide/unhide).
pub fn build(settings: &Settings) -> (gtk::Widget, Rc<dyn Fn()>) {
    let launch_opts = settings.launch_opts();

    let outer = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .build();

    // Break the self-referential cycle: store the real refresh fn in a RefCell.
    let on_refresh_cell: Rc<RefCell<Rc<dyn Fn()>>> = Rc::new(RefCell::new(Rc::new(|| {})));
    let outer_weak = outer.downgrade();
    let launch_opts_clone = launch_opts.clone();
    let cell_clone = on_refresh_cell.clone();
    let real_refresh: Rc<dyn Fn()> = Rc::new(move || {
        if let Some(o) = outer_weak.upgrade() {
            let cb = cell_clone.borrow().clone();
            populate(&o, launch_opts_clone.clone(), cb);
        }
    });
    *on_refresh_cell.borrow_mut() = real_refresh.clone();

    populate(&outer, launch_opts, real_refresh.clone());

    (outer.upcast(), real_refresh)
}

// ── Download-all dialog ───────────────────────────────────────────────────────

/// Show a dialog that downloads every non-local game file and every missing
/// cover, with a live progress bar. Calls `on_done` after a successful run so
/// the kiosk grid refreshes to reflect newly-downloaded games.
pub fn show_download_all_dialog(parent: &impl IsA<gtk::Widget>, on_done: Rc<dyn Fn()>) {
    let dialog = adw::Dialog::new();
    dialog.set_title("Download All Games");
    dialog.set_content_width(440);

    let vbox = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .margin_top(24)
        .margin_bottom(24)
        .margin_start(24)
        .margin_end(24)
        .build();

    let status_lbl = gtk::Label::builder()
        .label("Preparing…")
        .xalign(0.0)
        .wrap(true)
        .build();

    let progress = gtk::ProgressBar::builder()
        .show_text(false)
        .build();

    let action_btn = gtk::Button::builder()
        .label("Cancel")
        .halign(gtk::Align::End)
        .build();

    vbox.append(&status_lbl);
    vbox.append(&progress);
    vbox.append(&action_btn);

    let scroll = gtk::ScrolledWindow::builder()
        .child(&vbox)
        .propagate_natural_height(true)
        .build();
    dialog.set_child(Some(&scroll));
    dialog.present(Some(parent));

    let cancelled = Rc::new(std::cell::Cell::new(false));

    let cancelled_close = cancelled.clone();
    dialog.connect_closed(move |_| {
        cancelled_close.set(true);
    });

    let cancelled_btn = cancelled.clone();
    let dialog_btn = dialog.clone();
    action_btn.connect_clicked(move |_| {
        if cancelled_btn.get() {
            // Already done/cancelled — act as Close
            dialog_btn.close();
        } else {
            cancelled_btn.set(true);
            dialog_btn.close();
        }
    });

    let status_weak = status_lbl.downgrade();
    let progress_weak = progress.downgrade();
    let btn_weak = action_btn.downgrade();

    glib::spawn_future_local(async move {
        // Load rows + cover index
        let (rows, cidx) = run_blocking(|| {
            let all_rows = games::load_rows();
            let hidden = games::load_hidden();
            let visible: Vec<games::Row> = all_rows
                .into_iter()
                .filter(|r| !hidden.contains(&r.display))
                .collect();
            let cidx = cover::load_index();
            (visible, cidx)
        })
        .await;

        let non_local: Vec<games::Row> =
            rows.iter().filter(|r| !r.is_local()).cloned().collect();
        let total_games = non_local.len();
        let total_covers = rows.len();
        let total = (total_games + total_covers).max(1);
        let mut done = 0usize;

        // Phase 1: download game files
        for (i, row) in non_local.iter().enumerate() {
            if cancelled.get() {
                break;
            }
            if let Some(lbl) = status_weak.upgrade() {
                lbl.set_label(&format!(
                    "Game {}/{}: {}",
                    i + 1,
                    total_games,
                    row.title
                ));
            }
            let r = row.clone();
            run_blocking(move || games::resolve_no_rebuild(&r)).await;
            done += 1;
            if let Some(bar) = progress_weak.upgrade() {
                bar.set_fraction(done as f64 / total as f64);
            }
            // Polite delay between IA requests
            glib::timeout_future(std::time::Duration::from_millis(500)).await;
        }

        // Rebuild index once now that all game files are in place
        if !cancelled.get() && total_games > 0 {
            if let Some(lbl) = status_weak.upgrade() {
                lbl.set_label("Rebuilding index…");
            }
            if let Err(e) = run_blocking(breadbin_core::build_index::build).await {
                eprintln!("index rebuild failed: {e}");
            }
        }

        // Phase 2: fetch covers for all visible games
        for (i, row) in rows.iter().enumerate() {
            if cancelled.get() {
                break;
            }
            if let Some(lbl) = status_weak.upgrade() {
                lbl.set_label(&format!(
                    "Cover {}/{}: {}",
                    i + 1,
                    total_covers,
                    row.title
                ));
            }
            let r = row.clone();
            let c = cidx.clone();
            run_blocking(move || games::cover_for(&r, &c)).await;
            done += 1;
            if let Some(bar) = progress_weak.upgrade() {
                bar.set_fraction(done as f64 / total as f64);
            }
        }

        let finished = !cancelled.get();

        if let Some(lbl) = status_weak.upgrade() {
            lbl.set_label(if finished { "All done!" } else { "Cancelled." });
        }
        if let Some(btn) = btn_weak.upgrade() {
            btn.set_label("Close");
        }
        if finished {
            if let Some(bar) = progress_weak.upgrade() {
                bar.set_fraction(1.0);
            }
            on_done();
        }
    });
}

/// Show a dialog listing all hidden games with an Unhide button for each.
pub fn show_hidden_dialog(parent: &impl IsA<gtk::Widget>, on_refresh: Rc<dyn Fn()>) {
    let hidden: Vec<String> = {
        let mut v: Vec<String> = games::load_hidden().into_iter().collect();
        v.sort();
        v
    };

    let dialog = adw::Dialog::new();
    dialog.set_title("Hidden Games");
    dialog.set_content_width(380);

    let vbox = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(8)
        .margin_top(20)
        .margin_bottom(20)
        .margin_start(20)
        .margin_end(20)
        .build();

    if hidden.is_empty() {
        let lbl = gtk::Label::builder()
            .label("No hidden games.")
            .xalign(0.0)
            .build();
        lbl.add_css_class("caption");
        vbox.append(&lbl);
    } else {
        for display in &hidden {
            let row_box = gtk::Box::builder()
                .orientation(gtk::Orientation::Horizontal)
                .spacing(8)
                .build();

            let lbl = gtk::Label::builder()
                .label(display)
                .hexpand(true)
                .xalign(0.0)
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .build();

            let unhide_btn = gtk::Button::builder().label("Unhide").build();
            let d = display.clone();
            let dialog2 = dialog.clone();
            let on_refresh2 = on_refresh.clone();
            unhide_btn.connect_clicked(move |_| {
                games::set_hidden(&d, false);
                dialog2.close();
                on_refresh2();
            });

            row_box.append(&lbl);
            row_box.append(&unhide_btn);
            vbox.append(&row_box);
        }
    }

    let scroll = gtk::ScrolledWindow::builder()
        .child(&vbox)
        .propagate_natural_height(true)
        .max_content_height(480)
        .build();

    dialog.set_child(Some(&scroll));
    dialog.present(Some(parent));
}

/// Create a controller badge widget for the header bar.
/// Returns a box with the controller icon and name, or None if no controllers are connected.
pub fn create_device_badges() -> Option<gtk::Box> {
    let controllers = run::connected_controllers();
    
    if controllers.is_empty() {
        return None;
    }
    
    // Create a horizontal box to hold all controller badges
    let badges_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(6)
        .build();
    badges_box.add_css_class("device-badges");
    
    for controller in controllers {
        let badge = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(4)
            .build();
        badge.add_css_class("device-badge");
        
        let icon = gtk::Label::new(Some(&get_controller_icon(&controller)));
        icon.add_css_class("device-icon");
        
        let name = gtk::Label::new(Some(&controller));
        name.add_css_class("device-name");
        
        badge.append(&icon);
        badge.append(&name);
        badges_box.append(&badge);
    }
    
    Some(badges_box)
}

/// Get the appropriate icon for a controller type.
fn get_controller_icon(controller: &str) -> String {
    match controller {
        "TheC64" => "·✦· 🕹 ·✦·",
        "PS5" => "🎮",
        "Switch Pro" => "🎮",
        _ => "🕹",
    }
    .to_string()
}
