// c64kiosk - a cover "kiosk" for C64 games, organised by genre (ratatui rewrite).
//
// Opens on a genre overview: each genre shows a title bar and its top covers.
//   · click / Enter a cover  -> download (if needed) and play that game in place
//   · click / Enter a genre title -> expand the genre into a full grid of covers
// In an expanded genre, pick any cover to play; Esc returns to the overview.
// Arrow keys move the focus, Enter activates, q quits. Games launch straight into
// the emulator and return here on exit, never dropping back to the shell.

use std::collections::HashMap;
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

struct KioskState {
    all: Vec<Row>,
    groups: Vec<(String, Vec<usize>)>,
    cidx: HashMap<String, String>,
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
    ) -> Self {
        let mut groups = tui::group_by_genre(&all);
        // "latest played" synthetic section on top.
        let recent = tui::recent_plays(None);
        if !recent.is_empty() {
            let by_disp: HashMap<&str, usize> =
                all.iter().enumerate().map(|(i, r)| (r.display.as_str(), i)).collect();
            let latest: Vec<usize> = recent
                .iter()
                .filter_map(|d| by_disp.get(d.as_str()).copied())
                .collect();
            if !latest.is_empty() {
                groups.insert(0, (tui::LATEST_GENRE.to_string(), latest));
            }
        }

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

    fn ensure_proto(&mut self, path: &PathBuf) -> bool {
        if self.proto_cache.contains_key(path) {
            return true;
        }
        match image::open(path) {
            Ok(img) => {
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
        match self.cover_path(row_idx) {
            Some(path) if self.ensure_proto(&path) => {
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
    let picker = Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks());

    // covers per genre row: as many as fit across the width at the target width.
    let cols0 = crossterm::terminal::size().map(|(c, _)| c).unwrap_or(80);
    let topn = (cols0 / TARGET_CW).max(1) as usize;

    let mut state = KioskState::new(rows, cidx, runopts, picker, topn);

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
    tui::record_play(&row);
    match tui::resolve(&row, true) {
        Some(path) => {
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
