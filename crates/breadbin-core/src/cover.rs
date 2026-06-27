// cover - box-art lookup from the libretro-thumbnails Commodore 64 set
// (Named_Boxarts), plus the Internet Archive item-image fallback. Port of
// bb-cover's index/fetch logic and c64menu.ia_cover; the fzf preview CLI is not
// ported (the ratatui TUIs render covers directly).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;

use regex::Regex;

use crate::core;

const TREE: &str =
    "https://api.github.com/repos/libretro-thumbnails/Commodore_-_64/git/trees/master?recursive=1";
const RAW: &str = "https://raw.githubusercontent.com/libretro-thumbnails/Commodore_-_64/master/Named_Boxarts/";

pub fn cidx_path() -> PathBuf {
    core::data_path("covers_index.tsv")
}
pub fn cdir() -> PathBuf {
    // Use user data directory for cached covers
    crate::core::user_data_dir().join("covers")
}

fn region_rank(name: &str) -> u8 {
    let n = name.to_lowercase();
    if n.contains("(world") {
        0
    } else if n.contains("(europe") {
        1
    } else if n.contains("(usa") {
        2
    } else {
        3
    }
}

/// Fetch the libretro tree and write covers_index.tsv (canon<TAB>filename),
/// keeping the best region per canonical title.
pub fn build_cover_index() -> Result<(), String> {
    let body = core::fetch(TREE, &[])?;
    let json: serde_json::Value =
        serde_json::from_slice(&body).map_err(|e| e.to_string())?;
    let tree = json["tree"].as_array().ok_or("tree: not an array")?;

    let mut best: HashMap<String, (u8, String)> = HashMap::new();
    for p in tree {
        let path = p["path"].as_str().unwrap_or("");
        if !path.starts_with("Named_Boxarts/") || !path.ends_with(".png") {
            continue;
        }
        let fname = &path["Named_Boxarts/".len()..];
        let stem = &fname[..fname.len() - 4]; // drop ".png"
        let canon = core::norm(core::split_before(stem, r"\s*\("));
        let rank = region_rank(fname);
        if !canon.is_empty()
            && best.get(&canon).map_or(true, |(r, _)| rank < *r)
        {
            best.insert(canon, (rank, fname.to_string()));
        }
    }

    let mut out = String::new();
    for (c, (_, fname)) in &best {
        out.push_str(&format!("{c}\t{fname}\n"));
    }
    std::fs::write(cidx_path(), out).map_err(|e| e.to_string())
}

/// canon -> cover filename map, building the index on first use.
pub fn load_index() -> HashMap<String, String> {
    if !cidx_path().exists() {
        let _ = build_cover_index();
    }
    let mut map = HashMap::new();
    if let Ok(text) = std::fs::read_to_string(cidx_path()) {
        for line in text.lines() {
            if let Some((c, fname)) = line.split_once('\t') {
                map.insert(c.to_string(), fname.to_string());
            }
        }
    }
    map
}

fn cached(path: &PathBuf) -> bool {
    std::fs::metadata(path).map(|m| m.len() > 0).unwrap_or(false)
}

/// Cached cover path for a game (downloading once), or None.
pub fn ensure_cover(canon: &str, idx: &HashMap<String, String>) -> Option<PathBuf> {
    let fname = idx.get(canon)?;
    let cache = cdir().join(format!("{canon}.png"));
    if !cached(&cache) {
        let data = core::fetch(&format!("{RAW}{}", core::quote(fname)), &[]).ok()?;
        std::fs::create_dir_all(cdir()).ok()?;
        std::fs::write(&cache, data).ok()?;
    }
    Some(cache)
}

// ── Cover badges ─────────────────────────────────────────────────────────────
// These are ported verbatim from the old rust/src/kiosk.rs.

