// c64kiosk - a cover "kiosk" for C64 games, organised by genre (ratatui rewrite).
//
// Opens on a genre overview: each genre shows a title bar and its top covers.
//   · click / Enter a cover  -> download (if needed) and play that game in place
//   · click / Enter a genre title -> expand the genre into a full grid of covers
// In an expanded genre, pick any cover to play; Esc returns to the overview.
// Arrow keys move the focus, Enter activates, q quits. Games launch straight into
// the emulator and return here on exit, never dropping back to the shell.

use std::collections::{HashMap, HashSet};
use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::ExitCode;

use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, MouseButton, MouseEvent,
        MouseEventKind,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Rect},
    style::{Color, Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Frame, Terminal,
};
use ratatui_image::{picker::Picker, protocol::StatefulProtocol, Resize, StatefulImage};

use crate::tui::{self, Row};

const HELP: &str = "\
c64kiosk - a cover kiosk for C64 games, organised by genre.

Usage:
  c64kiosk            open the kiosk
  c64kiosk -w|-f|-r   pass --warp / --fullscreen / --real through to c64run
";

/// Synthetic section for GB64's curated classics, shown below "latest played".
const CLASSICS_GENRE: &str = "classics";

const TARGET_CW: u16 = 14; // expanded-genre card target width
const TARGET_CH: u16 = 16; // expanded-genre card target height
const OV_SECTION: u16 = 17; // overview: rows per genre section
const TITLE_H: u16 = 3; // overview: clickable genre bar height

#[derive(Clone, Copy)]
enum OFocus {
    Title(usize),       // genre index
    Game(usize, usize), // (genre index, position within the genre's top row)
}

#[derive(PartialEq)]
enum Mode {
    Overview,
    Genre,
}

/// GB64-derived data the kiosk decorates the catalogue with: the classics set, the
/// joystick-controlled set (for badges), and the named collections.tsv sections.
struct Curation {
    classics: HashSet<String>,
    joystick: HashSet<String>,
    top_rated: HashSet<String>,
    collections: Vec<(String, HashSet<String>)>,
}

struct KioskState {
    all: Vec<Row>,
    groups: Vec<(String, Vec<usize>)>,
    cidx: HashMap<String, String>,
    joystick: HashSet<String>, // canons GB64 marks as joystick-controlled
    top_rated: HashSet<String>, // canons GB64 rates 5/5
    runopts: Vec<String>,
    picker: Picker,

    cover_cache: HashMap<usize, Option<PathBuf>>,
    proto_cache: HashMap<PathBuf, StatefulProtocol>,

    mode: Mode,
    ofocus: Vec<OFocus>,
    osel: usize,
    otop: usize,
    topn: usize,

    genre: usize,
    sel: usize,
    top: usize,

    rects: Vec<(Rect, usize)>, // overview hit map -> ofocus index
    grid_rects: Vec<(Rect, usize)>, // genre hit map -> game index within genre
}

fn hit(rects: &[(Rect, usize)], col: u16, row: u16) -> Option<usize> {
    for (r, idx) in rects {
        if col >= r.x && col < r.x + r.width && row >= r.y && row < r.y + r.height {
            return Some(*idx);
        }
    }
    None
}

fn spaced_upper(s: &str) -> String {
    s.to_uppercase()
        .chars()
        .map(|c| c.to_string())
        .collect::<Vec<_>>()
        .join(" ")
}

