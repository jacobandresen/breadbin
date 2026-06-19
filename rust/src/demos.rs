// c64demos - a demoscene "kiosk" for the Commodore 64, organised by party.
//
// Browses the best C64 demos as ranked by CSDb (the Commodore Scene Database),
// grouped by the party they were released at, with each party showing its top 50
// rated demos across all years. Every demo is shown as a screenshot "cover" with
// its title, group, year and CSDb rating. The colours pay tribute to the classic
// C64 palette and raster-bar look of the scene.
//
//   · click / Enter a party title -> expand it to the full top-10 grid
//   · click / Enter a demo cover   -> download it and run it in VICE in place
//   · arrows move focus, Enter activates, esc backs out / quits, q quits
//
// All scene data and screenshots are cached on disk under the breadbin data dir
// (demos_index.tsv + demo_covers/), so after the first build it works offline and
// loads instantly. Re-fetch with `breadbin demos --refresh`.

use std::collections::HashMap;
use std::io::IsTerminal;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
    Frame, Terminal,
};
use ratatui_image::{
    picker::{cap_parser::QueryStdioOptions, Picker, ProtocolType},
    protocol::StatefulProtocol,
    Resize, StatefulImage,
};

use crate::core;

const HELP: &str = "\
c64demos - browse the best C64 demos of the demoscene, grouped by party.

Usage:
  c64demos                 open the demo kiosk
  c64demos --refresh       re-fetch the ranked demo list from CSDb
  c64demos --limit N       how many top demos to scan when building (default 1000)
  c64demos -w | -r         pass --warp / --real through to the emulator on launch

