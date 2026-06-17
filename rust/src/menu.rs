// c64menu - master/detail picker for ranked C64 games (ratatui rewrite).
//
// A keyboard- and mouse-driven list. Games are grouped by genre; each genre is a
// collapsible header. Expand a game (-> / Tab) to reveal a detail card with its
// box cover (rendered inline via ratatui-image) and GameBase64 facts. Enter is
// the action: it plays a game you own, or downloads-then-plays one you don't.
//
//   ● in your collection   .  Enter plays it (c64run: LOAD"*",8,1 : RUN)
//   ⬇ downloadable          .  Enter fetches it (c64disk), then plays it.

use std::collections::HashMap;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::process::ExitCode;

use crossterm::{
    event::{
        self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, MouseButton,
        MouseEvent, MouseEventKind,
    },
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
    Frame, Terminal,
};
use ratatui_image::{protocol::StatefulProtocol, picker::Picker, Resize, StatefulImage};
use rusqlite::Connection;

use crate::{info, tui};
use crate::tui::Row;

const HELP: &str = "\
c64menu - master/detail picker for ranked C64 games.

Usage:
  c64menu            open the picker
  c64menu -w|-f|-r   pass --warp / --fullscreen / --real through to c64run
  c64menu --refresh  re-pull popularity scores, re-scan availability, rebuild

Keys:  up/down move . ->/Tab expand . <- collapse . Enter play/get
       type to filter . Backspace edit . Esc clear filter or quit . q quit
";

const DETAIL_X: u16 = 24; // column where the detail text starts (cover sits left of it)
const COVER_W: u16 = 20; // cover width in cells

/// A flat view row: a collapsible genre header, or a game (index into `all`).
enum ViewItem {
    Group { genre: String, count: usize, expanded: bool },
    Game(usize),
}

/// What the input loop decided to do this tick.
enum Action {
    None,
    Quit,
    Chosen(usize), // index into `all`
}

struct MenuState {
    all: Vec<Row>,
    groups: Vec<(String, Vec<usize>)>,
    gopen: std::collections::HashSet<String>,
    filter: String,
    sel: usize,
    top: usize,
    detail_open: bool,
    view: Vec<ViewItem>,
    cidx: HashMap<String, String>,
    con: Connection,
    local_count: usize,
    runopts: Vec<String>,

    // caches keyed by the row's query / cover path
    rec_cache: HashMap<String, Option<info::InfoRecord>>,
    cover_cache: HashMap<String, Option<PathBuf>>,
    proto_cache: HashMap<PathBuf, StatefulProtocol>,
    picker: Picker,

    // screen-row -> view index, rebuilt each frame for mouse clicks
    rowmap: HashMap<u16, usize>,
}

fn trunc(s: &str, n: usize) -> String {
    let count = s.chars().count();
    if count <= n {
        return s.to_string();
    }
    if n == 0 {
        return String::new();
    }
    let mut out: String = s.chars().take(n - 1).collect();
    out.push('…');
    out
}

impl MenuState {
    fn new(
        all: Vec<Row>,
        cidx: HashMap<String, String>,
        con: Connection,
        runopts: Vec<String>,
        picker: Picker,
    ) -> Self {
        let local_count = all.iter().filter(|r| r.is_local()).count();
        let groups = tui::group_by_genre(&all);
        let mut s = Self {
            all,
            groups,
            gopen: std::collections::HashSet::new(),
            filter: String::new(),
            sel: 0,
            top: 0,
            detail_open: false,
            view: Vec::new(),
            cidx,
            con,
            local_count,
            runopts,
            rec_cache: HashMap::new(),
            cover_cache: HashMap::new(),
            proto_cache: HashMap::new(),
            picker,
            rowmap: HashMap::new(),
        };
        s.rebuild();
        s
    }

    /// Rebuild the flat view from the genre groups + the current filter.
    fn rebuild(&mut self) {
        let q = self.filter.to_lowercase();
        let mut view = Vec::new();
        for (genre, idxs) in &self.groups {
            let matched: Vec<usize> = if q.is_empty() {
                idxs.clone()
            } else {
                idxs.iter()
                    .copied()
                    .filter(|&i| self.all[i].title.to_lowercase().contains(&q))
                    .collect()
            };
            if matched.is_empty() {
                continue;
            }
            let expanded = !q.is_empty() || self.gopen.contains(genre);
            view.push(ViewItem::Group {
                genre: genre.clone(),
                count: matched.len(),
                expanded,
            });
            if expanded {
                view.extend(matched.into_iter().map(ViewItem::Game));
            }
        }
        self.view = view;
        if self.sel >= self.view.len() {
            self.sel = self.view.len().saturating_sub(1);
        }
    }