impl KioskState {
    fn new(
        all: Vec<Row>,
        cidx: HashMap<String, String>,
        runopts: Vec<String>,
        picker: Picker,
        topn: usize,
        curation: Curation,
    ) -> Self {
        let Curation { classics, joystick, top_rated, collections } = curation;
        let mut groups = tui::group_by_genre(&all);
        // Lead with what people actually grab: order each genre by Internet Archive
        // download count, and order the genre sections by their most-downloaded game.
        // (Counts come from c64_index.tsv; a pre-column index leaves them all 0, so
        // the previous rating order stands until the next `--refresh`.)
        for (_, idxs) in groups.iter_mut() {
            idxs.sort_by_key(|&i| std::cmp::Reverse(all[i].downloads));
        }
        groups.sort_by_key(|(_, idxs)| {
            std::cmp::Reverse(idxs.first().map(|&i| all[i].downloads).unwrap_or(0))
        });
        // Pin Arcade then Shoot'em Up to the top, and push the catch-all "Other"
        // bucket to the very bottom; the genres between keep their popularity order
        // (sort_by_key is stable).
        groups.sort_by_key(|(genre, _)| match genre.as_str() {
            "Arcade" => 0,
            "Shoot'em Up" => 1,
            g if g == tui::GENRE_OTHER => 3,
            _ => 2,
        });

        // Curated sections shown above the genres, top to bottom: latest played,
        // classics, then the named collections from collections.tsv. Each lists its
        // catalogue members most-downloaded first; empty ones are skipped.
        let members = |canons: &HashSet<String>| -> Vec<usize> {
            let mut idxs: Vec<usize> = all
                .iter()
                .enumerate()
                .filter(|(_, r)| canons.contains(&tui::canon_of(r)))
                .map(|(i, _)| i)
                .collect();
            idxs.sort_by_key(|&i| std::cmp::Reverse(all[i].downloads));
            idxs
        };
        let mut front: Vec<(String, Vec<usize>)> = Vec::new();

        let recent = tui::recent_plays(None);
        if !recent.is_empty() {
            let by_disp: HashMap<&str, usize> =
                all.iter().enumerate().map(|(i, r)| (r.display.as_str(), i)).collect();
            let latest: Vec<usize> = recent
                .iter()
                .filter_map(|d| by_disp.get(d.as_str()).copied())
                .collect();
            if !latest.is_empty() {
                front.push((tui::LATEST_GENRE.to_string(), latest));
            }
        }
        if !classics.is_empty() {
            let idxs = members(&classics);
            if !idxs.is_empty() {
                front.push((CLASSICS_GENRE.to_string(), idxs));
            }
        }
        for (name, canons) in &collections {
            let idxs = members(canons);
            if !idxs.is_empty() {
                front.push((name.clone(), idxs));
            }
        }

        // curated sections first, then the genre groups
        front.append(&mut groups);
        groups = front;

        let mut ofocus = Vec::new();
        for (gi, (_, idxs)) in groups.iter().enumerate() {
            ofocus.push(OFocus::Title(gi));
            for j in 0..topn.min(idxs.len()) {
                ofocus.push(OFocus::Game(gi, j));
            }
        }

        Self {
            all,
            groups,
            cidx,
            joystick,
            top_rated,
            runopts,
            picker,
            cover_cache: HashMap::new(),
            proto_cache: HashMap::new(),
            mode: Mode::Overview,
            ofocus,
            osel: 0,
            otop: 0,
            topn,
            genre: 0,
            sel: 0,
            top: 0,
            rects: Vec::new(),
            grid_rects: Vec::new(),
        }
    }

    fn focused_genre(&self) -> usize {
        match self.ofocus[self.osel] {
            OFocus::Title(gi) | OFocus::Game(gi, _) => gi,
        }
    }

    /// Cover path for a row index, cached (may fetch on first use).
    fn cover_path(&mut self, row_idx: usize) -> Option<PathBuf> {
        if let Some(c) = self.cover_cache.get(&row_idx) {
            return c.clone();
        }
        let c = tui::cover_for(&self.all[row_idx], &self.cidx);
        self.cover_cache.insert(row_idx, c.clone());
        c
    }

    fn ensure_proto(&mut self, path: &PathBuf, joystick: bool, top_rated: bool) -> bool {
        if self.proto_cache.contains_key(path) {
            return true;
        }
        match image::open(path) {
            Ok(img) => {
                // Paint badges into the bitmap itself so they survive graphics-protocol
                // rendering (which draws the image over any text cells). Cover paths are
                // 1:1 with a game, so caching the badged protocol by path is safe. The
                // joystick badge sits bottom-right and the rating star top-right, so
                // they never overlap.
                let img = if joystick || top_rated {
                    let mut rgba = img.to_rgba8();
                    if top_rated {
                        draw_rating_badge(&mut rgba);
                    }
                    if joystick {
                        draw_joystick_badge(&mut rgba);
                    }
                    image::DynamicImage::ImageRgba8(rgba)
                } else {
                    img
                };
                self.proto_cache.insert(path.clone(), self.picker.new_resize_protocol(img));
                true
            }
            Err(_) => false,
        }
    }