Data comes from CSDb (https://csdb.dk) and is cached locally so it works offline.
";

// CSDb endpoints.
const TOPLIST: &str = "https://csdb.dk/toplist.php?type=release&subtype=(1)";
fn event_ws(id: u32) -> String {
    format!("https://csdb.dk/webservice/?type=event&id={id}&depth=1")
}

/// How many of the all-time top demos to scan when building the index. The CSDb
/// top-demos toplist holds ~900 ranked releases; scanning them all lets the major
/// parties fill a top-50 section while smaller parties keep whatever they have.
const DEFAULT_LIMIT: usize = 1000;
/// A party needs at least this many top demos to earn its own section.
const MIN_PER_PARTY: usize = 3;
/// Each party section is capped at its best N demos.
const TOP_PER_PARTY: usize = 50;

const TARGET_CW: u16 = 22; // card target width (screenshots are landscape)
const TARGET_CH: u16 = 13; // card target height (cover + 2 title rows)
const OV_SECTION: u16 = 16; // overview rows per party section
const TITLE_H: u16 = 3; // clickable party bar height

// ---- C64 / demoscene palette (Pepto colours) -------------------------------
// Shared across breadbin's UIs; see core::palette.
use crate::core::palette::{BARS, CYAN, LIGHTBLUE, SCREEN, WHITE, YELLOW};

/// One ranked demo from CSDb.
#[derive(Clone)]
struct Demo {
    id: u32,
    name: String,
    group: String,
    rating: f32,
    party: String, // party-series name (year stripped), "" if released outside a party
    year: u32,
    place: u32, // compo placing; 1 = winner, 0 = unknown
    shot: String, // screenshot URL
    // Venue of the party edition this demo was released at, from the CSDb event
    // webservice. Blank for demos released outside a party (or built from an
    // older cache predating these columns).
    event_type: String, // e.g. "C64 Only Party"
    city: String,
    country: String,
    website: String,
}

fn demos_index_path() -> PathBuf {
    core::data_path("demos_index.tsv")
}
fn shots_dir() -> PathBuf {
    core::user_data_dir().join("demo_covers")
}
fn downloads_dir() -> PathBuf {
    core::user_data_dir().join("demos")
}

/// Pull the party event id, name, year, place and screenshot URL out of a
/// depth-1 release webservice XML document. The event id (0 if none) lets us
/// look the party venue up via the event webservice.
fn parse_release_xml(xml: &str) -> (u32, String, u32, u32, String) {
    let released_at = core::between(xml, "<ReleasedAt>", "</ReleasedAt>");
    let event_id = released_at
        .and_then(|b| core::between(b, "<ID>", "</ID>"))
        .and_then(|i| i.trim().parse::<u32>().ok())
        .unwrap_or(0);
    let party = released_at
        .and_then(|b| core::between(b, "<Name>", "</Name>"))
        .map(core::html_unescape)
        .unwrap_or_default();
    let year = core::between(xml, "<ReleaseYear>", "</ReleaseYear>")
        .and_then(|y| y.trim().parse::<u32>().ok())
        .unwrap_or(0);
    let place = core::between(xml, "<Place>", "</Place>")
        .and_then(|p| p.trim().parse::<u32>().ok())
        .unwrap_or(0);
    let shot = core::between(xml, "<ScreenShot>", "</ScreenShot>")
        .map(|s| core::html_unescape(s.trim()))
        .unwrap_or_default();
    (event_id, party, year, place, shot)
}

/// Pull the venue facts (party type, city, country, website) out of a depth-1
/// event webservice XML document.
fn parse_event_xml(xml: &str) -> (String, String, String, String) {
    let field = |tag_open: &str, tag_close: &str| {
        core::between(xml, tag_open, tag_close)
            .map(|s| core::html_unescape(s.trim()))
            .unwrap_or_default()
    };
    (
        field("<EventType>", "</EventType>"),
        field("<City>", "</City>"),
        field("<Country>", "</Country>"),
        field("<Website>", "</Website>"),
    )
}

/// Fetch the ranked top demos from CSDb, decorate the first `limit` with party
/// and screenshot metadata, and write demos_index.tsv. One small webservice call
/// per demo, with a progress bar.
fn build_index(limit: usize) -> Result<(), String> {
    eprintln!("Fetching the CSDb top demos list ...");
    let body = core::fetch(TOPLIST, &[])?;
    let html = String::from_utf8_lossy(&body);
    let ranked = core::parse_toplist(&html);
    if ranked.is_empty() {
        return Err("could not parse the CSDb top demos list".to_string());
    }
    let take = ranked.len().min(limit);
    let mut prog = core::Progress::new("Scanning demos", take as u64);
    let mut out = String::new();
    // Cache venue lookups by event id: a party edition hosts many ranked demos,
    // so we only hit the event webservice once per edition.
    let mut venues: HashMap<u32, (String, String, String, String)> = HashMap::new();
    for (n, (id, name, group, rating)) in ranked.iter().take(take).enumerate() {
        prog.set(n as u64 + 1);
        let (event_id, party, year, place, shot) = match core::fetch(&core::release_ws(*id, 1), &[]) {
            Ok(b) => parse_release_xml(&String::from_utf8_lossy(&b)),
            Err(_) => (0, String::new(), 0, 0, String::new()),
        };
        let (event_type, city, country, website) = if event_id == 0 {
            (String::new(), String::new(), String::new(), String::new())
        } else {
            venues
                .entry(event_id)
                .or_insert_with(|| match core::fetch(&event_ws(event_id), &[]) {
                    Ok(b) => parse_event_xml(&String::from_utf8_lossy(&b)),
                    Err(_) => (String::new(), String::new(), String::new(), String::new()),
                })
                .clone()
        };
        let series = core::party_series(&party);
        out.push_str(&format!(
            "{id}\t{}\t{}\t{:.2}\t{}\t{year}\t{place}\t{}\t{}\t{}\t{}\t{}\n",
            core::clean(name),
            core::clean(group),
            rating,
            core::clean(&series),
            core::clean(&shot),
            core::clean(&event_type),
            core::clean(&city),
            core::clean(&country),
            core::clean(&website),
        ));
    }
    prog.finish();
    std::fs::write(demos_index_path(), out).map_err(|e| e.to_string())?;
    Ok(())
}

/// Load demos_index.tsv into Demo records.
fn load_demos() -> Vec<Demo> {
    let mut v = Vec::new();
    let Ok(text) = std::fs::read_to_string(demos_index_path()) else {
        return v;
    };
    for line in text.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() < 8 {
            continue;
        }
        // Venue columns (8..12) were added later; tolerate older caches that
        // lack them so the kiosk still loads (just without venue info).
        let field = |n: usize| f.get(n).map(|s| s.to_string()).unwrap_or_default();
        v.push(Demo {
            id: f[0].parse().unwrap_or(0),
            name: f[1].to_string(),
            group: f[2].to_string(),
            rating: f[3].parse().unwrap_or(0.0),
            party: f[4].to_string(),
            year: f[5].parse().unwrap_or(0),
            place: f[6].parse().unwrap_or(0),
            shot: f[7].to_string(),
            event_type: field(8),
            city: field(9),
            country: field(10),
            website: field(11),
        });
    }
    v
}

