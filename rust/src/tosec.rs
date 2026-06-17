// c64tosec - browse the whole TOSEC C64 catalogue, download + play on selection.
// fzf menu of every game; selecting one downloads its complete release via the
// TOSEC source and launches it. Port of c64tosec (cover preview omitted - the
// bb-cover fzf preview CLI is not ported).

use std::collections::HashMap;
use std::io::Write;
use std::process::ExitCode;

use crate::{build_index, core, disk, run};

const HELP: &str = "\
c64tosec - browse the whole TOSEC C64 catalogue, download + play on selection.

Usage:
  c64tosec [-w] [-f] [-r]      browse + play   (flags pass through to c64run)
  c64tosec --refresh-index     rebuild the cached TOSEC listing first
";

fn have(cmd: &str) -> bool {
    which::which(cmd).is_ok()
}

pub fn main(argv: Vec<String>) -> ExitCode {
    let mut runopts: Vec<String> = Vec::new();
    for a in &argv {
        match a.as_str() {
            "-w" | "--warp" => runopts.push("-w".into()),
            "-f" | "--fullscreen" => runopts.push("-f".into()),
            "-r" | "--real" => runopts.push("-r".into()),
            "--refresh-index" => {
                let _ = std::fs::remove_file(core::data_path("tosec_index.tsv"));
            }
            "-h" | "--help" => {
                print!("{HELP}");
                return ExitCode::SUCCESS;
            }
            s => {
                eprintln!("unknown option: {s}");
                return ExitCode::from(2);
            }
        }
    }

    if !have("fzf") {
        eprintln!("fzf not found");
        return ExitCode::from(2);
    }

    let entries = disk::tosec_entries(); // [(title, url)], builds index if needed

    // one row per game (collapse disk-sides/cracks); keep the cleanest display title
    let mut best: HashMap<String, String> = HashMap::new();
    for (title, _url) in &entries {
        let g = disk::game_title(title);
        if g.is_empty() {
            continue;
        }
        let cleaner = best
            .get(&g)
            .map(|cur| (title.matches('[').count(), title.len()) < (cur.matches('[').count(), cur.len()))
            .unwrap_or(true);
        if cleaner {
            best.insert(g, title.clone());
        }
    }
    eprintln!("TOSEC: {} games", best.len());

    // fzf input: "display\tcanon", sorted by display (case-insensitive)
    let mut pairs: Vec<(&String, &String)> = best.iter().map(|(g, disp)| (disp, g)).collect();
    pairs.sort_by_key(|(disp, _)| disp.to_lowercase());
    let input = pairs.iter().map(|(disp, g)| format!("{disp}\t{g}")).collect::<Vec<_>>().join("\n");

    let mut child = std::process::Command::new("fzf")
        .args([
            "--with-nth=1",
            "--delimiter=\t",
            "--reverse",
            "--cycle",
            "--prompt",
            "TOSEC ▶ ",
            "--header",
            &format!("{} TOSEC games · type to filter · Enter to download + play", best.len()),
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .expect("spawn fzf");
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(input.as_bytes());
    }
    let out = child.wait_with_output().expect("fzf");
    let sel = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if sel.is_empty() {
        return ExitCode::SUCCESS;
    }
    let fields: Vec<&str> = sel.split('\t').collect();
    let disp = fields[0];
    let canon = *fields.last().unwrap();

    // gather the full, most-complete release for this exact game (canon match)
    let members = disk::best_release(
        entries
            .iter()
            .filter(|(t, _)| disk::game_title(t) == canon)
            .map(|(t, u)| (disk::Handle { src: "tosec", reff: u.clone() }, t.clone()))
            .collect(),
    );
    if members.is_empty() {
        eprintln!("no disk image found for that game");
        return ExitCode::from(1);
    }

    eprintln!("* {disp}");
    let dest = disk::dest_default();
    let mut files: Vec<String> = Vec::new();
    for (h, t) in &members {
        files.extend(disk::download(h, t, &dest, false));
    }
    if files.is_empty() {
        eprintln!("download failed");
        return ExitCode::from(1);
    }

    // rebuild so (a) breadbin menu sees it and (b) we boot the same disk the menu would
    build_index::main(vec![]);

    let dlset: std::collections::HashSet<String> =
        files.iter().map(|f| dest.join(f).to_string_lossy().into_owned()).collect();
    let mut boot: Option<String> = None;
    if let Ok(text) = std::fs::read_to_string(core::data_path("c64_index.tsv")) {
        for line in text.lines() {
            let f: Vec<&str> = line.split('\t').collect();
            if f.len() >= 3 && f[1] == "local" && dlset.contains(f[2]) {
                boot = Some(f[2].to_string());
                break;
            }
        }
    }
    let boot = boot.unwrap_or_else(|| {
        // fallback: lowest disk/side downloaded
        let f = files
            .iter()
            .min_by_key(|n| {
                let spaced = n.replace('_', " ");
                (disk::disk_no(&spaced), disk::side_no(&spaced))
            })
            .unwrap();
        dest.join(f).to_string_lossy().into_owned()
    });

    eprintln!("launching: {}", basename(&boot));
    let mut args = runopts;
    args.push(boot);
    run::main(args) // execs c64run/VICE, does not return
}

fn basename(p: &str) -> String {
    std::path::Path::new(p)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| p.to_string())
}