    /// Draw a cover into `area`, with a focus border when `focused`. Falls back to
    /// a "[no cover]" label.
    fn draw_card(&mut self, f: &mut Frame, area: Rect, row_idx: usize, focused: bool) {
        if area.width < 2 || area.height < 2 {
            return;
        }
        if focused {
            let block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::LightYellow).add_modifier(Modifier::BOLD));
            f.render_widget(block, area);
        }
        let inner = Rect::new(area.x + 1, area.y + 1, area.width.saturating_sub(2), area.height.saturating_sub(2));
        if inner.width == 0 || inner.height == 0 {
            return;
        }
        // Badges are composited into the cover (see ensure_proto): a joystick in the
        // lower-right for joystick games, a gold star in the top-right for 5/5 games.
        let canon = tui::canon_of(&self.all[row_idx]);
        let joystick = self.joystick.contains(&canon);
        let top_rated = self.top_rated.contains(&canon);
        match self.cover_path(row_idx) {
            Some(path) if self.ensure_proto(&path, joystick, top_rated) => {
                if let Some(proto) = self.proto_cache.get_mut(&path) {
                    let widget = StatefulImage::default().resize(Resize::Fit(None));
                    f.render_stateful_widget(widget, inner, proto);
                }
            }
            _ => {
                let label = Paragraph::new("[no cover]")
                    .style(Style::default().add_modifier(Modifier::DIM))
                    .alignment(Alignment::Center);
                let mid = Rect::new(inner.x, inner.y + inner.height / 2, inner.width, 1);
                f.render_widget(label, mid);
            }
        }
    }

    fn title_bar(&self, f: &mut Frame, area: Rect, genre: &str, count: usize, focused: bool, hint: &str) {
        let mut style = Style::default().fg(Color::White).bg(Color::Blue).add_modifier(Modifier::BOLD);
        if focused {
            style = style.add_modifier(Modifier::REVERSED);
        }
        let txt = format!("  {} ({count})   {hint}", spaced_upper(genre));
        // Center the label vertically in the (TITLE_H-row) bar so it reads as a
        // fuller, more prominent header rather than a thin top strip.
        let mut lines = vec![Line::from("")];
        lines.push(Line::from(txt));
        lines.push(Line::from(""));
        let para = Paragraph::new(lines).style(style);
        f.render_widget(para, area);
    }

    // ---- overview ----------------------------------------------------------
    fn overview_geometry(&self, area: Rect) -> (usize, u16, u16, u16) {
        let cols_term = area.width;
        let lines_term = area.height;
        let body = lines_term.saturating_sub(1);
        let vis_g = ((body / OV_SECTION).max(1) as usize).min(self.groups.len());
        let vis_g = vis_g.max(1);
        let sec_h = body / vis_g as u16;
        let card_h = sec_h.saturating_sub(TITLE_H + 1).max(1);
        let card_w = (cols_term / self.topn.max(1) as u16).max(1);
        (vis_g, sec_h, card_h, card_w)
    }

    fn scroll_overview(&mut self, area: Rect) {
        let (vis_g, _, _, _) = self.overview_geometry(area);
        let fgi = self.focused_genre();
        if fgi < self.otop {
            self.otop = fgi;
        } else if fgi >= self.otop + vis_g {
            self.otop = fgi + 1 - vis_g;
        }
        let max_top = self.groups.len().saturating_sub(vis_g);
        if self.otop > max_top {
            self.otop = max_top;
        }
    }

    fn render_overview(&mut self, f: &mut Frame) {
        let area = f.area();
        let (vis_g, sec_h, card_h, card_w) = self.overview_geometry(area);
        self.scroll_overview(area);
        self.rects.clear();

        let header = Line::styled(
            format!(
                "C64 kiosk  {} genres · click a cover to play · click a genre to expand · esc quit",
                self.groups.len()
            ),
            Style::default().add_modifier(Modifier::BOLD),
        );
        f.buffer_mut().set_line(0, 0, &header, area.width);

        // index of the first ofocus entry for each genre (title), for hit/focus.
        for vi in 0..vis_g {
            let gi = self.otop + vi;
            if gi >= self.groups.len() {
                break;
            }
            let (genre, idxs) = (self.groups[gi].0.clone(), self.groups[gi].1.clone());
            let base = 1 + vi as u16 * sec_h;
            let title_focus = self.ofocus_index_of_title(gi);
            let bar = Rect::new(0, base, area.width, TITLE_H.min(area.height.saturating_sub(base)));
            self.title_bar(f, bar, &genre, idxs.len(), self.osel == title_focus, "click / ⏎ to open");
            self.rects.push((bar, title_focus));

            let cards_y = base + TITLE_H;
            if cards_y >= area.height {
                continue;
            }
            let ch = card_h.min(area.height - cards_y);
            for j in 0..self.topn.min(idxs.len()) {
                let x = j as u16 * card_w;
                if x >= area.width {
                    break;
                }
                let w = card_w.min(area.width - x);
                let rect = Rect::new(x, cards_y, w, ch);
                let foc_idx = title_focus + 1 + j;
                let focused = self.osel == foc_idx;
                self.draw_card(f, rect, idxs[j], focused);
                self.rects.push((rect, foc_idx));
            }
        }
    }

    fn ofocus_index_of_title(&self, gi: usize) -> usize {
        // titles appear in genre order; count entries before this genre.
        let mut idx = 0;
        for (g, idxs) in self.groups.iter().enumerate() {
            if g == gi {
                return idx;
            }
            idx += 1 + self.topn.min(idxs.1.len());
        }
        idx
    }

    /// Activate the overview focus: expand a genre (returns None) or pick a game.
    fn activate_overview(&mut self) -> Option<usize> {
        match self.ofocus[self.osel] {
            OFocus::Title(gi) => {
                self.mode = Mode::Genre;
                self.genre = gi;
                self.sel = 0;
                self.top = 0;
                None
            }
            OFocus::Game(gi, j) => Some(self.groups[gi].1[j]),
        }
    }

    // ---- expanded genre grid ----------------------------------------------
    fn grid_geometry(&self, area: Rect) -> (usize, usize, u16, u16) {
        let cols = ((area.width / TARGET_CW).max(1)) as usize;
        let body = area.height.saturating_sub(1);
        let rows = ((body / TARGET_CH).max(1)) as usize;
        let card_w = (area.width / cols as u16).max(1);
        let card_h = (body / rows as u16).max(1);
        (cols, rows, card_w, card_h)
    }

    fn render_genre(&mut self, f: &mut Frame) {
        let area = f.area();
        let (cols, vis_rows, card_w, card_h) = self.grid_geometry(area);
        let idxs = self.groups[self.genre].1.clone();
        let n = idxs.len();

        let sel_row = self.sel / cols;
        if sel_row < self.top {
            self.top = sel_row;
        } else if sel_row >= self.top + vis_rows {
            self.top = sel_row + 1 - vis_rows;
        }

        let bar = Rect::new(0, 0, area.width, 1);
        let title = format!(
            "  {}    ({n} games)   click a cover to play · esc back · q quit",
            spaced_upper(&self.groups[self.genre].0)
        );
        let para = Paragraph::new(Line::from(title))
            .style(Style::default().fg(Color::White).bg(Color::Blue).add_modifier(Modifier::BOLD));
        f.render_widget(para, bar);

        self.grid_rects.clear();
        for r in 0..vis_rows {
            for c in 0..cols {
                let idx = (self.top + r) * cols + c;
                if idx >= n {
                    continue;
                }
                let x = c as u16 * card_w;
                let y = 1 + r as u16 * card_h;
                if x >= area.width || y >= area.height {
                    continue;
                }
                let w = card_w.min(area.width - x);
                let h = card_h.min(area.height - y);
                let rect = Rect::new(x, y, w, h);
                self.draw_card(f, rect, idxs[idx], idx == self.sel);
                self.grid_rects.push((rect, idx));
            }
        }
    }

    fn render(&mut self, f: &mut Frame) {
        match self.mode {
            Mode::Overview => self.render_overview(f),
            Mode::Genre => self.render_genre(f),
        }
    }
}

