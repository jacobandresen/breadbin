// tunes - the Tunes tab: SID music browser grouped by party with live playback,
// a now-playing card with CSDb details, and the demo-effect visualisers.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

use adw::prelude::*;
use rand::Rng;

use breadbin_core::{
    audio::Audio,
    tunes::{self, SidStatus, Tune, TuneInfo},
};

use crate::task::run_blocking;
use crate::viz::{self, Viz};

type AudioState = Rc<RefCell<Option<Audio>>>;

/// Polite pause between SID availability checks against CSDb.
const CHECK_DELAY: Duration = Duration::from_millis(150);

// ── Now-playing card ──────────────────────────────────────────────────────────

/// The card above the visualiser: screenshot, title, credits and CSDb facts.
#[derive(Clone)]
struct NowPlaying {
    root: gtk::Box,
    shot: gtk::Picture,
    title: gtk::Label,
    credits: gtk::Label,
    event: gtk::Label,
    facts: gtk::Label,
    group: gtk::Label,
    /// Which tune the card is showing, so late info for an older tune is dropped.
    current: Rc<RefCell<u32>>,
}

impl NowPlaying {
    fn new() -> NowPlaying {
        let shot = gtk::Picture::builder()
            .width_request(128)
            .height_request(96)
            .content_fit(gtk::ContentFit::Contain)
            .can_shrink(true)
            .valign(gtk::Align::Start)
            .build();
        shot.add_css_class("np-shot");

        let label = |class: &str, lines: i32| {
            let l = gtk::Label::builder()
                .xalign(0.0)
                .wrap(true)
                .wrap_mode(gtk::pango::WrapMode::WordChar)
                .lines(lines)
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .build();
            l.add_css_class(class);
            l
        };
        let title = label("np-title", 2);
        title.set_label("Pick a tune");
        let credits = label("np-credits", 2);
        let event = label("np-event", 2);
        let facts = label("np-facts", 1);
        let group = label("np-group", 3);

        let text = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(3)
            .hexpand(true)
            .valign(gtk::Align::Center)
            .build();
        for l in [&title, &credits, &event, &facts, &group] {
            text.append(l);
        }

        let root = gtk::Box::builder().spacing(14).build();
        root.add_css_class("np-card");
        root.append(&shot);
        root.append(&text);

        NowPlaying {
            root,
            shot,
            title,
            credits,
            event,
            facts,
            group,
            current: Rc::new(RefCell::new(0)),
        }
    }

    fn clear(&self) {
        *self.current.borrow_mut() = 0;
        self.title.set_label("Pick a tune");
        for l in [&self.credits, &self.event, &self.facts, &self.group] {
            l.set_label("");
        }
        self.shot.set_paintable(None::<&gtk::gdk::Paintable>);
    }

    /// Show what the index knows right away, then fill in CSDb details.
    fn show(&self, t: &Tune) {
        *self.current.borrow_mut() = t.id;
        self.title.set_label(&t.name);
        self.credits.set_label(&t.composer);
        let year = if t.year > 0 { t.year.to_string() } else { String::new() };
        self.event.set_label(&join([t.party.clone(), year]));
        self.facts.set_label(&format!("★ {:.1}", t.rating));
        self.group.set_label(&t.group);
        self.shot.set_paintable(None::<&gtk::gdk::Paintable>);

        let id = t.id;
        let this = self.clone();
        glib::spawn_future_local(async move {
            let fetched = run_blocking(move || {
                let info = tunes::fetch_info(id)?;
                let shot = tunes::ensure_screenshot(id, &info.screenshot_url);
                Ok::<_, String>((info, shot))
            })
            .await;
            if *this.current.borrow() != id {
                return;
            }
            match fetched {
                Ok((info, shot)) => {
                    this.fill(&info);
                    if let Some(path) = shot {
                        this.shot.set_filename(Some(&path));
                    }
                }
                Err(e) => eprintln!("CSDb info for {id}: {e}"),
            }
        });
    }

