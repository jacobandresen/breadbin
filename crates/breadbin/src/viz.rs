//! The Tunes visualisers: three classic demo effects drawn into a real 320x200
//! framebuffer in the Pepto palette, each a homage to a top demo. The host
//! scales it up with hard pixels inside a border that flashes on the beat
//! (`border`), and supplies the font once as 1-bit glyph masks (`set_font`).

use std::f64::consts::PI;

pub const W: usize = 320;
pub const H: usize = 200;

/// The Pepto VIC-II palette as RGBA bytes read little-endian (0xAABBGGRR).
const PEPTO: [u32; 16] = {
    const RGB: [u32; 16] = [
        0x000000, 0xFFFFFF, 0x883932, 0x67B6BD, 0x8B3F96, 0x55A049, 0x40318D, 0xBFCE72, 0x8B5429, 0x574200, 0xB86962, 0x505050,
        0x787878, 0x94E089, 0x7869C4, 0x9F9F9F,
    ];
    let mut out = [0; 16];
    let mut i = 0;
    while i < 16 {
        let c = RGB[i];
        out[i] = 0xff00_0000 | ((c & 0xff) << 16) | (c & 0xff00) | ((c >> 16) & 0xff);
        i += 1;
    }
    out
};

// Luma ramps (dark -> light) built from palette indices, as C64 coders did.
const RAMP_BLUE: &[u8] = &[0, 6, 11, 4, 14, 3, 13, 1];
const RAMP_RED: &[u8] = &[0, 9, 2, 8, 10, 15, 7, 1];
const RAMP_GREEN: &[u8] = &[0, 11, 5, 12, 13, 1];
const RAMP_GREY: &[u8] = &[0, 11, 12, 15, 1];
const RAMP_FIRE: &[u8] = &[0, 9, 2, 8, 10, 7, 1];

fn ramp_index(r: &[u8], v: f64) -> u8 {
    r[(v.clamp(0.0, 0.999) * r.len() as f64) as usize]
}
fn ramp(r: &[u8], v: f64) -> u32 {
    PEPTO[ramp_index(r, v) as usize]
}

type Face = (f64, [(f64, f64); 3], f64);

const GREETINGS: &str = "GREETINGS TO OXYRON * BOOZE DESIGN * CREST * PERFORMERS * \
CENSOR DESIGN * FAIRLIGHT * BONZAI * TRIAD * ARSENIC * GENESIS PROJECT * ";

/// Glyph cells: 8 wide, 10 tall, baseline at row 8; ASCII 32..127.
const GW: usize = 8;
const GH: usize = 10;

pub struct Viz {
    pub fb: Vec<u32>,
    font: Vec<u8>,
    playing: bool,
    level: f64,
    avg: f64,
    beat: f64,
    last_beat: f64,
    voices: [f64; 3],
    gates: [bool; 3],
    last_t: f64,
}

impl Default for Viz {
    fn default() -> Self {
        Viz {
            fb: vec![PEPTO[0]; W * H],
            font: vec![0; 96 * GW * GH],
            playing: false,
            level: 0.0,
            avg: 0.0,
            beat: 0.0,
            last_beat: 0.0,
            voices: [0.0; 3],
            gates: [false; 3],
            last_t: 0.0,
        }
    }
}

impl Viz {
    /// 96 glyphs (ASCII 32..127) of 8x10 bytes, nonzero = ink.
    pub fn set_font(&mut self, masks: &[u8]) {
        let n = masks.len().min(self.font.len());
        self.font[..n].copy_from_slice(&masks[..n]);
    }

