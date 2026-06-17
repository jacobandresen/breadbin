// core - shared helpers for the breadbin tools (was spread across the Python
// scripts and loaded via importlib). Title normalisation, data-file location,
// HTTP fetch, and a recursive file walk.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use regex::{Captures, Regex};

const UA: &str = "breadbin/1.0";

/// Directory holding breadbin's data files (c64_index.tsv, gb64.sqlitedb,
/// covers/, …). A published, standalone binary owns this directory: on first run
/// it downloads/builds every file it needs here, so no repo or bundled assets are
/// required. Defaults to the user data directory (see [`user_data_dir`]); set
/// `BREADBIN_HOME` to point at an existing data set (e.g. the source repo to
/// share files with the Python tools during the port).
pub fn data_dir() -> PathBuf {
    if let Some(home) = std::env::var_os("BREADBIN_HOME") {
        return PathBuf::from(home);
    }
    user_data_dir()
}

/// A path inside the data directory.
pub fn data_path(name: &str) -> PathBuf {
    data_dir().join(name)
}

/// Directory for mutable user data (downloads, caches). Defaults to
/// `$HOME/.breadbin` unless overridden by `BREADBIN_USER_DATA`.
pub fn user_data_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("BREADBIN_USER_DATA") {
        return PathBuf::from(dir);
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| "".to_string());
    if home.is_empty() {
        PathBuf::from(".")
    } else {
        PathBuf::from(format!("{}/.breadbin", home))
    }
}

/// Ensure breadbin's data directories exist; creates them if missing. Covers both
/// the resolved data dir and the user data dir (the same path unless BREADBIN_HOME
/// overrides it), so a fresh install can write its files on first run.
pub fn ensure_user_data_dir() {
    std::fs::create_dir_all(user_data_dir()).ok();
    std::fs::create_dir_all(data_dir()).ok();
}

/// The C64 collection root. Override with C64_LIB="/path/to/c64".
pub fn c64_lib() -> PathBuf {
    if let Ok(p) = std::env::var("C64_LIB") {
        return PathBuf::from(p);
    }
    let home = std::env::var("HOME").unwrap_or_default();
    PathBuf::from(format!("{home}/Games/Commodore/C64"))
}

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).expect("static regex")
}

/// Canonicalise a title to a comparison key: lowercase, expand a few
/// abbreviations and roman numerals, drop articles, strip non-alphanumerics.
/// Must stay byte-for-byte compatible with build_index.norm in Python — every
/// match in the toolkit keys off it.
pub fn norm(s: &str) -> String {
    static ABBREV: OnceLock<Regex> = OnceLock::new();
    static ROMAN: OnceLock<Regex> = OnceLock::new();
    static ARTICLES: OnceLock<Regex> = OnceLock::new();
    static NONALNUM: OnceLock<Regex> = OnceLock::new();

    let mut s = s.to_lowercase();
    s = s.replace('&', " and ");

    // Jr.->junior, 'n->and, &->and, bros->brothers, intl->international, vs->versus
    let abbrev = ABBREV.get_or_init(|| re(r"\b(jr|bros|intl|vs|n)\b\.?"));
    s = abbrev
        .replace_all(&s, |c: &Captures| match &c[1] {
            "jr" => "junior",
            "bros" => "brothers",
            "intl" => "international",
            "vs" => "versus",
            "n" => "and",
            _ => unreachable!(),
        })
        .into_owned();

    // roman numerals i..x -> 1..10 (numerals stay distinct: II != III)
    let roman = ROMAN.get_or_init(|| re(r"\b(i{1,3}|iv|v|vi{0,3}|ix|x)\b"));
    s = roman
        .replace_all(&s, |c: &Captures| match &c[1] {
            "i" => "1",
            "ii" => "2",
            "iii" => "3",
            "iv" => "4",
            "v" => "5",
            "vi" => "6",
            "vii" => "7",
            "viii" => "8",
            "ix" => "9",
            "x" => "10",
            _ => unreachable!(),
        })
        .into_owned();

    let articles = ARTICLES.get_or_init(|| re(r"\b(the|a)\b"));
    s = articles.replace_all(&s, " ").into_owned();

    let nonalnum = NONALNUM.get_or_init(|| re(r"[^a-z0-9]"));
    nonalnum.replace_all(&s, "").into_owned()
}