fn fill_ellipse(img: &mut image::RgbaImage, cx: f32, cy: f32, rx: f32, ry: f32, c: image::Rgba<u8>) {
    if rx <= 0.0 || ry <= 0.0 { return; }
    let (w, h) = img.dimensions();
    let x0 = (cx - rx).floor().max(0.0) as u32;
    let x1 = (cx + rx).ceil().min(w as f32 - 1.0) as u32;
    let y0 = (cy - ry).floor().max(0.0) as u32;
    let y1 = (cy + ry).ceil().min(h as f32 - 1.0) as u32;
    for y in y0..=y1 {
        for x in x0..=x1 {
            let dx = x as f32 + 0.5 - cx;
            let dy = y as f32 + 0.5 - cy;
            if (dx / rx) * (dx / rx) + (dy / ry) * (dy / ry) <= 1.0 {
                img.put_pixel(x, y, c);
            }
        }
    }
}

fn fill_circle(img: &mut image::RgbaImage, cx: f32, cy: f32, r: f32, c: image::Rgba<u8>) {
    fill_ellipse(img, cx, cy, r, r, c);
}

fn fill_disc_gradient(img: &mut image::RgbaImage, cx: f32, cy: f32, r: f32, inner: [u8; 3], outer: [u8; 3]) {
    let (w, h) = img.dimensions();
    let x0 = (cx - r).floor().max(0.0) as u32;
    let x1 = (cx + r).ceil().min(w as f32 - 1.0) as u32;
    let y0 = (cy - r).floor().max(0.0) as u32;
    let y1 = (cy + r).ceil().min(h as f32 - 1.0) as u32;
    let lerp = |a: u8, b: u8, t: f32| (a as f32 + (b as f32 - a as f32) * t).round() as u8;
    for y in y0..=y1 {
        for x in x0..=x1 {
            let dx = x as f32 + 0.5 - cx;
            let dy = y as f32 + 0.5 - cy;
            let d = (dx * dx + dy * dy).sqrt();
            if d <= r {
                let t = d / r;
                img.put_pixel(x, y, image::Rgba([
                    lerp(inner[0], outer[0], t),
                    lerp(inner[1], outer[1], t),
                    lerp(inner[2], outer[2], t),
                    255,
                ]));
            }
        }
    }
}

fn fill_rect(img: &mut image::RgbaImage, x: f32, y: f32, rw: f32, rh: f32, c: image::Rgba<u8>) {
    let (w, h) = img.dimensions();
    let x0 = x.max(0.0) as u32;
    let y0 = y.max(0.0) as u32;
    let x1 = (x + rw).clamp(0.0, w as f32) as u32;
    let y1 = (y + rh).clamp(0.0, h as f32) as u32;
    for yy in y0..y1 { for xx in x0..x1 { img.put_pixel(xx, yy, c); } }
}