    /// Feed one video frame: `rms` of the output audio (None when idle) and the
    /// SID register snapshot.
    pub fn update(&mut self, rms: Option<f64>, regs: &[u8], t: f64) {
        let dt = (t - self.last_t).clamp(0.0, 0.1);
        self.last_t = t;
        self.playing = rms.is_some();
        let Some(rms) = rms else {
            self.level *= 0.9;
            return;
        };
        self.level = (rms / 0.27).min(1.0);
        self.avg = self.avg * 0.95 + self.level * 0.05;
        if self.level > self.avg * 1.3 && self.level > 0.06 && t - self.last_beat > 0.22 {
            self.beat = 1.0;
            self.last_beat = t;
        }
        self.beat *= (-dt * 6.0).exp();
        // per-voice envelopes: a new note (gate rising) kicks the voice to full
        for v in 0..3 {
            let gate = regs.get(v * 7 + 4).is_some_and(|c| c & 1 != 0);
            if gate && !self.gates[v] {
                self.voices[v] = 1.0;
            }
            self.gates[v] = gate;
            let floor = if gate { (regs.get(v * 7 + 6).copied().unwrap_or(0) >> 4) as f64 / 15.0 * 0.6 } else { 0.0 };
            self.voices[v] = (self.voices[v] * (-dt * 4.0).exp()).max(floor);
        }
    }

    /// Render effect `kind` (or the READY screen when idle).
    pub fn draw(&mut self, kind: u32, title: &str, t: f64) {
        if !self.playing {
            self.ready_screen(t);
        } else {
            match kind {
                0 => self.twister(title, t),
                1 => self.vectors(t),
                _ => self.plasma(title, t),
            }
        }
    }

    /// The border colour for this frame, as 0xRRGGBB.
    pub fn border(&self) -> u32 {
        // warm: a red ramp flashing on the beat, brown at rest
        let c = if self.playing { PEPTO[ramp_index(RAMP_RED, 0.55 + self.beat * 0.45) as usize] } else { PEPTO[9] };
        ((c & 0xff) << 16) | (c & 0xff00) | ((c >> 16) & 0xff)
    }

    // ── primitives ──────────────────────────────────────────────────────────

    fn row(&mut self, y: i32, c: u32) {
        if (0..H as i32).contains(&y) {
            let y = y as usize;
            self.fb[y * W..y * W + W].fill(c);
        }
    }

    fn span(&mut self, y: i32, x0: f64, x1: f64, c: u32) {
        if !(0..H as i32).contains(&y) {
            return;
        }
        let (a, b) = (x0.round().max(0.0) as usize, (x1.round() as usize).min(W));
        if a < b {
            let y = y as usize;
            self.fb[y * W + a..y * W + b].fill(c);
        }
    }

    fn text(&mut self, x: f64, base_y: f64, s: &str, color: impl Fn(f64) -> u32) {
        let (mut x, top) = (x.round() as i32, base_y.round() as i32 - 8);
        for ch in s.chars() {
            let code = ch as usize;
            if (33..128).contains(&code) && x > -(GW as i32) && x < W as i32 {
                let c = color(x as f64);
                let g = &self.font[(code - 32) * GW * GH..][..GW * GH];
                for gy in 0..GH {
                    let y = top + gy as i32;
                    for gx in 0..GW {
                        let px = x + gx as i32;
                        if g[gy * GW + gx] != 0 && (0..H as i32).contains(&y) && (0..W as i32).contains(&px) {
                            self.fb[y as usize * W + px as usize] = c;
                        }
                    }
                }
            }
            x += GW as i32;
        }
    }

    fn triangle(&mut self, mut p: [(f64, f64); 3], c: u32) {
        p.sort_by(|a, b| a.1.total_cmp(&b.1));
        let [a, b, d] = p;
        let edge = |p: (f64, f64), q: (f64, f64), y: f64| if q.1 == p.1 { p.0 } else { p.0 + (q.0 - p.0) * (y - p.1) / (q.1 - p.1) };
        for y in (a.1.ceil().max(0.0) as i32)..=(d.1.floor().min(H as f64 - 1.0) as i32) {
            let yf = y as f64;
            let xa = edge(a, d, yf);
            let xb = if yf < b.1 { edge(a, b, yf) } else { edge(b, d, yf) };
            self.span(y, xa.min(xb), xa.max(xb) + 1.0, c);
        }
    }

