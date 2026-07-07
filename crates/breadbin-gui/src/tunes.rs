// tunes - the Tunes tab: SID music browser grouped by party with live playback.

use std::cell::RefCell;
use std::rc::Rc;
use std::f64::consts::PI;
use std::time::Duration;

use rand::{Rng, SeedableRng};
use adw::prelude::*;

use gtk::cairo;

use breadbin_core::{audio::Audio, core::palette, tunes};

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
    // Use C64 SCREEN color as background (dark blue)
    let bg_color = palette::SCREEN;
    cr.set_source_rgb(
        bg_color[0] as f64 / 255.0,
        bg_color[1] as f64 / 255.0,
        bg_color[2] as f64 / 255.0,
    );
    let _ = cr.paint();

    match visualizer_type {
        0 => scope_draw(cr, w, h, samples), // Scope
        1 => plasma_draw(cr, w, h, samples, time), // C64 Plasma
        2 => starfield_draw(cr, w, h, samples, time), // Starfield
        3 => raster_bars_draw(cr, w, h, samples, time), // Raster Bars
        4 => tunnel_draw(cr, w, h, samples, time), // 3D Tunnel
        5 => demo_scroller_draw(cr, w, h, samples, tune_description, time), // Demo Scroller
        _ => scope_draw(cr, w, h, samples), // Default to scope
    }
}