/// Composite a circular joystick badge into the lower-right of a cover bitmap: a
/// gradient-shaded disc carrying a little C64-style stick — domed base, cylindrical
/// shaft, fire button, and a glossy red ball top with a highlight. Drawn into the
/// image so it survives graphics-protocol rendering, which paints the cover over
/// any terminal text in those cells.
fn draw_joystick_badge(img: &mut image::RgbaImage) {
    let (w, h) = img.dimensions();
    if w < 16 || h < 16 {
        return;
    }
    let r = (w.min(h) as f32) * 0.15;
    let cx = w as f32 - r * 1.35; // lower-right corner, inset by ~0.35r
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

    // badge disc: dark rim + amber face shaded from a light centre to a deep edge.
    fill_circle(img, cx, cy, r, rim);
    fill_disc_gradient(img, cx, cy, r * 0.88, [255, 226, 132], [236, 158, 0]);

    // domed base with a lighter top lip
    fill_ellipse(img, cx, cy + r * 0.42, r * 0.60, r * 0.26, base_dark);
    fill_ellipse(img, cx, cy + r * 0.34, r * 0.46, r * 0.12, base_lit);
    // fire button on the base
    fill_circle(img, cx + r * 0.30, cy + r * 0.36, r * 0.085, red);
    fill_circle(img, cx + r * 0.275, cy + r * 0.335, r * 0.03, gloss);

    // shaft (with a soft left highlight for a cylindrical feel)
    fill_rect(img, cx - r * 0.085, cy - r * 0.30, r * 0.17, r * 0.62, shaft);
    fill_rect(img, cx - r * 0.085, cy - r * 0.30, r * 0.05, r * 0.62, shaft_lit);

    // glossy red ball top
    fill_circle(img, cx, cy - r * 0.36, r * 0.27, ball_shadow);
    fill_circle(img, cx - r * 0.02, cy - r * 0.38, r * 0.24, ball);
    fill_circle(img, cx - r * 0.09, cy - r * 0.45, r * 0.085, gloss);
}