    // ── idle: the oven warming up ───────────────────────────────────────────

    fn ready_screen(&mut self, t: f64) {
        self.fb.fill(PEPTO[0]);
        // a slow warm glow rising from the bottom, brown into orange
        for y in 120..H as i32 {
            let d = (y - 120) as f64 / (H as f64 - 120.0);
            self.row(y, if d + (t * 0.8).sin() * 0.05 > 0.7 { PEPTO[8] } else if d > 0.35 { PEPTO[9] } else { PEPTO[0] });
        }
        let lines = [("breadbin sid player", PEPTO[7]), ("", 0), ("pick a tune, or press radio", PEPTO[15]), ("the chip is warming up", PEPTO[12])];
        for (i, (l, c)) in lines.iter().enumerate() {
            let x = (W as f64 - l.len() as f64 * GW as f64) / 2.0;
            self.text(x, 40.0 + i as f64 * 12.0, l, |_| *c);
        }
        if (t * 1.6) as i64 % 2 == 0 {
            let y0 = 40 + lines.len() as i32 * 12;
            for y in y0..y0 + 8 {
                self.span(y, W as f64 / 2.0 - 4.0, W as f64 / 2.0 + 4.0, PEPTO[8]);
            }
        }
    }

    // ── 1. Twister, for Oxyron's Comaland ───────────────────────────────────

    fn twister(&mut self, title: &str, t: f64) {
        let (level, beat, voices) = (self.level, self.beat, self.voices);
        self.fb.fill(PEPTO[0]);
        // copper bars behind the twister, one per SID voice
        for (v, r) in [RAMP_RED, RAMP_GREEN, RAMP_BLUE].iter().enumerate() {
            let cy = 92.0 + (t * (0.9 + v as f64 * 0.37) + v as f64 * 2.1).sin() * 70.0;
            let half = 4.0 + voices[v] * 9.0;
            for y in (cy - half).max(0.0) as i32..((cy + half) as i32).min(H as i32 - 16) {
                let d = 1.0 - (y as f64 - cy).abs() / half;
                self.row(y, ramp(r, d * (0.4 + voices[v] * 0.6)));
            }
        }
        // a square bar whose rotation varies per scanline
        let faces = [RAMP_RED, RAMP_BLUE, RAMP_GREEN, RAMP_GREY];
        let r = 28.0 + beat * 10.0;
        for y in 0..H as i32 - 16 {
            let yf = y as f64;
            let a = t * 1.3 + (yf * 0.011 + t * 0.8).sin() * (1.2 + level * 2.5);
            let cx = 160.0 + (yf * 0.017 + t * 0.6).sin() * (20.0 + level * 50.0);
            for (i, face) in faces.iter().enumerate() {
                let x0 = cx + r * (a + i as f64 * PI / 2.0).sin();
                let x1 = cx + r * (a + (i + 1) as f64 * PI / 2.0).sin();
                if x1 > x0 {
                    self.span(y, x0.floor(), x1.floor(), ramp(face, 0.25 + (x1 - x0) / (2.0 * r) * 0.8));
                }
            }
        }
        self.row(H as i32 - 17, PEPTO[1]); // raster split for the scroller
        let msg = format!("NOW PLAYING: {}  ***  {GREETINGS}", title.to_uppercase());
        self.sine_scroller(&msg, t, H as f64 - 4.0);
    }

    fn sine_scroller(&mut self, msg: &str, t: f64, base_y: f64) {
        let chars: Vec<char> = msg.chars().collect();
        let span = chars.len() as f64 * 8.0;
        let offset = (t * 60.0) % span;
        for (i, ch) in chars.iter().enumerate() {
            for x in [i as f64 * 8.0 - offset, i as f64 * 8.0 - offset + span] {
                if (-8.0..W as f64).contains(&x) {
                    let y = base_y + (x * 0.04 + t * 4.0).sin() * 2.5;
                    let c = ramp(RAMP_FIRE, 0.45 + 0.5 * ((x * 0.02 + t * 2.0).sin() * 0.5 + 0.5));
                    self.text(x, y, &ch.to_string(), |_| c);
                }
            }
        }
    }

