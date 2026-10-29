// games - the game library behind the Games tab: loading the ranked index,
// grouping it into kiosk sections, resolving covers, the hidden list and play
// history, and downloading games on demand.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::{build_index, core, cover, disk, gb64};

/// Genre bucket for rows that carry no genre.
pub const GENRE_OTHER: &str = "Other";
/// Synthetic top section in the kiosk: the most recently played games.
pub const LATEST_GENRE: &str = "latest played";

/// One ranked game from c64_index.tsv:
/// display, status, target, title, query, identifier, genre, downloads.
#[derive(Clone)]
pub struct Row {
    pub display: String,
    pub status: String,
    pub target: String,
    pub title: String,
    pub query: String,
    pub ident: String,
    pub genre: String,
    /// Internet Archive download count — popularity, used to order the kiosk.
    /// 0 when absent (an index built before this column existed).
    pub downloads: i64,
}

impl Row {
    pub fn is_local(&self) -> bool {
        self.status == "local"
    }
    pub fn genre_or_other(&self) -> &str {
        if self.genre.is_empty() {
            GENRE_OTHER
        } else {
            &self.genre
        }
    }
}

pub fn index_path() -> PathBuf {
    core::data_path("c64_index.tsv")
}
pub fn played_path() -> PathBuf {
    core::data_path("played.tsv")
}
pub fn hidden_path() -> PathBuf {
    core::data_path("hidden.tsv")
}

/// Load the set of display keys for games the user has hidden.
pub fn load_hidden() -> HashSet<String> {
    let Ok(text) = std::fs::read_to_string(hidden_path()) else {
        return HashSet::new();
    };
    text.lines().filter(|l| !l.is_empty()).map(|l| l.to_string()).collect()
}

/// Add or remove a game from the hidden list, keyed by its display string.
pub fn set_hidden(display: &str, hide: bool) {
    let mut set = load_hidden();
    if hide {
        set.insert(display.to_string());
    } else {
        set.remove(display);
    }
    let content = set.into_iter().collect::<Vec<_>>().join("\n");
    let content = if content.is_empty() { content } else { format!("{content}\n") };
    let _ = std::fs::write(hidden_path(), content);
}

/// Load every ranked game from c64_index.tsv (rows with < 5 fields are skipped).
pub fn load_rows() -> Vec<Row> {
    let mut rows = Vec::new();
    let Ok(text) = std::fs::read_to_string(index_path()) else {
        return rows;
    };
    for line in text.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() >= 5 {
            rows.push(Row {
                display: f[0].to_string(),
                status: f[1].to_string(),
                target: f[2].to_string(),
                title: f[3].to_string(),
                query: f[4].to_string(),
                ident: f.get(5).copied().unwrap_or("").to_string(),
                genre: f.get(6).copied().unwrap_or("").to_string(),
                downloads: f.get(7).and_then(|s| s.parse().ok()).unwrap_or(0),
            });
        }
    }
    rows
}

/// Canonical cover key for a row, matching the catalogue's canon so the libretro
/// boxart lookup lines up.
pub fn canon_of(row: &Row) -> String {
    disk::game_title(&row.query)
}

/// Cover image path for a row: libretro boxart first, else the Internet Archive
/// item image. Both are cached on disk; this may fetch on first use.
pub fn cover_for(row: &Row, cidx: &HashMap<String, String>) -> Option<PathBuf> {
    if let Some(p) = cover::ensure_cover(&canon_of(row), cidx) {
        return Some(p);
    }
    cover::ia_cover(&row.ident)
}

/// Group rows by genre, preserving popularity order; genres appear in first-seen
/// order. Returns (genre, indices-into-rows).
pub fn group_by_genre(rows: &[Row]) -> Vec<(String, Vec<usize>)> {
    let mut order: Vec<String> = Vec::new();
    let mut map: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, r) in rows.iter().enumerate() {
        let g = r.genre_or_other().to_string();
        if !map.contains_key(&g) {
            order.push(g.clone());
        }
        map.entry(g).or_default().push(i);
    }
    order
        .into_iter()
        .map(|g| {
            let v = map.remove(&g).unwrap();
            (g, v)
        })
        .collect()
}