    fn fill(&self, i: &TuneInfo) {
        let credits: Vec<String> = i
            .musicians
            .iter()
            .map(|m| {
                let mut s = m.handle.clone();
                if !m.real_name.is_empty() {
                    s.push_str(&format!(" ({})", m.real_name));
                }
                if !m.country.is_empty() {
                    s.push_str(&format!(" · {}", m.country));
                }
                s
            })
            .collect();
        if !credits.is_empty() {
            self.credits.set_label(&credits.join("\n"));
        }

        let placing = match (&i.compo, i.year) {
            (Some((compo, place)), _) => format!("#{place} in the {compo} compo"),
            (None, Some(y)) => y.to_string(),
            (None, None) => String::new(),
        };
        self.event.set_label(&join([i.event.clone(), i.event_place.clone(), placing]));

        let model = format!("{} {}", i.sid_model, i.clock).trim().to_string();
        let songs = if i.songs > 1 { format!("{} songs", i.songs) } else { String::new() };
        let downloads = if i.downloads > 0 { format!("{} downloads", i.downloads) } else { String::new() };
        self.facts.set_label(&join([format!("★ {:.2}", i.rating), model, songs, downloads]));

        let group = i.group.as_ref().map(|g| {
            let mut s = g.name.clone();
            if !g.short.is_empty() && g.short != g.name {
                s.push_str(&format!(" ({})", g.short));
            }
            if let Some(y) = g.founded {
                s.push_str(&format!(" · since {y}"));
            }
            if !g.slogan.is_empty() {
                s.push_str(&format!("\n“{}”", g.slogan));
            } else if !g.trivia.is_empty() {
                s.push_str(&format!("\n{}", g.trivia));
            }
            s
        });
        self.group.set_label(group.as_deref().unwrap_or(""));
    }
}

/// Join the non-empty parts with " · ".
fn join<const N: usize>(parts: [String; N]) -> String {
    parts.into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · ")
}

// ── Tune rows ─────────────────────────────────────────────────────────────────

/// Set a row's status marker; only verified tunes can be activated.
fn set_row_status(row: &gtk::ListBoxRow, marker: &gtk::Label, status: Option<SidStatus>) {
    let (text, tip, class) = match status {
        Some(SidStatus::Ready) => ("▶", "Ready to play", "sid-ready"),
        Some(SidStatus::Unavailable) => ("✕", "SID download unavailable", "sid-missing"),
        None => ("…", "Checking the SID download…", "sid-checking"),
    };
    marker.set_label(text);
    marker.set_tooltip_text(Some(tip));
    for c in ["sid-ready", "sid-missing", "sid-checking"] {
        marker.remove_css_class(c);
    }
    marker.add_css_class(class);
    row.set_activatable(status == Some(SidStatus::Ready));
    row.set_sensitive(status != Some(SidStatus::Unavailable));
}

fn tune_row(tune: &Tune) -> (gtk::ListBoxRow, gtk::Label) {
    let marker = gtk::Label::builder().width_chars(2).valign(gtk::Align::Center).build();
    marker.add_css_class("sid-marker");

    let name_lbl = gtk::Label::builder()
        .label(&tune.name)
        .xalign(0.0)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .hexpand(true)
        .build();
    let byline_lbl = gtk::Label::builder()
        .label(join([tune.composer.clone(), tune.group.clone()]))
        .xalign(0.0)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .build();
    byline_lbl.add_css_class("dim-label");
    byline_lbl.add_css_class("caption");

    let meta = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(2)
        .hexpand(true)
        .valign(gtk::Align::Center)
        .build();
    meta.append(&name_lbl);
    meta.append(&byline_lbl);

    let rating_lbl = gtk::Label::builder()
        .label(format!("{:.1}★", tune.rating))
        .valign(gtk::Align::Center)
        .build();
    rating_lbl.add_css_class("caption");

    let hbox = gtk::Box::builder()
        .spacing(10)
        .margin_top(6)
        .margin_bottom(6)
        .margin_start(10)
        .margin_end(12)
        .build();
    hbox.append(&marker);
    hbox.append(&meta);
    hbox.append(&rating_lbl);

    let row = gtk::ListBoxRow::new();
    row.set_child(Some(&hbox));
    (row, marker)
}

// ── Tab ───────────────────────────────────────────────────────────────────────