    fn set_filter(&mut self, f: String) {
        self.filter = f;
        self.sel = 0;
        self.top = 0;
        self.detail_open = false;
        self.rebuild();
    }

    fn set_group(&mut self, genre: &str, want_open: bool) {
        if want_open {
            self.gopen.insert(genre.to_string());
        } else {
            self.gopen.remove(genre);
        }
        self.rebuild();
        for (i, it) in self.view.iter().enumerate() {
            if let ViewItem::Group { genre: g, .. } = it {
                if g == genre {
                    self.sel = i;
                    break;
                }
            }
        }
    }

    /// (GB64 record, cover path) for a row, both cached. Returns owned clones so
    /// the caller can keep borrowing `self` (e.g. `self.all`) afterwards.
    fn detail(&mut self, idx: usize) -> (Option<info::InfoRecord>, Option<PathBuf>) {
        let query = self.all[idx].query.clone();
        if !self.rec_cache.contains_key(&query) {
            let gid = info::best_match(&self.con, &query);
            let rec = gid.map(|g| info::record(&self.con, g));
            self.rec_cache.insert(query.clone(), rec);
        }
        if !self.cover_cache.contains_key(&query) {
            let cover = tui::cover_for(&self.all[idx], &self.cidx);
            self.cover_cache.insert(query.clone(), cover);
        }
        let rec = self.rec_cache.get(&query).and_then(|o| o.clone());
        let cover = self.cover_cache.get(&query).and_then(|o| o.clone());
        (rec, cover)
    }

    fn ensure_proto(&mut self, path: &PathBuf) -> bool {
        if self.proto_cache.contains_key(path) {
            return true;
        }
        match image::open(path) {
            Ok(img) => {
                let proto = self.picker.new_resize_protocol(img);
                self.proto_cache.insert(path.clone(), proto);
                true
            }
            Err(_) => false,
        }
    }