/// Group demos by party series, keeping each party's best `TOP_PER_PARTY` demos.
/// Parties with fewer than `MIN_PER_PARTY` top demos are folded into a trailing
/// "Released Outside Parties" catch-all. Parties are ordered by how many top
/// demos they hosted, then by their best rating.
/// Group demos by party, biggest sections first (the demo kiosk leads with the
/// parties that hosted the most ranked demos). See [`core::group_by_party`].
fn group_by_party(all: &[Demo]) -> Vec<(String, Vec<usize>)> {
    core::group_by_party(
        all,
        |d| d.party.as_str(),
        |d| d.rating,
        MIN_PER_PARTY,
        TOP_PER_PARTY,
        false,
    )
}

/// Cached screenshot path for a demo (downloading once), or None.
fn ensure_shot(d: &Demo) -> Option<PathBuf> {
    if d.shot.is_empty() {
        return None;
    }
    let ext = Path::new(&d.shot)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .filter(|e| matches!(e.as_str(), "png" | "gif" | "jpg" | "jpeg"))
        .unwrap_or_else(|| "png".to_string());
    let cache = shots_dir().join(format!("{}.{ext}", d.id));
    if cache.metadata().map(|m| m.len() > 0).unwrap_or(false) {
        return Some(cache);
    }
    let data = core::fetch(&d.shot, &[]).ok()?;
    if data.len() < 200 {
        return None;
    }
    std::fs::create_dir_all(shots_dir()).ok()?;
    std::fs::write(&cache, data).ok()?;
    Some(cache)
}

// Disk/program image extensions VICE can autostart, best first.
const RUN_EXTS: &[&str] = &[".d64", ".g64", ".d81", ".d71", ".prg", ".crt", ".t64", ".tap"];

/// Download a demo and return a local file VICE can run, caching it under the
/// demos/ data dir. Fetches the depth-2 release XML for its download links,
/// grabs the first HTTP(S) one, and unpacks a disk/program image from the zip.
fn fetch_and_prepare(d: &Demo) -> Result<PathBuf, String> {
    let dir = downloads_dir();
    // Already fetched?
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for e in entries.flatten() {
            let p = e.path();
            let stem_matches = p
                .file_stem()
                .map(|s| s.to_string_lossy().starts_with(&format!("{}_", d.id)))
                .unwrap_or(false);
            if stem_matches && p.metadata().map(|m| m.len() > 0).unwrap_or(false) {
                return Ok(p);
            }
        }
    }
    let body = core::fetch(&core::release_ws(d.id, 2), &[])?;
    let xml = String::from_utf8_lossy(&body);
    // Collect candidate download links; prefer http(s) over ftp (ureq has no ftp).
    let mut links: Vec<String> = Vec::new();
    let mut rest = xml.as_ref();
    while let Some(link) = core::between(rest, "<Link>", "</Link>") {
        links.push(core::html_unescape(link.trim()));
        let adv = rest.find("</Link>").map(|p| p + 7).unwrap_or(rest.len());
        rest = &rest[adv..];
    }
    let link = links
        .iter()
        .find(|l| l.starts_with("http"))
        .ok_or("no downloadable (http) link for this demo on CSDb")?;

    let data = core::fetch(link, &[])?;
    let safe: String = d
        .name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    // Zip archive -> extract the best disk/program image; otherwise save raw.
    if data.starts_with(b"PK") {
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(data))
            .map_err(|e| format!("bad zip: {e}"))?;
        let names: Vec<String> = zip.file_names().map(String::from).collect();
        let member = RUN_EXTS.iter().find_map(|ext| {
            let mut m: Vec<&String> = names
                .iter()
                .filter(|n| n.to_lowercase().ends_with(ext))
                .collect();
            m.sort_by_key(|n| n.len());
            m.first().map(|s| (*s).clone())
        });
        let member = member.ok_or("no runnable disk/program image inside the demo archive")?;
        let ext = Path::new(&member)
            .extension()
            .map(|e| format!(".{}", e.to_string_lossy()))
            .unwrap_or_default();
        let mut blob = Vec::new();
        use std::io::Read;
        zip.by_name(&member)
            .map_err(|e| e.to_string())?
            .read_to_end(&mut blob)
            .map_err(|e| e.to_string())?;
        let out = dir.join(format!("{}_{safe}{ext}", d.id));
        std::fs::write(&out, &blob).map_err(|e| e.to_string())?;
        Ok(out)
    } else {
        let ext = Path::new(link)
            .extension()
            .map(|e| format!(".{}", e.to_string_lossy().to_lowercase()))
            .filter(|e| RUN_EXTS.contains(&e.as_str()))
            .ok_or("the demo download is not a recognised C64 image")?;
        let out = dir.join(format!("{}_{safe}{ext}", d.id));
        std::fs::write(&out, &data).map_err(|e| e.to_string())?;
        Ok(out)
    }
}

