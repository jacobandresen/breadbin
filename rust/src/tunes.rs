// tunes - a demoscene "jukebox" for the Commodore 64, organised by party.
//
// Browses the best C64 SID music as ranked by CSDb (the Commodore Scene
// Database), grouped by the party it was released at (Fjälldata, X, ...), with
// the highest-rated tunes and parties first.
// Pick a tune and breadbin plays it - in pure Rust, no external player: the SID
// engine in src/sid.rs runs the tune's 6502 code and the reSID chip emulator,
// streaming audio to your sound card via cpal. While it plays, a visualiser
// driven by the live SID voices dances in time with the music.
//
//   · arrows move, Enter plays the focused tune, esc backs out / quits, q quits
//   · while playing: space pauses, n next tune, v cycles visuals, esc/left back
//   · r starts "radio": random tunes with random visuals, looping forever
//     (n skips to the next random tune; r or esc leaves radio)
//
// Scene data is cached on disk under the breadbin data dir (tunes_index.tsv +
// sids/), so after the first build it works offline. Re-fetch with
// `breadbin tunes --refresh`.

use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SizedSample};
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
    text::{Line, Span},
    widgets::{
        canvas::{Canvas, Points},
        Block, Borders, Paragraph,
    },
    Frame, Terminal,
};

use crate::core;
use crate::sid::{Player, Vis, NUM_REGS};

const HELP: &str = "\
c64tunes - play the best C64 SID music of the demoscene, grouped by party.

Usage:
  c64tunes                 open the tune jukebox
  c64tunes --radio         launch straight into radio: random tunes + visuals, looping
  c64tunes --refresh       re-fetch the ranked tune list from CSDb
  c64tunes --limit N       how many top tunes to scan when building (default 600)

Audio is rendered in pure Rust (reSID + a 6502 core) and played via your default
sound device - no sidplayfp or VICE required. Data comes from CSDb (csdb.dk) and
is cached locally so it works offline.
";

// CSDb: subtype (7) of the release toplist is "C64 Music", ranked by rating.
const TOPLIST: &str = "https://csdb.dk/toplist.php?type=release&subtype=(7)";

const DEFAULT_LIMIT: usize = 600;
/// A party needs at least this many top tunes to earn its own section.
const MIN_PER_PARTY: usize = 2;
/// Each party section is capped at this many tunes.
const TOP_PER_PARTY: usize = 30;

// ---- C64 / demoscene palette (Pepto colours) -------------------------------
// Shared across breadbin's UIs; see core::palette.
use crate::core::palette::{BARS, CYAN, GREEN, LIGHTBLUE, ORANGE, RED, SCREEN, WHITE, YELLOW};

/// One ranked tune from CSDb.
#[derive(Clone)]
struct Tune {
    id: u32,
    name: String,
    composer: String,
    group: String,
    party: String, // party-series the tune was released at, "" if none
    rating: f32,
    year: u32,
    sid_url: String,
}

fn tunes_index_path() -> PathBuf {
    core::data_path("tunes_index.tsv")
}
fn sids_dir() -> PathBuf {
    core::user_data_dir().join("sids")
}

/// Pull (composer, party, year, sid download URL) out of a depth-2 release
/// webservice XML document. The composer is the handle credited with "Music";
/// the party is the event it was released at; the SID URL is the first .sid link.
fn parse_release_xml(xml: &str) -> (String, String, u32, String) {
    let year = core::between(xml, "<ReleaseYear>", "</ReleaseYear>")
        .and_then(|y| y.trim().parse::<u32>().ok())
        .unwrap_or(0);

    // The party: the event name under <ReleasedAt>, with its edition year stripped.
    let party = core::between(xml, "<ReleasedAt>", "</ReleasedAt>")
        .and_then(|b| core::between(b, "<Name>", "</Name>"))
        .map(|n| core::party_series(&core::html_unescape(n)))
        .unwrap_or_default();

    // The .sid download link.
    let mut sid_url = String::new();
    let mut rest = xml;
    while let Some(link) = core::between(rest, "<Link>", "</Link>") {
        let url = core::html_unescape(link.trim());
        if url.starts_with("http") && url.to_lowercase().ends_with(".sid") {
            sid_url = url;
            break;
        }
        let adv = rest.find("</Link>").map(|p| p + 7).unwrap_or(rest.len());
        rest = &rest[adv..];
    }

    // The composer: walk each <Credit>..</Credit>, find the one crediting Music,
    // and take its first non-empty <Handle>.
    let mut composer = String::new();
    let mut rest = xml;
    while let Some(open) = rest.find("<Credit>") {
        let after = &rest[open + 8..];
        let block_end = after.find("</Credit>").unwrap_or(after.len());
        let block = &after[..block_end];
        if block.contains("<CreditType>Music") {
            // Handles nest: <Handle><ID>..</ID><Handle>NAME</Handle>...</Handle>.
            // Walk every <Handle> opener and take the first *leaf* one - content
            // with no nested tag - which is the credited handle's display name.
            let mut hrest = block;
            while let Some(h) = core::between(hrest, "<Handle>", "</Handle>") {
                let trimmed = h.trim();
                if !trimmed.is_empty() && !trimmed.contains('<') {
                    composer = core::html_unescape(trimmed);
                    break;
                }
                let adv = hrest.find("<Handle>").map(|p| p + 8).unwrap_or(hrest.len());
                hrest = &hrest[adv..];
            }
            if !composer.is_empty() {
                break;
            }
        }
        rest = &after[block_end..];
    }
    (composer, party, year, sid_url)
}

