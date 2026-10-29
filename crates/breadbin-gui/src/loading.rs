// loading - the start-up screen: a C64 tape-loader splash (flickering raster
// stripes in the border, a blue screen with LOADING messages and a progress bar)
// shown over the window until the Games shelves are ready. Remaining time is
// estimated from how long each phase took on the previous start.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::time::Instant;

use adw::prelude::*;
use gtk::cairo;

use crate::viz::{c64_text, set_rgb, PEPTO};

/// A start-up phase: its key in the timings file and what the screen says.
pub struct Phase {
    pub key: &'static str,
    pub label: &'static str,
    /// Expected seconds when there is no history yet.
    pub default_secs: f64,
}

/// Progress reports, sendable from worker threads.
pub enum Msg {
    /// Start the phase with this key.
    Phase(&'static str),
    /// `done` of `total` steps of the current phase are complete.
    Step(u64, u64),
    /// Everything is ready: save timings and fade out.
    Done,
}

struct State {
    phases: &'static [Phase],
    history: HashMap<String, f64>,
    current: usize,
    phase_start: Instant,
    started: Instant,
    /// Seconds each phase took this run; None for phases that were skipped.
    actual: Vec<Option<f64>>,
    step: Option<(u64, u64)>,
    log: Vec<String>,
    done: bool,
}

impl State {
    fn expected(&self, i: usize) -> f64 {
        let p = &self.phases[i];
        self.history.get(p.key).copied().unwrap_or(p.default_secs)
    }

    /// Seconds left: the current phase extrapolated from its step rate (or its
    /// expected length), plus later phases scaled by how this run compares with
    /// the last one so far.
    fn remaining(&self) -> f64 {
        let in_phase = self.phase_start.elapsed().as_secs_f64();
        let expected_now = self.expected(self.current);
        let current = match self.step {
            Some((done, total)) if done > 0 && total >= done => {
                in_phase * (total - done) as f64 / done as f64
            }
            _ => (expected_now - in_phase).max(expected_now * 0.1),
        };
        let (spent, planned): (f64, f64) = (0..self.current)
            .filter_map(|i| Some((self.actual[i]?, self.expected(i))))
            .fold((0.0, 0.0), |(a, b), (x, y)| (a + x, b + y));
        let pace = if planned > 0.05 { (spent / planned).clamp(0.3, 3.0) } else { 1.0 };
        let later: f64 = (self.current + 1..self.phases.len()).map(|i| self.expected(i)).sum();
        current + later * pace
    }

    fn fraction(&self) -> f64 {
        if self.done {
            return 1.0;
        }
        let elapsed = self.started.elapsed().as_secs_f64();
        (elapsed / (elapsed + self.remaining())).clamp(0.0, 0.99)
    }

    fn advance_to(&mut self, key: &str) {
        let Some(i) = self.phases.iter().position(|p| p.key == key) else {
            return;
        };
        let now = self.phase_start.elapsed().as_secs_f64();
        // Close the current phase and any skipped in between.
        if i <= self.current {
            return;
        }
        self.actual[self.current] = Some(now); // phases in between stay None: skipped
        self.current = i;
        self.phase_start = Instant::now();
        self.step = None;
        self.log.push(self.phases[i].label.to_string());
    }

    fn finish(&mut self) {
        self.actual[self.current] = Some(self.phase_start.elapsed().as_secs_f64());
        self.done = true;
        save_history(self.phases, &self.actual, &self.history);
    }
}

fn history_path() -> std::path::PathBuf {
    breadbin_core::core::data_path("startup_timings.tsv")
}

fn load_history() -> HashMap<String, f64> {
    std::fs::read_to_string(history_path())
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.split_once('\t'))
        .filter_map(|(k, v)| Some((k.to_string(), v.parse().ok()?)))
        .collect()
}

/// Store this run's timings; skipped phases keep their previous value.
fn save_history(phases: &[Phase], actual: &[Option<f64>], old: &HashMap<String, f64>) {
    let out: String = phases
        .iter()
        .zip(actual)
        .filter_map(|(p, secs)| Some((p.key, secs.or_else(|| old.get(p.key).copied())?)))
        .map(|(key, secs)| format!("{key}\t{secs:.3}\n"))
        .collect();
    let _ = std::fs::write(history_path(), out);
}

