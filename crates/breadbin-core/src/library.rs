// library - curated section ordering for the kiosk, recovered from the old
// kiosk.rs (rust/src/kiosk.rs in git history). Groups rows into named sections
// ready for the GUI to render as a scrollable grid.

use std::collections::HashMap;

use crate::tui::{self, Row};

/// Synthetic label for the GB64 classics section.
pub const CLASSICS_GENRE: &str = "classics";

/// Build the ordered list of (section_name, row_indices_into_all) for the
/// kiosk. The ordering mirrors the old terminal kiosk:
///
/// 1. Within each genre, sort rows by downloads descending.
/// 2. Sort genres by their most-downloaded game (descending).
/// 3. Pin "Arcade" → 0, "Shoot'em Up" → 1, everything else → 2, "Other" → 3.
/// 4. Prepend curated front sections: latest played, classics, named collections.
/// 5. Return as a vec of (section_name, row_indices) pairs.
pub fn sections(all: &[Row]) -> Vec<(String, Vec<usize>)> {
    let mut groups = tui::group_by_genre(all);

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
        g if g == tui::GENRE_OTHER => 3,
        _ => 2,
    });

    // Helper: collect rows whose canon matches the given set, sorted by downloads.
    let members = |canons: &std::collections::HashSet<String>| -> Vec<usize> {
        let mut idxs: Vec<usize> = all
            .iter()
            .enumerate()
            .filter(|(_, r)| canons.contains(&tui::canon_of(r)))
            .map(|(i, _)| i)
            .collect();
        idxs.sort_by_key(|&i| std::cmp::Reverse(all[i].downloads));
        idxs
    };

    let mut front: Vec<(String, Vec<usize>)> = Vec::new();

    // "latest played" section — most-recently-played first.
    let recent = tui::recent_plays(None);
    if !recent.is_empty() {
        let by_disp: HashMap<&str, usize> =
            all.iter().enumerate().map(|(i, r)| (r.display.as_str(), i)).collect();
        let latest: Vec<usize> =
            recent.iter().filter_map(|d| by_disp.get(d.as_str()).copied()).collect();
        if !latest.is_empty() {
            front.push((tui::LATEST_GENRE.to_string(), latest));
        }
    }

    // "classics" section (GB64-curated).
    let classics = tui::classic_canons();
    if !classics.is_empty() {
        let idxs = members(&classics);
        if !idxs.is_empty() {
            front.push((CLASSICS_GENRE.to_string(), idxs));
        }
    }

    // Named collections from collections.tsv.
    for (name, canons) in tui::collections() {
        let idxs = members(&canons);
        if !idxs.is_empty() {
            front.push((name, idxs));
        }
    }

    // Curated sections first, then the genre groups.
    front.append(&mut groups);
    front
}