/// Fetch the ranked top tunes from CSDb, decorate the first `limit` with
/// composer/year/SID-link metadata, and write tunes_index.tsv.
fn build_index(limit: usize) -> Result<(), String> {
    eprintln!("Fetching the CSDb top C64 Music list ...");
    let body = core::fetch(TOPLIST, &[])?;
    let html = String::from_utf8_lossy(&body);
    let ranked = core::parse_toplist(&html);
    if ranked.is_empty() {
        return Err("could not parse the CSDb top music list".to_string());
    }
    let take = ranked.len().min(limit);
    let mut prog = core::Progress::new("Scanning tunes", take as u64);
    let mut out = String::new();
    for (n, (id, name, group, rating)) in ranked.iter().take(take).enumerate() {
        prog.set(n as u64 + 1);
        let (composer, party, year, sid_url) = match core::fetch(&core::release_ws(*id, 2), &[]) {
            Ok(b) => parse_release_xml(&String::from_utf8_lossy(&b)),
            Err(_) => (String::new(), String::new(), 0, String::new()),
        };
        // Skip releases with no downloadable .sid - nothing to play.
        if sid_url.is_empty() {
            continue;
        }
        out.push_str(&format!(
            "{id}\t{}\t{}\t{}\t{:.2}\t{year}\t{}\t{}\n",
            core::clean(name),
            core::clean(&composer),
            core::clean(group),
            rating,
            core::clean(&sid_url),
            core::clean(&party),
        ));
    }
    prog.finish();
    std::fs::write(tunes_index_path(), out).map_err(|e| e.to_string())?;
    Ok(())
}

/// Load tunes_index.tsv into Tune records.
fn load_tunes() -> Vec<Tune> {
    let mut v = Vec::new();
    let Ok(text) = std::fs::read_to_string(tunes_index_path()) else {
        return v;
    };
    for line in text.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() < 8 {
            continue;
        }
        v.push(Tune {
            id: f[0].parse().unwrap_or(0),
            name: f[1].to_string(),
            composer: f[2].to_string(),
            group: f[3].to_string(),
            rating: f[4].parse().unwrap_or(0.0),
            year: f[5].parse().unwrap_or(0),
            sid_url: f[6].to_string(),
            party: f[7].to_string(),
        });
    }
    v
}

/// Group tunes by party, keeping each party's best `TOP_PER_PARTY`. Parties with
/// fewer than `MIN_PER_PARTY` top tunes fold into a trailing "Released Outside
/// Parties" catch-all. Top ranking first: within a party tunes are sorted by
/// rating, and parties are ordered by their best tune's rating (so the party
/// hosting the #1 tune leads), then by how many top tunes they have.
/// Group tunes by party, highest-rated section first (the jukebox leads with the
/// party hosting the #1 tune). See [`core::group_by_party`].
fn group_by_party(all: &[Tune]) -> Vec<(String, Vec<usize>)> {
    core::group_by_party(
        all,
        |t| t.party.as_str(),
        |t| t.rating,
        MIN_PER_PARTY,
        TOP_PER_PARTY,
        true,
    )
}

/// Download a tune's .sid (caching it under sids/) and return the bytes.
fn ensure_sid(t: &Tune) -> Result<Vec<u8>, String> {
    let cache = sids_dir().join(format!("{}.sid", t.id));
    if let Ok(b) = std::fs::read(&cache) {
        if b.len() > 0x7c && (&b[0..4] == b"PSID" || &b[0..4] == b"RSID") {
            return Ok(b);
        }
    }
    if t.sid_url.is_empty() {
        return Err("no SID download link for this tune".into());
    }
    let data = core::fetch(&t.sid_url, &[])?;
    if data.len() < 0x7c || (&data[0..4] != b"PSID" && &data[0..4] != b"RSID") {
        return Err("download was not a SID file".into());
    }
    std::fs::create_dir_all(sids_dir()).ok();
    std::fs::write(&cache, &data).ok();
    Ok(data)
}

// ---- audio: cpal output + a generator thread running the SID engine --------

/// The shared, lock-protected PCM ring the audio callback drains.
type Ring = Arc<Mutex<VecDeque<i16>>>;

/// Number of points in the oscilloscope snapshot.
const SCOPE_PTS: usize = 256;

/// Owns the live playback: the audio stream, the generator thread, and the
/// snapshots the visualiser reads. Dropping it stops the sound.
struct Audio {
    _stream: cpal::Stream,
    stop: Arc<AtomicBool>,
    paused: Arc<AtomicBool>,
    vis: Arc<Mutex<Vis>>,
    scope: Arc<Mutex<Vec<i16>>>,
    handle: Option<JoinHandle<()>>,
}

fn build_stream<T>(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    ring: Ring,
) -> Result<cpal::Stream, String>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = config.channels as usize;
    let err_fn = |e| eprintln!("c64tunes: audio stream error: {e}");
    device
        .build_output_stream(
            config,
            move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                let mut rb = ring.lock().unwrap();
                for frame in data.chunks_mut(channels) {
                    let s = rb.pop_front().unwrap_or(0);
                    let v: T = T::from_sample(s as f32 / 32768.0);
                    for ch in frame.iter_mut() {
                        *ch = v;
                    }
                }
            },
            err_fn,
            None,
        )
        .map_err(|e| e.to_string())
}

impl Audio {
    fn start(sid_bytes: Vec<u8>, song: u16) -> Result<Audio, String> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or("no audio output device found")?;
        let supported = device.default_output_config().map_err(|e| e.to_string())?;
        let sample_format = supported.sample_format();
        let config: cpal::StreamConfig = supported.into();
        let sample_rate = config.sample_rate.0;