/// Cleaned leading-title key: drop a "Subtitle" after ":" or " - ", then norm.
pub fn title_key(name: &str) -> String {
    norm(split_before(name, r":| - "))
}

/// The substring of `s` before the first match of `pattern` (or all of `s`).
/// Mirrors Python's `re.split(pattern, s, maxsplit=1)[0]`.
pub fn split_before<'a>(s: &'a str, pattern: &str) -> &'a str {
    static CACHE: OnceLock<std::sync::Mutex<std::collections::HashMap<String, Regex>>> =
        OnceLock::new();
    let cache = CACHE.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()));
    let mut map = cache.lock().unwrap();
    let rx = map.entry(pattern.to_string()).or_insert_with(|| re(pattern));
    match rx.find(s) {
        Some(m) => &s[..m.start()],
        None => s,
    }
}

/// True if `cmd` is an executable on PATH (used to gate optional tools like fzf).
pub fn command_exists(cmd: &str) -> bool {
    which::which(cmd).is_ok()
}

/// Decode the handful of HTML entities that appear in the archive directory
/// listings we scrape (Internet Archive / UTA file names).
pub fn html_unescape(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
}

/// HTTP GET returning the body bytes. Sends a polite User-Agent unless overridden.
pub fn fetch(url: &str, headers: &[(&str, &str)]) -> Result<Vec<u8>, String> {
    let mut req = ureq::get(url);
    let mut has_ua = false;
    for (k, v) in headers {
        if k.eq_ignore_ascii_case("user-agent") {
            has_ua = true;
        }
        req = req.set(k, v);
    }
    if !has_ua {
        req = req.set("User-Agent", UA);
    }
    let resp = req.call().map_err(|e| e.to_string())?;
    let mut buf = Vec::new();
    use std::io::Read;
    resp.into_reader()
        .read_to_end(&mut buf)
        .map_err(|e| e.to_string())?;
    Ok(buf)
}

/// A single-line stderr progress bar for long scans/downloads. It no-ops unless
/// stderr is a terminal, so piped or redirected output stays clean (the tools
/// already print machine-readable data to stdout). Call [`set`](Self::set) /
/// [`inc`](Self::inc) as work proceeds; [`finish`](Self::finish) prints the
/// closing newline so later messages start on their own line.
pub struct Progress {
    label: String,
    total: u64,
    current: u64,
    tty: bool,
    width: usize,
}

impl Progress {
    /// A bar labelled `label` for `total` units of work (0 = unknown total, in
    /// which case only a running count is shown).
    pub fn new(label: &str, total: u64) -> Self {
        use std::io::IsTerminal;
        let p = Self {
            label: label.to_string(),
            total,
            current: 0,
            tty: std::io::stderr().is_terminal(),
            width: 30,
        };
        p.draw();
        p
    }

    /// Set the absolute progress and redraw.
    pub fn set(&mut self, current: u64) {
        self.current = if self.total > 0 { current.min(self.total) } else { current };
        self.draw();
    }

    /// Advance progress by `n` units and redraw.
    #[allow(dead_code)]
    pub fn inc(&mut self, n: u64) {
        self.set(self.current + n);
    }

    fn draw(&self) {
        if !self.tty {
            return;
        }
        use std::io::Write;
        let mut err = std::io::stderr();
        if self.total > 0 {
            let frac = (self.current as f64 / self.total as f64).clamp(0.0, 1.0);
            let filled = (frac * self.width as f64).round() as usize;
            let bar = "█".repeat(filled) + &"░".repeat(self.width - filled);
            let _ = write!(
                err,
                "\r{} [{bar}] {:3}% ({}/{})",
                self.label,
                (frac * 100.0) as u32,
                self.current,
                self.total
            );
        } else {
            let _ = write!(err, "\r{} {} ...", self.label, self.current);
        }
        let _ = err.flush();
    }