pub fn build() -> gtk::Widget {
    let audio_state: AudioState = Rc::new(RefCell::new(None));
    // Verified-playable tunes, for radio mode.
    let ready_tunes: Rc<RefCell<Vec<Tune>>> = Rc::new(RefCell::new(Vec::new()));
    let viz_state = Rc::new(RefCell::new(Viz::new()));

    let hpaned = gtk::Paned::builder()
        .orientation(gtk::Orientation::Horizontal)
        .wide_handle(true)
        .vexpand(true)
        .hexpand(true)
        .build();
    hpaned.add_css_class("tunes-paned");

    // ── Left pane: tune list ──────────────────────────────────────────────────
    let left = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .vexpand(true)
        .build();
    left.add_css_class("tunes-list-pane");

    let spinner = gtk::Spinner::new();
    spinner.start();
    spinner.set_margin_top(40);
    spinner.set_halign(gtk::Align::Center);
    left.append(&spinner);

    // ── Right pane: now playing, visualiser, controls ─────────────────────────
    let right = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(10)
        .margin_top(12)
        .margin_bottom(12)
        .margin_start(12)
        .margin_end(12)
        .vexpand(true)
        .build();
    right.add_css_class("tunes-player-pane");

    let now = NowPlaying::new();

    let scope = gtk::DrawingArea::builder().vexpand(true).hexpand(true).build();
    // The draw func paints the whole area, so the monitor bezel lives on a frame around it.
    let scope_bezel = gtk::Box::builder().vexpand(true).hexpand(true).build();
    scope_bezel.add_css_class("scope");
    scope_bezel.set_overflow(gtk::Overflow::Hidden);
    scope_bezel.append(&scope);

    let start_time = std::time::Instant::now();
    let viz_draw = viz_state.clone();
    let title_draw = now.title.clone();
    scope.set_draw_func(move |_, cr, w, h| {
        let t = start_time.elapsed().as_secs_f64();
        viz_draw.borrow_mut().draw(cr, w, h, &title_draw.text(), t);
    });

    let btn_box = gtk::Box::builder().spacing(8).halign(gtk::Align::Center).build();
    let play_pause_btn = gtk::Button::builder().label("Pause").build();
    play_pause_btn.add_css_class("pill");
    let stop_btn = gtk::Button::builder().label("Stop").build();
    stop_btn.add_css_class("pill");
    let skip_btn = gtk::Button::builder().label("⏭ Skip").build();
    skip_btn.add_css_class("pill");
    let radio_btn = gtk::ToggleButton::builder().label("📻 Radio").build();
    radio_btn.add_css_class("pill");
    let visualizer_combo = gtk::DropDown::from_strings(&viz::NAMES);
    visualizer_combo.set_tooltip_text(Some("Visualiser"));

    btn_box.append(&play_pause_btn);
    btn_box.append(&stop_btn);
    btn_box.append(&skip_btn);
    btn_box.append(&radio_btn);

    let viz_combo = viz_state.clone();
    visualizer_combo.connect_selected_notify(move |combo| {
        viz_combo.borrow_mut().set_kind(combo.selected() as usize);
    });

    right.append(&now.root);
    right.append(&scope_bezel);
    right.append(&visualizer_combo);
    right.append(&btn_box);

    // Start a tune: fetch (normally cached) bytes, swap the audio, update the card.
    let play: Rc<dyn Fn(Tune)> = {
        let audio = audio_state.clone();
        let now = now.clone();
        let pp = play_pause_btn.clone();
        Rc::new(move |t: Tune| {
            let (audio, now, pp) = (audio.clone(), now.clone(), pp.clone());
            glib::spawn_future_local(async move {
                let tune = t.clone();
                match run_blocking(move || tunes::ensure_sid(&tune)).await {
                    Ok(bytes) => match Audio::start(bytes, 0) {
                        Ok(a) => {
                            *audio.borrow_mut() = Some(a);
                            now.show(&t);
                            pp.set_label("Pause");
                        }
                        Err(e) => eprintln!("audio error: {e}"),
                    },
                    Err(e) => eprintln!("SID fetch error: {e}"),
                }
            });
        })
    };

    // Radio: skip plays a random verified tune.
    let ready_for_skip = ready_tunes.clone();
    let play_for_skip = play.clone();
    let radio_for_skip = radio_btn.clone();
    skip_btn.connect_clicked(move |_| {
        if !radio_for_skip.is_active() {
            return;
        }
        let pick = {
            let ready = ready_for_skip.borrow();
            (!ready.is_empty())
                .then(|| ready[rand::thread_rng().gen_range(0..ready.len())].clone())
        };
        if let Some(t) = pick {
            play_for_skip(t);
        }
    });

    let skip_for_radio = skip_btn.clone();
    let hpaned_for_radio = hpaned.clone();
    radio_btn.connect_toggled(move |btn| {
        if btn.is_active() {
            hpaned_for_radio.set_position(0); // radio is lean-back: hide the list
            skip_for_radio.emit_clicked();
        } else {
            hpaned_for_radio.set_position(600);
        }
    });

    // ~60 fps: feed the visualiser and redraw.
    let audio_for_timer = audio_state.clone();
    let viz_timer = viz_state.clone();
    let scope_widget = scope.clone();
    glib::timeout_add_local(Duration::from_millis(16), move || {
        let t = start_time.elapsed().as_secs_f64();
        match &*audio_for_timer.borrow() {
            Some(audio) if !audio.is_paused() => {
                let (samples, vis) = (audio.scope(), audio.snapshot());
                viz_timer.borrow_mut().update(Some((&samples, &vis)), t);
            }
            Some(_) => {} // paused: freeze the effect
            None => viz_timer.borrow_mut().update(None, t),
        }
        scope_widget.queue_draw();
        glib::ControlFlow::Continue
    });

    let audio_for_pp = audio_state.clone();
    play_pause_btn.connect_clicked(move |btn| {
        if let Some(ref audio) = *audio_for_pp.borrow() {
            audio.toggle_pause();
            btn.set_label(if audio.is_paused() { "Play" } else { "Pause" });
        }
    });

    let audio_for_stop = audio_state.clone();
    let pp_btn_for_stop = play_pause_btn.clone();
    let now_for_stop = now.clone();
    stop_btn.connect_clicked(move |_| {
        *audio_for_stop.borrow_mut() = None;
        pp_btn_for_stop.set_label("Play");
        now_for_stop.clear();
    });

    // ── Async load tunes ──────────────────────────────────────────────────────
    let left_weak = left.downgrade();
    glib::spawn_future_local(async move {
        let (all_tunes, groups, unavailable) = run_blocking(|| {
            // Diesel DB merged with the TSV index (fills missing SID URLs etc.)
            let tunes = tunes::load_tunes_merged();
            let groups = tunes::group_by_party(&tunes);
            (tunes, groups, tunes::load_unavailable())
        })
        .await;

        let Some(left) = left_weak.upgrade() else {
            return;
        };
        if let Some(child) = left.first_child() {
            left.remove(&child);
        }

        if all_tunes.is_empty() {
            let lbl = gtk::Label::builder()
                .label("No tunes — run Refresh catalogue to build the SID index.")
                .wrap(true)
                .xalign(0.5)
                .margin_top(60)
                .build();
            lbl.add_css_class("dim-label");
            left.append(&lbl);
            return;
        }

        let scrolled = gtk::ScrolledWindow::builder().vexpand(true).hexpand(true).build();
        let listbox = gtk::ListBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .vexpand(true)
            .build();
        listbox.add_css_class("tunes-list");

        let all_tunes = Rc::new(all_tunes);
        // Listbox row position -> tune index (None for party headers).
        let mut row_tune: Vec<Option<usize>> = Vec::new();
        // Tune index -> (row, status marker), for the availability checker.
        let mut rows: Vec<Option<(gtk::ListBoxRow, gtk::Label)>> = vec![None; all_tunes.len()];
        let mut to_check: Vec<(usize, Tune)> = Vec::new();

        for (party_name, idxs) in &groups {
            let header_row = gtk::ListBoxRow::new();
            header_row.set_activatable(false);
            header_row.add_css_class("tunes-party-header");
            let header_label = gtk::Label::builder()
                .label(party_name.to_uppercase())
                .xalign(0.0)
                .build();
            header_row.set_child(Some(&header_label));
            listbox.append(&header_row);
            row_tune.push(None);

            for &idx in idxs {
                let tune = &all_tunes[idx];
                let (row, marker) = tune_row(tune);
                let status = tunes::known_sid_status(tune, &unavailable);
                set_row_status(&row, &marker, status);
                match status {
                    Some(SidStatus::Ready) => ready_tunes.borrow_mut().push(tune.clone()),
                    Some(SidStatus::Unavailable) => {}
                    None => to_check.push((idx, tune.clone())),
                }
                listbox.append(&row);
                rows[idx] = Some((row, marker));
                row_tune.push(Some(idx));
            }
        }

        // Verify unchecked SIDs in list order on a worker thread. Each verified
        // download is cached, so a row only becomes playable once it truly is.
        let (tx, rx) = async_channel::unbounded::<(usize, SidStatus)>();
        std::thread::spawn(move || {
            for (idx, tune) in to_check {
                if tx.send_blocking((idx, tunes::verify_sid(&tune))).is_err() {
                    break; // the tab is gone
                }
                std::thread::sleep(CHECK_DELAY);
            }
        });
        let all_for_check = all_tunes.clone();
        let ready_for_check = ready_tunes.clone();
        glib::spawn_future_local(async move {
            while let Ok((idx, status)) = rx.recv().await {
                if let Some((row, marker)) = &rows[idx] {
                    set_row_status(row, marker, Some(status));
                }
                if status == SidStatus::Ready {
                    ready_for_check.borrow_mut().push(all_for_check[idx].clone());
                }
            }
        });

        // ListBoxRow's "activate" is keybinding-only; clicks arrive as "row-activated".
        let all_for_click = all_tunes.clone();
        listbox.connect_row_activated(move |_, row| {
            if let Some(Some(idx)) = row_tune.get(row.index() as usize).copied() {
                play(all_for_click[idx].clone());
            }
        });

        scrolled.set_child(Some(&listbox));
        left.append(&scrolled);
    });

    hpaned.set_start_child(Some(&left));
    hpaned.set_end_child(Some(&right));
    hpaned.set_position(600);

    hpaned.upcast()
}
