// demos - the Demos tab: one horizontal strip per party, expandable to full grid.

use std::path::PathBuf;

use adw::prelude::*;

use breadbin_core::{demos, run};

use crate::config::Settings;
use crate::task::run_blocking;

extern crate async_channel;

const STRIP_MAX: usize = 6;

fn texture_from_path(path: &std::path::Path) -> Option<gdk::Texture> {
    let rgba = image::open(path).ok()?.to_rgba8();
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

fn show_detail(
    parent: &gtk::Window,
    demo: demos::Demo,
    shot_path: Option<PathBuf>,
    launch_opts: Option<run::LaunchOpts>,
) {
    let dialog = adw::Dialog::new();
    dialog.set_title(&demo.name);
    dialog.set_content_width(400);

    let vbox = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(12)
        .margin_top(24)
        .margin_bottom(24)
        .margin_start(24)
        .margin_end(24)
        .build();

    if let Some(path) = &shot_path {
        if let Some(texture) = texture_from_path(path) {
            let pic = gtk::Picture::builder()
                .paintable(&texture)
                .content_fit(gtk::ContentFit::Contain)
                .height_request(240)
                .build();
            vbox.append(&pic);
        }
    }

    let title_lbl = gtk::Label::builder()
        .label(&demo.name)
        .wrap(true)
        .xalign(0.0)
        .build();
    title_lbl.add_css_class("title-2");
    vbox.append(&title_lbl);

    let info = format!(
        "{} · {} · {}",
        demo.group,
        if demo.year > 0 { demo.year.to_string() } else { "—".to_string() },
        demo.party
    );
    let info_lbl = gtk::Label::builder()
        .label(&info)
        .xalign(0.0)
        .wrap(true)
        .build();
    info_lbl.add_css_class("caption");
    vbox.append(&info_lbl);

    let btn_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .halign(gtk::Align::End)
        .build();

    let launch_btn = gtk::Button::builder().label("Launch").build();
    launch_btn.add_css_class("suggested-action");

    if let Some(opts) = launch_opts {
        let demo2 = demo.clone();
        let dialog2 = dialog.clone();
        launch_btn.connect_clicked(move |_| {
            let d = demo2.clone();
            let o = breadbin_core::run::LaunchOpts {
                warp: opts.warp,
                fullscreen: opts.fullscreen,
                keyboard: opts.keyboard,
                drive_sound: opts.drive_sound,
                forever: opts.forever.clone(),
            };
            glib::spawn_future_local(async move {
                let result = run_blocking(move || demos::fetch_and_prepare(&d)).await;
                match result {
                    Ok(path) => {
                        let _ = run::spawn(&path, &o);
                    }
                    Err(e) => eprintln!("demo fetch error: {e}"),
                }
            });
            dialog2.close();
        });
    } else {
        launch_btn.set_sensitive(false);
        launch_btn.set_tooltip_text(Some("C64 Forever not detected — configure in Preferences"));
    }

    btn_box.append(&launch_btn);
    vbox.append(&btn_box);

    let scroll = gtk::ScrolledWindow::builder()
        .child(&vbox)
        .propagate_natural_height(true)
        .build();

    dialog.set_child(Some(&scroll));
    dialog.present(Some(parent));
}

// ── Bootstrap loader UI ───────────────────────────────────────────────────────

fn make_loader(label_text: &str) -> (gtk::Box, gtk::ProgressBar, gtk::Label) {
    let vbox = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(16)
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .vexpand(true)
        .build();

    let title = gtk::Label::builder()
        .label(label_text)
        .build();
    title.add_css_class("c64-boot-label");

    let bar = gtk::ProgressBar::builder()
        .width_request(360)
        .show_text(true)
        .build();

    let status = gtk::Label::builder()
        .label("SEARCHING...")
        .build();
    status.add_css_class("caption");

    vbox.append(&title);
    vbox.append(&bar);
    vbox.append(&status);
    (vbox, bar, status)
}

// ── Demo card builder ─────────────────────────────────────────────────────────

fn build_demo_card(
    demo: &demos::Demo,
    launch_opts: Option<run::LaunchOpts>,
    hexpand: bool,
) -> gtk::Box {
    let picture = gtk::Picture::builder()
        .width_request(160)
        .height_request(120)
        .content_fit(gtk::ContentFit::Cover)
        .build();
    picture.add_css_class("cover-picture");

    let title_lbl = gtk::Label::builder()
        .label(&demo.name)
        .max_width_chars(18)
        .wrap(true)
        .lines(2)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .xalign(0.5)
        .build();
    title_lbl.add_css_class("cover-title");

    let card = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(4)
        .width_request(168)
        .hexpand(hexpand)
        .build();
    card.add_css_class("cover-card");
    card.append(&picture);
    card.append(&title_lbl);

    let demo_for_shot = demo.clone();
    let pic_weak = picture.downgrade();
    glib::spawn_future_local(async move {
        let shot_path =
            run_blocking(move || demos::ensure_shot(&demo_for_shot)).await;
        let Some(pic) = pic_weak.upgrade() else { return };
        if let Some(path) = shot_path {
            if let Some(texture) = texture_from_path(&path) {
                pic.set_paintable(Some(&texture));
            }
        }
    });

    let gesture = gtk::GestureClick::new();
    let demo_for_click = demo.clone();
    let launch_for_click = launch_opts.clone();
    gesture.connect_released(move |g, _, _, _| {
        let Some(widget) = g.widget() else { return };
        let Some(root) = widget.root() else { return };
        let Some(win) = root.downcast_ref::<gtk::Window>() else { return };
        let d = demo_for_click.clone();
        let win_c = win.clone();
        let launch = launch_for_click.clone();
        glib::spawn_future_local(async move {
            let shot = run_blocking({
                let d2 = d.clone();
                move || demos::ensure_shot(&d2)
            })
            .await;
            show_detail(&win_c, d, shot, launch);
        });
    });
    card.add_controller(gesture);

    card
}

// ── Party section (kiosk row + expand grid) ───────────────────────────────────

fn build_party_section(
    party_name: &str,
    idxs: &[usize],
    all_demos: &[demos::Demo],
    launch_opts: &Option<run::LaunchOpts>,
) -> gtk::Box {
    let section_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .build();

    let header_row = crate::kiosk::section_header_widget(party_name);
    let has_overflow = idxs.len() > STRIP_MAX;
    let toggle_btn = gtk::Button::builder()
        .label(&format!("Show All ({})", idxs.len()))
        .visible(has_overflow)
        .build();
    toggle_btn.add_css_class("pill");
    header_row.append(&toggle_btn);
    section_box.append(&header_row);

    let stack = gtk::Stack::builder()
        .transition_type(gtk::StackTransitionType::SlideUpDown)
        .transition_duration(180)
        .build();

    // ── Strip page ────────────────────────────────────────────────────────────
    let strip_scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Automatic)
        .vscrollbar_policy(gtk::PolicyType::Never)
        .build();
    let strip_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .hexpand(true)
        .build();

    for &idx in idxs.iter().take(STRIP_MAX) {
        let hexpand = idxs.len() <= STRIP_MAX;
        let card = build_demo_card(&all_demos[idx], launch_opts.clone(), hexpand);
        strip_box.append(&card);
    }

    strip_scroll.set_child(Some(&strip_box));
    stack.add_named(&strip_scroll, Some("strip"));

    // ── Grid page (empty — populated lazily on first expand) ─────────────────
    let flow = gtk::FlowBox::builder()
        .column_spacing(8)
        .row_spacing(8)
        .homogeneous(true)
        .selection_mode(gtk::SelectionMode::None)
        .build();

    stack.add_named(&flow, Some("grid"));
    stack.set_visible_child_name("strip");
    section_box.append(&stack);

    let grid_demos: Vec<demos::Demo> = idxs.iter().map(|&i| all_demos[i].clone()).collect();
    let launch_for_grid = launch_opts.clone();

    let stack_weak = stack.downgrade();
    let flow_weak = flow.downgrade();
    let total = idxs.len();
    let grid_populated = std::rc::Rc::new(std::cell::Cell::new(false));
    toggle_btn.connect_clicked(move |btn| {
        let Some(s) = stack_weak.upgrade() else { return };
        if s.visible_child_name().as_deref() == Some("strip") {
            if !grid_populated.get() {
                grid_populated.set(true);
                if let Some(flow) = flow_weak.upgrade() {
                    for demo in &grid_demos {
                        let card = build_demo_card(demo, launch_for_grid.clone(), false);
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
        let all_demos = run_blocking(demos::load_demos).await;

        let Some(outer) = outer_weak.upgrade() else { return };

        if let Some(child) = outer.first_child() {
            outer.remove(&child);
        }

        let all_demos = if all_demos.is_empty() {
            let (loader_box, bar, status_lbl) =
                make_loader("LOADING DEMOSCENE INDEX...");
            outer.append(&loader_box);

            let (tx, rx) = async_channel::bounded::<(u64, u64)>(64);

            let bar_weak = bar.downgrade();
            let status_weak = status_lbl.downgrade();
            glib::spawn_future_local(async move {
                while let Ok((done, total)) = rx.recv().await {
                    if let Some(b) = bar_weak.upgrade() {
                        let frac = if total > 0 { done as f64 / total as f64 } else { 0.0 };
                        b.set_fraction(frac);
                        b.set_text(Some(&format!("{done}/{total}")));
                    }
                    if let Some(s) = status_weak.upgrade() {
                        s.set_label(&format!("LOADING {done} OF {total}..."));
                    }
                }
            });

            let result = run_blocking(move || {
                demos::build_index(500, &mut |done, total| {
                    let _ = tx.send_blocking((done, total));
                })
            })
            .await;

            let Some(outer) = outer_weak.upgrade() else { return };
            if let Some(child) = outer.first_child() {
                outer.remove(&child);
            }

            if let Err(e) = result {
                let lbl = gtk::Label::builder()
                    .label(&format!("Failed to build demos index: {e}"))
                    .wrap(true)
                    .margin_top(60)
                    .build();
                outer.append(&lbl);
                return;
            }

            run_blocking(demos::load_demos).await
        } else {
            all_demos
        };

        let Some(outer) = outer_weak.upgrade() else { return };

        if all_demos.is_empty() {
            let lbl = gtk::Label::builder()
                .label("No demos found — check your network connection.")
                .wrap(true)
                .margin_top(60)
                .build();
            lbl.add_css_class("dim-label");
            outer.append(&lbl);
            return;
        }

        let groups = demos::group_by_party(&all_demos);

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

        for (party_name, idxs) in &groups {
            let section = build_party_section(party_name, idxs, &all_demos, &launch_opts);
            content.append(&section);
        }

        scrolled.set_child(Some(&content));
        outer.append(&scrolled);
    });

    outer.upcast()
}