fn fill_circle(img: &mut image::RgbaImage, cx: f32, cy: f32, r: f32, c: image::Rgba<u8>) {
    fill_ellipse(img, cx, cy, r, r, c);
}

fn fill_ellipse(img: &mut image::RgbaImage, cx: f32, cy: f32, rx: f32, ry: f32, c: image::Rgba<u8>) {
    if rx <= 0.0 || ry <= 0.0 {
        return;
    }
    let (w, h) = img.dimensions();
    let x0 = (cx - rx).floor().max(0.0) as u32;
    let x1 = (cx + rx).ceil().min(w as f32 - 1.0) as u32;
    let y0 = (cy - ry).floor().max(0.0) as u32;
    let y1 = (cy + ry).ceil().min(h as f32 - 1.0) as u32;
    for y in y0..=y1 {
        for x in x0..=x1 {
            let dx = (x as f32 + 0.5 - cx) / rx;
            let dy = (y as f32 + 0.5 - cy) / ry;
            if dx * dx + dy * dy <= 1.0 {
                img.put_pixel(x, y, c);
            }
        }
    }
}

/// Filled disc shaded radially from `inner` (centre) to `outer` (edge).
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
                img.put_pixel(
                    x,
                    y,
                    image::Rgba([
                        lerp(inner[0], outer[0], t),
                        lerp(inner[1], outer[1], t),
                        lerp(inner[2], outer[2], t),
                        255,
                    ]),
                );
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
    for yy in y0..y1 {
        for xx in x0..x1 {
            img.put_pixel(xx, yy, c);
        }
    }
}