// ---- UI --------------------------------------------------------------------
// The overview/grid navigation is shared with the game kiosk; see crate::grid.

use crate::grid::{self, Action, Metrics, Mode};

struct DemoState {
    all: Vec<Demo>,
    groups: Vec<(String, Vec<usize>)>,
    runopts: Vec<String>,
    picker: Picker,

    shot_cache: HashMap<usize, Option<PathBuf>>,
    proto_cache: HashMap<PathBuf, StatefulProtocol>,

    nav: grid::Nav,
}

const METRICS: Metrics = Metrics { ov_section: OV_SECTION, title_h: TITLE_H, target_cw: TARGET_CW };

impl DemoState {
    fn new(all: Vec<Demo>, runopts: Vec<String>, picker: Picker, topn: usize) -> Self {
        let groups = group_by_party(&all);
        let nav = grid::Nav::new(&groups, topn, METRICS);
        Self {
            all,
            groups,
            runopts,
            picker,
            shot_cache: HashMap::new(),
            proto_cache: HashMap::new(),
            nav,
        }
    }

    fn shot_path(&mut self, idx: usize) -> Option<PathBuf> {
        if let Some(c) = self.shot_cache.get(&idx) {
            return c.clone();
        }
        let c = ensure_shot(&self.all[idx]);
        self.shot_cache.insert(idx, c.clone());
        c
    }

    fn ensure_proto(&mut self, path: &PathBuf) -> bool {
        if self.proto_cache.contains_key(path) {
            return true;
        }
        match image::open(path) {
            Ok(img) => {
                self.proto_cache
                    .insert(path.clone(), self.picker.new_resize_protocol(img));
                true
            }
            Err(_) => false,
        }
    }