        let ring: Ring = Arc::new(Mutex::new(VecDeque::with_capacity(sample_rate as usize)));
        let stream = match sample_format {
            cpal::SampleFormat::F32 => build_stream::<f32>(&device, &config, ring.clone()),
            cpal::SampleFormat::I16 => build_stream::<i16>(&device, &config, ring.clone()),
            cpal::SampleFormat::U16 => build_stream::<u16>(&device, &config, ring.clone()),
            other => Err(format!("unsupported audio sample format: {other:?}")),
        }?;

        let stop = Arc::new(AtomicBool::new(false));
        let paused = Arc::new(AtomicBool::new(false));
        let vis = Arc::new(Mutex::new(Vis {
            regs: [0u8; NUM_REGS],
            frame: 0,
        }));
        let scope = Arc::new(Mutex::new(vec![0i16; SCOPE_PTS]));

        // Keep the ring this full (in samples). Small => the visuals stay tight
        // with what is heard; large enough to avoid underruns on a busy TUI.
        let target = (sample_rate / 12) as usize;
        let frame_len = (sample_rate / 50).max(1) as usize;

        let mut player = Player::new(&sid_bytes, song, sample_rate)?;
        let (gstop, gpause, gvis, gscope, gring) =
            (stop.clone(), paused.clone(), vis.clone(), scope.clone(), ring.clone());
        let handle = std::thread::spawn(move || {
            let mut buf = vec![0i16; frame_len];
            while !gstop.load(Ordering::Relaxed) {
                if gpause.load(Ordering::Relaxed) {
                    std::thread::sleep(Duration::from_millis(15));
                    continue;
                }
                let len = gring.lock().unwrap().len();
                if len < target {
                    let snap = player.render(&mut buf);
                    {
                        let mut rb = gring.lock().unwrap();
                        rb.extend(buf.iter().copied());
                    }
                    // downsample the just-played frame into the scope buffer.
                    {
                        let mut sc = gscope.lock().unwrap();
                        for (i, slot) in sc.iter_mut().enumerate() {
                            let src = i * buf.len() / SCOPE_PTS;
                            *slot = buf[src.min(buf.len() - 1)];
                        }
                    }
                    *gvis.lock().unwrap() = snap;
                } else {
                    std::thread::sleep(Duration::from_millis(2));
                }
            }
        });

        stream.play().map_err(|e| e.to_string())?;
        Ok(Audio {
            _stream: stream,
            stop,
            paused,
            vis,
            scope,
            handle: Some(handle),
        })
    }

    fn toggle_pause(&self) {
        let now = !self.paused.load(Ordering::Relaxed);
        self.paused.store(now, Ordering::Relaxed);
    }
    fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }
    fn snapshot(&self) -> Vis {
        self.vis.lock().unwrap().clone()
    }
    fn scope(&self) -> Vec<i16> {
        self.scope.lock().unwrap().clone()
    }
}

impl Drop for Audio {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

// ---- UI --------------------------------------------------------------------

#[derive(Clone, Copy)]
enum Row {
    Header(usize),      // a party section header (group index)
    Tune(usize, usize), // (group index, slot within group)
}

/// Which visualiser the player screen is showing. Cycle with `v`.
#[derive(Clone, Copy, PartialEq)]
enum VisMode {
    Scope,
    Fireball,
    Cubes,
}

impl VisMode {
    fn next(self) -> VisMode {
        match self {
            VisMode::Scope => VisMode::Fireball,
            VisMode::Fireball => VisMode::Cubes,
            VisMode::Cubes => VisMode::Scope,
        }
    }
    fn label(self) -> &'static str {
        match self {
            VisMode::Scope => "scope",
            VisMode::Fireball => "fireball",
            VisMode::Cubes => "cubes",
        }
    }
    /// Pick a visualiser at random (used by radio mode).
    fn random(rng: &mut Rng) -> VisMode {
        match rng.below(3) {
            0 => VisMode::Scope,
            1 => VisMode::Fireball,
            _ => VisMode::Cubes,
        }
    }
}

/// Tiny xorshift64 RNG - the crate tree has no `rand`, and radio mode only needs
/// cheap, unbiased-enough picks for which tune and visual to show next.
struct Rng(u64);

impl Rng {
    fn new() -> Rng {
        let seed = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9e3779b97f4a7c15);
        Rng(seed | 1) // never seed with 0 (xorshift would stick there)
    }
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    /// A value in 0..n (n must be > 0).
    fn below(&mut self, n: usize) -> usize {
        (self.next_u64() % n as u64) as usize
    }
}

/// How long radio lingers on one tune (in seconds of playback) before it picks
/// a fresh random tune and visual. SID tunes generally loop forever, so radio
/// advances on a timer rather than waiting for an end that never comes.
const RADIO_SECS: u64 = 90;

struct TunesState {
    all: Vec<Tune>,
    groups: Vec<(String, Vec<usize>)>,
    rows: Vec<Row>,
    sel: usize,
    top: usize,
    rects: Vec<(Rect, usize)>,

    audio: Option<Audio>,
    now: Option<usize>, // index into `all` of the playing tune
    mode: VisMode,
    status: String,

    radio: bool, // auto-advancing "radio" mode: random tunes + random visuals
    rng: Rng,
}

impl TunesState {
    fn new(all: Vec<Tune>) -> Self {
        let groups = group_by_party(&all);
        let mut rows = Vec::new();
        for (gi, (_, idxs)) in groups.iter().enumerate() {
            rows.push(Row::Header(gi));
            for j in 0..idxs.len() {
                rows.push(Row::Tune(gi, j));
            }
        }
        TunesState {
            all,
            groups,
            rows,
            sel: 0,
            top: 0,
            rects: Vec::new(),
            audio: None,
            now: None,
            mode: VisMode::Scope,
            status: String::new(),
            radio: false,
            rng: Rng::new(),
        }
    }