/// Composite a gold five-pointed star into the top-right of a cover bitmap: a
/// "top rated" badge for GB64's 5/5 games. Top-right keeps it clear of the
/// joystick badge in the bottom-right.
fn draw_rating_badge(img: &mut image::RgbaImage) {
    let (w, h) = img.dimensions();
    if w < 16 || h < 16 {
        return;
    }
    let r = (w.min(h) as f32) * 0.16; // star outer radius
    let cx = w as f32 - r * 1.2;
    let cy = r * 1.2;
    let ir = 0.42; // inner/outer radius ratio of a crisp 5-point star
    let outline = image::Rgba([92u8, 60, 0, 255]);
    let gold = image::Rgba([255u8, 186, 10, 255]);
    let sheen = image::Rgba([255u8, 232, 150, 255]);
    fill_star(img, cx, cy, r * 1.15, r * 1.15 * ir, outline);
    fill_star(img, cx, cy, r, r * ir, gold);
    fill_star(img, cx, cy - r * 0.06, r * 0.5, r * 0.5 * ir, sheen);
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

/// Scanline-free filled polygon via per-pixel even-odd test over the bounding box.
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

/// Centered one-line banner (used while a game loads).
fn banner(term: &mut Terminal<CrosstermBackend<std::io::Stdout>>, msg: &str) -> std::io::Result<()> {
    term.draw(|f| {
        let area = f.area();
        let y = area.height / 2;
        let para = Paragraph::new(msg).alignment(Alignment::Center);
        f.render_widget(para, Rect::new(0, y, area.width, 1));
    })?;
    Ok(())
}

/// Draw a centered modal error dialog over the current screen and block until the
/// user presses a key (or clicks). Used when a game fails to launch so the reason
/// (e.g. "no VICE found", "?DEVICE NOT PRESENT") is shown instead of silently
/// bouncing back to the grid.
fn error_dialog(
    term: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    title: &str,
    detail: &str,
) -> std::io::Result<()> {
    term.draw(|f| {
        let area = f.area();
        // Box ~60% wide (clamped), tall enough for the wrapped detail + hint.
        let w = area.width.saturating_mul(3) / 5;
        let w = w.clamp(30.min(area.width), area.width.saturating_sub(2)).max(1);
        let inner_w = w.saturating_sub(4).max(1) as usize;
        // wrap each source line to the inner width to size the box height
        let detail_rows: u16 = detail
            .lines()
            .map(|l| (l.chars().count() / inner_w + 1).max(1) as u16)
            .sum::<u16>()
            .max(1);
        let h = (detail_rows + 4).min(area.height); // title + blank + detail + blank + hint
        let x = (area.width.saturating_sub(w)) / 2;
        let y = (area.height.saturating_sub(h)) / 2;
        let rect = Rect::new(x, y, w, h);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Red).add_modifier(Modifier::BOLD))
            .title(Line::from(format!(" {title} ")));

        let mut body = vec![Line::from("")];
        body.extend(detail.lines().map(|l| Line::from(l.to_string())));
        body.push(Line::from(""));
        body.push(Line::styled(
            "press any key to continue",
            Style::default().add_modifier(Modifier::DIM),
        ));
        let para = Paragraph::new(body)
            .block(block)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true });

        f.render_widget(Clear, rect); // mask the grid behind the dialog
        f.render_widget(para, rect);
    })?;
    // Swallow input until a real key/click, then return so the loop redraws.
    loop {
        match event::read()? {
            Event::Key(k) if k.kind == event::KeyEventKind::Press => break,
            Event::Mouse(m) if matches!(m.kind, MouseEventKind::Down(_)) => break,
            _ => {}
        }
    }
    Ok(())
}