    fn group_line(genre: &str, count: usize, expanded: bool, width: u16, selected: bool) -> Line<'static> {
        // A full-width colored bar (white-on-blue, bold, UPPERCASE) so genre headers
        // stand out clearly from the game rows beneath them.
        let caret = if expanded { "▾" } else { "▸" };
        let name = trunc(genre, width.saturating_sub(14) as usize).to_uppercase();
        let text = format!(" {caret} {name}  ({count}) ");
        let pad = (width as usize).saturating_sub(text.chars().count());
        let mut style = Style::default()
            .fg(Color::White)
            .bg(Color::Blue)
            .add_modifier(Modifier::BOLD);
        if selected {
            style = style.add_modifier(Modifier::REVERSED);
        }
        Line::from(Span::styled(format!("{text}{}", " ".repeat(pad)), style))
    }

    fn game_line(row: &Row, width: u16, selected: bool, caret_down: bool) -> Line<'static> {
        let marker = if row.is_local() { "●" } else { "⬇" };
        let caret = if caret_down { "▾" } else { " " };
        let (btn_txt, btn_color) = if row.is_local() {
            ("↵ Play", Color::Green)
        } else {
            ("↵ Get ", Color::Yellow)
        };
        let left_fixed = format!("    {caret}{marker} ");
        let left_w = left_fixed.chars().count();
        let avail = (width as usize).saturating_sub(left_w + 6 + 2).max(1);
        let label = trunc(&row.title, avail);
        let used = left_w + label.chars().count();
        let pad = (width as usize).saturating_sub(used + 6).max(1);
        let mut spans = vec![
            Span::raw(left_fixed),
            Span::raw(label),
            Span::raw(" ".repeat(pad)),
            Span::styled(btn_txt.to_string(), Style::default().fg(btn_color)),
        ];
        if selected {
            for s in &mut spans {
                s.style = s.style.add_modifier(Modifier::REVERSED);
            }
        }
        Line::from(spans)
    }

    /// Detail-card text lines (right of the cover).
    fn detail_text(row: &Row, rec: Option<&info::InfoRecord>, right: usize, max_lines: usize) -> Vec<Line<'static>> {
        let dim = Style::default().add_modifier(Modifier::DIM);
        let bold = Style::default().add_modifier(Modifier::BOLD);
        let mut lines: Vec<Line> = Vec::new();
        match rec {
            Some(rec) => {
                let mut head = row.title.clone();
                if let Some(y) = &rec.year {
                    head.push_str(&format!("  ({y})"));
                }
                lines.push(Line::styled(head, bold));
                let d: HashMap<&str, &str> =
                    rec.rows.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
                let join = |parts: &[Option<String>]| -> Option<String> {
                    let v: Vec<String> = parts.iter().flatten().cloned().collect();
                    if v.is_empty() { None } else { Some(v.join(" · ")) }
                };
                let get = |k: &str| d.get(k).map(|s| s.to_string());
                let music = get("Music").map(|m| format!("♪ {m}"));
                let rating = get("Rating").map(|r| format!("★{r}"));
                let classic = get("Classic").map(|_| "classic".to_string());
                let candidates: Vec<Option<String>> = vec![
                    get("Genre"),
                    join(&[get("Publisher"), get("Developer")]),
                    join(&[get("Programmer"), music]),
                    join(&[get("Players"), rating, classic]),
                    get("Language"),
                ];
                for v in candidates.into_iter().flatten() {
                    lines.push(Line::styled(trunc(&v, right), dim));
                }
                if let Some(note) = &rec.note {
                    if lines.len() < max_lines.saturating_sub(1) {
                        lines.push(Line::raw(""));
                        for w in wrap(note, right) {
                            if lines.len() >= max_lines {
                                break;
                            }
                            lines.push(Line::styled(w, dim));
                        }
                    }
                }
            }
            None => {
                lines.push(Line::styled(row.title.clone(), bold));
                lines.push(Line::styled("(no GameBase64 entry)".to_string(), dim));
            }
        }
        lines.truncate(max_lines);
        lines
    }

    fn render(&mut self, f: &mut Frame) {
        let area = f.area();
        let cols = area.width;
        let lines = area.height;
        if lines < 3 {
            return;
        }
        let body_top = 1u16;
        let body_h = lines.saturating_sub(2); // header (1) + footer (1)

        // Decide the detail card geometry for the selected game.
        let mut card_h: u16 = 0;
        let mut sel_idx: Option<usize> = None;
        if self.detail_open {
            if let Some(ViewItem::Game(i)) = self.view.get(self.sel) {
                sel_idx = Some(*i);
                card_h = body_h.saturating_sub(4).clamp(5, 11);
            }
        }

        let budget = body_h.saturating_sub(card_h).max(1) as usize;
        if self.sel < self.top {
            self.top = self.sel;
        } else if self.sel >= self.top + budget {
            self.top = self.sel + 1 - budget;
        }
        let max_top = self.view.len().saturating_sub(budget);
        if self.top > max_top {
            self.top = max_top;
        }

        // Header.
        let cur = Span::styled(
            format!(" {} ", if self.filter.is_empty() { "type to filter" } else { &self.filter }),
            Style::default().add_modifier(Modifier::REVERSED),
        );
        let header = Line::from(vec![
            Span::styled("C64 ▶ ", Style::default().add_modifier(Modifier::BOLD)),
            cur,
            Span::raw(format!(
                "   {} genres · {} games · ●{} here",
                self.groups.len(),
                self.all.len(),
                self.local_count
            )),
        ]);
        f.buffer_mut().set_line(0, 0, &header, cols);

        // Body.
        self.rowmap.clear();
        let mut y = body_top;
        let end = (self.top + budget).min(self.view.len());
        // collect what to draw first (avoids borrow tangles with the image render)
        let mut card_render: Option<(usize, u16)> = None; // (row index into all, y of card)
        for vi in self.top..end {
            let selected = vi == self.sel;
            let line = match &self.view[vi] {
                ViewItem::Group { genre, count, expanded } => {
                    Self::group_line(genre, *count, *expanded, cols, selected)
                }
                ViewItem::Game(i) => {
                    let caret_down = selected && self.detail_open;
                    Self::game_line(&self.all[*i], cols, selected, caret_down)
                }
            };
            f.buffer_mut().set_line(0, y, &line, cols);
            self.rowmap.insert(y, vi);
            y += 1;
            if selected && card_h > 0 {
                if let ViewItem::Game(i) = &self.view[vi] {
                    card_render = Some((*i, y));
                    for k in 0..card_h {
                        self.rowmap.insert(y + k, vi);
                    }
                    y += card_h;
                }
            }
        }

        // Footer.
        let foot = "↑↓/scroll/click select · → expand · ↵ open/play · type to filter · esc quit";
        let footer = Line::styled(trunc(foot, cols as usize), Style::default().add_modifier(Modifier::DIM));
        f.buffer_mut().set_line(0, lines - 1, &footer, cols);

        // Detail card: cover (left) + GB64 text (right).
        if let (Some(i), Some((_, card_y))) = (sel_idx, card_render) {
            let right = (cols.saturating_sub(DETAIL_X)).max(10) as usize;
            let (rec, cover) = self.detail(i);
            let texts = Self::detail_text(&self.all[i], rec.as_ref(), right, card_h as usize);
            for (k, ln) in texts.iter().enumerate() {
                f.buffer_mut().set_line(DETAIL_X, card_y + k as u16, ln, cols - DETAIL_X);
            }
            let cover_h = card_h.saturating_sub(1).max(1);
            match cover {
                Some(path) if self.ensure_proto(&path) => {
                    let rect = Rect::new(1, card_y, COVER_W.min(DETAIL_X - 2), cover_h);
                    if let Some(proto) = self.proto_cache.get_mut(&path) {
                        let widget = StatefulImage::default().resize(Resize::Fit(None));
                        f.render_stateful_widget(widget, rect, proto);
                    }
                }
                _ => {
                    let ln = Line::styled("[no cover]".to_string(), Style::default().add_modifier(Modifier::DIM));
                    f.buffer_mut().set_line(3, card_y + 1, &ln, DETAIL_X);
                }
            }
        }
    }

    fn move_sel(&mut self, delta: i64) {
        let n = self.view.len() as i64;
        if n == 0 {
            return;
        }
        let mut s = self.sel as i64 + delta;
        s = s.clamp(0, n - 1);
        self.sel = s as usize;
        self.detail_open = false;
    }

    fn handle_key(&mut self, ev: KeyEvent) -> Action {
        match ev.code {
            KeyCode::Char('q') => return Action::Quit,
            KeyCode::Esc => {
                if !self.filter.is_empty() {
                    self.set_filter(String::new());
                } else {
                    return Action::Quit;
                }
            }
            KeyCode::Up => self.move_sel(-1),
            KeyCode::Down => self.move_sel(1),
            KeyCode::Right | KeyCode::Tab => match self.view.get(self.sel) {
                Some(ViewItem::Group { genre, .. }) => {
                    let g = genre.clone();
                    self.set_group(&g, true);
                }
                Some(ViewItem::Game(_)) => self.detail_open = true,
                None => {}
            },
            KeyCode::Left => match self.view.get(self.sel) {
                Some(ViewItem::Game(i)) => {
                    if self.detail_open {
                        self.detail_open = false;
                    } else {
                        let g = self.all[*i].genre_or_other().to_string();
                        self.set_group(&g, false);
                    }
                }
                Some(ViewItem::Group { genre, .. }) => {
                    let g = genre.clone();
                    self.set_group(&g, false);
                }
                None => {}
            },
            KeyCode::Enter => match self.view.get(self.sel) {
                Some(ViewItem::Group { genre, .. }) => {
                    let g = genre.clone();
                    let open = self.gopen.contains(&g);
                    self.set_group(&g, !open);
                }
                Some(ViewItem::Game(i)) => return Action::Chosen(*i),
                None => {}
            },
            KeyCode::Backspace => {
                if !self.filter.is_empty() {
                    let mut f = self.filter.clone();
                    f.pop();
                    self.set_filter(f);
                }
            }
            KeyCode::Char(c) => {
                if !c.is_control() {
                    let mut f = self.filter.clone();
                    f.push(c);
                    self.set_filter(f);
                }
            }
            _ => {}
        }
        Action::None
    }

    fn handle_mouse(&mut self, ev: MouseEvent) {
        match ev.kind {
            MouseEventKind::ScrollUp => self.move_sel(-1),
            MouseEventKind::ScrollDown => self.move_sel(1),
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(&vi) = self.rowmap.get(&ev.row) {
                    self.sel = vi;
                    self.detail_open = false;
                }
            }
            _ => {}
        }
    }
}