    /// Draw a demo card: screenshot cover with a two-line title (name, then group
    /// · year · rating) beneath it. A gold star marks compo winners.
    fn draw_card(&mut self, f: &mut Frame, area: Rect, idx: usize, focused: bool) {
        if area.width < 4 || area.height < 4 {
            return;
        }
        let border_col = if focused { YELLOW } else { LIGHTBLUE };
        let block = Block::default()
            .borders(Borders::ALL)
            .border_set(crate::core::PETSCII_BORDER)
            .border_style(
                Style::default()
                    .fg(border_col)
                    .add_modifier(if focused { Modifier::BOLD } else { Modifier::empty() }),
            );
        f.render_widget(block, area);
        let inner = Rect::new(
            area.x + 1,
            area.y + 1,
            area.width.saturating_sub(2),
            area.height.saturating_sub(2),
        );
        if inner.width == 0 || inner.height < 3 {
            return;
        }
        // bottom two rows are the title; the rest is the cover.
        let title_h = 2u16;
        let img_h = inner.height.saturating_sub(title_h);
        let img_area = Rect::new(inner.x, inner.y, inner.width, img_h);
        let d = self.all[idx].clone();
        match self.shot_path(idx) {
            Some(path) if self.ensure_proto(&path) => {
                if let Some(proto) = self.proto_cache.get_mut(&path) {
                    let widget = StatefulImage::default().resize(Resize::Fit(None));
                    f.render_stateful_widget(widget, img_area, proto);
                }
            }
            _ => {
                let label = Paragraph::new("[no shot]")
                    .style(Style::default().fg(LIGHTBLUE).add_modifier(Modifier::DIM))
                    .alignment(Alignment::Center);
                let mid = Rect::new(img_area.x, img_area.y + img_area.height / 2, img_area.width, 1);
                f.render_widget(label, mid);
            }
        }
        // title line 1: name (+ winner star)
        let name = if d.place == 1 {
            format!("\u{2605} {}", d.name)
        } else {
            d.name.clone()
        };
        let l1 = Paragraph::new(Line::from(Span::styled(
            name,
            Style::default()
                .fg(if d.place == 1 { YELLOW } else { WHITE })
                .add_modifier(Modifier::BOLD),
        )))
        .alignment(Alignment::Center);
        f.render_widget(l1, Rect::new(inner.x, inner.y + img_h, inner.width, 1));
        // title line 2: group · year · rating
        let meta = format!("{}  ·  '{:02}  ·  {:.2}", d.group, d.year % 100, d.rating);
        let l2 = Paragraph::new(Line::from(Span::styled(meta, Style::default().fg(CYAN))))
            .alignment(Alignment::Center);
        f.render_widget(l2, Rect::new(inner.x, inner.y + img_h + 1, inner.width, 1));
    }

    fn title_bar(&self, f: &mut Frame, area: Rect, gi: usize, count: usize, focused: bool, hint: &str) {
        let bg = BARS[gi % BARS.len()];
        let mut style = Style::default().fg(WHITE).bg(bg).add_modifier(Modifier::BOLD);
        if focused {
            style = style.add_modifier(Modifier::REVERSED);
        }
        let txt = format!("  {}   ({count} demos)   {hint}", core::spaced_upper(&self.groups[gi].0));
        let lines = vec![Line::from(""), Line::from(txt), Line::from("")];
        f.render_widget(Paragraph::new(lines).style(style), area);
    }

    fn render_overview(&mut self, f: &mut Frame) {
        let area = f.area();
        f.render_widget(Clear, area);
        f.buffer_mut().set_style(area, Style::default().bg(SCREEN));
        let n_groups = self.groups.len();
        let (vis_g, sec_h, card_h, card_w) = self.nav.overview_geometry(area, n_groups);
        self.nav.scroll_overview(area, n_groups);
        self.nav.rects.clear();

        let header = Line::from(vec![
            Span::styled("\u{25b2} BREADBIN DEMOS \u{25b2}", Style::default().fg(YELLOW).add_modifier(Modifier::BOLD)),
            Span::styled(
                format!(
                    "   top C64 demos by party · {} parties · click a cover to run · esc quit",
                    self.groups.len()
                ),
                Style::default().fg(LIGHTBLUE),
            ),
        ]);
        f.buffer_mut().set_line(0, 0, &header, area.width);

        for vi in 0..vis_g {
            let gi = self.nav.otop + vi;
            if gi >= self.groups.len() {
                break;
            }
            let idxs = self.groups[gi].1.clone();
            let base = 1 + vi as u16 * sec_h;
            let title_focus = self.nav.ofocus_index_of_title(gi, &self.groups);
            let bar = Rect::new(0, base, area.width, TITLE_H.min(area.height.saturating_sub(base)));
            self.title_bar(f, bar, gi, idxs.len(), self.nav.osel == title_focus, "click / \u{23ce} to open");
            self.nav.rects.push((bar, title_focus));

            let cards_y = base + TITLE_H;
            if cards_y >= area.height {
                continue;
            }
            let ch = card_h.min(area.height - cards_y);
            for j in 0..self.nav.topn.min(idxs.len()) {
                let x = j as u16 * card_w;
                if x >= area.width {
                    break;
                }
                let w = card_w.min(area.width - x);
                let rect = Rect::new(x, cards_y, w, ch);
                let foc_idx = title_focus + 1 + j;
                self.draw_card(f, rect, idxs[j], self.nav.osel == foc_idx);
                self.nav.rects.push((rect, foc_idx));
            }
        }
    }