/// Show the control scheme for this launch (joystick vs keyboard, per player) and
/// wait for the player to start (any key / click) or cancel (Esc). Returns true to
/// go ahead and launch. The scheme comes from the same detection c64run uses, so
/// what's shown is what the game will actually get.
fn controls_dialog(
    term: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    game: &str,
) -> std::io::Result<bool> {
    let joystick = crate::run::joystick_present();
    let scheme = crate::run::controls_description(joystick);
    term.draw(|f| {
        let area = f.area();
        let w = area.width.saturating_mul(3) / 5;
        let w = w.clamp(40.min(area.width), area.width.saturating_sub(2)).max(1);
        // blank + one line per player + blank + hint, inside the bordered block.
        let h = (scheme.len() as u16 + 5).min(area.height);
        let x = (area.width.saturating_sub(w)) / 2;
        let y = (area.height.saturating_sub(h)) / 2;
        let rect = Rect::new(x, y, w, h);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::LightGreen).add_modifier(Modifier::BOLD))
            .title(Line::from(format!(" Controls — {game} ")));

        let mut body = vec![Line::from("")];
        for line in &scheme {
            body.push(Line::from(line.clone()));
        }
        body.push(Line::from(""));
        body.push(Line::styled(
            "press any key to start · Esc to cancel",
            Style::default().add_modifier(Modifier::DIM),
        ));
        let para = Paragraph::new(body)
            .block(block)
            .alignment(Alignment::Center)
            .wrap(Wrap { trim: true });

        f.render_widget(Clear, rect); // mask the grid behind the dialog
        f.render_widget(para, rect);
    })?;
    loop {
        match event::read()? {
            Event::Key(k) if k.kind == event::KeyEventKind::Press => {
                return Ok(k.code != KeyCode::Esc);
            }
            Event::Mouse(m) if matches!(m.kind, MouseEventKind::Down(_)) => return Ok(true),
            _ => {}
        }
    }
}

/// Entry point for c64kiosk.
pub fn main(argv: Vec<String>) -> ExitCode {
    let mut runopts: Vec<String> = Vec::new();
    for a in &argv {
        match a.as_str() {
            "-w" | "--warp" => runopts.push("-w".to_string()),
            "-f" | "--fullscreen" => runopts.push("-f".to_string()),
            "-r" | "--real" => runopts.push("-r".to_string()),
            "-h" | "--help" => {
                print!("{HELP}");
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("c64kiosk: unknown option: {other} (try --help)");
                return ExitCode::from(1);
            }
        }
    }

    // First-run bootstrap: build the index into the user data dir if it's missing.
    let index = tui::index_path();
    let empty = std::fs::metadata(&index).map(|m| m.len() == 0).unwrap_or(true);
    if empty {
        if let Err(e) = tui::refresh() {
            eprintln!("c64kiosk: {e}");
            return ExitCode::from(1);
        }
    }

    if !std::io::stdin().is_terminal() {
        eprintln!("c64kiosk: needs an interactive terminal");
        return ExitCode::from(1);
    }

    let rows = tui::load_rows();
    if rows.is_empty() {
        eprintln!("c64kiosk: no games to show (try: c64menu --refresh)");
        return ExitCode::from(1);
    }
    let cidx = crate::cover::load_index();
    // GB64-derived decoration (may download the DB on first use).
    let curation = Curation {
        classics: tui::classic_canons(),
        joystick: tui::joystick_canons(),
        top_rated: tui::top_rated_canons(),
        collections: tui::collections(),
    };
    let picker = Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks());

    // covers per genre row: as many as fit across the width at the target width.
    let cols0 = crossterm::terminal::size().map(|(c, _)| c).unwrap_or(80);
    let topn = (cols0 / TARGET_CW).max(1) as usize;

    let mut state = KioskState::new(rows, cidx, runopts, picker, topn, curation);

    match run_loop(&mut state) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("c64kiosk: terminal error: {e}");
            ExitCode::from(1)
        }
    }
}

fn run_loop(state: &mut KioskState) -> std::io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut term = Terminal::new(backend)?;
    term.hide_cursor()?;

    let result = event_loop(state, &mut term);

    disable_raw_mode()?;
    execute!(term.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    term.show_cursor()?;
    result
}

fn event_loop(
    state: &mut KioskState,
    term: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
) -> std::io::Result<()> {
    loop {
        term.draw(|f| state.render(f))?;
        let chosen: Option<usize> = match event::read()? {
            Event::Key(k)
                if k.kind == event::KeyEventKind::Press || k.kind == event::KeyEventKind::Repeat =>
            {
                match state.mode {
                    Mode::Overview => handle_overview_key(state, k.code),
                    Mode::Genre => handle_genre_key(state, k.code),
                }
            }
            Event::Mouse(m) => match state.mode {
                Mode::Overview => handle_overview_mouse(state, m),
                Mode::Genre => handle_genre_mouse(state, m),
            },
            _ => None,
        };
        match chosen {
            Some(QUIT) => return Ok(()),
            Some(row_idx) => launch(state, term, row_idx)?,
            None => {}
        }
    }
}

