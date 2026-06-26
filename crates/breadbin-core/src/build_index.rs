// build_index - match the ranked IA catalogue (ia_index.tsv) against the local
// collection and write c64_index.tsv (display<TAB>status<TAB>target<TAB>title
// <TAB>query<TAB>identifier<TAB>genre<TAB>downloads), ordered by popularity. Port of
// build_index.py's main(); norm() lives in core.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::OnceLock;

use regex::Regex;

use crate::core;

const EXTS: &[&str] = &[
    ".d64", ".d71", ".d81", ".t64", ".tap", ".crt", ".g64", ".nib", ".p00", ".x64",
];

fn re(p: &str) -> Regex {
    Regex::new(p).expect("static regex")
}

/// Cleaned leading-title key for a ROM filename (mirrors build_index.file_key).
fn file_key(basename: &str) -> String {
    static EXT: OnceLock<Regex> = OnceLock::new();
    static YEAR: OnceLock<Regex> = OnceLock::new();
    static VER: OnceLock<Regex> = OnceLock::new();

    let ext = EXT.get_or_init(|| re(r"(?i)\.[a-z0-9]{2,4}$"));
    let stem = ext.replace(basename, "").into_owned();
    let stem = stem.replace('_', " "); // UTA uses underscores

    // drop (year)(publisher)[flags]
    let stem = match stem.find(['(', '[']) {
        Some(i) => &stem[..i],
        None => &stem[..],
    };

    // IA names embed year+publisher with no parens; cut at the first space-delimited
    // year that has title text before it (keeps games like "1942").
    let year = YEAR.get_or_init(|| re(r"\S\s(?:19xx|19\d\d|20\d\d)(?:\s|$)"));
    let stem: String = match year.find(stem) {
        Some(m) => {
            let first = stem[m.start()..].chars().next().unwrap();
            stem[..m.start() + first.len_utf8()].to_string()
        }
        None => stem.to_string(),
    };

    // drop subtitle / media descriptor
    let stem = core::split_before(&stem, r" - | (?:Side|Tape|Disk|Part) \d");

    let ver = VER.get_or_init(|| re(r"\bv\d+(\.\d+)*\b"));
    let stem = ver.replace_all(stem, "");
    core::norm(&stem)
}

/// Multi-load media boot order: (tape/disk/part number, side). (1,0) = single file.
fn load_rank(basename: &str) -> (i32, i32) {
    let b = basename.replace('_', " ");
    let num = |kind: &str| -> Option<i32> {
        let digit = re(&format!(r"(?i)\b{kind}\s*([1-9])\b"));
        if let Some(c) = digit.captures(&b) {
            return c[1].parse().ok();
        }
        let letter = re(&format!(r"(?i)\b{kind}\s*([a-i])\b"));
        if let Some(c) = letter.captures(&b) {
            let ch = c[1].chars().next().unwrap().to_ascii_lowercase();
            return Some(ch as i32 - 'a' as i32 + 1);
        }
        None
    };
    let tape = num("tape").or_else(|| num("disk")).or_else(|| num("part")).unwrap_or(1);
    let side = num("side").unwrap_or_else(|| {
        if re(r"(?i)\bstart\b").is_match(&b) {
            1
        } else if re(r"(?i)\bend\b").is_match(&b) {
            3
        } else {
            0
        }
    });
    (tape, side)
}

/// Lowest tape/side, then fewest [..] flags, then shortest name.
fn pick(paths: &[PathBuf]) -> PathBuf {
    paths
        .iter()
        .min_by_key(|p| {
            let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
            let (tape, side) = load_rank(name);
            (tape, side, name.matches('[').count(), name.len())
        })
        .cloned()
        .unwrap()
}

fn has_image_ext(name: &str) -> bool {
    let lower = name.to_lowercase();
    EXTS.iter().any(|e| lower.ends_with(e))
}

/// Rebuild c64_index.tsv from ia_index.tsv + the local library. Library entry point
/// for the GUI (replaces re-exec'ing the `index` subcommand).
pub fn build() {
    let _ = main(Vec::new());
}

pub fn main(_argv: Vec<String>) -> ExitCode {
    let out_path = core::data_path("c64_index.tsv");
    let ia_index = core::data_path("ia_index.tsv");
    let downloaded_path = core::data_path("downloaded.tsv");

    // collect local files grouped by their cleaned title key
    let mut by_key: HashMap<String, Vec<PathBuf>> = HashMap::new();
    core::walk_files(&core::c64_lib(), &mut |p: &Path| {
        if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
            if has_image_ext(name) {
                by_key.entry(file_key(name)).or_default().push(p.to_path_buf());
            }
        }
    });

    if !ia_index.exists() {
        eprintln!("no ia_index.tsv yet (run: c64menu --refresh); keeping existing index");
        return ExitCode::SUCCESS; // don't wipe a shipped index
    }

    // games downloaded via c64disk --id, matched by exact IA identifier
    let mut downloaded: HashMap<String, String> = HashMap::new();
    if let Ok(text) = std::fs::read_to_string(&downloaded_path) {
        for line in text.lines() {
            if let Some((ident, path)) = line.split_once('\t') {
                if Path::new(path).exists() {
                    downloaded.insert(ident.to_string(), path.to_string());
                }
            }
        }
    }

    let ia_text = match std::fs::read_to_string(&ia_index) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("build_index: cannot read {}: {e}", ia_index.display());
            return ExitCode::from(1);
        }
    };

    let (mut n_local, mut n_avail) = (0u32, 0u32);
    let mut out = String::new();
    for line in ia_text.lines() {
        // columns: canon, rating, downloads, genre, ident, title
        let cols: Vec<&str> = line.splitn(6, '\t').collect();
        if cols.len() < 6 {
            continue;
        }
        let (canon, rating, downloads, genre, ident, title) =
            (cols[0], cols[1], cols[2], cols[3], cols[4], cols[5]);

        let (status, target): (&str, String) = if let Some(p) = downloaded.get(ident) {
            n_local += 1;
            ("local", p.clone())
        } else if let Some(paths) = by_key.get(canon) {
            n_local += 1;
            ("local", pick(paths).to_string_lossy().into_owned())
        } else {
            n_avail += 1;
            ("available", "ia".to_string())
        };

        let disp = if rating != "0" && !rating.is_empty() {
            format!("{title}   ★{rating}")
        } else {
            title.to_string()
        };
        out.push_str(&format!(
            "{disp}\t{status}\t{target}\t{title}\t{title}\t{ident}\t{genre}\t{downloads}\n"
        ));
    }

    if let Err(e) = std::fs::write(&out_path, out) {
        eprintln!("build_index: cannot write {}: {e}", out_path.display());
        return ExitCode::from(1);
    }
    eprintln!(
        "{n_local} local + {n_avail} downloadable = {} games (IA · details, ranked) -> {}",
        n_local + n_avail,
        out_path.display()
    );
    ExitCode::SUCCESS
}