    /// Finish the bar: print a newline so subsequent output is on its own line.
    pub fn finish(&self) {
        if self.tty {
            use std::io::Write;
            let _ = writeln!(std::io::stderr());
        }
    }
}

/// Percent-encode like Python's urllib.parse.quote (default safe="/").
pub fn quote(s: &str) -> String {
    quote_with(s, b"/")
}

fn quote_with(s: &str, safe: &[u8]) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-' | b'~') || safe.contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Percent-decode like urllib.parse.unquote (%XX -> byte, then UTF-8 lossy).
pub fn unquote(s: &str) -> String {
    let bytes = s.as_bytes();
    let hex = |b: u8| (b as char).to_digit(16).map(|d| d as u8);
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(h), Some(l)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                out.push(h * 16 + l);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Form-encode key/value pairs like urllib.parse.urlencode (quote_plus: space->+).
pub fn urlencode(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .map(|(k, v)| format!("{}={}", quote_plus(k), quote_plus(v)))
        .collect::<Vec<_>>()
        .join("&")
}

fn quote_plus(s: &str) -> String {
    // quote_plus = quote(s, safe=" ") then ' ' -> '+'
    quote_with(s, b" ").replace(' ', "+")
}

/// Python difflib.SequenceMatcher(None, a, b).ratio(): 2*M/(len(a)+len(b)) where
/// M is the total size of the matching blocks. Inputs here are short title keys,
/// so difflib's autojunk (only active for sequences > 200) never triggers and is
/// omitted. Used by c64disk's fuzzy matcher — must track Python.
pub fn seq_ratio(a: &str, b: &str) -> f64 {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let total = a.len() + b.len();
    if total == 0 {
        return 1.0;
    }
    // b2j: char -> indices in b
    let mut b2j: std::collections::HashMap<char, Vec<usize>> = std::collections::HashMap::new();
    for (j, &c) in b.iter().enumerate() {
        b2j.entry(c).or_default().push(j);
    }

    let matches = matching_size(&a, &b, &b2j, 0, a.len(), 0, b.len());
    2.0 * matches as f64 / total as f64
}

/// Sum of matching-block sizes via difflib's recursive longest-match split.
fn matching_size(
    a: &[char],
    b: &[char],
    b2j: &std::collections::HashMap<char, Vec<usize>>,
    alo: usize,
    ahi: usize,
    blo: usize,
    bhi: usize,
) -> usize {
    let (i, j, k) = find_longest_match(a, b, b2j, alo, ahi, blo, bhi);
    if k == 0 {
        return 0;
    }
    let mut total = k;
    if alo < i && blo < j {
        total += matching_size(a, b, b2j, alo, i, blo, j);
    }
    if i + k < ahi && j + k < bhi {
        total += matching_size(a, b, b2j, i + k, ahi, j + k, bhi);
    }
    total
}

/// difflib find_longest_match (without junk handling): returns (i, j, size).
fn find_longest_match(
    a: &[char],
    _b: &[char],
    b2j: &std::collections::HashMap<char, Vec<usize>>,
    alo: usize,
    ahi: usize,
    blo: usize,
    bhi: usize,
) -> (usize, usize, usize) {
    let (mut besti, mut bestj, mut bestsize) = (alo, blo, 0usize);
    let mut j2len: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    for i in alo..ahi {
        let mut newj2len: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
        if let Some(indices) = b2j.get(&a[i]) {
            for &j in indices {
                if j < blo {
                    continue;
                }
                if j >= bhi {
                    break;
                }
                let k = j2len.get(&j.wrapping_sub(1)).copied().unwrap_or(0) + 1;
                newj2len.insert(j, k);
                if k > bestsize {
                    besti = i + 1 - k;
                    bestj = j + 1 - k;
                    bestsize = k;
                }
            }
        }
        j2len = newj2len;
    }
    (besti, bestj, bestsize)
}

/// Recursively visit every file under `dir`, calling `f` for each path.
pub fn walk_files(dir: &Path, f: &mut dyn FnMut(&Path)) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        match entry.file_type() {
            Ok(t) if t.is_dir() => walk_files(&path, f),
            Ok(_) => f(&path),
            Err(_) => {}
        }
    }
}