    /// Card grid layout for a body region: (cols, visible rows, card w, card h).
    fn grid_geometry(&self, body: Rect) -> (usize, usize, u16, u16) {
        let cols = ((body.width / TARGET_CW).max(1)) as usize;
        let rows = ((body.height / TARGET_CH).max(1)) as usize;
        let card_w = (body.width / cols as u16).max(1);
        let card_h = (body.height / rows as u16).max(1);
        (cols, rows, card_w, card_h)
    }

    /// Summarise a party section for its info panel: a venue/edition headline
    /// (type · location · year span), the website of its most recent edition,
    /// and the distinct groups whose demos rank in this section. Derived from
    /// the section's demos, so it works offline and needs no extra fetches.
    fn party_summary(&self, gi: usize) -> (String, String, Vec<String>) {
        let idxs = &self.groups[gi].1;

        // distinct groups, kept in the section's ranked order.
        let mut groups: Vec<String> = Vec::new();
        for &i in idxs {
            let g = &self.all[i].group;
            if !g.is_empty() && !groups.iter().any(|x| x == g) {
                groups.push(g.clone());
            }
        }

        // year span across the ranked demos.
        let years: Vec<u32> = idxs.iter().map(|&i| self.all[i].year).filter(|&y| y > 0).collect();
        let span = match (years.iter().min(), years.iter().max()) {
            (Some(&lo), Some(&hi)) if lo != hi => format!("{lo}\u{2013}{hi}"),
            (Some(&lo), _) => format!("{lo}"),
            _ => String::new(),
        };

        // representative venue facts: the value seen on the most editions.
        let most_common = |pick: &dyn Fn(&Demo) -> &str| -> String {
            let mut counts: HashMap<&str, usize> = HashMap::new();
            for &i in idxs {
                let v = pick(&self.all[i]);
                if !v.is_empty() {
                    *counts.entry(v).or_default() += 1;
                }
            }
            counts.into_iter().max_by_key(|&(_, c)| c).map(|(v, _)| v.to_string()).unwrap_or_default()
        };
        let etype = most_common(&|d| d.event_type.as_str());
        let city = most_common(&|d| d.city.as_str());
        let country = most_common(&|d| d.country.as_str());
        let location = match (city.is_empty(), country.is_empty()) {
            (false, false) => format!("{city}, {country}"),
            (true, false) => country,
            (false, true) => city,
            _ => String::new(),
        };

        // website of the most recent edition that lists one.
        let website = idxs
            .iter()
            .filter(|&&i| !self.all[i].website.is_empty())
            .max_by_key(|&&i| self.all[i].year)
            .map(|&i| self.all[i].website.clone())
            .unwrap_or_default();

        let headline = [etype, location, span]
            .into_iter()
            .filter(|p| !p.is_empty())
            .collect::<Vec<_>>()
            .join("   \u{00b7}   ");
        (headline, website, groups)
    }

    /// Draw the party info panel (venue + attending groups) starting at row
    /// `y`, returning the number of rows it consumed. Lines that have no data
    /// are skipped, so the "Released Outside Parties" catch-all shrinks to just
    /// its group list.
    fn render_party_info(&self, f: &mut Frame, area: Rect, y: u16) -> u16 {
        let (headline, website, groups) = self.party_summary(self.nav.section);
        let pad = 2u16; // line up with the title bar's leading spaces
        let inner_w = area.width.saturating_sub(pad * 2) as usize;
        let mut lines: Vec<Line> = Vec::new();
        if !headline.is_empty() {
            lines.push(Line::from(Span::styled(
                core::ellipsize(&headline, inner_w),
                Style::default().fg(CYAN).add_modifier(Modifier::BOLD),
            )));
        }
        if !groups.is_empty() {
            lines.push(Line::from(vec![
                Span::styled("Groups: ", Style::default().fg(LIGHTBLUE)),
                Span::styled(
                    core::ellipsize(&groups.join(", "), inner_w.saturating_sub(8)),
                    Style::default().fg(WHITE),
                ),
            ]));
        }
        if !website.is_empty() {
            lines.push(Line::from(vec![
                Span::styled("Web: ", Style::default().fg(LIGHTBLUE)),
                Span::styled(core::ellipsize(&website, inner_w.saturating_sub(5)), Style::default().fg(YELLOW)),
            ]));
        }
        if lines.is_empty() {
            return 0;
        }
        let h = (lines.len() as u16 + 1).min(area.height.saturating_sub(y)); // +1 trailing gap
        let rect = Rect::new(area.x + pad, y, area.width.saturating_sub(pad), h);
        f.render_widget(Paragraph::new(lines), rect);
        h
    }

