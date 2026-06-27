// demos - the Demos tab: screenshot grid of demoscene releases grouped by party.

use std::path::PathBuf;

use adw::prelude::*;

use breadbin_core::{demos, run};

use crate::config::Settings;
use crate::task::run_blocking;

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
        let (all_demos, groups) = run_blocking(|| {
            let all = demos::load_demos();
            let groups = demos::group_by_party(&all);
            (all, groups)
        })
        .await;

        let Some(outer) = outer_weak.upgrade() else {
            return;
        };

        if let Some(child) = outer.first_child() {
            outer.remove(&child);
        }

        if all_demos.is_empty() {
            let status = adw::StatusPage::builder()
                .icon_name("video-display-symbolic")
                .title("No demos found")
                .description("The demos index hasn't been built yet.")
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

        for (party_name, idxs) in &groups {
            let header = gtk::Label::builder()
                .label(&party_name.to_uppercase())
                .xalign(0.0)
                .build();
            header.add_css_class("section-header");
            content.append(&header);

            let flow = gtk::FlowBox::builder()
                .column_spacing(8)
                .row_spacing(8)
                .homogeneous(true)
                .selection_mode(gtk::SelectionMode::None)
                .build();

            for &idx in idxs.iter().take(20) {
                let demo = &all_demos[idx];

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
                    .build();
                card.add_css_class("cover-card");
                card.append(&picture);
                card.append(&title_lbl);

                // Load screenshot async
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

                // Click for detail
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

                let fb_child = gtk::FlowBoxChild::new();
                fb_child.set_child(Some(&card));
                flow.append(&fb_child);
            }

            content.append(&flow);
        }

        scrolled.set_child(Some(&content));
        outer.append(&scrolled);
    });

    outer.upcast()
}
