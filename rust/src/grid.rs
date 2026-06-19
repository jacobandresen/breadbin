// grid - shared "kiosk" scaffolding for breadbin's cover/screenshot browsers
// (the game kiosk in kiosk.rs and the demo kiosk in demos.rs).
//
// Both present the same two-mode UI: a scrolling *overview* of party/genre
// sections, each showing a top row of cards, and an *expanded* single-section
// grid. The focus model, geometry maths, scrolling and keyboard/mouse handling
// are identical between them — only the cards themselves and what activating one
// launches differ, so those stay in the per-UI modules. Everything common lives
// here as [`Nav`], the navigation state the UIs embed and delegate to.

use crossterm::event::{
    self, Event, KeyCode, KeyEventKind, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::Line,
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
    Terminal,
};

use crate::core;

/// What carries the keyboard/overview cursor in the overview: either a clickable
/// section title or one of the cards in that section's top row.
#[derive(Clone, Copy)]
pub enum OFocus {
    Title(usize),      // section (group) index
    Item(usize, usize), // (section index, slot within the section's top row)
}

/// Which screen the kiosk is showing.
#[derive(PartialEq)]
pub enum Mode {
    Overview,
    Section,
}

/// The outcome of handling one input event, returned to the UI's event loop.
pub enum Action {
    Quit,
    Launch(usize), // launch this item (index into the UI's `all`)
    None,
}

/// Per-UI layout constants the shared geometry depends on.
#[derive(Clone, Copy)]
pub struct Metrics {
    pub ov_section: u16, // overview rows per section
    pub title_h: u16,    // clickable title-bar height
    pub target_cw: u16,  // card target width (drives column count)
}

/// A section list: `(name, item indices)` — the shape both UIs group their
/// catalogue into.
pub type Groups = [(String, Vec<usize>)];

/// The navigation state shared by both kiosks: the overview focus list and
/// cursor, the expanded-section cursor, and the click hit-maps the renderers fill.
pub struct Nav {
    pub topn: usize, // cards per section row in the overview
    pub metrics: Metrics,

    pub ofocus: Vec<OFocus>,
    pub osel: usize, // overview cursor (index into ofocus)
    pub otop: usize, // first visible section in the overview

    pub mode: Mode,
    pub section: usize, // expanded section index
    pub sel: usize,     // expanded-grid cursor (slot within the section)
    pub top: usize,     // first visible grid row

    pub rects: Vec<(Rect, usize)>,      // overview hit-map -> ofocus index
    pub grid_rects: Vec<(Rect, usize)>, // expanded-grid hit-map -> slot index
}

impl Nav {
    /// Build the overview focus list (each section: its title, then its top row of
    /// cards) and start on the first entry.
    pub fn new(groups: &Groups, topn: usize, metrics: Metrics) -> Nav {
        let mut ofocus = Vec::new();
        for (gi, (_, idxs)) in groups.iter().enumerate() {
            ofocus.push(OFocus::Title(gi));
            for j in 0..topn.min(idxs.len()) {
                ofocus.push(OFocus::Item(gi, j));
            }
        }
        Nav {
            topn,
            metrics,
            ofocus,
            osel: 0,
            otop: 0,
            mode: Mode::Overview,
            section: 0,
            sel: 0,
            top: 0,
            rects: Vec::new(),
            grid_rects: Vec::new(),
        }
    }

    /// The section the overview cursor is currently within.
    pub fn focused_section(&self) -> usize {
        match self.ofocus[self.osel] {
            OFocus::Title(gi) | OFocus::Item(gi, _) => gi,
        }
    }

    /// Overview layout for `area`: (visible sections, section height, card height,
    /// card width).
    pub fn overview_geometry(&self, area: Rect, n_groups: usize) -> (usize, u16, u16, u16) {
        let body = area.height.saturating_sub(1);
        let vis_g = ((body / self.metrics.ov_section).max(1) as usize)
            .min(n_groups)
            .max(1);
        let sec_h = body / vis_g as u16;
        let card_h = sec_h.saturating_sub(self.metrics.title_h + 1).max(1);
        let card_w = (area.width / self.topn.max(1) as u16).max(1);
        (vis_g, sec_h, card_h, card_w)
    }

    /// Keep the focused section within the overview's visible window.
    pub fn scroll_overview(&mut self, area: Rect, n_groups: usize) {
        let (vis_g, _, _, _) = self.overview_geometry(area, n_groups);
        let fgi = self.focused_section();
        if fgi < self.otop {
            self.otop = fgi;
        } else if fgi >= self.otop + vis_g {
            self.otop = fgi + 1 - vis_g;
        }
        let max_top = n_groups.saturating_sub(vis_g);
        if self.otop > max_top {
            self.otop = max_top;
        }
    }