    // ── 2. Filled vectors, for Booze Design's Edge of Disgrace ──────────────

    fn vectors(&mut self, t: f64) {
        let (level, beat) = (self.level, self.beat);
        for y in 0..110 {
            self.row(y, ramp(RAMP_BLUE, y as f64 / 110.0 * 0.75));
        }
        // a perspective checkerboard floor scrolling towards us
        for y in 110..H {
            let z = 2400.0 / (y as f64 - 104.0);
            let shade = (1.0 - (z / 300.0).min(1.0)) * 0.7;
            let row = ((z + t * 120.0) / 24.0).floor() as i64;
            let (grey, blue) = (ramp(RAMP_GREY, shade * 0.9), ramp(RAMP_BLUE, shade * 0.9));
            for x in 0..W {
                let wx = (x as f64 - 160.0) * z / 160.0;
                self.fb[y * W + x] = if ((wx / 24.0).floor() as i64 + row) & 1 == 0 { grey } else { blue };
            }
        }

        // an icosahedron, flat-shaded with palette ramps and painter-sorted
        let g = (1.0 + 5f64.sqrt()) / 2.0;
        let verts: [[f64; 3]; 12] = [
            [-1.0, g, 0.0], [1.0, g, 0.0], [-1.0, -g, 0.0], [1.0, -g, 0.0], [0.0, -1.0, g], [0.0, 1.0, g],
            [0.0, -1.0, -g], [0.0, 1.0, -g], [g, 0.0, -1.0], [g, 0.0, 1.0], [-g, 0.0, -1.0], [-g, 0.0, 1.0],
        ];
        const FACES: [[usize; 3]; 20] = [
            [0, 11, 5], [0, 5, 1], [0, 1, 7], [0, 7, 10], [0, 10, 11], [1, 5, 9], [5, 11, 4], [11, 10, 2], [10, 7, 6], [7, 1, 8],
            [3, 9, 4], [3, 4, 2], [3, 2, 6], [3, 6, 8], [3, 8, 9], [4, 9, 5], [2, 4, 11], [6, 2, 10], [8, 6, 7], [9, 8, 1],
        ];
        let (ax, ay) = (t * 0.7 + beat * 0.4, t * 0.45);
        let (sx, cx, sy, cy) = (ax.sin(), ax.cos(), ay.sin(), ay.cos());
        let world: Vec<[f64; 3]> = verts
            .iter()
            .map(|p| {
                let (y, z) = (p[1] * cx - p[2] * sx, p[1] * sx + p[2] * cx);
                [p[0] * cy + z * sy, y, -p[0] * sy + z * cy]
            })
            .collect();
        let size = 30.0 * (1.0 + beat * 0.25 + level * 0.15);
        let bob = (t * 1.7).sin() * 10.0;
        let project = |p: [f64; 3]| {
            let k = 4.5 / (4.5 + p[2]);
            (160.0 + p[0] * size * k, 80.0 + bob + p[1] * size * k)
        };
        let light = {
            let n = (0.4f64 * 0.4 + 0.6 * 0.6 + 0.7 * 0.7).sqrt();
            [0.4 / n, -0.6 / n, -0.7 / n]
        };
        let face_ramp = [RAMP_FIRE, RAMP_BLUE, RAMP_GREEN][(t / 8.0) as usize % 3];
        // (depth, projected corners, brightness) per visible face
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
                Some((a[2] + b[2] + c[2], [project(a), project(b), project(c)], lit.max(0.0)))
            })
            .collect();
        visible.sort_by(|a, b| b.0.total_cmp(&a.0));

        // shadow on the floor, then the faces
        let (rx, ry) = (size * 1.6, size * 0.25);
        for y in (150.0 - ry).ceil() as i32..=(150.0 + ry) as i32 {
            let hw = rx * (1.0 - ((y as f64 - 150.0) / ry).powi(2)).max(0.0).sqrt();
            self.span(y, 160.0 - hw, 160.0 + hw, PEPTO[0]);
        }
        for (_, pts, lit) in &visible {
            self.triangle(*pts, ramp(face_ramp, 0.15 + lit * 0.85 + beat * 0.15));
        }
        self.text(8.0, 12.0, "BREADBIN  FILLED VECTORS", |_| PEPTO[1]);
    }

    // ── 3. Char plasma, for Crest & Oxyron's Deus Ex Machina ────────────────

    fn plasma(&mut self, title: &str, t: f64) {
        // 8x8 dither patterns standing in for PETSCII shading characters
        const PATTERNS: [[u8; 8]; 5] = [
            [0x00; 8],
            [0x88, 0x00, 0x22, 0x00, 0x88, 0x00, 0x22, 0x00],
            [0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55, 0xAA, 0x55],
            [0x77, 0xFF, 0xDD, 0xFF, 0x77, 0xFF, 0xDD, 0xFF],
            [0xFF; 8],
        ];
        // luma-ordered and symmetric, so neighbouring values never jump in brightness
        const CYCLES: [&[u8]; 2] = [&[0, 6, 4, 14, 3, 13, 1, 13, 3, 14, 4, 6], &[0, 9, 2, 8, 10, 7, 1, 7, 10, 8, 2, 9]];
        let (level, beat, voices) = (self.level, self.beat, self.voices);
        let cycle = CYCLES[(t / 12.0) as usize % CYCLES.len()];
        let (mx, my) = (20.0 + (t * 0.6).sin() * 12.0, 12.5 + (t * 0.8).cos() * 8.0);
        for cy in 0..25 {
            for cx in 0..40 {
                let (x, y) = (cx as f64, cy as f64);
                let d = ((x - mx).powi(2) + (y - my).powi(2)).sqrt();
                let v = (x * 0.16 + t).sin() + (y * 0.21 - t * 1.3).sin() + ((x + y) * 0.11 + t * 0.7).sin() + (d * (0.3 + level * 0.25) - t * 2.0).sin();
                let v = (v / 8.0 + 0.5).clamp(0.0, 0.999);
                let pos = v * cycle.len() as f64 * 1.5 + t * 1.5;
                let fg = PEPTO[cycle[(pos as usize + 1) % cycle.len()] as usize];
                let bg = PEPTO[cycle[pos as usize % cycle.len()] as usize];
                let pat = &PATTERNS[(pos.fract() * PATTERNS.len() as f64) as usize];
                for (row, bits) in pat.iter().enumerate() {
                    let base = (cy * 8 + row) * W + cx * 8;
                    for bit in 0..8 {
                        self.fb[base + bit] = if bits & (0x80 >> bit) != 0 { fg } else { bg };
                    }
                }
            }
        }
        // SID voice raster bars across the plasma
        for (v, r) in [RAMP_RED, RAMP_GREEN, RAMP_BLUE].iter().enumerate() {
            let cy = 56.0 + v as f64 * 48.0;
            let half = 1.0 + voices[v] * 10.0;
            for y in (cy - half) as i32..(cy + half) as i32 {
                self.row(y, ramp(r, 0.3 + (1.0 - (y as f64 - cy).abs() / half) * 0.7));
            }
        }
        // title bar
        self.fb[..12 * W].fill(PEPTO[0]);
        self.row(12, ramp(RAMP_FIRE, 0.6 + beat * 0.4));
        let label = title.to_uppercase();
        let tw = label.chars().count() as f64 * 8.0;
        let x = if tw < W as f64 { (W as f64 - tw) / 2.0 } else { 4.0 - ((t * 30.0) % (tw - W as f64 + 40.0)) };
        let c = ramp(RAMP_GREY, 0.7 + beat * 0.3);
        self.text(x, 9.0, &label, |_| c);
    }
}
