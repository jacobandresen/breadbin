// viz - the Tunes tab's visualisers: three classic demo effects rendered into a
// real 320x200 framebuffer in the Pepto palette, scaled up with nearest-neighbour
// inside a C64 border that flashes on the beat. Each is a homage to a top demo.

use std::f64::consts::PI;

use gtk::cairo;

use breadbin_core::sid::Vis;

/// Dropdown labels, in effect order.
pub const NAMES: [&str; 3] = [
    "Twister · for Oxyron's Comaland",
    "Filled Vectors · for Booze Design's Edge of Disgrace",
    "Char Plasma · for Crest & Oxyron's Deus Ex Machina",
];

const W: usize = 320;
const H: usize = 200;

/// The Pepto VIC-II palette, by C64 colour index.
pub(crate) const PEPTO: [u32; 16] = [
    0x000000, 0xFFFFFF, 0x883932, 0x67B6BD, 0x8B3F96, 0x55A049, 0x40318D, 0xBFCE72,
    0x8B5429, 0x574200, 0xB86962, 0x505050, 0x787878, 0x94E089, 0x7869C4, 0x9F9F9F,
];

// Luma ramps (dark → light) built from palette indices, as C64 coders did.
const RAMP_BLUE: &[u8] = &[0, 6, 11, 4, 14, 3, 13, 1];
const RAMP_RED: &[u8] = &[0, 9, 2, 8, 10, 15, 7, 1];
const RAMP_GREEN: &[u8] = &[0, 11, 5, 12, 13, 1];
const RAMP_GREY: &[u8] = &[0, 11, 12, 15, 1];
const RAMP_FIRE: &[u8] = &[0, 9, 2, 8, 10, 7, 1];

/// Pick from a ramp by brightness 0..1.
fn ramp(r: &[u8], v: f64) -> u32 {
    let i = (v.clamp(0.0, 0.999) * r.len() as f64) as usize;
    PEPTO[r[i] as usize]
}

const GREETINGS: &str = "GREETINGS TO OXYRON * BOOZE DESIGN * CREST * PERFORMERS * \
CENSOR DESIGN * FAIRLIGHT * BONZAI * TRIAD * ARSENIC * GENESIS PROJECT * ";

/// Visualiser state: audio analysis carried between frames plus the framebuffer.
pub struct Viz {
    kind: usize,
    playing: bool,
    level: f64,
    avg: f64,
    beat: f64,
    last_beat: f64,
    voices: [f64; 3],
    gates: [bool; 3],
    last_t: f64,
    fb: cairo::ImageSurface,
}

impl Viz {
    pub fn new() -> Viz {
        Viz {
            kind: 0,
            playing: false,
            level: 0.0,
            avg: 0.0,
            beat: 0.0,
            last_beat: 0.0,
            voices: [0.0; 3],
            gates: [false; 3],
            last_t: 0.0,
            fb: cairo::ImageSurface::create(cairo::Format::Rgb24, W as i32, H as i32)
                .expect("framebuffer"),
        }
    }

    pub fn set_kind(&mut self, kind: usize) {
        self.kind = kind.min(NAMES.len() - 1);
    }

    /// Feed one frame of audio: the scope samples and the SID register snapshot.
    /// `None` means nothing is playing.
    pub fn update(&mut self, audio: Option<(&[i16], &Vis)>, t: f64) {
        let dt = (t - self.last_t).clamp(0.0, 0.1);
        self.last_t = t;
        self.playing = audio.is_some();
        let Some((samples, vis)) = audio else {
            self.level *= 0.9;
            return;
        };

        let rms = if samples.is_empty() {
            0.0
        } else {
            (samples.iter().map(|&s| (s as f64).powi(2)).sum::<f64>() / samples.len() as f64)
                .sqrt()
        };
        self.level = (rms / 9000.0).min(1.0);
        self.avg = self.avg * 0.95 + self.level * 0.05;
        if self.level > self.avg * 1.3 && self.level > 0.06 && t - self.last_beat > 0.22 {
            self.beat = 1.0;
            self.last_beat = t;
        }
        self.beat *= (-dt * 6.0).exp();

        // Per-voice envelopes: a new note (gate rising) kicks the voice to full.
        for v in 0..3 {
            let gate = vis.voice_gate(v);
            if gate && !self.gates[v] {
                self.voices[v] = 1.0;
            }
            self.gates[v] = gate;
            let floor = if gate { vis.voice_sustain(v) as f64 / 15.0 * 0.6 } else { 0.0 };
            self.voices[v] = (self.voices[v] * (-dt * 4.0).exp()).max(floor);
        }
    }

