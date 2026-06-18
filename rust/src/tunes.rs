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
//   · while playing: space pauses, n next tune, esc/left back to the list
//
// Scene data is cached on disk under the breadbin data dir (tunes_index.tsv +
// sids/), so after the first build it works offline. Re-fetch with
// `breadbin tunes --refresh`.

use std::collections::{HashMap, VecDeque};
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
  c64tunes --refresh       re-fetch the ranked tune list from CSDb
  c64tunes --limit N       how many top tunes to scan when building (default 600)

Audio is rendered in pure Rust (reSID + a 6502 core) and played via your default
sound device - no sidplayfp or VICE required. Data comes from CSDb (csdb.dk) and
is cached locally so it works offline.
";

// CSDb: subtype (7) of the release toplist is "C64 Music", ranked by rating.
const TOPLIST: &str = "https://csdb.dk/toplist.php?type=release&subtype=(7)";
fn release_ws(id: u32, depth: u8) -> String {
    format!("https://csdb.dk/webservice/?type=release&id={id}&depth={depth}")
}

const DEFAULT_LIMIT: usize = 600;
/// A party needs at least this many top tunes to earn its own section.
const MIN_PER_PARTY: usize = 2;
/// Each party section is capped at this many tunes.
const TOP_PER_PARTY: usize = 30;

// ---- C64 / demoscene palette (Pepto colours) -------------------------------
const SCREEN: Color = Color::Rgb(0x40, 0x31, 0x8D);
const LIGHTBLUE: Color = Color::Rgb(0x70, 0x6D, 0xEB);
const WHITE: Color = Color::Rgb(0xFF, 0xFF, 0xFF);
const YELLOW: Color = Color::Rgb(0xED, 0xF1, 0x71);
const CYAN: Color = Color::Rgb(0x75, 0xCE, 0xC8);
const GREEN: Color = Color::Rgb(0x56, 0xAC, 0x4D);
const RED: Color = Color::Rgb(0x88, 0x39, 0x32);
const PURPLE: Color = Color::Rgb(0x8E, 0x3C, 0x97);
/// Raster-bar accent colours cycled across composer title bars / visuals.
const BARS: &[Color] = &[RED, Color::Rgb(0x8E, 0x50, 0x29), YELLOW, GREEN, CYAN, LIGHTBLUE, PURPLE];

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

/// Bucket a party instance ("Fjälldata 2026", "X'2024") into its series name
/// ("Fjälldata", "X") by stripping a trailing edition year.
fn party_series(event: &str) -> String {
    let e = event.trim();
    let bytes = e.as_bytes();
    let mut i = bytes.len();
    while i > 0 && bytes[i - 1].is_ascii_digit() {
        i -= 1;
    }
    let digits = bytes.len() - i;
    if (2..=4).contains(&digits) && i > 0 {
        let sep = bytes[i - 1];
        if sep == b' ' || sep == b'\'' || sep == b'`' {
            return e[..i - 1].trim().to_string();
        }
    }
    e.to_string()
}

fn tunes_index_path() -> PathBuf {
    core::data_path("tunes_index.tsv")
}
fn sids_dir() -> PathBuf {
    core::user_data_dir().join("sids")
}

fn clean(s: &str) -> String {
    s.replace(['\t', '\n', '\r'], " ").trim().to_string()
}

/// The substring of `s` between the first `open` and the next following `close`.
fn between<'a>(s: &'a str, open: &str, close: &str) -> Option<&'a str> {
    let i = s.find(open)? + open.len();
    let j = s[i..].find(close)? + i;
    Some(&s[i..j])
}

/// Strip anchor tags from a toplist "group" cell, leaving names joined with ", ".
fn names_from_cell(cell: &str) -> String {
    let mut names: Vec<String> = Vec::new();
    let mut rest = cell;
    while let Some(open) = rest.find("<a ") {
        let after = &rest[open..];
        let Some(gt) = after.find('>') else { break };
        let tail = &after[gt + 1..];
        let Some(end) = tail.find("</a>") else { break };
        names.push(core::html_unescape(&tail[..end]));
        rest = &tail[end + 4..];
    }
    if names.is_empty() {
        core::html_unescape(cell.trim())
    } else {
        names.join(", ")
    }
}

/// Parse the CSDb release toplist HTML into (id, name, group, rating) rows in
/// ranked order. Identical row shape to the demos toplist.
fn parse_toplist(html: &str) -> Vec<(u32, String, String, f32)> {
    let mut out = Vec::new();
    for tr in html.split("<tr>") {
        let Some(idpart) = between(tr, "/release/?id=", "\"") else { continue };
        let Ok(id) = idpart.parse::<u32>() else { continue };
        let after_id = match tr.find("/release/?id=") {
            Some(p) => &tr[p..],
            None => continue,
        };
        let Some(name_raw) = between(after_id, "\">", "</a>") else { continue };
        let name = core::html_unescape(name_raw);
        let group = between(tr, "</a> by ", "</td>")
            .map(names_from_cell)
            .unwrap_or_default();
        let rating = between(tr, "<font size=1>", "</font>")
            .and_then(|r| r.trim().parse::<f32>().ok())
            .unwrap_or(0.0);
        if !name.is_empty() {
            out.push((id, name, group, rating));
        }
    }
    out
}