/// Canonical keys of the GB64 games matching `where_clause` (a constant SQL
/// predicate on the `Games` table), for matching rows via [`canon_of`]. Empty if
/// the GB64 database can't be read.
fn canons_where(where_clause: &str) -> HashSet<String> {
    let mut set = HashSet::new();
    let Ok(con) = gb64::connect() else {
        return set;
    };
    let sql = format!("SELECT Name FROM Games WHERE Name <> '' AND ({where_clause})");
    let Ok(mut stmt) = con.prepare(&sql) else {
        return set;
    };
    let Ok(rows) = stmt.query_map([], |r| Ok(gb64::decode_text(r.get_ref(0)?))) else {
        return set;
    };
    for name in rows.flatten().flatten() {
        let canon = disk::game_title(&name);
        if !canon.is_empty() {
            set.insert(canon);
        }
    }
    set
}

/// GB64's curated "classic" games (~50 hand-picked titles), for the Classics section.
pub fn classic_canons() -> HashSet<String> {
    canons_where("Classic <> 0")
}

/// Games GB64 marks as joystick-controlled (Control 1/2 = joystick port 1/2), for
/// the joystick badge. GB64's Control field is only filled in for some games, so
/// this flags the games *known* to need a joystick, not every one.
pub fn joystick_canons() -> HashSet<String> {
    canons_where("Control IN (1, 2)")
}

/// Games GB64 rates at the top (5/5) — a small, curated set — for the top-rating badge.
pub fn top_rated_canons() -> HashSet<String> {
    canons_where("Rating = 5")
}

/// Curated game collections embedded from the repo's `collections.tsv` at build
/// time. Each non-comment line is "Collection Name<TAB>Game Title"; titles are
/// canonicalised so they match catalogue rows via [`canon_of`]. Returns
/// (name, member canons) in the order collections first appear in the file.
pub fn collections() -> Vec<(String, HashSet<String>)> {
    const TSV: &str = include_str!("collections.tsv");
    let mut order: Vec<String> = Vec::new();
    let mut map: HashMap<String, HashSet<String>> = HashMap::new();
    for line in TSV.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((name, title)) = line.split_once('\t') else {
            continue;
        };
        let canon = disk::game_title(title.trim());
        if canon.is_empty() {
            continue;
        }
        let name = name.trim().to_string();
        if !map.contains_key(&name) {
            order.push(name.clone());
        }
        map.entry(name).or_default().insert(canon);
    }
    order.into_iter().filter_map(|n| map.remove(&n).map(|s| (n, s))).collect()
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Append a play event (timestamp<TAB>display) so the kiosk's "latest played"
/// row can surface it. Keyed by the row's display string.
pub fn record_play(row: &Row) {
    use std::io::Write;
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(played_path())
    {
        let _ = writeln!(f, "{}\t{}", now_secs(), row.display);
    }
}

/// Display keys of recently-played games, newest first, de-duplicated.
pub fn recent_plays(limit: Option<usize>) -> Vec<String> {
    let Ok(text) = std::fs::read_to_string(played_path()) else {
        return Vec::new();
    };
    let mut seen: HashSet<String> = HashSet::new();
    let mut order: Vec<String> = Vec::new();
    for line in text.lines().rev() {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() < 2 || seen.contains(parts[1]) {
            continue;
        }
        seen.insert(parts[1].to_string());
        order.push(parts[1].to_string());
        if let Some(l) = limit {
            if order.len() >= l {
                break;
            }
        }
    }
    order
}

/// A step of [`refresh`], for progress reporting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefreshStage {
    /// Opening GameBase64, downloading it on first use.
    GameBase,
    /// Scanning the Internet Archive collection: (items scanned, total).
    Archive(u64, u64),
    /// Matching the catalogue against the local collection.
    Index,
}

/// Re-pull the ranked IA catalogue, then rebuild c64_index.tsv. This is also the
/// first-run bootstrap that fills a fresh user data directory.
pub fn refresh(progress: &mut dyn FnMut(RefreshStage)) -> Result<(), String> {
    progress(RefreshStage::GameBase);
    disk::build_ia_index(&mut |done, total| progress(RefreshStage::Archive(done, total)))?;
    progress(RefreshStage::Index);
    build_index::build()
}

/// True once a catalogue has been built (false on a fresh install).
pub fn has_catalogue() -> bool {
    index_path().exists()
}

