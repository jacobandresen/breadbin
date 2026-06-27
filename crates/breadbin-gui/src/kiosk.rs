// kiosk - the Games tab: scrollable grid of cover sections.

use std::path::PathBuf;

use adw::prelude::*;

use breadbin_core::{cover, library, run, tui};

use crate::config::Settings;
use crate::task::run_blocking;

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
    fn new(title: &str, is_local: bool) -> Self {
        let picture = gtk::Picture::builder()
            .width_request(160)
            .height_request(200)
            .content_fit(gtk::ContentFit::Cover)
            .build();
        picture.add_css_class("cover-picture");

        let label = gtk::Label::builder()
            .label(title)
            .max_width_chars(18)
            .wrap(true)
            .lines(2)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .xalign(0.5)
            .build();
        label.add_css_class("cover-title");

        let root = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(4)
            .width_request(168)
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

// ── Section row ───────────────────────────────────────────────────────────────

fn section_header(title: &str) -> gtk::Label {
    let label = gtk::Label::builder()
        .label(&title.to_uppercase())
        .xalign(0.0)
        .build();
    label.add_css_class("section-header");
    label
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

    // Cover image
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

    // Info labels
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

    // Buttons row
    let btn_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .halign(gtk::Align::End)
        .build();

    // Download button (only when not local)
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

    // Play button
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
        // Load everything on a worker thread.
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

        // Remove spinner
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
            .spacing(16)
            .margin_top(16)
            .margin_bottom(16)
            .margin_start(16)
            .margin_end(16)
            .build();

        for (section_name, row_indices) in &sections {
            let header = section_header(section_name);
            content.append(&header);

            let flow = gtk::FlowBox::builder()
                .column_spacing(8)
                .row_spacing(8)
                .homogeneous(true)
                .selection_mode(gtk::SelectionMode::None)
                .build();

            for &idx in row_indices.iter().take(30) {
                let row = &rows[idx];
                let is_local = row.is_local();
                let canon = tui::canon_of(row);
                let joystick = joystick_canons.contains(&canon);
                let top_rated = top_rated_canons.contains(&canon);

                let card = Card::new(&row.title, is_local);

                // Load cover asynchronously
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
                });

                // Click handler opens detail dialog
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
                    let joystick_c = joystick;
                    let top_rated_c = top_rated;
                    let launch = launch_for_click.clone();
                    glib::spawn_future_local(async move {
                        let row_snap2 = row_snap.clone();
                        let cidx_c2 = cidx_c.clone();
                        let cover_path =
                            run_blocking(move || tui::cover_for(&row_snap2, &cidx_c2)).await;
                        show_detail(
                            &win_c,
                            row_snap,
                            cover_path,
                            joystick_c,
                            top_rated_c,
                            launch,
                        );
                    });
                });
                card.root.add_controller(gesture);

                let fb_child = gtk::FlowBoxChild::new();
                fb_child.set_child(Some(&card.root));
                flow.append(&fb_child);
            }

            content.append(&flow);
        }

        scrolled.set_child(Some(&content));
        outer.append(&scrolled);
    });

    outer.upcast()
}