// A row index of usize::MAX is the sentinel "quit".
const QUIT: usize = usize::MAX;

fn launch(
    state: &mut KioskState,
    term: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    row_idx: usize,
) -> std::io::Result<()> {
    let row = state.all[row_idx].clone();
    banner(term, &format!("Loading  {} ...", row.title))?;
    match tui::resolve(&row, true) {
        Some(path) => {
            // Describe the controls and let the player start or back out.
            if !controls_dialog(term, &row.title)? {
                return Ok(());
            }
            tui::record_play(&row);
            if let Err(e) = tui::launch_inplace(&path.to_string_lossy(), &state.runopts) {
                error_dialog(term, &format!("Could not start {}", row.title), &e)?;
            }
        }
        None => {
            error_dialog(
                term,
                &format!("Could not load {}", row.title),
                "No matching disk was found or the download failed.",
            )?;
        }
    }
    Ok(())
}

fn handle_overview_key(state: &mut KioskState, code: KeyCode) -> Option<usize> {
    let last = state.ofocus.len().saturating_sub(1);
    match code {
        KeyCode::Char('q') | KeyCode::Esc => return Some(QUIT),
        KeyCode::Right | KeyCode::Down | KeyCode::Tab => {
            state.osel = (state.osel + 1).min(last);
        }
        KeyCode::Left | KeyCode::Up => {
            state.osel = state.osel.saturating_sub(1);
        }
        KeyCode::Enter => return state.activate_overview(),
        _ => {}
    }
    None
}

fn handle_overview_mouse(state: &mut KioskState, m: MouseEvent) -> Option<usize> {
    let last = state.ofocus.len().saturating_sub(1);
    match m.kind {
        MouseEventKind::ScrollUp => state.osel = state.osel.saturating_sub(1),
        MouseEventKind::ScrollDown => state.osel = (state.osel + 1).min(last),
        MouseEventKind::Down(MouseButton::Left) => {
            if let Some(idx) = hit(&state.rects, m.column, m.row) {
                state.osel = idx;
                return state.activate_overview();
            }
        }
        _ => {}
    }
    None
}

fn handle_genre_key(state: &mut KioskState, code: KeyCode) -> Option<usize> {
    let n = state.groups[state.genre].1.len();
    // recompute columns from the current terminal size
    let cols = (crossterm::terminal::size().map(|(c, _)| c).unwrap_or(80) / TARGET_CW).max(1) as usize;
    match code {
        KeyCode::Char('q') => return Some(QUIT),
        KeyCode::Esc => {
            state.mode = Mode::Overview;
        }
        KeyCode::Right => state.sel = (state.sel + 1).min(n.saturating_sub(1)),
        KeyCode::Left => state.sel = state.sel.saturating_sub(1),
        KeyCode::Down => state.sel = (state.sel + cols).min(n.saturating_sub(1)),
        KeyCode::Up => state.sel = state.sel.saturating_sub(cols),
        KeyCode::Enter => return Some(state.groups[state.genre].1[state.sel]),
        _ => {}
    }
    None
}

fn handle_genre_mouse(state: &mut KioskState, m: MouseEvent) -> Option<usize> {
    let n = state.groups[state.genre].1.len();
    let cols = (crossterm::terminal::size().map(|(c, _)| c).unwrap_or(80) / TARGET_CW).max(1) as usize;
    match m.kind {
        MouseEventKind::ScrollUp => state.sel = state.sel.saturating_sub(cols),
        MouseEventKind::ScrollDown => state.sel = (state.sel + cols).min(n.saturating_sub(1)),
        MouseEventKind::Down(MouseButton::Left) => {
            if let Some(idx) = hit(&state.grid_rects, m.column, m.row) {
                return Some(state.groups[state.genre].1[idx]);
            }
        }
        _ => {}
    }
    None
}