fn scope_draw(cr: &cairo::Context, w: i32, h: i32, samples: &[i16]) {
    if samples.is_empty() {
        return;
    }

    // Use C64 yellow
    let c64_yellow = palette::YELLOW;
    cr.set_source_rgb(
        c64_yellow[0] as f64 / 255.0,
        c64_yellow[1] as f64 / 255.0,
        c64_yellow[2] as f64 / 255.0,
    );
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


/// Demo scroller visualizer - oldschool C64 demo style text scroller
fn demo_scroller_draw(cr: &cairo::Context, w: i32, h: i32, samples: &[i16], text: &str, time: f64) {
    // Use C64 SCREEN color as background (dark blue)
    let bg_color = palette::SCREEN;
    cr.set_source_rgb(
        bg_color[0] as f64 / 255.0,
        bg_color[1] as f64 / 255.0,
        bg_color[2] as f64 / 255.0,
    );
    cr.paint().ok();
    
    // Audio energy for particle effects
    let audio_energy: f64 = samples.iter().map(|&s| s.abs() as f64 / 32768.0).sum::<f64>() / samples.len() as f64;
    
    // Audio reactive stars/particles - more of them
    let particle_count = 100 + (audio_energy * 50.0) as usize;
    let mut rng = rand::thread_rng();
    
    // C64 colors for particles
    let particle_colors = [palette::YELLOW, palette::WHITE, palette::CYAN, palette::LIGHTBLUE];
    
    for i in 0..particle_count {
        let x = rng.gen_range(0.0..w as f64);
        let y = rng.gen_range(0.0..h as f64);
        let size = 0.5 + samples.get(i % samples.len()).map(|&s| s.abs() as f64 / 32768.0 * 4.0).unwrap_or(0.0);
        let brightness = 0.2 + samples.get(i % samples.len()).map(|&s| s.abs() as f64 / 32768.0 * 0.8).unwrap_or(0.0);
        
        // Use C64 colors
        let color_idx = i % particle_colors.len();
        let color = particle_colors[color_idx];
        cr.set_source_rgba(
            color[0] as f64 / 255.0,
            color[1] as f64 / 255.0,
            color[2] as f64 / 255.0,
            brightness * brightness,
        );
        cr.arc(x, y, size, 0.0, PI * 2.0);
        cr.fill().ok();
    }
    
    // Scrolling text setup - use C64 yellow
    let c64_yellow = palette::YELLOW;
    cr.set_source_rgb(
        c64_yellow[0] as f64 / 255.0,
        c64_yellow[1] as f64 / 255.0,
        c64_yellow[2] as f64 / 255.0,
    );
    cr.select_font_face("monospace", cairo::FontSlant::Normal, cairo::FontWeight::Bold);
    cr.set_font_size(16.0);
    
    // Calculate text width approximately
    let text_width = text.len() as f64 * 10.0;
    let line_height = 20.0;
    
    // Multiple scrolling text lines that cover the entire screen height
    let scroll_speed = 30.0 + audio_energy * 10.0;
    let scroll_x = (time * scroll_speed).rem_euclid(w as f64 + text_width);
    
    // Create a sine wave pattern for vertical positioning
    let wave_amplitude = h as f64 / 3.0;
    
    // Number of text rows to display
    let num_rows = (h as f64 / line_height).ceil() as i32 + 2;
    
    // Draw text scrolling horizontally across the entire screen at multiple vertical positions
    for row in 0..=num_rows {
        // Each row has a different vertical position with wave effect
        let base_y = (row as f64 * line_height) - scroll_x * 0.1 * (row as f64 * 0.3);
        let wave_offset = (time * 0.5 + row as f64 * 0.7).sin() * wave_amplitude * 0.3;
        let text_y = base_y + wave_offset;
        
        // Only draw if visible
        if text_y > -50.0 && text_y < h as f64 + 50.0 {
            let start_x = -scroll_x;
            cr.move_to(start_x, text_y);
            cr.show_text(text).ok();
            
            // Draw again to the right for seamless looping
            cr.move_to(start_x + text_width + 40.0, text_y);
            cr.show_text(text).ok();
        }
    }
    
    // Additional effect: diagonal scrolling text in opposite direction
    let diag_scroll_speed = 20.0 + audio_energy * 8.0;
    let diag_scroll = (time * diag_scroll_speed).rem_euclid(w as f64 + h as f64);
    
    let c64_yellow = palette::YELLOW;
    cr.set_source_rgba(
        c64_yellow[0] as f64 / 255.0,
        c64_yellow[1] as f64 / 255.0,
        c64_yellow[2] as f64 / 255.0,
        0.7,
    );
    cr.set_font_size(12.0);
    
    for i in 0..=(num_rows + 1) {
        let diag_start_x = -diag_scroll + (i as f64 * line_height * 0.5);
        let diag_y = (i as f64 * line_height * 0.7) + diag_scroll * 0.3;
        
        if diag_y > -30.0 && diag_y < h as f64 + 30.0 {
            cr.move_to(diag_start_x, diag_y);
            cr.show_text(text).ok();
            cr.move_to(diag_start_x + text_width + 40.0, diag_y);
            cr.show_text(text).ok();
        }
    }
}

/// C64 Plasma effect - classic demoscene plasma with sine-based color cycling
fn plasma_draw(cr: &cairo::Context, w: i32, h: i32, samples: &[i16], time: f64) {
    
    // Audio energy affects plasma parameters
    let audio_energy: f64 = samples.iter().map(|&s| s.abs() as f64 / 32768.0).sum::<f64>() / samples.len() as f64;
    
    // C64 palette for plasma (using the authentic colors)
    let plasma_colors = [
        palette::PURPLE,  // Deep purple
        palette::RED,     // Red
        palette::ORANGE,  // Orange
        palette::YELLOW,  // Yellow
        palette::LIGHTGREEN, // Light green
        palette::GREEN,   // Green
        palette::CYAN,    // Cyan
        palette::LIGHTBLUE, // Light blue
    ];
    
    // Create a lower resolution grid for that classic C64 plasma look
    let grid_size = 12;
    let cols = (w as f64 / grid_size as f64).ceil() as usize;
    let rows = (h as f64 / grid_size as f64).ceil() as usize;
    
    // Time-based offsets with audio reactivity
    let offset_x = time * 0.3 + audio_energy * 0.2;
    let offset_y = time * 0.2 + audio_energy * 0.15;
    let offset_z = time * 0.15;
    
    // Draw plasma cells
    for row in 0..rows {
        for col in 0..cols {
            let x = col as f64 * grid_size as f64;
            let y = row as f64 * grid_size as f64;
            
            // Classic plasma formula: combine multiple sine waves
            let p1 = (x * 0.08 + offset_x).sin() * 2.0;
            let p2 = (y * 0.06 + offset_y).sin() * 2.0;
            let p3 = ((x + y) * 0.05 + offset_z).sin() * 2.0;
            let p4 = (x * 0.12 - y * 0.08 + time * 0.4).sin() * 1.5;
            
            // Combine all plasma values
            let plasma_value = (p1 + p2 + p3 + p4 + audio_energy * 3.0) * 0.4;
            
            // Map to color palette (clamped and modulated)
            let color_index = (((plasma_value + 4.0) * 0.5 * plasma_colors.len() as f64).floor() as usize 
                % plasma_colors.len()).min(plasma_colors.len() - 1);
            
            let color = plasma_colors[color_index];
            cr.set_source_rgb(
                color[0] as f64 / 255.0,
                color[1] as f64 / 255.0,
                color[2] as f64 / 255.0,
            );
            
            // Draw a cell
            cr.rectangle(x, y, grid_size as f64, grid_size as f64);
            cr.fill().ok();
        }
    }
    
    // Add some scanline effect for that authentic C64 look
    cr.set_source_rgba(0.0, 0.0, 0.0, 0.1 + audio_energy * 0.2);
    for y in (0..h).step_by(2) {
        let alpha = if y % 4 == 0 { 0.15 + audio_energy * 0.1 } else { 0.05 };
        cr.set_source_rgba(0.0, 0.0, 0.0, alpha);
        cr.move_to(0.0, y as f64);
        cr.line_to(w as f64, y as f64);
        cr.stroke().ok();
    }
}

/// C64 Starfield - parallax starfield with depth layers
fn starfield_draw(cr: &cairo::Context, w: i32, h: i32, samples: &[i16], time: f64) {

    
    // Audio energy affects starfield parameters
    let audio_energy: f64 = samples.iter().map(|&s| s.abs() as f64 / 32768.0).sum::<f64>() / samples.len() as f64;
    
    // Create a seeded random number generator for consistent stars
    // We'll use a simple hash of time to create deterministic stars
    
    // Star layers (depth: 0 = farthest, 2 = closest)
    let num_layers = 3;
    let stars_per_layer = [40usize, 80, 120]; // More stars in background
    let star_sizes = [1.0, 2.0, 3.5]; // Bigger stars closer
    let star_speeds = [0.3, 0.6, 1.2]; // Faster stars closer
    let star_brightness = [0.3, 0.6, 1.0]; // Brighter stars closer
    
    // C64 colors for stars
    let star_colors = [
        palette::WHITE,      // Bright white stars
        palette::YELLOW,     // Yellow stars
        palette::CYAN,       // Cyan stars
        palette::LIGHTBLUE,  // Light blue stars
    ];
    
    // Draw each layer
    for layer in 0..num_layers {
        let stars = stars_per_layer[layer];
        let size = star_sizes[layer];
        let speed = star_speeds[layer] * (1.0 + audio_energy * 0.5);
        let brightness = star_brightness[layer];
        
        // Create stars at consistent positions for this layer
        for i in 0..stars {
            // Use deterministic positioning based on layer and index
            let seed = layer as u64 * 1000 + i as u64;
            let mut local_rng = rand::rngs::StdRng::seed_from_u64(seed);
            
            let x_pos = local_rng.gen_range(0.0..w as f64);
            let y_pos = local_rng.gen_range(0.0..h as f64);
            
            // Calculate star movement (wrapping around)
            let star_time = time * speed + (i as f64 * 0.1);
            let star_x = (x_pos + star_time * 100.0).rem_euclid(w as f64);
            let star_y = y_pos;
            
            // Star twinkling based on audio
            let twinkle = if i % 3 == 0 {
                (star_time * 2.0).sin().abs() * 0.3 + 0.7
            } else {
                1.0
            };
            
            // Choose star color
            let color_idx = i % star_colors.len();
            let color = star_colors[color_idx];
            
            let final_brightness = brightness * twinkle * (0.7 + audio_energy * 0.3);
            
            cr.set_source_rgba(
                color[0] as f64 / 255.0 * final_brightness,
                color[1] as f64 / 255.0 * final_brightness,
                color[2] as f64 / 255.0 * final_brightness,
                1.0,
            );
            
            // Draw star with glow effect
            let glow_size = size * 2.0;
            let gradient = cairo::RadialGradient::new(
                star_x, star_y, 0.0,
                star_x, star_y, glow_size
            );
            gradient.add_color_stop_rgba(0.0, 
                color[0] as f64 / 255.0 * final_brightness, 
                color[1] as f64 / 255.0 * final_brightness, 
                color[2] as f64 / 255.0 * final_brightness, 
                final_brightness);
            gradient.add_color_stop_rgba(1.0, 0.0, 0.0, 0.0, 0.0);
            
            cr.set_source(gradient).ok();
            cr.arc(star_x, star_y, glow_size, 0.0, PI * 2.0);
            cr.fill().ok();
            
            // Draw core star
            cr.set_source_rgba(
                color[0] as f64 / 255.0 * final_brightness,
                color[1] as f64 / 255.0 * final_brightness,
                color[2] as f64 / 255.0 * final_brightness,
                1.0,
            );
            cr.arc(star_x, star_y, size, 0.0, PI * 2.0);
            cr.fill().ok();
        }
    }
    
    // Add subtle grid lines for depth perception (classic C64 grid effect)
    cr.set_source_rgba(0.1, 0.1, 0.3, 0.15 * (1.0 - audio_energy * 0.5));
    cr.set_line_width(0.5);
    
    let grid_spacing = 40.0;
    for x in (0..w).step_by(grid_spacing as usize) {
        let xf = x as f64;
        let movement = (time * 0.1 + xf * 0.001).sin() * 2.0;
        cr.move_to(xf, 0.0 + movement);
        cr.line_to(xf, h as f64 + movement);
        cr.stroke().ok();
    }
    
    for y in (0..h).step_by(grid_spacing as usize) {
        let yf = y as f64;
        let movement = (time * 0.15 + yf * 0.001).cos() * 2.0;
        cr.move_to(0.0, yf + movement);
        cr.line_to(w as f64, yf + movement);
        cr.stroke().ok();
    }
}

/// C64 Raster Bars - authentic horizontal raster bars that move with audio
fn raster_bars_draw(cr: &cairo::Context, w: i32, h: i32, samples: &[i16], time: f64) {
    
    // Audio energy drives the raster bar movement
    let audio_energy: f64 = samples.iter().map(|&s| s.abs() as f64 / 32768.0).sum::<f64>() / samples.len() as f64;
    
    // Use C64 raster bar colors from the palette
    let bar_colors = palette::BARS;
    
    // Draw moving raster bars
    let bar_height = 8.0;
    let bar_spacing = bar_height * 1.5;
    let total_height = (bar_height + bar_spacing) * bar_colors.len() as f64;
    
    // Starting position with audio reactivity
    let start_y = (time * 20.0).sin() * 10.0 + audio_energy * 20.0;
    
    for (i, &color) in bar_colors.iter().enumerate() {
        let y = start_y + i as f64 * (bar_height + bar_spacing);
        let y_mod = y.rem_euclid(h as f64 + total_height) - total_height;
        
        // Apply audio-driven distortion
        let distortion = (time * 0.5 + i as f64 * 0.3 + audio_energy * 2.0).sin() * 5.0;
        let final_y = y_mod + distortion;
        
        // Color brightness based on audio
        let brightness = 0.8 + audio_energy * 0.4;
        
        cr.set_source_rgba(
            color[0] as f64 / 255.0 * brightness,
            color[1] as f64 / 255.0 * brightness,
            color[2] as f64 / 255.0 * brightness,
            0.9 + audio_energy * 0.2,
        );
        
        cr.rectangle(0.0, final_y, w as f64, bar_height);
        cr.fill().ok();
        
        // Add edge glow
        cr.set_source_rgba(
            color[0] as f64 / 255.0 * brightness * 1.2,
            color[1] as f64 / 255.0 * brightness * 1.2,
            color[2] as f64 / 255.0 * brightness * 1.2,
            0.4,
        );
        cr.rectangle(0.0, final_y, w as f64, 2.0);
        cr.fill().ok();
    }
    
    // Add vertical raster bars on the sides
    let side_bar_width = 4.0;
    for (i, &color) in bar_colors.iter().enumerate() {
        let x = (time * 15.0 + i as f64 * 12.0).rem_euclid(w as f64 + 50.0);
        let brightness = 0.6 + audio_energy * 0.4;
        
        cr.set_source_rgba(
            color[0] as f64 / 255.0 * brightness,
            color[1] as f64 / 255.0 * brightness,
            color[2] as f64 / 255.0 * brightness,
            0.7,
        );
        
        // Left side
        cr.rectangle(x, 0.0, side_bar_width, h as f64);
        cr.fill().ok();
        
        // Right side
        cr.rectangle(x + 10.0, 0.0, side_bar_width, h as f64);
        cr.fill().ok();
    }
}

/// C64 3D Tunnel - rotating tunnel effect
fn tunnel_draw(cr: &cairo::Context, w: i32, h: i32, samples: &[i16], time: f64) {
    
    // Audio energy affects tunnel parameters
    let audio_energy: f64 = samples.iter().map(|&s| s.abs() as f64 / 32768.0).sum::<f64>() / samples.len() as f64;
    
    let center_x = w as f64 / 2.0;
    let center_y = h as f64 / 2.0;
    
    // Tunnel parameters with audio reactivity
    let tunnel_radius = 150.0 + audio_energy * 50.0;
    let _tunnel_depth = 300.0 + audio_energy * 100.0;
    let rotation = time * 0.3 + audio_energy * 0.1;
    
    // C64 colors for tunnel
    let tunnel_colors = [
        palette::PURPLE,
        palette::RED,
        palette::ORANGE,
        palette::YELLOW,
        palette::LIGHTGREEN,
        palette::GREEN,
        palette::CYAN,
        palette::LIGHTBLUE,
    ];
    
    // Draw tunnel rings
    let num_rings = 20;
    for ring in 0..num_rings {
        let ring_progress = ring as f64 / num_rings as f64;
        let ring_radius = tunnel_radius * ring_progress * ring_progress;
        let ring_width = 12.0 + audio_energy * 8.0;
        let ring_y_offset = -50.0 + ring as f64 * 5.0 * (1.0 + audio_energy);
        
        // Calculate ring color based on position
        let color_idx = ((ring as f64 * 2.0 + time * 0.5).sin() * 0.5 + 0.5) * (tunnel_colors.len() - 1) as f64;
        let color_idx = (color_idx as usize) % tunnel_colors.len();
        let color = tunnel_colors[color_idx];
        
        // Color brightness based on depth and audio
        let brightness = 1.0 - ring_progress * 0.7 + audio_energy * 0.3;
        
        // Draw the ring as a series of segments
        let num_segments = 32 + (audio_energy * 16.0) as usize;
        for seg in 0..num_segments {
            let seg_angle = seg as f64 * PI * 2.0 / num_segments as f64 + rotation;
            let next_seg_angle = (seg + 1) as f64 * PI * 2.0 / num_segments as f64 + rotation;
            
            let x1 = center_x + ring_radius * seg_angle.cos();
            let y1 = center_y + ring_radius * seg_angle.sin() + ring_y_offset;
            let x2 = center_x + ring_radius * next_seg_angle.cos();
            let y2 = center_y + ring_radius * next_seg_angle.sin() + ring_y_offset;
            
            // Perspective scaling
            let scale = 1.0 - ring_progress * 0.8;
            let line_width = ring_width * scale;
            
            // Draw segment with gradient
            cr.set_source_rgba(
                color[0] as f64 / 255.0 * brightness,
                color[1] as f64 / 255.0 * brightness,
                color[2] as f64 / 255.0 * brightness,
                0.9,
            );
            
            cr.set_line_width(line_width);
            cr.move_to(x1, y1);
            cr.line_to(x2, y2);
            cr.stroke().ok();
        }
    }
    
    // Add center point with audio pulse
    let center_pulse = (time * 4.0).sin().abs() * 0.5 + 0.5 + audio_energy * 0.5;
    let center_size = 4.0 + center_pulse * 8.0;
    
    cr.set_source_rgb(
        palette::WHITE[0] as f64 / 255.0,
        palette::WHITE[1] as f64 / 255.0,
        palette::WHITE[2] as f64 / 255.0,
    );
    
    let gradient = cairo::RadialGradient::new(
        center_x, center_y, 0.0,
        center_x, center_y, center_size * 2.0
    );
    gradient.add_color_stop_rgba(0.0, 1.0, 1.0, 1.0, center_pulse);
    gradient.add_color_stop_rgba(1.0, 1.0, 1.0, 1.0, 0.0);
    
    cr.set_source(gradient).ok();
    cr.arc(center_x, center_y, center_size * 2.0, 0.0, PI * 2.0);
    cr.fill().ok();
    
    // Draw center point
    cr.set_source_rgb(
        palette::WHITE[0] as f64 / 255.0,
        palette::WHITE[1] as f64 / 255.0,
        palette::WHITE[2] as f64 / 255.0,
    );
    cr.arc(center_x, center_y, center_size, 0.0, PI * 2.0);
    cr.fill().ok();
}

// Radio state and all tunes storage
struct RadioData {
    all_tunes: Vec<tunes::Tune>,
}

pub fn build() -> gtk::Widget {
    let audio_state: AudioState = Rc::new(RefCell::new(None));
    let radio_state: RadioState = Rc::new(RefCell::new(false));
    let visualizer_state = Rc::new(RefCell::new(0)); // 0: scope, 1: plasma, 2: starfield, 3: raster bars, 4: tunnel, 5: demo scroller
    
    // Store all tunes for radio functionality - will be populated later
    let radio_data: Rc<RefCell<RadioData>> = Rc::new(RefCell::new(RadioData {
        all_tunes: Vec::new(),
    }));

    let hpaned = gtk::Paned::builder()
        .orientation(gtk::Orientation::Horizontal)
        .wide_handle(true)
        .vexpand(true)
        .hexpand(true)
        .build();
    hpaned.add_css_class("tunes-paned");

    // ── Left pane: tune list ──────────────────────────────────────────────────
    let left = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .vexpand(true)
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
        .vexpand(true)
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
        .vexpand(true)
        .hexpand(true)
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
    let visualizer_combo = gtk::DropDown::from_strings(&["Scope", "C64 Plasma", "Starfield", "Raster Bars", "3D Tunnel", "Demo Scroller"]);
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
    let hpaned_for_radio = hpaned.clone();
    radio_btn.connect_toggled(move |btn| {
        let enabled = btn.is_active();
        *radio_state_for_toggle.borrow_mut() = enabled;
        update_radio_style();
        
        // Collapse/expand song list based on radio state
        if enabled {
            // Collapse left pane (song list) when radio is on
            hpaned_for_radio.set_position(0);
            btn.set_active(true);
            let radio_data_ref = radio_data_for_toggle.borrow();
            if !radio_data_ref.all_tunes.is_empty() {
                // Trigger skip to play first random tune
                skip_btn_for_radio.activate();
            }
        } else {
            // Restore left pane when radio is off
            hpaned_for_radio.set_position(600);
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

        let scrolled = gtk::ScrolledWindow::builder().vexpand(true).hexpand(true).build();
        let listbox = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .vexpand(true)
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