/// Entry point for c64menu.
pub fn main(argv: Vec<String>) -> ExitCode {
    let mut runopts: Vec<String> = Vec::new();
    let mut do_refresh = false;
    let mut i = 0;
    while i < argv.len() && argv[i].starts_with('-') {
        match argv[i].as_str() {
            "-w" | "--warp" => runopts.push("-w".to_string()),
            "-f" | "--fullscreen" => runopts.push("-f".to_string()),
            "-r" | "--real" => runopts.push("-r".to_string()),
            "--refresh" => do_refresh = true,
            "-h" | "--help" => {
                print!("{HELP}");
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("c64menu: unknown option: {other} (try --help)");
                return ExitCode::from(1);
            }
        }
        i += 1;
    }

    if do_refresh {
        if let Err(e) = tui::refresh() {
            eprintln!("c64menu: {e}");
            return ExitCode::from(1);
        }
    }
    // First-run bootstrap: build the index into the user data dir if it's missing.
    let index = tui::index_path();
    let empty = std::fs::metadata(&index).map(|m| m.len() == 0).unwrap_or(true);
    if empty {
        if let Err(e) = tui::refresh() {
            eprintln!("c64menu: {e}");
            return ExitCode::from(1);
        }
    }

    if !std::io::stdin().is_terminal() {
        eprintln!("c64menu: needs an interactive terminal");
        return ExitCode::from(1);
    }

    let rows = tui::load_rows();
    if rows.is_empty() {
        eprintln!("c64menu: no games to show (try: c64menu --refresh)");
        return ExitCode::from(1);
    }
    let cidx = crate::cover::load_index();
    let con = info::connect(); // pre-warm (may download the GB64 DB) before the TUI

    // Detect the terminal's image protocol before taking over the screen.
    let picker = Picker::from_query_stdio().unwrap_or_else(|_| Picker::halfblocks());

    let mut state = MenuState::new(rows, cidx, con, runopts, picker);

    let chosen = match run_loop(&mut state) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("c64menu: terminal error: {e}");
            return ExitCode::from(1);
        }
    };

    let Some(idx) = chosen else {
        return ExitCode::SUCCESS;
    };
    let row = state.all[idx].clone();
    let runopts = state.runopts.clone();
    drop(state); // free the DB / image caches before exec

    if row.is_local() {
        return tui::play_exec(&row.target, &runopts, Some(&row));
    }
    // downloadable: fetch then play
    eprintln!("Downloading \"{}\" from the Internet Archive ...", row.title);
    match tui::resolve(&row, false) {
        Some(path) => {
            eprintln!("Done - launching {}", row.title);
            tui::play_exec(&path.to_string_lossy(), &runopts, Some(&row))
        }
        None => {
            let retry = if row.ident.is_empty() {
                format!("Retry:  c64disk --source ia \"{}\"", row.query)
            } else {
                format!("Retry:  c64disk --id {}", row.ident)
            };
            eprintln!(
                "c64menu: download of \"{}\" produced no matching disk. {retry}",
                row.title
            );
            ExitCode::from(1)
        }
    }
}