/// Resolve a local disk path for a row, downloading from the Internet Archive if
/// needed, then rebuild the index so the row shows as local. Returns None if the
/// download produced no matching disk.
pub fn resolve(row: &Row) -> Option<PathBuf> {
    let path = download(row);
    if !row.is_local() {
        let _ = build_index::build();
    }
    path.or_else(|| local_by_title(row))
}

/// Like [`resolve`] but without rebuilding the index, for bulk downloads that
/// rebuild once at the end.
pub fn resolve_no_rebuild(row: &Row) -> Option<PathBuf> {
    download(row).or_else(|| local_by_title(row))
}

fn download(row: &Row) -> Option<PathBuf> {
    if row.is_local() {
        return Some(PathBuf::from(&row.target));
    }
    let dest = disk::dest_default();
    let path = if !row.ident.is_empty() {
        disk::download_by_id(&row.ident, &dest)
    } else {
        disk::download_query(&row.query, &["ia".to_string()], &dest).into_iter().next()
    };
    path.filter(|p| p.exists())
}

/// Fallback: a local game in the index with the same title.
fn local_by_title(row: &Row) -> Option<PathBuf> {
    load_rows()
        .into_iter()
        .find(|f| f.title == row.title && f.is_local())
        .map(|f| PathBuf::from(f.target))
}

/// Synthetic label for the GB64 classics section.
pub const CLASSICS_GENRE: &str = "classics";

/// Build the ordered list of (section_name, row_indices_into_all) for the
/// kiosk. The ordering:
///
/// 1. Within each genre, sort rows by downloads descending.
/// 2. Sort genres by their most-downloaded game (descending).
/// 3. Pin "Arcade" → 0, "Shoot'em Up" → 1, everything else → 2, "Other" → 3.
/// 4. Prepend curated front sections: latest played, classics, named collections.
/// 5. Return as a vec of (section_name, row_indices) pairs.
pub fn sections(all: &[Row]) -> Vec<(String, Vec<usize>)> {
    let mut groups = group_by_genre(all);

    // Sort each genre internally by downloads descending.
    for (_, idxs) in groups.iter_mut() {
        idxs.sort_by_key(|&i| std::cmp::Reverse(all[i].downloads));
    }

    // Sort genres by their most-downloaded game, descending.
    groups.sort_by_key(|(_, idxs)| {
        std::cmp::Reverse(idxs.first().map(|&i| all[i].downloads).unwrap_or(0))
    });

    // Pin Arcade → 0, Shoot'em Up → 1, Other → 3, everything else → 2.
    groups.sort_by_key(|(genre, _)| match genre.as_str() {
        "Arcade" => 0i32,
        "Shoot'em Up" => 1,
        g if g == GENRE_OTHER => 3,
        _ => 2,
    });

    // Helper: collect rows whose canon matches the given set, sorted by downloads.
    let members = |canons: &std::collections::HashSet<String>| -> Vec<usize> {
        let mut idxs: Vec<usize> = all
            .iter()
            .enumerate()
            .filter(|(_, r)| canons.contains(&canon_of(r)))
            .map(|(i, _)| i)
            .collect();
        idxs.sort_by_key(|&i| std::cmp::Reverse(all[i].downloads));
        idxs
    };

    let mut front: Vec<(String, Vec<usize>)> = Vec::new();

    // "latest played" section — most-recently-played first.
    let recent = recent_plays(None);
    if !recent.is_empty() {
        let by_disp: HashMap<&str, usize> =
            all.iter().enumerate().map(|(i, r)| (r.display.as_str(), i)).collect();
        let latest: Vec<usize> =
            recent.iter().filter_map(|d| by_disp.get(d.as_str()).copied()).collect();
        if !latest.is_empty() {
            front.push((LATEST_GENRE.to_string(), latest));
        }
    }

    // "classics" section (GB64-curated).
    let classics = classic_canons();
    if !classics.is_empty() {
        let idxs = members(&classics);
        if !idxs.is_empty() {
            front.push((CLASSICS_GENRE.to_string(), idxs));
        }
    }

    // Named collections from collections.tsv.
    for (name, canons) in collections() {
        let idxs = members(&canons);
        if !idxs.is_empty() {
            front.push((name, idxs));
        }
    }

    // Curated sections first, then the genre groups.
    front.append(&mut groups);
    front
}