    fn tune_at(&self, row: usize) -> Option<usize> {
        match self.rows.get(row)? {
            Row::Tune(gi, j) => Some(self.groups[*gi].1[*j]),
            Row::Header(_) => None,
        }
    }

    /// Start playing tune `idx` (into `all`). Replaces any current playback.
    fn play(&mut self, idx: usize) {
        self.audio = None; // stop current first
        let t = self.all[idx].clone();
        self.status = format!("Loading {} ...", t.name);
        match ensure_sid(&t).and_then(|b| Audio::start(b, 1)) {
            Ok(a) => {
                self.audio = Some(a);
                self.now = Some(idx);
                self.status.clear();
            }
            Err(e) => {
                self.now = None;
                self.status = format!("Could not play {}: {e}", t.name);
            }
        }
    }

    /// Play the next tune after the current one within the row list.
    fn play_next(&mut self) {
        let Some(cur) = self.now else { return };
        let pos = self
            .rows
            .iter()
            .position(|r| matches!(r, Row::Tune(gi, j) if self.groups[*gi].1[*j] == cur));
        if let Some(p) = pos {
            for r in (p + 1)..self.rows.len() {
                if let Some(idx) = self.tune_at(r) {
                    self.sel = r;
                    self.play(idx);
                    return;
                }
            }
        }
    }

    /// Move the browser selection onto the row showing tune `idx`, if it has one
    /// (a randomly picked tune may fall outside the truncated party lists).
    fn select_tune(&mut self, idx: usize) {
        if let Some(p) = self
            .rows
            .iter()
            .position(|r| matches!(r, Row::Tune(gi, j) if self.groups[*gi].1[*j] == idx))
        {
            self.sel = p;
        }
    }

    /// Radio: pick a random tune and a random visual and play it. Tries a few
    /// tunes so a single bad download doesn't stall the stream; if nothing plays
    /// it drops out of radio with a message.
    fn radio_next(&mut self) {
        self.mode = VisMode::random(&mut self.rng);
        for _ in 0..12 {
            let idx = self.rng.below(self.all.len());
            self.play(idx);
            if self.now.is_some() {
                self.select_tune(idx);
                return;
            }
        }
        self.radio = false;
        self.status = "Radio: could not find a playable tune".to_string();
    }

    /// True once the current radio tune has played long enough to move on (or if
    /// playback has stopped), so the event loop should pick the next one.
    fn radio_should_advance(&self) -> bool {
        match self.audio.as_ref() {
            Some(a) => a.snapshot().frame / 50 >= RADIO_SECS,
            None => true,
        }
    }

    fn visible_rows(&self, area: Rect) -> usize {
        area.height.saturating_sub(2) as usize
    }

    fn scroll(&mut self, area: Rect) {
        let vis = self.visible_rows(area).max(1);
        if self.sel < self.top {
            self.top = self.sel;
        } else if self.sel >= self.top + vis {
            self.top = self.sel + 1 - vis;
        }
        let max_top = self.rows.len().saturating_sub(vis);
        if self.top > max_top {
            self.top = max_top;
        }
    }

    fn render_browse(&mut self, f: &mut Frame) {
        let area = f.area();
        f.buffer_mut().set_style(area, Style::default().bg(SCREEN));
        self.scroll(area);
        self.rects.clear();

        let header = Line::from(vec![
            Span::styled(
                "\u{266a} BREADBIN TUNES \u{266a}",
                Style::default().fg(YELLOW).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                format!(
                    "   top C64 SID music by party · {} parties · Enter to play · r radio · q quit",
                    self.groups.len()
                ),
                Style::default().fg(LIGHTBLUE),
            ),
        ]);
        f.buffer_mut().set_line(0, 0, &header, area.width);

        let vis = self.visible_rows(area);
        for vi in 0..vis {
            let ri = self.top + vi;
            if ri >= self.rows.len() {
                break;
            }
            let y = 1 + vi as u16;
            let selected = ri == self.sel;
            match self.rows[ri] {
                Row::Header(gi) => {
                    let bg = BARS[gi % BARS.len()];
                    let txt = format!(
                        " {}  ({} tunes)",
                        self.groups[gi].0.to_uppercase(),
                        self.groups[gi].1.len()
                    );
                    let mut st = Style::default().fg(WHITE).bg(bg).add_modifier(Modifier::BOLD);
                    if selected {
                        st = st.add_modifier(Modifier::REVERSED);
                    }
                    f.render_widget(Paragraph::new(Line::from(txt)).style(st), Rect::new(0, y, area.width, 1));
                }
                Row::Tune(gi, j) => {
                    let idx = self.groups[gi].1[j];
                    let t = &self.all[idx];
                    let playing = self.now == Some(idx);
                    let marker = if playing { "\u{25b6} " } else { "  " };
                    let star = if t.rating >= 9.5 { "\u{2605}" } else { " " };
                    let meta = format!(
                        "{marker}{star} {:<34} {:>5.2}  {}  '{:02}",
                        core::ellipsize(&t.name, 34),
                        t.rating,
                        core::ellipsize(&t.group, 18),
                        t.year % 100
                    );
                    let mut st = Style::default().fg(if playing { YELLOW } else { WHITE });
                    if selected {
                        st = st.fg(SCREEN).bg(CYAN).add_modifier(Modifier::BOLD);
                    }
                    f.render_widget(Paragraph::new(Line::from(meta)).style(st), Rect::new(0, y, area.width, 1));
                }
            }
            self.rects.push((Rect::new(0, y, area.width, 1), ri));
        }

        if !self.status.is_empty() {
            let s = Paragraph::new(Span::styled(
                self.status.clone(),
                Style::default().fg(RED).add_modifier(Modifier::BOLD),
            ));
            f.render_widget(s, Rect::new(0, area.height.saturating_sub(1), area.width, 1));
        }
    }

