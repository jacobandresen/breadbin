// tunes - the Tunes tab: SID music browser grouped by party with live playback.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use adw::prelude::*;

use breadbin_core::{audio::Audio, tunes};

use crate::task::run_blocking;

type AudioState = Rc<RefCell<Option<Audio>>>;

fn scope_draw(
    _area: &gtk::DrawingArea,
    cr: &gtk::cairo::Context,
    w: i32,
    h: i32,
    samples: &[i16],
) {
    // Background
    cr.set_source_rgb(0.25, 0.19, 0.55);
    let _ = cr.paint();

    if samples.is_empty() {
        return;
    }

    cr.set_source_rgb(0.93, 0.94, 0.44); // C64 yellow
    cr.set_line_width(1.5);

    let n = samples.len();
    for (i, &s) in samples.iter().enumerate() {
        let x = i as f64 * w as f64 / n as f64;
        let y = (h as f64 / 2.0) - (s as f64 / 32768.0) * (h as f64 / 2.0) * 0.9;
        if i == 0 {
            cr.move_to(x, y);
        } else {
            cr.line_to(x, y);
        }
    }
    let _ = cr.stroke();
}

pub fn build() -> gtk::Widget {
    let audio_state: AudioState = Rc::new(RefCell::new(None));

    let hpaned = gtk::Paned::builder()
        .orientation(gtk::Orientation::Horizontal)
        .wide_handle(true)
        .build();
    hpaned.add_css_class("tunes-paned");

    // ── Left pane: tune list ──────────────────────────────────────────────────
    let left = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .build();
    left.add_css_class("tunes-list-pane");

    let spinner = gtk::Spinner::new();
    spinner.start();
    spinner.set_margin_top(40);
    spinner.set_halign(gtk::Align::Center);
    left.append(&spinner);

    // ── Right pane: now-playing bar ───────────────────────────────────────────
    let right = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(8)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .build();
    right.add_css_class("tunes-player-pane");

    let now_title = gtk::Label::builder()
        .label("—")
        .xalign(0.5)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .build();
    now_title.add_css_class("title-3");

    let now_composer = gtk::Label::builder()
        .label("")
        .xalign(0.5)
        .build();
    now_composer.add_css_class("caption");

    let scope = gtk::DrawingArea::builder()
        .content_width(280)
        .content_height(100)
        .vexpand(true)
        .build();
    scope.add_css_class("scope");

    // Scope samples shared between timer and draw function
    let scope_samples: Rc<RefCell<Vec<i16>>> = Rc::new(RefCell::new(vec![]));
    let scope_samples_draw = scope_samples.clone();

    scope.set_draw_func(move |area, cr, w, h| {
        let samples = scope_samples_draw.borrow().clone();
        scope_draw(area, cr, w, h, &samples);
    });

    let btn_box = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .halign(gtk::Align::Center)
        .build();

    let play_pause_btn = gtk::Button::builder().label("Pause").build();
    play_pause_btn.add_css_class("pill");
    let stop_btn = gtk::Button::builder().label("Stop").build();
    stop_btn.add_css_class("pill");

    btn_box.append(&play_pause_btn);
    btn_box.append(&stop_btn);

    right.append(&now_title);
    right.append(&now_composer);
    right.append(&scope);
    right.append(&btn_box);

    // Scope timer
    let audio_for_timer = audio_state.clone();
    let scope_for_timer = scope_samples.clone();
    let scope_widget = scope.clone();
    glib::timeout_add_local(Duration::from_millis(16), move || {
        if let Some(ref audio) = *audio_for_timer.borrow() {
            *scope_for_timer.borrow_mut() = audio.scope();
            scope_widget.queue_draw();
        }
        glib::ControlFlow::Continue
    });

    // Play/pause button
    let audio_for_pp = audio_state.clone();
    play_pause_btn.connect_clicked(move |btn| {
        if let Some(ref audio) = *audio_for_pp.borrow() {
            audio.toggle_pause();
            btn.set_label(if audio.is_paused() { "Play" } else { "Pause" });
        }
    });

    // Stop button
    let audio_for_stop = audio_state.clone();
    let pp_btn_for_stop = play_pause_btn.clone();
    let title_for_stop = now_title.clone();
    let composer_for_stop = now_composer.clone();
    stop_btn.connect_clicked(move |_| {
        *audio_for_stop.borrow_mut() = None;
        pp_btn_for_stop.set_label("Play");
        title_for_stop.set_label("—");
        composer_for_stop.set_label("");
    });

    // ── Async load tunes ──────────────────────────────────────────────────────
    let left_weak = left.downgrade();
    let audio_for_load = audio_state.clone();
    let now_title_load = now_title.clone();
    let now_composer_load = now_composer.clone();
    let pp_btn_load = play_pause_btn.clone();

    glib::spawn_future_local(async move {
        let (all_tunes, groups) = run_blocking(|| {
            let tunes = tunes::load_tunes();
            let groups = tunes::group_by_party(&tunes);
            (tunes, groups)
        })
        .await;

        let Some(left) = left_weak.upgrade() else {
            return;
        };

        // Remove spinner
        if let Some(child) = left.first_child() {
            left.remove(&child);
        }

        if all_tunes.is_empty() {
            let lbl = gtk::Label::builder()
                .label("No tunes — run Refresh catalogue to build the SID index.")
                .wrap(true)
                .xalign(0.5)
                .margin_top(60)
                .build();
            lbl.add_css_class("dim-label");
            left.append(&lbl);
            return;
        }

        let scrolled = gtk::ScrolledWindow::builder().vexpand(true).build();
        let listbox = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .build();
        listbox.add_css_class("tunes-list");

        let all_tunes = Rc::new(all_tunes);

        for (party_name, idxs) in &groups {
            // Party header row
            let header_row = gtk::ListBoxRow::new();
            header_row.set_activatable(false);
            header_row.add_css_class("tunes-party-header");
            let header_label = gtk::Label::builder()
                .label(&party_name.to_uppercase())
                .xalign(0.0)
                .build();
            header_row.set_child(Some(&header_label));
            listbox.append(&header_row);

            for &idx in idxs {
                let tune = &all_tunes[idx];

                let name_lbl = gtk::Label::builder()
                    .label(&tune.name)
                    .xalign(0.0)
                    .ellipsize(gtk::pango::EllipsizeMode::End)
                    .hexpand(true)
                    .build();

                let composer_lbl_row = gtk::Label::builder()
                    .label(&tune.composer)
                    .xalign(0.0)
                    .ellipsize(gtk::pango::EllipsizeMode::End)
                    .build();
                composer_lbl_row.add_css_class("dim-label");
                composer_lbl_row.add_css_class("caption");

                let meta = gtk::Box::builder()
                    .orientation(gtk::Orientation::Vertical)
                    .spacing(2)
                    .hexpand(true)
                    .valign(gtk::Align::Center)
                    .build();
                meta.append(&name_lbl);
                meta.append(&composer_lbl_row);

                let rating_lbl = gtk::Label::builder()
                    .label(&format!("{:.1}★", tune.rating))
                    .valign(gtk::Align::Center)
                    .build();
                rating_lbl.add_css_class("caption");

                let hbox = gtk::Box::builder()
                    .orientation(gtk::Orientation::Horizontal)
                    .spacing(8)
                    .margin_top(6)
                    .margin_bottom(6)
                    .margin_start(12)
                    .margin_end(12)
                    .build();
                hbox.append(&meta);
                hbox.append(&rating_lbl);

                let lb_row = gtk::ListBoxRow::builder().activatable(true).build();
                lb_row.set_child(Some(&hbox));

                let tune_for_click = all_tunes[idx].clone();
                let audio_for_click = audio_for_load.clone();
                let title_lbl = now_title_load.clone();
                let composer_lbl = now_composer_load.clone();
                let pp_btn = pp_btn_load.clone();

                lb_row.connect_activate(move |_| {
                    let t = tune_for_click.clone();
                    let audio_rc = audio_for_click.clone();
                    let title_l = title_lbl.clone();
                    let composer_l = composer_lbl.clone();
                    let pp = pp_btn.clone();
                    let name = t.name.clone();
                    let composer = t.composer.clone();

                    glib::spawn_future_local(async move {
                        let result = run_blocking(move || tunes::ensure_sid(&t)).await;
                        match result {
                            Ok(bytes) => match Audio::start(bytes, 0) {
                                Ok(new_audio) => {
                                    *audio_rc.borrow_mut() = Some(new_audio);
                                    title_l.set_label(&name);
                                    composer_l.set_label(&composer);
                                    pp.set_label("Pause");
                                }
                                Err(e) => eprintln!("audio error: {e}"),
                            },
                            Err(e) => eprintln!("SID fetch error: {e}"),
                        }
                    });
                });

                listbox.append(&lb_row);
            }
        }

        scrolled.set_child(Some(&listbox));
        left.append(&scrolled);
    });

    hpaned.set_start_child(Some(&left));
    hpaned.set_end_child(Some(&right));
    hpaned.set_position(600);

    hpaned.upcast()
}