    /// Render the current effect and blit it into a `w`x`h` area.
    pub fn draw(&mut self, cr: &cairo::Context, w: i32, h: i32, title: &str, t: f64) {
        if !self.playing {
            self.ready_screen(t);
        } else {
            match self.kind {
                0 => self.twister(title, t),
                1 => self.vectors(t),
                _ => self.plasma(title, t),
            }
        }
        self.blit(cr, w as f64, h as f64);
    }

    fn blit(&self, cr: &cairo::Context, w: f64, h: f64) {
        // Border: light blue, flashing up the blue ramp on each beat.
        let border = if self.playing { ramp(RAMP_BLUE, 0.55 + self.beat * 0.45) } else { PEPTO[14] };
        set_rgb(cr, border);
        let _ = cr.paint();

        let margin = (w.min(h) * 0.07).max(8.0);
        let scale = ((w - 2.0 * margin) / W as f64).min((h - 2.0 * margin) / H as f64);
        if scale <= 0.0 {
            return;
        }
        let (ox, oy) = ((w - W as f64 * scale) / 2.0, (h - H as f64 * scale) / 2.0);
        let _ = cr.save();
        cr.translate(ox, oy);
        cr.scale(scale, scale);
        let _ = cr.set_source_surface(&self.fb, 0.0, 0.0);
        cr.source().set_filter(cairo::Filter::Nearest);
        let _ = cr.paint();
        let _ = cr.restore();
    }

    /// Direct pixel access to the framebuffer.
    fn with_pixels(&mut self, f: impl FnOnce(&mut [u32])) {
        self.fb.flush();
        {
            let mut data = self.fb.data().expect("framebuffer is not shared");
            let px: &mut [u32] = bytemuck_u32(&mut data);
            f(px);
        }
        self.fb.mark_dirty();
    }

    /// A cairo context on the framebuffer with antialiasing off, for crisp pixels.
    fn ctx(&self) -> cairo::Context {
        let cr = cairo::Context::new(&self.fb).expect("framebuffer context");
        cr.set_antialias(cairo::Antialias::None);
        cr
    }

    // ── Idle: the power-on screen ────────────────────────────────────────────

    fn ready_screen(&mut self, t: f64) {
        self.with_pixels(|px| px.fill(PEPTO[6]));
        let cr = self.ctx();
        set_rgb(&cr, PEPTO[14]);
        let lines = [
            "    **** BREADBIN SID PLAYER ****",
            "",
            " 64K RAM SYSTEM  38911 BASIC BYTES FREE",
            "",
            "READY.",
            "LOAD\"*\",8,1",
        ];
        // Sixtyfour's glyphs stand a little taller than a C64 cell, so rows are 10px.
        for (i, l) in lines.iter().enumerate() {
            text(&cr, 0.0, 20.0 + i as f64 * 10.0, l);
        }
        if (t * 1.6) as i64 % 2 == 0 {
            cr.rectangle(0.0, 20.0 + lines.len() as f64 * 10.0 - 8.0, 8.0, 9.0);
            let _ = cr.fill();
        }
    }

    // ── 1. Twister ───────────────────────────────────────────────────────────

