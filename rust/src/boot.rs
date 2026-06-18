// boot - a brief Commodore 64 power-on screen shown before the kiosk/menu open.
// Renders the BASIC V2 banner on the C64's two-tone blue, then "types"
// LOAD"BREADBIN",8,1 + RUN and dissolves into the app. Any key press (or the
// BREADBIN_NO_BOOT env var) skips it. Failures are non-fatal: we just return and
// let the real UI take over the alternate screen.

use std::io::Stdout;
use std::time::{Duration, Instant};

use crossterm::event::{self, Event};
use ratatui::{
    backend::CrosstermBackend,
    layout::Rect,
    style::Style,
    text::Line,
    widgets::{Block, Paragraph},
    Terminal,
};

use crate::core::palette::{BARS, LIGHTBLUE, SCREEN};

type Term = Terminal<CrosstermBackend<Stdout>>;

/// The canonical C64 power-on banner (PAL, 38911 bytes free).
const BANNER: &[&str] = &[
    "",
    "    **** COMMODORE 64 BASIC V2 ****",
    "",
    " 64K RAM SYSTEM  38911 BASIC BYTES FREE",
    "",
];
/// The command the C64 "types" to boot breadbin.
const LOAD_CMD: &str = "LOAD\"BREADBIN\",8,1";
const RUN_CMD: &str = "RUN";

/// Show the boot screen, then return so the caller's event loop takes over.
pub fn boot_screen(term: &mut Term) {
    if std::env::var_os("BREADBIN_NO_BOOT").is_some() {
        return;
    }
    let _ = run(term);
}

/// How many characters of `s` have been "typed" after `elapsed` at `per_char`.
fn typed_prefix(s: &str, elapsed: Duration, per_char: Duration) -> &str {
    let step = per_char.as_millis().max(1);
    let n = (elapsed.as_millis() / step) as usize;
    let end = s.char_indices().nth(n).map(|(i, _)| i).unwrap_or(s.len());
    &s[..end]
}

fn shrink(a: Rect, dx: u16, dy: u16) -> Rect {
    Rect {
        x: a.x + dx.min(a.width / 2),
        y: a.y + dy.min(a.height / 2),
        width: a.width.saturating_sub(dx * 2),
        height: a.height.saturating_sub(dy * 2),
    }
}

/// Run `work` on a background thread while showing the C64 loading screen — a
/// flashing raster-stripe border (the tape-loader look) around a SEARCHING /
/// LOADING text crawl — then return whatever `work` produced. Shown for at least
/// ~0.7s so even an already-local game gets the load-screen beat. Input is
/// drained (ignored) while it runs. If `work` panics, that panic is propagated.
pub fn loading<T, F>(term: &mut Term, title: &str, work: F) -> std::io::Result<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    let handle = std::thread::spawn(work);
    let start = Instant::now();
    let min = Duration::from_millis(700);
    let upper = title.to_uppercase();

    while !(handle.is_finished() && start.elapsed() >= min) {
        let t = start.elapsed();
        // ~30fps frame pacing; drain any input so stray keys don't leak into the app.
        if event::poll(Duration::from_millis(33))? {
            let _ = event::read()?;
        }
        let scroll = (t.as_millis() / 60) as usize;
        let cur = if (t.as_millis() / 300) % 2 == 0 { "█" } else { " " };

        let mut lines: Vec<Line> = vec![Line::raw(""), Line::raw(format!("SEARCHING FOR {upper}"))];
        if t >= Duration::from_millis(450) {
            lines.push(Line::raw("LOADING"));
        }
        lines.push(Line::raw(cur.to_string()));

        term.draw(|f| {
            let area = f.area();
            // Raster stripes fill the whole frame and scroll; the screen masks the
            // middle, leaving moving colour bands in the border — the C64 loader look.
            for y in area.top()..area.bottom() {
                let c = BARS[(y as usize + scroll) % BARS.len()];
                f.render_widget(
                    Block::default().style(Style::default().bg(c)),
                    Rect::new(area.x, y, area.width, 1),
                );
            }
            let screen = shrink(area, 2, 1);
            f.render_widget(Block::default().style(Style::default().bg(SCREEN)), screen);
            let text = shrink(screen, 1, 1);
            f.render_widget(
                Paragraph::new(lines.clone()).style(Style::default().fg(LIGHTBLUE).bg(SCREEN)),
                text,
            );
        })?;
    }
    match handle.join() {
        Ok(value) => Ok(value),
        Err(panic) => std::panic::resume_unwind(panic),
    }
}

fn run(term: &mut Term) -> std::io::Result<()> {
    let start = Instant::now();
    let per_char = Duration::from_millis(45);
    let type_start = Duration::from_millis(650);
    let cmd_done = type_start + per_char * LOAD_CMD.len() as u32;
    let loading_at = cmd_done + Duration::from_millis(300);
    let run_at = loading_at + Duration::from_millis(750);
    let run_done = run_at + per_char * RUN_CMD.len() as u32;
    let finish = run_done + Duration::from_millis(450);

    loop {
        let t = start.elapsed();
        if t >= finish {
            break;
        }
        // ~30fps frame delay that doubles as a skip-on-keypress check.
        if event::poll(Duration::from_millis(33))? {
            if let Event::Key(_) = event::read()? {
                break;
            }
        }

        // The C64 cursor is a solid block that blinks roughly every 0.4s.
        let cur = if (t.as_millis() / 400) % 2 == 0 { "█" } else { " " };

        let mut lines: Vec<Line> = BANNER.iter().map(|s| Line::raw(*s)).collect();
        lines.push(Line::raw("READY."));

        if t < type_start {
            lines.push(Line::raw(cur.to_string()));
        } else {
            let typed = typed_prefix(LOAD_CMD, t - type_start, per_char);
            if t < cmd_done {
                lines.push(Line::raw(format!("{typed}{cur}")));
            } else {
                // RETURN pressed: the command is entered, cursor gone.
                lines.push(Line::raw(typed.to_string()));
                if t >= loading_at {
                    lines.push(Line::raw(""));
                    lines.push(Line::raw("SEARCHING FOR BREADBIN"));
                    lines.push(Line::raw("LOADING"));
                    lines.push(Line::raw("READY."));
                    if t < run_at {
                        lines.push(Line::raw(cur.to_string()));
                    } else {
                        let rtyped = typed_prefix(RUN_CMD, t - run_at, per_char);
                        if t < run_done {
                            lines.push(Line::raw(format!("{rtyped}{cur}")));
                        } else {
                            lines.push(Line::raw(rtyped.to_string()));
                        }
                    }
                }
            }
        }

        term.draw(|f| {
            let area = f.area();
            // Two-tone C64 screen: a light-blue border band around a blue screen.
            f.render_widget(Block::default().style(Style::default().bg(LIGHTBLUE)), area);
            let screen = shrink(area, 2, 1);
            f.render_widget(Block::default().style(Style::default().bg(SCREEN)), screen);
            let text = shrink(screen, 1, 1);
            f.render_widget(
                Paragraph::new(lines.clone()).style(Style::default().fg(LIGHTBLUE).bg(SCREEN)),
                text,
            );
        })?;
    }
    Ok(())
}