    fn render_player(&mut self, f: &mut Frame) {
        let area = f.area();
        f.buffer_mut().set_style(area, Style::default().bg(SCREEN));
        let Some(idx) = self.now else { return };
        let t = self.all[idx].clone();
        let vis = self.audio.as_ref().map(|a| a.snapshot()).unwrap_or(Vis {
            regs: [0; NUM_REGS],
            frame: 0,
        });
        let scope = self.audio.as_ref().map(|a| a.scope()).unwrap_or_default();
        let paused = self.audio.as_ref().map(|a| a.is_paused()).unwrap_or(false);

        // Title bar with raster-bar colouring that drifts with the music.
        let phase = (vis.frame / 4) as usize;
        let bg = BARS[phase % BARS.len()];
        let secs = vis.frame / 50;
        let by = if t.composer.is_empty() { t.group.as_str() } else { t.composer.as_str() };
        let title = format!(
            "  {} {}   by {}   ({} '{:02})   {:02}:{:02}   [{}]{}",
            if self.radio { "\u{1f4fb}" } else { "\u{266a}" },
            t.name,
            by,
            t.group,
            t.year % 100,
            secs / 60,
            secs % 60,
            self.mode.label(),
            if paused { "   [PAUSED]" } else if self.radio { "   [RADIO]" } else { "" },
        );
        f.render_widget(
            Paragraph::new(Line::from(title)).style(Style::default().fg(WHITE).bg(bg).add_modifier(Modifier::BOLD)),
            Rect::new(0, 0, area.width, 1),
        );

        // Layout: oscilloscope on top, voice meters below, help at the bottom.
        let body_y = 2u16;
        let voices_h = 4u16;
        let body_h = area.height.saturating_sub(body_y + voices_h + 1);
        if body_h >= 3 {
            let body = Rect::new(1, body_y, area.width.saturating_sub(2), body_h);
            match self.mode {
                VisMode::Scope => self.draw_scope(f, body, &scope, phase),
                VisMode::Fireball => self.draw_fireball(f, body, &vis, &scope),
                VisMode::Cubes => self.draw_cubes(f, body, &vis, &scope),
            }
        }
        self.draw_voices(
            f,
            Rect::new(1, area.height.saturating_sub(voices_h + 1), area.width.saturating_sub(2), voices_h),
            &vis,
        );

        let help = Paragraph::new(Span::styled(
            "space pause · n next · v visual · r radio · \u{2190}/esc back to list · q quit",
            Style::default().fg(LIGHTBLUE),
        ))
        .alignment(Alignment::Center);
        f.render_widget(help, Rect::new(0, area.height.saturating_sub(1), area.width, 1));
    }

    /// Oscilloscope of the live audio, drawn as a braille line in a bordered box.
    fn draw_scope(&self, f: &mut Frame, area: Rect, scope: &[i16], phase: usize) {
        let col = BARS[(phase / 3) % BARS.len()];
        let block = Block::default()
            .borders(Borders::ALL)
            .border_set(crate::core::PETSCII_BORDER)
            .border_style(Style::default().fg(LIGHTBLUE))
            .title(Span::styled(" oscilloscope ", Style::default().fg(CYAN)));
        let inner = block.inner(area);
        f.render_widget(block, area);
        if scope.is_empty() || inner.width < 2 || inner.height < 2 {
            return;
        }
        let w = inner.width as f64;
        let pts: Vec<(f64, f64)> = (0..scope.len())
            .map(|i| {
                let x = i as f64 / scope.len() as f64 * w;
                let y = (scope[i] as f64 / 32768.0).clamp(-1.0, 1.0);
                (x, y)
            })
            .collect();
        let canvas = Canvas::default()
            .x_bounds([0.0, w])
            .y_bounds([-1.0, 1.0])
            .marker(ratatui::symbols::Marker::Braille)
            .paint(move |ctx| {
                ctx.draw(&Points { coords: &pts, color: col });
            });
        f.render_widget(canvas, inner);
    }