    fn twister(&mut self, title: &str, t: f64) {
        let (level, beat, voices) = (self.level, self.beat, self.voices);
        let faces: [&[u8]; 4] = [RAMP_RED, RAMP_BLUE, RAMP_GREEN, RAMP_GREY];
        self.with_pixels(|px| {
            px.fill(0);
            // Copper bars behind the twister, one per SID voice.
            for (v, ramp_v) in [RAMP_RED, RAMP_GREEN, RAMP_BLUE].iter().enumerate() {
                let cy = 92.0 + (t * (0.9 + v as f64 * 0.37) + v as f64 * 2.1).sin() * 70.0;
                let half = 4.0 + voices[v] * 9.0;
                for y in (cy - half).max(0.0) as usize..((cy + half) as usize).min(H - 16) {
                    let d = 1.0 - ((y as f64 - cy).abs() / half);
                    let c = ramp(ramp_v, d * (0.4 + voices[v] * 0.6));
                    px[y * W..y * W + W].fill(c);
                }
            }
            // The twister: a square bar whose rotation varies per scanline.
            let r = 28.0 + beat * 10.0;
            for y in 0..H - 16 {
                let yf = y as f64;
                let a = t * 1.3 + (yf * 0.011 + t * 0.8).sin() * (1.2 + level * 2.5);
                let cx = 160.0 + (yf * 0.017 + t * 0.6).sin() * (20.0 + level * 50.0);
                for (i, face) in faces.iter().enumerate() {
                    let x0 = cx + r * (a + i as f64 * PI / 2.0).sin();
                    let x1 = cx + r * (a + (i + 1) as f64 * PI / 2.0).sin();
                    if x1 <= x0 {
                        continue; // back face
                    }
                    let c = ramp(face, 0.25 + (x1 - x0) / (2.0 * r) * 0.8);
                    let (a0, a1) = (x0.max(0.0) as usize, (x1 as usize).min(W));
                    px[y * W + a0..y * W + a1].fill(c);
                }
            }
            // Raster split for the scroller.
            px[(H - 17) * W..(H - 16) * W].fill(PEPTO[1]);
        });
        let text = format!("NOW PLAYING: {}  ***  {GREETINGS}", title.to_uppercase());
        self.sine_scroller(&text, t, H as f64 - 4.0);
    }

    /// A one-line scroller where every character rides a sine wave.
    fn sine_scroller(&self, msg: &str, t: f64, base_y: f64) {
        let cr = self.ctx();
        let chars: Vec<char> = msg.chars().collect();
        let span = chars.len() as f64 * 8.0;
        let offset = (t * 60.0) % span;
        for (i, ch) in chars.iter().enumerate() {
            let x = i as f64 * 8.0 - offset;
            for x in [x, x + span] {
                if !(-8.0..W as f64).contains(&x) {
                    continue;
                }
                let y = base_y + ((x * 0.04) + t * 4.0).sin() * 2.5;
                set_rgb(&cr, ramp(RAMP_FIRE, 0.45 + 0.5 * ((x * 0.02 + t * 2.0).sin() * 0.5 + 0.5)));
                text(&cr, x, y, &ch.to_string());
            }
        }
    }

    // ── 2. Filled vectors ────────────────────────────────────────────────────