fn time_left_text(secs: f64) -> String {
    match secs.round() as u64 {
        0..=1 => "ALMOST THERE".into(),
        s @ 2..=90 => format!("ABOUT {s} SECONDS LEFT"),
        s => format!("ABOUT {} MINUTES LEFT", (s + 30) / 60),
    }
}

/// The start-up screen. Put [`Loader::widget`] over the window content and
/// report progress through [`Loader::sender`].
pub struct Loader {
    area: gtk::DrawingArea,
    tx: async_channel::Sender<Msg>,
}

impl Loader {
    /// A loader over `phases`, starting at the phase keyed `first` (earlier
    /// phases are skipped and don't count towards the estimate).
    pub fn new(phases: &'static [Phase], first: &str) -> Loader {
        let start = phases.iter().position(|p| p.key == first).unwrap_or(0);
        let state = Rc::new(RefCell::new(State {
            phases,
            history: load_history(),
            current: start,
            phase_start: Instant::now(),
            started: Instant::now(),
            actual: vec![None; phases.len()],
            step: None,
            log: vec![phases[start].label.to_string()],
            done: false,
        }));

        let area = gtk::DrawingArea::builder().hexpand(true).vexpand(true).build();
        area.set_can_target(true); // swallow clicks while loading
        let draw_state = state.clone();
        let seed = Instant::now();
        area.set_draw_func(move |_, cr, w, h| {
            draw(cr, w as f64, h as f64, &draw_state.borrow(), seed.elapsed().as_secs_f64());
        });
        let tick_state = state.clone();
        area.add_tick_callback(move |a, _| {
            // Fade out once done, then hide for good.
            if tick_state.borrow().done {
                let o = a.opacity() - 0.08;
                a.set_opacity(o.max(0.0));
                if o <= 0.0 {
                    a.set_visible(false);
                    return glib::ControlFlow::Break;
                }
            }
            a.queue_draw();
            glib::ControlFlow::Continue
        });

        let (tx, rx) = async_channel::unbounded::<Msg>();
        glib::spawn_future_local(async move {
            while let Ok(msg) = rx.recv().await {
                let mut s = state.borrow_mut();
                match msg {
                    Msg::Phase(key) => s.advance_to(key),
                    Msg::Step(done, total) => s.step = Some((done, total)),
                    Msg::Done => {
                        s.finish();
                        break;
                    }
                }
            }
        });

        Loader { area, tx }
    }

    pub fn widget(&self) -> &gtk::DrawingArea {
        &self.area
    }

    pub fn sender(&self) -> async_channel::Sender<Msg> {
        self.tx.clone()
    }
}