    /// Three voice meters: a colour-by-waveform bar whose height tracks the
    /// voice's note level, with its pitch shown as a moving marker beneath.
    fn draw_voices(&self, f: &mut Frame, area: Rect, vis: &Vis) {
        if area.width < 9 || area.height < 1 {
            return;
        }
        let cw = area.width / 3;
        let barw = cw.saturating_sub(2) as usize;
        for v in 0..3usize {
            let gate = vis.voice_gate(v);
            let wave = vis.voice_wave(v);
            let color = wave_color(wave);
            let level = if gate { vis.voice_sustain(v) } else { 0 };
            let cells = (level as usize * barw) / 15;
            let bar: String = "█".repeat(cells);
            let x = area.x + v as u16 * cw;

            let label = format!("V{}: {}", v + 1, wave_name(wave));
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    label,
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ))),
                Rect::new(x, area.y, cw, 1),
            );
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(bar, Style::default().fg(color)))),
                Rect::new(x, area.y + 1, cw, 1),
            );
            // pitch as a position along the meter width.
            let pitch = ((vis.voice_freq(v) * barw as f32) as usize).min(barw.saturating_sub(1));
            let mut pr = " ".repeat(pitch);
            pr.push('\u{25c6}');
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(pr, Style::default().fg(color).add_modifier(Modifier::DIM)))),
                Rect::new(x, area.y + 2, cw, 1),
            );
        }
    }

    /// A pulsing fireball: a turbulent radial flame whose size breathes with the
    /// master volume and whose edge flickers with the live audio energy. Cells
    /// are coloured along a black→red→orange→yellow→white heat ramp.
    fn draw_fireball(&self, f: &mut Frame, area: Rect, vis: &Vis, scope: &[i16]) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_set(crate::core::PETSCII_BORDER)
            .border_style(Style::default().fg(RED))
            .title(Span::styled(" fireball ", Style::default().fg(ORANGE)));
        let inner = block.inner(area);
        f.render_widget(block, area);
        if inner.width < 4 || inner.height < 3 {
            return;
        }
        // Square the aspect: terminal cells are ~twice as tall as wide.
        let asp = inner.width as f64 / (inner.height as f64 * 2.0);
        let t = vis.frame as f64 * 0.05;
        let energy = scope_energy(scope) as f64;
        let vol = vis.volume() as f64 / 15.0;
        // Ball radius breathes with volume + a slow idle pulse.
        let pulse = 0.42 + 0.16 * energy + 0.18 * vol + 0.05 * (t * 0.7).sin();

        // Sample at the braille sub-cell resolution, capped for big terminals.
        let nx = ((inner.width as usize) * 2).min(180);
        let ny = ((inner.height as usize) * 4).min(140);
        const NB: usize = 14; // heat buckets (one Points draw each)
        let mut buckets: BTreeMap<usize, Vec<(f64, f64)>> = BTreeMap::new();
        for iy in 0..ny {
            let y = -1.0 + 2.0 * iy as f64 / ny as f64;
            for ix in 0..nx {
                let x = -asp + 2.0 * asp * ix as f64 / nx as f64;
                let r = (x * x + y * y).sqrt();
                let ang = y.atan2(x);
                // Layered sine turbulence licks the flame's edge.
                let turb = 0.16 * (ang * 5.0 + t * 1.3).sin() * (r * 7.0 - t * 1.7).sin()
                    + 0.10 * (ang * 3.0 - t * 0.9).sin()
                    + 0.06 * (ang * 9.0 + t * 2.1).sin();
                let edge = pulse + turb * (0.5 + energy);
                let mut heat = (edge - r) / edge.max(0.05);
                if heat <= 0.0 {
                    continue;
                }
                heat = (heat * (0.85 + 0.35 * vol)).powf(0.8);
                let key = (heat.clamp(0.0, 1.0) * (NB as f64 - 1.0)).round() as usize;
                buckets.entry(key).or_default().push((x, y));
            }
        }

        let canvas = Canvas::default()
            .x_bounds([-asp, asp])
            .y_bounds([-1.0, 1.0])
            .marker(ratatui::symbols::Marker::Braille)
            .paint(move |ctx| {
                // Ascending key => brighter cells drawn last, on top.
                for (k, pts) in &buckets {
                    let col = heat_color((*k as f32 + 0.5) / NB as f32);
                    ctx.draw(&Points { coords: pts, color: col });
                }
            });
        f.render_widget(canvas, inner);
    }

    /// "Marching cubes": a grid of small wireframe cubes laid on a plane, their
    /// heights riding a travelling wave that marches outward and swells with the
    /// audio, the whole scene slowly tumbling in 3D. Colour tracks height.
    fn draw_cubes(&self, f: &mut Frame, area: Rect, vis: &Vis, scope: &[i16]) {
        let block = Block::default()
            .borders(Borders::ALL)
            .border_set(crate::core::PETSCII_BORDER)
            .border_style(Style::default().fg(LIGHTBLUE))
            .title(Span::styled(" marching cubes ", Style::default().fg(CYAN)));
        let inner = block.inner(area);
        f.render_widget(block, area);
        if inner.width < 4 || inner.height < 3 {
            return;
        }
        let asp = inner.width as f64 / (inner.height as f64 * 2.0);
        let t = vis.frame as f64;
        let energy = scope_energy(scope) as f64;

        // Global tumble; the tilt wobbles gently so the field reads as 3D.
        let ay = t * 0.012;
        let ax = 0.55 + 0.12 * (t * 0.02).sin();

        // Unit cube corners and its 12 edges.
        const V: [[f64; 3]; 8] = [
            [-1.0, -1.0, -1.0], [1.0, -1.0, -1.0], [1.0, 1.0, -1.0], [-1.0, 1.0, -1.0],
            [-1.0, -1.0, 1.0], [1.0, -1.0, 1.0], [1.0, 1.0, 1.0], [-1.0, 1.0, 1.0],
        ];
        const E: [(usize, usize); 12] = [
            (0, 1), (1, 2), (2, 3), (3, 0), // back face
            (4, 5), (5, 6), (6, 7), (7, 4), // front face
            (0, 4), (1, 5), (2, 6), (3, 7), // connectors
        ];
        const GRID: usize = 5;
        const HALF: f64 = 0.12; // cube half-size
        const SCALE: f64 = 0.55; // fit the projected scene into the bounds

        let mut buckets: BTreeMap<usize, Vec<(f64, f64)>> = BTreeMap::new();
        for gz in 0..GRID {
            for gx in 0..GRID {
                let fx = (gx as f64 / (GRID - 1) as f64 - 0.5) * 1.7;
                let fz = (gz as f64 / (GRID - 1) as f64 - 0.5) * 1.7;
                let dist = (fx * fx + fz * fz).sqrt();
                // The marching wave: a ripple travelling out from the centre.
                let wave = 0.5 + 0.5 * (dist * 4.0 - t * 0.08).sin();
                let h = wave * (0.45 + 0.9 * energy);
                let center = [fx, h - 0.3, fz];

                let mut proj = [(0.0f64, 0.0f64); 8];
                for (k, v) in V.iter().enumerate() {
                    let p = [
                        center[0] + v[0] * HALF,
                        center[1] + v[1] * HALF,
                        center[2] + v[2] * HALF,
                    ];
                    let (sx, sy) = project(rot3(p, ax, ay));
                    proj[k] = (sx * SCALE, sy * SCALE);
                }

                let key = (wave * 6.0).round() as usize;
                let entry = buckets.entry(key).or_default();
                for &(a, b) in E.iter() {
                    let (x0, y0) = proj[a];
                    let (x1, y1) = proj[b];
                    let steps = 14;
                    for s in 0..=steps {
                        let tt = s as f64 / steps as f64;
                        entry.push((x0 + (x1 - x0) * tt, y0 + (y1 - y0) * tt));
                    }
                }
            }
        }

        let canvas = Canvas::default()
            .x_bounds([-asp, asp])
            .y_bounds([-1.0, 1.0])
            .marker(ratatui::symbols::Marker::Braille)
            .paint(move |ctx| {
                for (k, pts) in &buckets {
                    let col = BARS[k % BARS.len()];
                    ctx.draw(&Points { coords: pts, color: col });
                }
            });
        f.render_widget(canvas, inner);
    }
}