    fn render_party(&mut self, f: &mut Frame) {
        let area = f.area();
        f.render_widget(Clear, area);
        f.buffer_mut().set_style(area, Style::default().bg(SCREEN));
        let idxs = self.groups[self.nav.section].1.clone();
        let n = idxs.len();

        let bg = BARS[self.nav.section % BARS.len()];
        let title = format!(
            "  {}    (top {n})   click a cover to run · esc back · q quit",
            core::spaced_upper(&self.groups[self.nav.section].0)
        );
        f.render_widget(
            Paragraph::new(Line::from(title)).style(
                Style::default().fg(WHITE).bg(bg).add_modifier(Modifier::BOLD),
            ),
            Rect::new(0, 0, area.width, 1),
        );

        // Venue + attending groups, then the cover grid in the space below.
        let info_h = self.render_party_info(f, area, 1);
        let grid_y = 1 + info_h;
        let grid = Rect::new(0, grid_y, area.width, area.height.saturating_sub(grid_y));
        let (cols, vis_rows, card_w, card_h) = self.grid_geometry(grid);

        let sel_row = self.nav.sel / cols;
        if sel_row < self.nav.top {
            self.nav.top = sel_row;
        } else if sel_row >= self.nav.top + vis_rows {
            self.nav.top = sel_row + 1 - vis_rows;
        }

        self.nav.grid_rects.clear();
        for r in 0..vis_rows {
            for c in 0..cols {
                let idx = (self.nav.top + r) * cols + c;
                if idx >= n {
                    continue;
                }
                let x = c as u16 * card_w;
                let y = grid.y + r as u16 * card_h;
                if x >= area.width || y >= area.height {
                    continue;
                }
                let w = card_w.min(area.width - x);
                let h = card_h.min(area.height - y);
                self.draw_card(f, Rect::new(x, y, w, h), idxs[idx], idx == self.nav.sel);
                self.nav.grid_rects.push((Rect::new(x, y, w, h), idx));
            }
        }
    }

    fn render(&mut self, f: &mut Frame) {
        match self.nav.mode {
            Mode::Overview => self.render_overview(f),
            Mode::Section => self.render_party(f),
        }
    }
}

/// Centered one-line banner over the screen blue, used while a demo loads.
fn banner(term: &mut Terminal<CrosstermBackend<std::io::Stdout>>, msg: &str) -> std::io::Result<()> {
    term.draw(|f| {
        let area = f.area();
        f.buffer_mut().set_style(area, Style::default().bg(SCREEN));
        let y = area.height / 2;
        let para = Paragraph::new(Span::styled(msg, Style::default().fg(YELLOW).add_modifier(Modifier::BOLD)))
            .alignment(Alignment::Center);
        f.render_widget(para, Rect::new(0, y, area.width, 1));
    })?;
    Ok(())
}

fn launch(
    state: &mut DemoState,
    term: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    idx: usize,
) -> std::io::Result<()> {
    let d = state.all[idx].clone();
    banner(term, &format!("Downloading  {}  by {} ...", d.name, d.group))?;
    match fetch_and_prepare(&d) {
        Ok(path) => {
            banner(term, &format!("Loading  {} ...", d.name))?;
            if let Err(e) = crate::tui::launch_inplace(&path.to_string_lossy(), &state.runopts) {
                grid::error_dialog(term, &format!("Could not run {}", d.name), &e)?;
            }
        }
        Err(e) => {
            grid::error_dialog(term, &format!("Could not load {}", d.name), &e)?;
        }
    }
    Ok(())
}