    fn vectors(&mut self, t: f64) {
        let (level, beat) = (self.level, self.beat);
        self.with_pixels(|px| {
            // Raster sky...
            for y in 0..110 {
                let c = ramp(RAMP_BLUE, (y as f64 / 110.0) * 0.75);
                px[y * W..y * W + W].fill(c);
            }
            // ...over a perspective checkerboard floor scrolling towards us.
            for y in 110..H {
                let z = 2400.0 / (y as f64 - 104.0);
                let shade = (1.0 - (z / 300.0).min(1.0)) * 0.7;
                for x in 0..W {
                    let wx = (x as f64 - 160.0) * z / 160.0;
                    let check = ((wx / 24.0).floor() as i64 + ((z + t * 120.0) / 24.0).floor() as i64) & 1;
                    let r = if check == 0 { RAMP_GREY } else { RAMP_BLUE };
                    px[y * W + x] = ramp(r, shade * 0.9);
                }
            }
        });

        // An icosahedron, flat-shaded with palette ramps and painter-sorted.
        let g = (1.0 + 5f64.sqrt()) / 2.0;
        let verts: [[f64; 3]; 12] = [
            [-1.0, g, 0.0], [1.0, g, 0.0], [-1.0, -g, 0.0], [1.0, -g, 0.0],
            [0.0, -1.0, g], [0.0, 1.0, g], [0.0, -1.0, -g], [0.0, 1.0, -g],
            [g, 0.0, -1.0], [g, 0.0, 1.0], [-g, 0.0, -1.0], [-g, 0.0, 1.0],
        ];
        const FACES: [[usize; 3]; 20] = [
            [0, 11, 5], [0, 5, 1], [0, 1, 7], [0, 7, 10], [0, 10, 11],
            [1, 5, 9], [5, 11, 4], [11, 10, 2], [10, 7, 6], [7, 1, 8],
            [3, 9, 4], [3, 4, 2], [3, 2, 6], [3, 6, 8], [3, 8, 9],
            [4, 9, 5], [2, 4, 11], [6, 2, 10], [8, 6, 7], [9, 8, 1],
        ];
        let (ax, ay) = (t * 0.7 + beat * 0.4, t * 0.45);
        let rot = |p: [f64; 3]| {
            let (sx, cx, sy, cy) = (ax.sin(), ax.cos(), ay.sin(), ay.cos());
            let (y, z) = (p[1] * cx - p[2] * sx, p[1] * sx + p[2] * cx);
            [p[0] * cy + z * sy, y, -p[0] * sy + z * cy]
        };
        let size = 30.0 * (1.0 + beat * 0.25 + level * 0.15);
        let bob = (t * 1.7).sin() * 10.0;
        let world: Vec<[f64; 3]> = verts.iter().map(|&v| rot(v)).collect();
        let project = |p: [f64; 3]| {
            let k = 4.5 / (4.5 + p[2]);
            (160.0 + p[0] * size * k, 80.0 + bob + p[1] * size * k)
        };
        let light = { let l = [0.4f64, -0.6, -0.7]; let n = (l[0] * l[0] + l[1] * l[1] + l[2] * l[2]).sqrt(); [l[0] / n, l[1] / n, l[2] / n] };
        let ramps: [&[u8]; 3] = [RAMP_FIRE, RAMP_BLUE, RAMP_GREEN];
        let face_ramp = ramps[((t / 8.0) as usize) % 3];

        /// (depth, projected corners, brightness) for one visible face.
        type Face = (f64, [(f64, f64); 3], f64);
        let mut visible: Vec<Face> = FACES
            .iter()
            .filter_map(|f| {
                let [a, b, c] = [world[f[0]], world[f[1]], world[f[2]]];
                let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
                let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
                let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
                let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
                if n[2] / len > 0.0 {
                    return None; // facing away
                }
                let lit = (n[0] * light[0] + n[1] * light[1] + n[2] * light[2]) / len;
                let depth = a[2] + b[2] + c[2];
                Some((depth, [project(a), project(b), project(c)], lit.max(0.0)))
            })
            .collect();
        visible.sort_by(|a, b| b.0.total_cmp(&a.0));

        let cr = self.ctx();
        // Shadow on the floor.
        set_rgb(&cr, PEPTO[0]);
        cr.save().ok();
        cr.translate(160.0, 150.0);
        cr.scale(size * 1.6, size * 0.25);
        cr.arc(0.0, 0.0, 1.0, 0.0, 2.0 * PI);
        cr.restore().ok();
        let _ = cr.fill();
        for (_, pts, lit) in &visible {
            set_rgb(&cr, ramp(face_ramp, 0.15 + lit * 0.85 + beat * 0.15));
            cr.move_to(pts[0].0, pts[0].1);
            cr.line_to(pts[1].0, pts[1].1);
            cr.line_to(pts[2].0, pts[2].1);
            cr.close_path();
            let _ = cr.fill();
        }
        set_rgb(&cr, PEPTO[1]);
        text(&cr, 8.0, 12.0, "BREADBIN  FILLED VECTORS");
    }