/// Take over the terminal, run the event loop, restore on exit. Returns the
/// chosen row index, or None if the user quit.
fn run_loop(state: &mut MenuState) -> std::io::Result<Option<usize>> {
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut term = Terminal::new(backend)?;
    term.hide_cursor()?;

    let result = (|| -> std::io::Result<Option<usize>> {
        loop {
            term.draw(|f| state.render(f))?;
            match event::read()? {
                Event::Key(k) if k.kind == event::KeyEventKind::Press || k.kind == event::KeyEventKind::Repeat => {
                    match state.handle_key(k) {
                        Action::Quit => return Ok(None),
                        Action::Chosen(i) => return Ok(Some(i)),
                        Action::None => {}
                    }
                }
                Event::Mouse(m) => state.handle_mouse(m),
                _ => {}
            }
        }
    })();

    disable_raw_mode()?;
    execute!(term.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    term.show_cursor()?;
    result
}

/// Word-wrap `text` to `width` columns (whitespace-collapsing, like textwrap).
fn wrap(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return Vec::new();
    }
    let mut out: Vec<String> = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        if line.is_empty() {
            line.push_str(word);
        } else if line.chars().count() + 1 + word.chars().count() <= width {
            line.push(' ');
            line.push_str(word);
        } else {
            out.push(std::mem::take(&mut line));
            line.push_str(word);
        }
    }
    if !line.is_empty() {
        out.push(line);
    }
    out
}