/// Peak amplitude of the live scope buffer as a 0.0..1.0 energy level.
fn scope_energy(scope: &[i16]) -> f32 {
    let peak = scope.iter().map(|&s| (s as f32).abs()).fold(0.0, f32::max);
    (peak / 32768.0).clamp(0.0, 1.0)
}

/// Rotate a 3D point around the Y axis then the X axis.
fn rot3(p: [f64; 3], ax: f64, ay: f64) -> [f64; 3] {
    let (sy, cy) = ay.sin_cos();
    let (x, z) = (p[0] * cy + p[2] * sy, -p[0] * sy + p[2] * cy);
    let (sx, cx) = ax.sin_cos();
    let (y, z2) = (p[1] * cx - z * sx, p[1] * sx + z * cx);
    [x, y, z2]
}

/// Perspective-project a rotated 3D point to 2D screen coordinates.
fn project(p: [f64; 3]) -> (f64, f64) {
    const FOCAL: f64 = 3.5;
    let f = FOCAL / (FOCAL - p[2]);
    (p[0] * f, p[1] * f)
}

/// Map a 0..1 heat value to a fire-gradient colour
/// (ember → red → orange → yellow → white).
fn heat_color(i: f32) -> Color {
    const STOPS: [(f32, [f32; 3]); 5] = [
        (0.0, [24.0, 0.0, 36.0]),      // dark ember
        (0.35, [0x88 as f32, 0x39 as f32, 0x32 as f32]), // RED
        (0.6, [0x8E as f32, 0x50 as f32, 0x29 as f32]),  // ORANGE
        (0.82, [0xED as f32, 0xF1 as f32, 0x71 as f32]), // YELLOW
        (1.0, [255.0, 255.0, 255.0]),  // WHITE
    ];
    let i = i.clamp(0.0, 1.0);
    let (mut a, mut b) = (STOPS[0], STOPS[STOPS.len() - 1]);
    for w in STOPS.windows(2) {
        if i >= w[0].0 && i <= w[1].0 {
            a = w[0];
            b = w[1];
            break;
        }
    }
    let span = (b.0 - a.0).max(1e-6);
    let tt = (i - a.0) / span;
    let c = |k: usize| (a.1[k] + (b.1[k] - a.1[k]) * tt) as u8;
    Color::Rgb(c(0), c(1), c(2))
}

fn wave_color(wave: u8) -> Color {
    if wave & 0x8 != 0 {
        RED // noise
    } else if wave & 0x4 != 0 {
        GREEN // pulse
    } else if wave & 0x2 != 0 {
        YELLOW // sawtooth
    } else if wave & 0x1 != 0 {
        CYAN // triangle
    } else {
        LIGHTBLUE
    }
}
fn wave_name(wave: u8) -> &'static str {
    if wave & 0x8 != 0 {
        "noise"
    } else if wave & 0x4 != 0 {
        "pulse"
    } else if wave & 0x2 != 0 {
        "saw"
    } else if wave & 0x1 != 0 {
        "tri"
    } else {
        "-"
    }
}

// ---- event loop ------------------------------------------------------------

const QUIT: u8 = 1;
const BACK: u8 = 2;

fn handle_browse_key(state: &mut TunesState, code: KeyCode) -> u8 {
    let last = state.rows.len().saturating_sub(1);
    match code {
        KeyCode::Char('q') | KeyCode::Esc => return QUIT,
        KeyCode::Down | KeyCode::Tab => state.sel = (state.sel + 1).min(last),
        KeyCode::Up => state.sel = state.sel.saturating_sub(1),
        KeyCode::Enter => {
            if let Some(idx) = state.tune_at(state.sel) {
                state.play(idx);
            }
        }
        KeyCode::Char('r') => {
            state.radio = true;
            state.radio_next();
        }
        _ => {}
    }
    0
}

fn handle_player_key(state: &mut TunesState, code: KeyCode) -> u8 {
    match code {
        KeyCode::Char('q') => return QUIT,
        KeyCode::Esc | KeyCode::Left | KeyCode::Backspace => return BACK,
        KeyCode::Char(' ') => {
            if let Some(a) = state.audio.as_ref() {
                a.toggle_pause();
            }
        }
        KeyCode::Char('n') => {
            if state.radio {
                state.radio_next();
            } else {
                state.play_next();
            }
        }
        KeyCode::Char('r') => {
            state.radio = !state.radio;
            if state.radio {
                state.radio_next();
            }
        }
        KeyCode::Char('v') | KeyCode::Tab | KeyCode::Right => state.mode = state.mode.next(),
        _ => {}
    }
    0
}