/// Pull (composer, party, year, sid download URL) out of a depth-2 release
/// webservice XML document. The composer is the handle credited with "Music";
/// the party is the event it was released at; the SID URL is the first .sid link.
fn parse_release_xml(xml: &str) -> (String, String, u32, String) {
    let year = between(xml, "<ReleaseYear>", "</ReleaseYear>")
        .and_then(|y| y.trim().parse::<u32>().ok())
        .unwrap_or(0);

    // The party: the event name under <ReleasedAt>, with its edition year stripped.
    let party = between(xml, "<ReleasedAt>", "</ReleasedAt>")
        .and_then(|b| between(b, "<Name>", "</Name>"))
        .map(|n| party_series(&core::html_unescape(n)))
        .unwrap_or_default();

    // The .sid download link.
    let mut sid_url = String::new();
    let mut rest = xml;
    while let Some(link) = between(rest, "<Link>", "</Link>") {
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
            while let Some(h) = between(hrest, "<Handle>", "</Handle>") {
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
    let ranked = parse_toplist(&html);
    if ranked.is_empty() {
        return Err("could not parse the CSDb top music list".to_string());
    }
    let take = ranked.len().min(limit);
    let mut prog = core::Progress::new("Scanning tunes", take as u64);
    let mut out = String::new();
    for (n, (id, name, group, rating)) in ranked.iter().take(take).enumerate() {
        prog.set(n as u64 + 1);
        let (composer, party, year, sid_url) = match core::fetch(&release_ws(*id, 2), &[]) {
            Ok(b) => parse_release_xml(&String::from_utf8_lossy(&b)),
            Err(_) => (String::new(), String::new(), 0, String::new()),
        };
        // Skip releases with no downloadable .sid - nothing to play.
        if sid_url.is_empty() {
            continue;
        }
        out.push_str(&format!(
            "{id}\t{}\t{}\t{}\t{:.2}\t{year}\t{}\t{}\n",
            clean(name),
            clean(&composer),
            clean(group),
            rating,
            clean(&sid_url),
            clean(&party),
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
fn group_by_party(all: &[Tune]) -> Vec<(String, Vec<usize>)> {
    let mut map: HashMap<String, Vec<usize>> = HashMap::new();
    let mut loners: Vec<usize> = Vec::new();
    for (i, t) in all.iter().enumerate() {
        if t.party.is_empty() {
            loners.push(i);
        } else {
            map.entry(t.party.clone()).or_default().push(i);
        }
    }
    let mut groups: Vec<(String, Vec<usize>)> = Vec::new();
    for (party, mut idxs) in map {
        if idxs.len() < MIN_PER_PARTY {
            loners.append(&mut idxs);
            continue;
        }
        idxs.sort_by(|&a, &b| all[b].rating.total_cmp(&all[a].rating));
        idxs.truncate(TOP_PER_PARTY);
        groups.push((party, idxs));
    }
    // top ranking first: best rating leads, ties broken by section size.
    groups.sort_by(|a, b| {
        all[b.1[0]]
            .rating
            .total_cmp(&all[a.1[0]].rating)
            .then_with(|| b.1.len().cmp(&a.1.len()))
    });
    if !loners.is_empty() {
        loners.sort_by(|&a, &b| all[b].rating.total_cmp(&all[a].rating));
        loners.truncate(TOP_PER_PARTY);
        groups.push(("Released Outside Parties".to_string(), loners));
    }
    groups
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

struct TunesState {
    all: Vec<Tune>,
    groups: Vec<(String, Vec<usize>)>,
    rows: Vec<Row>,
    sel: usize,
    top: usize,
    rects: Vec<(Rect, usize)>,

    audio: Option<Audio>,
    now: Option<usize>, // index into `all` of the playing tune
    status: String,
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
            status: String::new(),
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
                    "   top C64 SID music by party · {} parties · Enter to play · q quit",
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
                        ellipsize(&t.name, 34),
                        t.rating,
                        ellipsize(&t.group, 18),
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
            "  \u{266a} {}   by {}   ({} '{:02})   {:02}:{:02}{}",
            t.name,
            by,
            t.group,
            t.year % 100,
            secs / 60,
            secs % 60,
            if paused { "   [PAUSED]" } else { "" },
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
            self.draw_scope(f, Rect::new(1, body_y, area.width.saturating_sub(2), body_h), &scope, phase);
        }
        self.draw_voices(
            f,
            Rect::new(1, area.height.saturating_sub(voices_h + 1), area.width.saturating_sub(2), voices_h),
            &vis,
        );

        let help = Paragraph::new(Span::styled(
            "space pause · n next · \u{2190}/esc back to list · q quit",
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
            let bar: String = std::iter::repeat('█').take(cells).collect();
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

/// Truncate `s` to at most `max` columns, marking a cut with an ellipsis.
fn ellipsize(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let cut: String = s.chars().take(max.saturating_sub(1)).collect();
    format!("{cut}\u{2026}")
}

fn hit(rects: &[(Rect, usize)], col: u16, row: u16) -> Option<usize> {
    for (r, idx) in rects {
        if col >= r.x && col < r.x + r.width && row >= r.y && row < r.y + r.height {
            return Some(*idx);
        }
    }
    None
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
        KeyCode::Char('n') => state.play_next(),
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
            if let Some(ri) = hit(&state.rects, m.column, m.row) {
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
                    }
                    _ => {}
                }
            }
            Event::Mouse(m) => {
                if state.now.is_none() {
                    handle_browse_mouse(state, m);
                }
            }
            _ => {}
        }
    }
}

fn run_loop(all: Vec<Tune>) -> std::io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut term = Terminal::new(backend)?;
    term.hide_cursor()?;

    let mut state = TunesState::new(all);
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
    let mut limit = DEFAULT_LIMIT;
    let mut i = 0;
    while i < argv.len() {
        match argv[i].as_str() {
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

    match run_loop(all) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("c64tunes: terminal error: {e}");
            ExitCode::from(1)
        }
    }
}