fn point_in_poly(px: f32, py: f32, pts: &[(f32, f32)]) -> bool {
    let mut inside = false;
    let n = pts.len();
    let mut j = n - 1;
    for i in 0..n {
        let (xi, yi) = pts[i];
        let (xj, yj) = pts[j];
        if (yi > py) != (yj > py) && px < (xj - xi) * (py - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

fn fill_polygon(img: &mut image::RgbaImage, pts: &[(f32, f32)], c: image::Rgba<u8>) {
    let (w, h) = img.dimensions();
    let minx = pts.iter().map(|p| p.0).fold(f32::INFINITY, f32::min).floor().max(0.0) as u32;
    let maxx = pts.iter().map(|p| p.0).fold(f32::NEG_INFINITY, f32::max).ceil().min(w as f32 - 1.0) as u32;
    let miny = pts.iter().map(|p| p.1).fold(f32::INFINITY, f32::min).floor().max(0.0) as u32;
    let maxy = pts.iter().map(|p| p.1).fold(f32::NEG_INFINITY, f32::max).ceil().min(h as f32 - 1.0) as u32;
    for y in miny..=maxy {
        for x in minx..=maxx {
            if point_in_poly(x as f32 + 0.5, y as f32 + 0.5, pts) {
                img.put_pixel(x, y, c);
            }
        }
    }
}

fn fill_star(img: &mut image::RgbaImage, cx: f32, cy: f32, r_out: f32, r_in: f32, c: image::Rgba<u8>) {
    let mut pts = [(0.0f32, 0.0f32); 10];
    for (k, p) in pts.iter_mut().enumerate() {
        let ang = -std::f32::consts::FRAC_PI_2 + k as f32 * std::f32::consts::PI / 5.0;
        let rr = if k % 2 == 0 { r_out } else { r_in };
        *p = (cx + rr * ang.cos(), cy + rr * ang.sin());
    }
    fill_polygon(img, &pts, c);
}

/// Composite a gold five-pointed star into the top-right of a cover bitmap (5/5 badge).
pub fn draw_rating_badge(img: &mut image::RgbaImage) {
    let (w, h) = img.dimensions();
    if w < 16 || h < 16 { return; }
    let r = (w.min(h) as f32) * 0.16;
    let cx = w as f32 - r * 1.2;
    let cy = r * 1.2;
    let ir = 0.42;
    fill_star(img, cx, cy, r * 1.15, r * 1.15 * ir, image::Rgba([92u8, 60, 0, 255]));
    fill_star(img, cx, cy, r, r * ir, image::Rgba([255u8, 186, 10, 255]));
    fill_star(img, cx, cy - r * 0.06, r * 0.5, r * 0.5 * ir, image::Rgba([255u8, 232, 150, 255]));
}

/// Composite a joystick badge into the bottom-right of a cover bitmap.
pub fn draw_joystick_badge(img: &mut image::RgbaImage) {
    let (w, h) = img.dimensions();
    if w < 16 || h < 16 { return; }
    let r = (w.min(h) as f32) * 0.15;
    let cx = w as f32 - r * 1.35;
    let cy = h as f32 - r * 1.35;
    let rim = image::Rgba([18u8, 18, 22, 255]);
    let base_dark = image::Rgba([28u8, 30, 36, 255]);
    let base_lit = image::Rgba([78u8, 82, 96, 255]);
    let shaft = image::Rgba([40u8, 42, 50, 255]);
    let shaft_lit = image::Rgba([120u8, 124, 138, 255]);
    let ball = image::Rgba([206u8, 38, 34, 255]);
    let ball_shadow = image::Rgba([138u8, 20, 18, 255]);
    let gloss = image::Rgba([255u8, 235, 230, 255]);
    let red = image::Rgba([214u8, 44, 40, 255]);
    fill_circle(img, cx, cy, r, rim);
    fill_disc_gradient(img, cx, cy, r * 0.88, [255, 226, 132], [236, 158, 0]);
    fill_ellipse(img, cx, cy + r * 0.42, r * 0.60, r * 0.26, base_dark);
    fill_ellipse(img, cx, cy + r * 0.34, r * 0.46, r * 0.12, base_lit);
    fill_circle(img, cx + r * 0.30, cy + r * 0.36, r * 0.085, red);
    fill_circle(img, cx + r * 0.275, cy + r * 0.335, r * 0.03, gloss);
    fill_rect(img, cx - r * 0.085, cy - r * 0.30, r * 0.17, r * 0.62, shaft);
    fill_rect(img, cx - r * 0.085, cy - r * 0.30, r * 0.05, r * 0.62, shaft_lit);
    fill_circle(img, cx, cy - r * 0.36, r * 0.27, ball_shadow);
    fill_circle(img, cx - r * 0.02, cy - r * 0.38, r * 0.24, ball);
    fill_circle(img, cx - r * 0.09, cy - r * 0.45, r * 0.085, gloss);
}

/// Fallback cover: the Internet Archive item's own image (services/img), cached.
pub fn ia_cover(ident: &str) -> Option<PathBuf> {
    if ident.is_empty() {
        return None;
    }
    static SANITIZE: OnceLock<Regex> = OnceLock::new();
    let sanitize = SANITIZE.get_or_init(|| Regex::new(r"[^A-Za-z0-9_.-]").unwrap());
    let safe = sanitize.replace_all(ident, "_");
    let cache = cdir().join(format!("ia_{safe}.jpg"));
    if cached(&cache) {
        return Some(cache);
    }
    let data = core::fetch(&format!("https://archive.org/services/img/{ident}"), &[]).ok()?;
    if data.len() < 1000 {
        return None; // generic placeholder icon -> treat as none
    }
    std::fs::create_dir_all(cdir()).ok()?;
    std::fs::write(&cache, data).ok()?;
    Some(cache)
}
