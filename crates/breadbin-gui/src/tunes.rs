// tunes - the Tunes tab: SID music browser grouped by party with live playback.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use adw::prelude::*;
use gtk::prelude::*;
use gtk::cairo;

use breadbin_core::{audio::Audio, tunes};

use crate::task::run_blocking;

type AudioState = Rc<RefCell<Option<Audio>>>;
type RadioState = Rc<RefCell<bool>>;

/// Draw different visualizers based on the selected type
fn visualizer_draw(
    _area: &gtk::DrawingArea,
    cr: &cairo::Context,
    w: i32,
    h: i32,
    samples: &[i16],
    visualizer_type: u8,
    tune_description: &str,
    time: f64, // Time in seconds for animations
) {
    // Background
    cr.set_source_rgb(0.25, 0.19, 0.55);
    let _ = cr.paint();

    match visualizer_type {
        0 => scope_draw(cr, w, h, samples), // Scope
        1 => fireball_draw(cr, w, h, samples, time), // Fireball
        2 => marching_cubes_draw(cr, w, h, samples, time), // Marching Cubes
        3 => demo_scroller_draw(cr, w, h, samples, tune_description, time), // Demo Scroller
        _ => scope_draw(cr, w, h, samples), // Default to scope
    }
}

fn scope_draw(cr: &cairo::Context, w: i32, h: i32, samples: &[i16]) {
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

/// Fireball visualizer - animated fireball that reacts to audio
fn fireball_draw(cr: &cairo::Context, w: i32, h: i32, samples: &[i16], time: f64) {
    use rand::Rng;
    let center_x = w as f64 / 2.0;
    let center_y = h as f64 / 2.0;
    
    // Fireball base color (orange-red)
    let base_r = 0.9;
    let base_g = 0.4;
    let base_b = 0.2;
    
    // Audio reactive radius
    let radius = 30.0 + samples.iter().map(|&s| s.abs() as f64 / 32768.0 * 20.0).sum::<f64>() / samples.len() as f64;
    
    // Pulsing effect based on time
    let pulse = (time * 2.0).sin() * 0.1 + 1.0;
    let final_radius = radius * pulse;
    
    // Draw fireball with gradient
    let pattern = cairo::RadialGradient::new(center_x, center_y, 0.0, center_x, center_y, final_radius);
    pattern.add_color_stop_rgba(0.0, base_r, base_g, base_b, 0.8);
    pattern.add_color_stop_rgba(0.7, base_r * 0.8, base_g * 0.6, base_b * 0.4, 0.6);
    pattern.add_color_stop_rgba(1.0, base_r * 0.4, base_g * 0.2, base_b * 0.1, 0.0);
    
    cr.set_source(pattern).ok();
    cr.arc(center_x, center_y, final_radius, 0.0, std::f64::consts::PI * 2.0);
    cr.fill().ok();
    
    // Inner glow
    cr.set_source_rgba(base_r, base_g, base_b, 0.9);
    cr.arc(center_x, center_y, final_radius * 0.6, 0.0, std::f64::consts::PI * 2.0);
    cr.fill().ok();
}

/// Marching cubes visualizer - 3D cubes that march across the screen
fn marching_cubes_draw(cr: &cairo::Context, w: i32, h: i32, samples: &[i16], time: f64) {
    let cube_size = 15.0;
    let spacing = 20.0;
    let speed = 30.0;
    
    cr.set_source_rgb(0.93, 0.94, 0.44); // C64 yellow
    
    let offset = (time * speed).rem_euclid(w as f64);
    
    // Audio reactive height
    let audio_height = samples.iter().map(|&s| s.abs() as f64 / 32768.0 * h as f64 * 0.4).sum::<f64>() / samples.len() as f64;
    
    // Draw marching cubes
    let mut x = 0.0;
    while x < w as f64 {
        let cube_x = (x - offset).rem_euclid(w as f64);
        if cube_x < 0.0 || cube_x >= w as f64 {
            x += spacing;
            continue;
        }
        
        let cube_height = cube_size + audio_height * 0.5;
        let cube_y = h as f64 - cube_height;
        
        // Draw cube (simplified as rectangle with 3D effect)
        cr.rectangle(cube_x, cube_y, cube_size, cube_height);
        cr.fill().ok();
        
        // 3D effect - top edge
        cr.set_source_rgba(1.0, 1.0, 1.0, 0.3);
        cr.rectangle(cube_x, cube_y, cube_size, 2.0);
        cr.fill().ok();
        
        // 3D effect - side edge  
        cr.set_source_rgba(0.6, 0.6, 0.0, 0.3);
        cr.rectangle(cube_x + cube_size - 2.0, cube_y, 2.0, cube_height);
        cr.fill().ok();
        
        cr.set_source_rgb(0.93, 0.94, 0.44); // Reset color
        x += spacing;
    }
}

/// Demo scroller visualizer - oldschool C64 demo style text scroller
fn demo_scroller_draw(cr: &cairo::Context, w: i32, h: i32, samples: &[i16], text: &str, time: f64) {
    // Background
    cr.set_source_rgb(0.1, 0.1, 0.2);
    cr.paint().ok();
    
    // Audio reactive stars/particles
    let particle_count = 50;
    let mut rng = rand::thread_rng();
    
    use rand::Rng;
    for i in 0..particle_count {
        let x = rng.gen_range(0.0..w as f64);
        let y = rng.gen_range(0.0..h as f64);
        let size = 1.0 + samples.get(i % samples.len()).map(|&s| s.abs() as f64 / 32768.0 * 3.0).unwrap_or(0.0);
        let brightness = 0.3 + samples.get(i % samples.len()).map(|&s| s.abs() as f64 / 32768.0 * 0.7).unwrap_or(0.0);
        
        cr.set_source_rgba(0.93, 0.94, 0.44, brightness);
        cr.arc(x, y, size, 0.0, std::f64::consts::PI * 2.0);
        cr.fill().ok();
    }
    
    // Scrolling text
    let scroll_speed = 40.0;
    let scroll_x = (time * scroll_speed).rem_euclid(w as f64 + text.len() as f64 * 10.0);
    
    cr.set_source_rgb(0.93, 0.94, 0.44); // C64 yellow
    cr.select_font_face("monospace", cairo::FontSlant::Normal, cairo::FontWeight::Bold);
    cr.set_font_size(14.0);
    
    let text_y = h as f64 / 2.0 + 10.0;
    
    // Draw text at multiple positions for wrapping effect
    // Calculate text width approximately (chars * font_size)
    let text_width = text.len() as f64 * 8.0; // Approximate width
    let start_x = -scroll_x;
    cr.move_to(start_x, text_y);
    cr.show_text(text).ok();
    
    // Draw text again to the right for seamless looping
    cr.move_to(start_x + text_width + 20.0, text_y);
    cr.show_text(text).ok();
    
    // Draw text again to the left for seamless looping
    cr.move_to(start_x - text_width - 20.0, text_y);
    cr.show_text(text).ok();
}

// Radio state and all tunes storage
struct RadioData {
    enabled: bool,
    all_tunes: Vec<tunes::Tune>,
}

pub fn build() -> gtk::Widget {
    let audio_state: AudioState = Rc::new(RefCell::new(None));
    let radio_state: RadioState = Rc::new(RefCell::new(false));
    let visualizer_state = Rc::new(RefCell::new(0)); // 0: scope, 1: fireball, 2: marching cubes, 3: demo scroller
    
    // Store all tunes for radio functionality - will be populated later
    let radio_data: Rc<RefCell<RadioData>> = Rc::new(RefCell::new(RadioData {
        enabled: false,
        all_tunes: Vec::new(),
    }));

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

    // Scope samples and visualizer state shared between timer and draw function
    let scope_samples: Rc<RefCell<Vec<i16>>> = Rc::new(RefCell::new(vec![]));
    let scope_samples_draw = scope_samples.clone();
    let visualizer_state_draw = visualizer_state.clone();
    let now_title_draw = now_title.clone();
    
    // Start time for animations
    let start_time = std::time::Instant::now();
    
    scope.set_draw_func(move |area, cr, w, h| {
        let samples = scope_samples_draw.borrow().clone();
        let visualizer_type = *visualizer_state_draw.borrow();
        let elapsed = start_time.elapsed().as_secs_f64();
        let tune_desc = now_title_draw.text().to_string();
        visualizer_draw(area, cr, w, h, &samples, visualizer_type, &tune_desc, elapsed);
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
    
    // Radio controls
    let skip_btn = gtk::Button::builder().label("⏭ Skip").build();
    skip_btn.add_css_class("pill");
    let radio_btn = gtk::ToggleButton::builder().label("📻 Radio").build();
    radio_btn.add_css_class("pill");
    radio_btn.add_css_class("radio-badge");
    
    // Visualizer selection dropdown
    let visualizer_combo = gtk::DropDown::from_strings(&["Scope", "Fireball", "Marching Cubes", "Demo Scroller"]);
    visualizer_combo.set_selected(0); // Default to Scope
    
    // Set initial radio state
    radio_btn.set_active(false);

    btn_box.append(&play_pause_btn);
    btn_box.append(&stop_btn);
    btn_box.append(&skip_btn);
    btn_box.append(&radio_btn);
    btn_box.append(&visualizer_combo);
    
    // Style radio badge based on state
    let radio_state_for_style = radio_state.clone();
    let radio_btn_for_style = radio_btn.clone();
    let update_radio_style = move || {
        let enabled = radio_state_for_style.borrow();
        if *enabled {
            radio_btn_for_style.add_css_class("radio-enabled");
            radio_btn_for_style.remove_css_class("radio-disabled");
        } else {
            radio_btn_for_style.add_css_class("radio-disabled");
            radio_btn_for_style.remove_css_class("radio-enabled");
        }
    };
    
    // Initial style update
    update_radio_style();
    
    // Connect visualizer combo to update visualizer state
    let visualizer_state_combo = visualizer_state.clone();
    let scope_for_visualizer = scope.clone();
    visualizer_combo.connect_selected_notify(move |combo| {
        let selected = combo.selected() as u8;
        *visualizer_state_combo.borrow_mut() = selected;
        scope_for_visualizer.queue_draw(); // Redraw with new visualizer
    });

    right.append(&now_title);
    right.append(&now_composer);
    right.append(&scope);
    right.append(&btn_box);

    // Radio functionality
    let radio_state_for_skip = radio_state.clone();
    let audio_for_skip = audio_state.clone();
    let radio_data_for_skip = radio_data.clone();
    let now_title_for_skip = now_title.clone();
    let now_composer_for_skip = now_composer.clone();
    let pp_btn_for_skip = play_pause_btn.clone();
    
    skip_btn.connect_clicked(move |_| {
        if !*radio_state_for_skip.borrow() {
            return;
        }
        
        let radio_data_ref = radio_data_for_skip.borrow();
        if radio_data_ref.all_tunes.is_empty() {
            return;
        }
        
        // Select a random tune
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let random_index = rng.gen_range(0..radio_data_ref.all_tunes.len());
        let random_tune = radio_data_ref.all_tunes[random_index].clone();
        
        let audio_for_skip_clone = audio_for_skip.clone();
        let now_title_for_skip_clone = now_title_for_skip.clone();
        let now_composer_for_skip_clone = now_composer_for_skip.clone();
        let pp_btn_for_skip_clone = pp_btn_for_skip.clone();
        let random_tune_clone = random_tune.clone();
        
        glib::spawn_future_local(async move {
            let tune_for_sid = random_tune_clone.clone();
            let result = run_blocking(move || tunes::ensure_sid(&tune_for_sid)).await;
            match result {
                Ok(bytes) => match Audio::start(bytes, 0) {
                    Ok(new_audio) => {
                        *audio_for_skip_clone.borrow_mut() = Some(new_audio);
                        now_title_for_skip_clone.set_label(&random_tune_clone.name);
                        now_composer_for_skip_clone.set_label(&random_tune_clone.composer);
                        pp_btn_for_skip_clone.set_label("Pause");
                    }
                    Err(e) => eprintln!("audio error: {e}"),
                },
                Err(e) => eprintln!("SID fetch error: {e}"),
            }
        });
    });

    // Radio toggle button
    let radio_state_for_toggle = radio_state.clone();
    let radio_data_for_toggle = radio_data.clone();
    let skip_btn_for_radio = skip_btn.clone();
    radio_btn.connect_toggled(move |btn| {
        let enabled = btn.is_active();
        *radio_state_for_toggle.borrow_mut() = enabled;
        update_radio_style();
        
        // If enabling radio, start playing a random tune
        if enabled {
            btn.set_active(true);
            let radio_data_ref = radio_data_for_toggle.borrow();
            if !radio_data_ref.all_tunes.is_empty() {
                // Trigger skip to play first random tune
                skip_btn_for_radio.activate();
            }
        }
    });

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
    let radio_data_load = radio_data.clone();

    glib::spawn_future_local(async move {
        let (all_tunes, groups) = run_blocking(|| {
            let tunes = tunes::load_tunes();
            let groups = tunes::group_by_party(&tunes);
            (tunes, groups)
        })
        .await;
        
        // Store all tunes in radio data for random selection
        radio_data_load.borrow_mut().all_tunes = all_tunes.clone();

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