    /// The ofocus index of section `gi`'s title (titles appear in section order).
    pub fn ofocus_index_of_title(&self, gi: usize, groups: &Groups) -> usize {
        let mut idx = 0;
        for (g, item) in groups.iter().enumerate() {
            if g == gi {
                return idx;
            }
            idx += 1 + self.topn.min(item.1.len());
        }
        idx
    }

    /// Number of grid columns the expanded section uses at the given terminal width.
    fn grid_cols(&self, term_cols: u16) -> usize {
        (term_cols / self.metrics.target_cw).max(1) as usize
    }

    /// Activate the overview cursor: expand a section (returns [`Action::None`]) or
    /// pick an item to launch.
    fn activate_overview(&mut self, groups: &Groups) -> Action {
        match self.ofocus[self.osel] {
            OFocus::Title(gi) => {
                self.mode = Mode::Section;
                self.section = gi;
                self.sel = 0;
                self.top = 0;
                Action::None
            }
            OFocus::Item(gi, j) => Action::Launch(groups[gi].1[j]),
        }
    }

    pub fn overview_key(&mut self, code: KeyCode, groups: &Groups) -> Action {
        let last = self.ofocus.len().saturating_sub(1);
        match code {
            KeyCode::Char('q') | KeyCode::Esc => return Action::Quit,
            KeyCode::Right | KeyCode::Down | KeyCode::Tab => self.osel = (self.osel + 1).min(last),
            KeyCode::Left | KeyCode::Up => self.osel = self.osel.saturating_sub(1),
            KeyCode::Enter => return self.activate_overview(groups),
            _ => {}
        }
        Action::None
    }

    pub fn overview_mouse(&mut self, m: MouseEvent, groups: &Groups) -> Action {
        let last = self.ofocus.len().saturating_sub(1);
        match m.kind {
            MouseEventKind::ScrollUp => self.osel = self.osel.saturating_sub(1),
            MouseEventKind::ScrollDown => self.osel = (self.osel + 1).min(last),
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(idx) = core::hit(&self.rects, m.column, m.row) {
                    self.osel = idx;
                    return self.activate_overview(groups);
                }
            }
            _ => {}
        }
        Action::None
    }

    /// `items` is the expanded section's item slice; `term_cols` the live width.
    pub fn section_key(&mut self, code: KeyCode, items: &[usize], term_cols: u16) -> Action {
        let n = items.len();
        let cols = self.grid_cols(term_cols);
        match code {
            KeyCode::Char('q') => return Action::Quit,
            KeyCode::Esc => self.mode = Mode::Overview,
            KeyCode::Right => self.sel = (self.sel + 1).min(n.saturating_sub(1)),
            KeyCode::Left => self.sel = self.sel.saturating_sub(1),
            KeyCode::Down => self.sel = (self.sel + cols).min(n.saturating_sub(1)),
            KeyCode::Up => self.sel = self.sel.saturating_sub(cols),
            KeyCode::Enter => return Action::Launch(items[self.sel]),
            _ => {}
        }
        Action::None
    }

    pub fn section_mouse(&mut self, m: MouseEvent, items: &[usize], term_cols: u16) -> Action {
        let n = items.len();
        let cols = self.grid_cols(term_cols);
        match m.kind {
            MouseEventKind::ScrollUp => self.sel = self.sel.saturating_sub(cols),
            MouseEventKind::ScrollDown => self.sel = (self.sel + cols).min(n.saturating_sub(1)),
            MouseEventKind::Down(MouseButton::Left) => {
                if let Some(idx) = core::hit(&self.grid_rects, m.column, m.row) {
                    return Action::Launch(items[idx]);
                }
            }
            _ => {}
        }
        Action::None
    }
}

/// Draw a centered modal error dialog over the current screen and block until the
/// user presses a key (or clicks). Used when a launch fails so the reason is shown
/// instead of silently bouncing back to the grid.
pub fn error_dialog(
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
            .border_set(core::PETSCII_BORDER)
            .border_style(Style::default().fg(core::palette::RED).add_modifier(Modifier::BOLD))
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
    loop {
        match event::read()? {
            Event::Key(k) if k.kind == KeyEventKind::Press => break,
            Event::Mouse(m) if matches!(m.kind, MouseEventKind::Down(_)) => break,
            _ => {}
        }
    }
    Ok(())
}
