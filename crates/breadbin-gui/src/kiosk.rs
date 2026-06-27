// kiosk - the Games tab: one horizontal strip per genre, expandable to full grid.

use std::path::PathBuf;

use adw::prelude::*;

use breadbin_core::{cover, library, run, tui};

use crate::config::Settings;
use crate::task::run_blocking;

extern crate async_channel;

const STRIP_MAX: usize = 8;

// ── Cover texture ─────────────────────────────────────────────────────────────

fn texture_from_path(
    path: &std::path::Path,
    joystick: bool,
    top_rated: bool,
) -> Option<gdk::Texture> {
    let mut rgba = image::open(path).ok()?.to_rgba8();
    if top_rated {
        breadbin_core::cover::draw_rating_badge(&mut rgba);
    }
    if joystick {
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
            .spacing(3)
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

/// A full-width section header with a C64 colour-bar chip on the left.
pub fn section_header_widget(title: &str) -> gtk::Box {
    let [r, g, b] = breadbin_core::core::palette::bar_for(title);
    let color_css = format!("#{r:02X}{g:02X}{b:02X}");

    let chip = gtk::Box::builder()
        .width_request(8)
        .height_request(32)
        .build();
    chip.add_css_class("section-chip");
    let provider = gtk::CssProvider::new();
    provider.load_from_data(&format!("box {{ background-color: {color_css}; border-radius: 2px; }}"));
    chip.style_context()
        .add_provider(&provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);

    let label = gtk::Label::builder()
        .label(&title.to_uppercase())
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
    row: tui::Row,
    cover_path: Option<PathBuf>,
    joystick: bool,
    top_rated: bool,
    launch_opts: Option<run::LaunchOpts>,
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
        if let Some(texture) = texture_from_path(path, joystick, top_rated) {
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
            .label(&format!("Genre: {}", row.genre))
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

    if !row.is_local() {
        let dl_btn = gtk::Button::builder().label("Download").build();
        let row2 = row.clone();
        let dialog2 = dialog.clone();
        dl_btn.connect_clicked(move |_| {
            let r = row2.clone();
            glib::spawn_future_local(async move {
                let _ = run_blocking(move || tui::resolve(&r, true)).await;
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
                        let path = run_blocking(move || tui::resolve(&r, true)).await;
                        if let Some(p) = path {
                            tui::record_play(&r2);
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
    row: &tui::Row,
    cidx: std::collections::HashMap<String, String>,
    joystick: bool,
    top_rated: bool,
    launch_opts: Option<run::LaunchOpts>,
    hexpand: bool,
    cover_done: Option<async_channel::Sender<()>>,
) -> gtk::Box {
    let is_local = row.is_local();
    let card = Card::new(&row.title, is_local, hexpand);

    let cidx2 = cidx.clone();
    let row_clone = row.clone();
    let pic = card.picture.clone();
    glib::spawn_future_local(async move {
        let cover_path =
            run_blocking(move || tui::cover_for(&row_clone, &cidx2)).await;
        if let Some(path) = &cover_path {
            if let Some(texture) = texture_from_path(path, joystick, top_rated) {
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
    gesture.connect_released(move |g, _, _, _| {
        let Some(widget) = g.widget() else { return };
        let Some(root) = widget.root() else { return };
        let Some(win) = root.downcast_ref::<gtk::Window>() else { return };

        let row_snap = row_for_click.clone();
        let cidx_c = cidx3.clone();
        let win_c = win.clone();
        let launch = launch_for_click.clone();
        glib::spawn_future_local(async move {
            let row_snap2 = row_snap.clone();
            let cidx_c2 = cidx_c.clone();
            let cover_path =
                run_blocking(move || tui::cover_for(&row_snap2, &cidx_c2)).await;
            show_detail(&win_c, row_snap, cover_path, joystick, top_rated, launch);
        });
    });
    card.root.add_controller(gesture);

    card.root
}

// ── Kiosk genre section ───────────────────────────────────────────────────────

fn build_genre_section(
    section_name: &str,
    row_indices: &[usize],
    rows: &[tui::Row],
    cidx: &std::collections::HashMap<String, String>,
    joystick_canons: &std::collections::HashSet<String>,
    top_rated_canons: &std::collections::HashSet<String>,
    launch_opts: &Option<run::LaunchOpts>,
) -> gtk::Box {
    let section_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .build();

    // Header row with optional expand button
    let header_row = section_header_widget(section_name);
    let has_overflow = row_indices.len() > STRIP_MAX;
    let toggle_btn = gtk::Button::builder()
        .label(&format!("Show All ({})", row_indices.len()))
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
        let canon = tui::canon_of(row);
        let joystick = joystick_canons.contains(&canon);
        let top_rated = top_rated_canons.contains(&canon);
        // Cards expand to fill the strip when fewer than STRIP_MAX items
        let hexpand = row_indices.len() <= STRIP_MAX;
        let card_widget = build_game_card(row, cidx.clone(), joystick, top_rated, launch_opts.clone(), hexpand, None);
        strip_box.append(&card_widget);
    }

    strip_scroll.set_child(Some(&strip_box));
    stack.add_named(&strip_scroll, Some("strip"));

    // ── Grid page: loading header + FlowBox ──────────────────────────────────
    let grid_vbox = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(8)
        .build();

    let loading_header = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(4)
        .margin_top(8)
        .margin_bottom(4)
        .halign(gtk::Align::Center)
        .build();

    let loading_label = gtk::Label::builder()
        .label(&format!("Loading covers — 0 / {}", row_indices.len()))
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
        .selection_mode(gtk::SelectionMode::None)
        .build();

    grid_vbox.append(&loading_header);
    grid_vbox.append(&flow);
    stack.add_named(&grid_vbox, Some("grid"));
    stack.set_visible_child_name("strip");
    section_box.append(&stack);

    // Snapshot what the grid needs; owned data so the closure is 'static.
    let grid_rows: Vec<(tui::Row, bool, bool)> = row_indices
        .iter()
        .map(|&idx| {
            let row = rows[idx].clone();
            let canon = tui::canon_of(&row);
            let joystick = joystick_canons.contains(&canon);
            let top_rated = top_rated_canons.contains(&canon);
            (row, joystick, top_rated)
        })
        .collect();
    let cidx_for_grid = cidx.clone();
    let launch_for_grid = launch_opts.clone();

    // Wire up the toggle button — populate grid on first expand
    let stack_weak = stack.downgrade();
    let flow_weak = flow.downgrade();
    let loading_header_weak = loading_header.downgrade();
    let loading_label_weak = loading_label.downgrade();
    let loading_bar_weak = loading_bar.downgrade();
    let total = row_indices.len();
    let grid_populated = std::rc::Rc::new(std::cell::Cell::new(false));
    toggle_btn.connect_clicked(move |btn| {
        let Some(s) = stack_weak.upgrade() else { return };
        if s.visible_child_name().as_deref() == Some("strip") {
            if !grid_populated.get() {
                grid_populated.set(true);
                if let Some(flow) = flow_weak.upgrade() {
                    let (tx, rx) = async_channel::bounded::<()>(total.max(1));

                    if let Some(hdr) = loading_header_weak.upgrade() {
                        hdr.set_visible(true);
                    }
                    if let Some(lbl) = loading_label_weak.upgrade() {
                        lbl.set_label(&format!("Loading covers — 0 / {total}"));
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
                            let frac = done as f64 / total as f64;
                            if let Some(b) = bar_weak2.upgrade() {
                                b.set_fraction(frac);
                            }
                            if let Some(l) = lbl_weak2.upgrade() {
                                l.set_label(&format!("Loading covers — {done} / {total}"));
                            }
                            if done >= total {
                                if let Some(h) = hdr_weak2.upgrade() {
                                    h.set_visible(false);
                                }
                                break;
                            }
                        }
                    });

                    for (row, joystick, top_rated) in &grid_rows {
                        let card = build_game_card(
                            row,
                            cidx_for_grid.clone(),
                            *joystick,
                            *top_rated,
                            launch_for_grid.clone(),
                            false,
                            Some(tx.clone()),
                        );
                        let fb_child = gtk::FlowBoxChild::new();
                        fb_child.set_child(Some(&card));
                        flow.append(&fb_child);
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

pub fn build(settings: &Settings) -> gtk::Widget {
    let launch_opts = settings.launch_opts();

    let outer = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .build();

    let spinner = gtk::Spinner::new();
    spinner.start();
    spinner.set_margin_top(60);
    spinner.set_halign(gtk::Align::Center);
    outer.append(&spinner);

    let outer_weak = outer.downgrade();
    glib::spawn_future_local(async move {
        let (rows, cidx, sections, joystick_canons, top_rated_canons) =
            run_blocking(|| {
                let rows = tui::load_rows();
                let cidx = cover::load_index();
                let sections = library::sections(&rows);
                let joystick_canons = tui::joystick_canons();
                let top_rated_canons = tui::top_rated_canons();
                (rows, cidx, sections, joystick_canons, top_rated_canons)
            })
            .await;

        let Some(outer) = outer_weak.upgrade() else {
            return;
        };

        if let Some(child) = outer.first_child() {
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
                &joystick_canons,
                &top_rated_canons,
                &launch_opts,
            );
            content.append(&section);
        }

        scrolled.set_child(Some(&content));
        outer.append(&scrolled);
    });

    outer.upcast()
}