    // ── 3. Char plasma ───────────────────────────────────────────────────────

    fn plasma(&mut self, title: &str, t: f64) {
        // 8x8 dither patterns standing in for PETSCII shading characters.
        const PATTERNS: [[u8; 8]; 5] = [
            [0x00; 8],
            [0x88, 0x00, 0x22, 0x00, 0x88, 0x00, 0x22, 0x00],
            [0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55],
            [0x77, 0xFF, 0xDD, 0xFF, 0x77, 0xFF, 0xDD, 0xFF],
            [0xFF; 8],
        ];
        // Luma-ordered and symmetric, so neighbouring values never jump in brightness.
        const CYCLES: [&[u8]; 2] = [
            &[0, 6, 4, 14, 3, 13, 1, 13, 3, 14, 4, 6],
            &[0, 9, 2, 8, 10, 7, 1, 7, 10, 8, 2, 9],
        ];
        let (level, beat, voices) = (self.level, self.beat, self.voices);
        let cycle = CYCLES[(t / 12.0) as usize % CYCLES.len()];
        self.with_pixels(|px| {
            let (mx, my) = (20.0 + (t * 0.6).sin() * 12.0, 12.5 + (t * 0.8).cos() * 8.0);
            for cy in 0..25 {
                for cx in 0..40 {
                    let (x, y) = (cx as f64, cy as f64);
                    let d = ((x - mx).powi(2) + (y - my).powi(2)).sqrt();
                    let v = (x * 0.16 + t).sin()
                        + (y * 0.21 - t * 1.3).sin()
                        + ((x + y) * 0.11 + t * 0.7).sin()
                        + (d * (0.3 + level * 0.25) - t * 2.0).sin();
                    let v = (v / 8.0 + 0.5).clamp(0.0, 0.999);
                    let pos = v * cycle.len() as f64 * 1.5 + t * 1.5;
                    let (fg, bg) = (
                        PEPTO[cycle[(pos as usize + 1) % cycle.len()] as usize],
                        PEPTO[cycle[pos as usize % cycle.len()] as usize],
                    );
                    let pat = &PATTERNS[(pos.fract() * PATTERNS.len() as f64) as usize];
                    for (row, bits) in pat.iter().enumerate() {
                        let base = (cy * 8 + row) * W + cx * 8;
                        for bit in 0..8 {
                            px[base + bit] = if bits & (0x80 >> bit) != 0 { fg } else { bg };
                        }
                    }
                }
            }
            // SID voice raster bars across the plasma.
            for (v, r) in [RAMP_RED, RAMP_GREEN, RAMP_BLUE].iter().enumerate() {
                let cy = 56.0 + v as f64 * 48.0;
                let half = 1.0 + voices[v] * 10.0;
                for y in (cy - half) as usize..(cy + half) as usize {
                    let d = 1.0 - ((y as f64 - cy).abs() / half);
                    px[y * W..y * W + W].fill(ramp(r, 0.3 + d * 0.7));
                }
            }
            // Top bar for the title.
            px[..12 * W].fill(PEPTO[0]);
            px[12 * W..13 * W].fill(ramp(RAMP_FIRE, 0.6 + beat * 0.4));
        });
        let cr = self.ctx();
        let label = title.to_uppercase();
        let tw = label.chars().count() as f64 * 8.0;
        let x = if tw < W as f64 { (W as f64 - tw) / 2.0 } else { 4.0 - ((t * 30.0) % (tw - W as f64 + 40.0)) };
        set_rgb(&cr, ramp(RAMP_GREY, 0.7 + beat * 0.3));
        text(&cr, x, 9.0, &label);
    }
}

/// Draw `s` in Sixtyfour at 8px — one C64 character cell per glyph — with its
/// baseline at `y`, unantialiased so it lands on whole framebuffer pixels.
fn text(cr: &cairo::Context, x: f64, y: f64, s: &str) {
    c64_text(cr, x, y, s, 8.0);
}