fn event_loop(
    state: &mut DemoState,
    term: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
) -> std::io::Result<()> {
    loop {
        term.draw(|f| state.render(f))?;
        let term_cols = crossterm::terminal::size().map(|(c, _)| c).unwrap_or(80);
        let action = match event::read()? {
            Event::Key(k)
                if k.kind == event::KeyEventKind::Press || k.kind == event::KeyEventKind::Repeat =>
            {
                match state.nav.mode {
                    Mode::Overview => state.nav.overview_key(k.code, &state.groups),
                    Mode::Section => {
                        let items = state.groups[state.nav.section].1.clone();
                        state.nav.section_key(k.code, &items, term_cols)
                    }
                }
            }
            Event::Mouse(m) => match state.nav.mode {
                Mode::Overview => state.nav.overview_mouse(m, &state.groups),
                Mode::Section => {
                    let items = state.groups[state.nav.section].1.clone();
                    state.nav.section_mouse(m, &items, term_cols)
                }
            },
            _ => Action::None,
        };
        match action {
            Action::Quit => return Ok(()),
            Action::Launch(idx) => launch(state, term, idx)?,
            Action::None => {}
        }
    }
}

fn run_loop(all: Vec<Demo>, runopts: Vec<String>, topn: usize) -> std::io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut term = Terminal::new(backend)?;
    term.hide_cursor()?;

    let picker = Picker::from_query_stdio_with_options(QueryStdioOptions {
        blacklist_protocols: vec![ProtocolType::Kitty],
        ..Default::default()
    })
    .unwrap_or_else(|_| Picker::halfblocks());
    let mut state = DemoState::new(all, runopts, picker, topn);

    let result = event_loop(&mut state, &mut term);

    disable_raw_mode()?;
    execute!(term.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    term.show_cursor()?;
    result
}

/// Entry point for c64demos.
pub fn main(argv: Vec<String>) -> ExitCode {
    let mut runopts: Vec<String> = Vec::new();
    let mut refresh = false;
    let mut limit = DEFAULT_LIMIT;
    let mut i = 0;
    while i < argv.len() {
        match argv[i].as_str() {
            "-w" | "--warp" => runopts.push("-w".to_string()),
            "-r" | "--real" => runopts.push("-r".to_string()),
            "-f" | "--fullscreen" => runopts.push("-f".to_string()),
            "--refresh" => refresh = true,
            "--limit" => {
                i += 1;
                limit = argv.get(i).and_then(|s| s.parse().ok()).unwrap_or(DEFAULT_LIMIT);
            }
            "-h" | "--help" => {
                print!("{HELP}");
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("c64demos: unknown option: {other} (try --help)");
                return ExitCode::from(1);
            }
        }
        i += 1;
    }

    let empty = std::fs::metadata(demos_index_path()).map(|m| m.len() == 0).unwrap_or(true);
    if refresh || empty {
        if let Err(e) = build_index(limit) {
            eprintln!("c64demos: {e}");
            return ExitCode::from(1);
        }
    }

    let all = load_demos();
    if all.is_empty() {
        eprintln!("c64demos: no demos to show (try: c64demos --refresh)");
        return ExitCode::from(1);
    }

    if !std::io::stdin().is_terminal() {
        // Non-interactive: just print the party overview so the data is usable
        // from scripts and pipelines.
        for (party, idxs) in group_by_party(&all) {
            println!("== {party} ==");
            for (r, &idx) in idxs.iter().enumerate() {
                let d = &all[idx];
                println!("  {:2}. {:.2}  {}  ({} '{:02})", r + 1, d.rating, d.name, d.group, d.year % 100);
            }
        }
        return ExitCode::SUCCESS;
    }

    let cols0 = crossterm::terminal::size().map(|(c, _)| c).unwrap_or(80);
    let topn = (cols0 / TARGET_CW).max(1) as usize;

    match run_loop(all, runopts, topn) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("c64demos: terminal error: {e}");
            ExitCode::from(1)
        }
    }
}