fn draw(cr: &cairo::Context, w: f64, h: f64, s: &State, t: f64) {
    // Border: tape-loader raster stripes, re-randomised every frame-ish.
    let frame = (t * 25.0) as u64;
    let mut y = 0.0;
    let mut n = frame.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    while y < h {
        n ^= n << 13;
        n ^= n >> 7;
        n ^= n << 17;
        let band = 3.0 + (n % 18) as f64;
        let colour = if s.done { PEPTO[14] } else { PEPTO[[2, 3, 7, 6, 8, 13, 10, 14][(n >> 8) as usize % 8]] };
        set_rgb(cr, colour);
        cr.rectangle(0.0, y, w, band);
        let _ = cr.fill();
        y += band;
    }

    // The screen, 40x25 cells of 8x8 scaled to fit with a border around it.
    let cell = ((w * 0.86) / 320.0).min((h * 0.82) / 200.0).floor().max(1.0) * 8.0;
    let (sw, sh) = (cell * 40.0, cell * 25.0);
    let (ox, oy) = ((w - sw) / 2.0, (h - sh) / 2.0);
    set_rgb(cr, PEPTO[6]);
    cr.rectangle(ox, oy, sw, sh);
    let _ = cr.fill();

    // Sixtyfour's glyphs stand taller than wide, so rows get 1.2 cells.
    let line = |row: f64| oy + cell * 1.2 * (row + 1.0);
    set_rgb(cr, PEPTO[14]);
    let header = "**** BREADBIN C64 KIOSK ****";
    c64_text(cr, ox + (sw - header.len() as f64 * cell) / 2.0, line(1.0), header, cell);
    c64_text(cr, ox + cell, line(3.0), "PRESS PLAY ON TAPE", cell);
    c64_text(cr, ox + cell, line(4.0), "OK", cell);
    c64_text(cr, ox + cell, line(6.0), "LOADING", cell);

    // The phase log, newest last; the current phase blinks a cursor.
    let shown: Vec<&String> = s.log.iter().rev().take(8).collect();
    for (i, msg) in shown.iter().rev().enumerate() {
        let row = 8.0 + i as f64;
        let current = i == shown.len() - 1 && !s.done;
        let mut text = format!("{}{}", msg.to_uppercase(), if current { "..." } else { "" });
        if current {
            match s.step {
                // Big counts as a percentage, so the line stays within 40 columns.
                Some((done, total)) if total > 99 => {
                    text.push_str(&format!(" {}%", done * 100 / total.max(1)));
                }
                Some((done, total)) => text.push_str(&format!(" {done}/{total}")),
                None => {}
            }
        }
        set_rgb(cr, if current { PEPTO[1] } else { PEPTO[14] });
        c64_text(cr, ox + cell, line(row), &text, cell);
    }

    // Progress bar of character blocks, then the estimate.
    let frac = s.fraction();
    let blocks = 38.0;
    let filled = (frac * blocks).floor();
    set_rgb(cr, PEPTO[14]);
    cr.rectangle(ox + cell, line(16.5) - cell * 0.85, blocks * cell, cell);
    let _ = cr.stroke();
    set_rgb(cr, PEPTO[1]);
    cr.rectangle(ox + cell, line(16.5) - cell * 0.85, filled * cell, cell);
    let _ = cr.fill();

    set_rgb(cr, PEPTO[14]);
    let eta = if s.done { "READY.".to_string() } else { time_left_text(s.remaining()) };
    c64_text(cr, ox + cell, line(18.0), &eta, cell);
    if (t * 1.6) as i64 % 2 == 0 {
        cr.rectangle(ox + cell, line(19.0) - cell * 0.85, cell, cell);
        let _ = cr.fill();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static PHASES: [Phase; 3] = [
        Phase { key: "a", label: "A", default_secs: 1.0 },
        Phase { key: "b", label: "B", default_secs: 2.0 },
        Phase { key: "c", label: "C", default_secs: 4.0 },
    ];

    fn state() -> State {
        State {
            phases: &PHASES,
            history: HashMap::new(),
            current: 0,
            phase_start: Instant::now(),
            started: Instant::now(),
            actual: vec![None; 3],
            step: None,
            log: Vec::new(),
            done: false,
        }
    }

    #[test]
    fn estimate_uses_defaults_then_pace_and_steps() {
        let mut s = state();
        // Fresh: the whole plan is ahead.
        assert!((s.remaining() - 7.0).abs() < 0.05);
        // Phase a took twice as long as planned: later phases are scaled 2x.
        s.actual[0] = Some(2.0);
        s.current = 1;
        s.phase_start = Instant::now();
        assert!((s.remaining() - (2.0 + 8.0)).abs() < 0.05);
        // Half the steps of b done instantly: b is nearly finished.
        s.step = Some((5, 10));
        assert!(s.remaining() < 8.1);
    }

    #[test]
    fn skipped_phases_keep_their_history() {
        let mut s = state();
        s.history.insert("b".into(), 9.0);
        s.advance_to("c"); // b skipped
        assert_eq!(s.actual[1], None);
        assert!(s.actual[0].is_some());
        // Only c is left, at its default.
        assert!((s.remaining() - 4.0).abs() < 0.05);
    }

    #[test]
    fn time_left_wording() {
        assert_eq!(time_left_text(0.4), "ALMOST THERE");
        assert_eq!(time_left_text(12.2), "ABOUT 12 SECONDS LEFT");
        assert_eq!(time_left_text(200.0), "ABOUT 3 MINUTES LEFT");
    }
}