/// Draw `s` in unantialiased Sixtyfour with glyphs `px` pixels wide and its
/// baseline at `y`. Returns the drawn width.
pub(crate) fn c64_text(cr: &cairo::Context, x: f64, y: f64, s: &str, px: f64) -> f64 {
    let mut font = pango::FontDescription::from_string("Sixtyfour");
    font.set_absolute_size(px * pango::SCALE as f64);
    font.set_variations(Some("SCAN=100,BLED=0"));
    let layout = pangocairo::functions::create_layout(cr);
    let mut fo = cairo::FontOptions::new().expect("font options");
    fo.set_antialias(cairo::Antialias::None);
    fo.set_hint_style(cairo::HintStyle::Full);
    pangocairo::functions::context_set_font_options(&layout.context(), Some(&fo));
    layout.context().changed();
    layout.set_font_description(Some(&font));
    layout.set_text(s);
    let baseline = layout.baseline() as f64 / pango::SCALE as f64;
    cr.move_to(x, y - baseline);
    pangocairo::functions::show_layout(cr, &layout);
    layout.pixel_size().0 as f64
}

pub(crate) fn set_rgb(cr: &cairo::Context, c: u32) {
    cr.set_source_rgb(
        ((c >> 16) & 0xff) as f64 / 255.0,
        ((c >> 8) & 0xff) as f64 / 255.0,
        (c & 0xff) as f64 / 255.0,
    );
}

/// View cairo's RGB24 bytes (native-endian xRGB words) as u32 pixels.
fn bytemuck_u32(bytes: &mut [u8]) -> &mut [u32] {
    assert_eq!(bytes.as_ptr() as usize % 4, 0, "cairo image data is 4-byte aligned");
    // SAFETY: cairo RGB24 rows are 4-byte aligned words; with width 320 the stride
    // equals W*4, so the buffer is exactly W*H u32s.
    unsafe { std::slice::from_raw_parts_mut(bytes.as_mut_ptr() as *mut u32, bytes.len() / 4) }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Render each effect with synthetic audio; set VIZ_PNG_DIR to keep the frames.
    #[test]
    fn renders_every_effect() {
        let dir = std::env::var("VIZ_PNG_DIR").ok();
        use pango::prelude::*;
        let font = concat!(env!("CARGO_MANIFEST_DIR"), "/data/fonts/Sixtyfour.ttf");
        pangocairo::FontMap::default().add_font_file(font).unwrap();
        let samples: Vec<i16> = (0..256).map(|i| ((i as f64 * 0.3).sin() * 12000.0) as i16).collect();
        let mut regs = [0u8; breadbin_core::sid::NUM_REGS];
        for v in 0..3 {
            regs[v * 7 + 4] = 0x41; // pulse + gate
            regs[v * 7 + 6] = 0xA0;
        }
        let vis = Vis { regs, frame: 0 };
        let mut viz = Viz::new();
        for kind in 0..=NAMES.len() {
            viz.set_kind(kind);
            let playing = kind < NAMES.len();
            for f in 0..40 {
                let t = 3.0 + f as f64 / 60.0;
                viz.update(playing.then_some((&samples[..], &vis)), t);
            }
            let mut out = cairo::ImageSurface::create(cairo::Format::Rgb24, 640, 480).unwrap();
            viz.draw(&cairo::Context::new(&out).unwrap(), 640, 480, "Sheds on a Sea-Girt Isle", 3.7);
            if let Some(d) = &dir {
                let stride = out.stride() as usize;
                let data = out.data().unwrap();
                let img = image::RgbImage::from_fn(640, 480, |x, y| {
                    let i = y as usize * stride + x as usize * 4;
                    image::Rgb([data[i + 2], data[i + 1], data[i]]) // little-endian xRGB
                });
                img.save(format!("{d}/viz{kind}.png")).unwrap();
            }
        }
    }
}