fn handle_browse_mouse(state: &mut TunesState, m: MouseEvent) {
    let last = state.rows.len().saturating_sub(1);
    match m.kind {
        MouseEventKind::ScrollUp => state.sel = state.sel.saturating_sub(1),
        MouseEventKind::ScrollDown => state.sel = (state.sel + 1).min(last),
        MouseEventKind::Down(MouseButton::Left) => {
            if let Some(ri) = core::hit(&state.rects, m.column, m.row) {
                state.sel = ri;
                if let Some(idx) = state.tune_at(ri) {
                    state.play(idx);
                }
            }
        }
        _ => {}
    }
}

fn event_loop(state: &mut TunesState, term: &mut Terminal<CrosstermBackend<std::io::Stdout>>) -> std::io::Result<()> {
    loop {
        // playing -> the visualiser screen; otherwise the browser.
        if state.now.is_some() {
            term.draw(|f| state.render_player(f))?;
        } else {
            term.draw(|f| state.render_browse(f))?;
        }

        // Radio auto-advances to a fresh random tune + visual once the current
        // one has played long enough (or stopped).
        if state.radio && state.radio_should_advance() {
            state.radio_next();
        }

        // Poll so the visualiser keeps animating even without input.
        if !event::poll(Duration::from_millis(33))? {
            continue;
        }
        match event::read()? {
            Event::Key(k) if k.kind == event::KeyEventKind::Press || k.kind == event::KeyEventKind::Repeat => {
                let action = if state.now.is_some() {
                    handle_player_key(state, k.code)
                } else {
                    handle_browse_key(state, k.code)
                };
                match action {
                    QUIT => return Ok(()),
                    BACK => {
                        state.audio = None;
                        state.now = None;
                        state.radio = false;
                    }
                    _ => {}
                }
            }
            Event::Mouse(m) if state.now.is_none() => {
                handle_browse_mouse(state, m);
            }
            _ => {}
        }
    }
}

fn run_loop(all: Vec<Tune>, start_radio: bool) -> std::io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut term = Terminal::new(backend)?;
    term.hide_cursor()?;

    let mut state = TunesState::new(all);
    if start_radio {
        state.radio = true;
        state.radio_next();
    }
    let result = event_loop(&mut state, &mut term);
    drop(state.audio.take()); // stop audio before leaving raw mode

    disable_raw_mode()?;
    execute!(term.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
    term.show_cursor()?;
    result
}

/// Entry point for c64tunes.
pub fn main(argv: Vec<String>) -> ExitCode {
    // hidden dev aid: `--play <file.sid>` plays a local SID through the real
    // cpal audio path for ~6s, printing the live snapshot. Validates the engine
    // -> audio chain without the TUI.
    if argv.first().map(String::as_str) == Some("--play") {
        let path = argv.get(1).cloned().unwrap_or_default();
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("--play: cannot read {path}: {e}");
                return ExitCode::from(1);
            }
        };
        match Audio::start(bytes, 0) {
            Ok(audio) => {
                eprintln!("playing for 6s ...");
                for _ in 0..6 {
                    std::thread::sleep(Duration::from_secs(1));
                    let v = audio.snapshot();
                    eprintln!(
                        "  t={:02}s frame={} vol={} voices=[{} {} {}]",
                        v.frame / 50,
                        v.frame,
                        v.volume(),
                        wave_name(v.voice_wave(0)),
                        wave_name(v.voice_wave(1)),
                        wave_name(v.voice_wave(2)),
                    );
                }
                return ExitCode::SUCCESS;
            }
            Err(e) => {
                eprintln!("--play: {e}");
                return ExitCode::from(1);
            }
        }
    }

    let mut refresh = false;
    let mut radio = false;
    let mut limit = DEFAULT_LIMIT;
    let mut i = 0;
    while i < argv.len() {
        match argv[i].as_str() {
            "--refresh" => refresh = true,
            "--radio" => radio = true,
            "--limit" => {
                i += 1;
                limit = argv.get(i).and_then(|s| s.parse().ok()).unwrap_or(DEFAULT_LIMIT);
            }
            "-h" | "--help" => {
                print!("{HELP}");
                return ExitCode::SUCCESS;
            }
            other => {
                eprintln!("c64tunes: unknown option: {other} (try --help)");
                return ExitCode::from(1);
            }
        }
        i += 1;
    }

    let empty = std::fs::metadata(tunes_index_path()).map(|m| m.len() == 0).unwrap_or(true);
    if refresh || empty {
        if let Err(e) = build_index(limit) {
            eprintln!("c64tunes: {e}");
            return ExitCode::from(1);
        }
    }

    let all = load_tunes();
    if all.is_empty() {
        eprintln!("c64tunes: no tunes to show (try: c64tunes --refresh)");
        return ExitCode::from(1);
    }

    if !std::io::IsTerminal::is_terminal(&std::io::stdin()) {
        for (party, idxs) in group_by_party(&all) {
            println!("== {party} ==");
            for (r, &idx) in idxs.iter().enumerate() {
                let t = &all[idx];
                println!("  {:2}. {:.2}  {}  ({} '{:02})", r + 1, t.rating, t.name, t.group, t.year % 100);
            }
        }
        return ExitCode::SUCCESS;
    }

    match run_loop(all, radio) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("c64tunes: terminal error: {e}");
            ExitCode::from(1)
        }
    }
}
