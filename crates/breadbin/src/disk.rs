//! Disk images and archives: reading D64 directories and files, zip members,
//! and turning a download into something the emulator can start.

/// A D64 (1541) disk image, 35 or 40 tracks, with or without error bytes.
pub struct D64<'a> {
    img: &'a [u8],
}

/// One directory entry. `name` is raw PETSCII without the $A0 padding.
#[derive(Clone, Debug)]
pub struct Entry {
    pub name: Vec<u8>,
    pub kind: u8, // 0 DEL, 1 SEQ, 2 PRG, 3 USR, 4 REL
    pub track: u8,
    pub sector: u8,
    pub blocks: u16,
}

fn sectors_in(track: u8) -> usize {
    match track {
        1..=17 => 21,
        18..=24 => 19,
        25..=30 => 18,
        _ => 17,
    }
}

impl<'a> D64<'a> {
    pub fn new(img: &'a [u8]) -> Result<Self, String> {
        let d = D64 { img };
        d.sector(18, 0).map(|_| d).ok_or_else(|| "not a D64 disk image".into())
    }

    fn sector(&self, track: u8, sector: u8) -> Option<&'a [u8]> {
        if !(1..=42).contains(&track) || sector as usize >= sectors_in(track) {
            return None;
        }
        let first: usize = (1..track).map(sectors_in).sum();
        let o = (first + sector as usize) * 256;
        self.img.get(o..o + 256)
    }

    /// Follow a track/sector chain, stopping on loops or bad links.
    fn chain(&self, mut t: u8, mut s: u8, mut f: impl FnMut(&'a [u8])) {
        let mut seen = std::collections::HashSet::new();
        while t != 0 && seen.insert((t, s)) {
            let Some(sec) = self.sector(t, s) else { break };
            f(sec);
            (t, s) = (sec[0], sec[1]);
        }
    }

    /// Disk name and id, $A0 padding turned into spaces.
    pub fn header(&self) -> (Vec<u8>, Vec<u8>) {
        let bam = self.sector(18, 0).unwrap_or(&[0; 256]);
        let unpad = |b: &[u8]| b.iter().map(|&c| if c == 0xa0 { b' ' } else { c }).collect();
        (unpad(&bam[0x90..0xa0]), unpad(&bam[0xa2..0xa7]))
    }

    /// Closed (properly written) files, in directory order.
    pub fn entries(&self) -> Vec<Entry> {
        let mut out = Vec::new();
        self.chain(18, 1, |sec| {
            for e in sec.chunks(32) {
                if e[2] & 0x80 == 0 || e[2] & 7 == 0 {
                    continue;
                }
                let mut name = e[5..21].to_vec();
                while name.last() == Some(&0xa0) {
                    name.pop();
                }
                out.push(Entry { name, kind: e[2] & 7, track: e[3], sector: e[4], blocks: u16::from_le_bytes([e[30], e[31]]) });
            }
        });
        out
    }

    /// A file's bytes (for a PRG: load address first).
    pub fn read(&self, e: &Entry) -> Vec<u8> {
        let mut out = Vec::new();
        self.chain(e.track, e.sector, |sec| {
            // the last sector's link byte holds the index of its last used byte
            let end = if sec[0] == 0 { (sec[1] as usize + 1).max(2) } else { 256 };
            out.extend_from_slice(&sec[2..end]);
        });
        out
    }

    /// The file a LOAD"name" would get: '*' and '?' wildcards, a "0:" drive
    /// prefix and a ",P,R" type and mode ignored, PRGs preferred.
    pub fn find(&self, pattern: &[u8]) -> Option<Entry> {
        let pat = match pattern.iter().position(|&c| c == b':') {
            Some(i) if i <= 2 => &pattern[i + 1..],
            _ => pattern,
        };
        let pat = pat.split(|&c| c == b',').next().unwrap_or(pat);
        let matches = |name: &[u8]| {
            for (i, &c) in pat.iter().enumerate() {
                if c == b'*' {
                    return true;
                }
                if i >= name.len() || (c != b'?' && c != name[i]) {
                    return false;
                }
            }
            pat.len() == name.len()
        };
        let all = self.entries();
        all.iter().find(|e| e.kind == 2 && matches(&e.name)).or_else(|| all.iter().find(|e| matches(&e.name))).cloned()
    }

    /// LOAD"$": the directory as the BASIC program a 1541 sends.
    pub fn directory(&self) -> Vec<u8> {
        let mut out = vec![0x01, 0x08];
        let mut addr: u16 = 0x0801;
        let mut line = |num: u16, text: &[u8]| {
            addr += 5 + text.len() as u16;
            out.extend_from_slice(&addr.to_le_bytes());
            out.extend_from_slice(&num.to_le_bytes());
            out.extend_from_slice(text);
            out.push(0);
        };
        let (name, id) = self.header();
        let mut head = vec![0x12, b'"'];
        head.extend(name);
        head.extend(b"\" ");
        head.extend(id);
        line(0, &head);
        for e in self.entries() {
            let blocks = e.blocks.to_string();
            let mut t = vec![b' '; 3usize.saturating_sub(blocks.len())];
            t.push(b'"');
            t.extend(&e.name);
            t.push(b'"');
            t.extend(std::iter::repeat_n(b' ', 17usize.saturating_sub(e.name.len())));
            t.extend(["DEL", "SEQ", "PRG", "USR", "REL"].get(e.kind as usize).unwrap_or(&"PRG").as_bytes());
            line(e.blocks, &t);
        }
        line(0, b"BLOCKS FREE.");
        out.extend([0, 0]);
        out
    }
}

/// PETSCII (upper case / graphics set) to readable ASCII.
pub fn petscii(name: &[u8]) -> String {
    name.iter()
        .map(|&b| {
            let c = if (0xc1..=0xda).contains(&b) { b - 0x80 } else { b };
            if (0x20..0x7f).contains(&c) { c as char } else { '?' }
        })
        .collect()
}

/// The address of a cracker's one-line "SYS 2061 <junk>" BASIC stub, if `prg`
/// (load address first) is one. The stock BASIC jumps away at the SYS; the
/// Open ROMs check the whole line first and stop with ?SYNTAX ERROR.
pub fn sys_stub(prg: &[u8]) -> Option<u16> {
    if prg.len() < 8 || prg[0..2] != [0x01, 0x08] {
        return None;
    }
    let next = u16::from_le_bytes([prg[2], prg[3]]) as usize;
    let mut i = 6;
    while prg.get(i) == Some(&b' ') {
        i += 1;
    }
    if prg.get(i) != Some(&0x9e) {
        return None;
    }
    i += 1;
    while matches!(prg.get(i), Some(b' ') | Some(b'(')) {
        i += 1;
    }
    let digits: String = prg[i..].iter().take_while(|c| c.is_ascii_digit()).map(|&c| c as char).collect();
    // only a lone line: a real BASIC program after it should RUN normally
    let after = next.checked_sub(0x0801)? + 2;
    let lone = prg.get(after..after + 2) == Some(&[0, 0]);
    digits.parse().ok().filter(|_| lone)
}

// ── tapes ───────────────────────────────────────────────────────────────────

/// A named program: (PETSCII name, PRG with load address).
pub type Program = (Vec<u8>, Vec<u8>);

/// The programs in a T64 tape image.
pub fn t64(img: &[u8]) -> Result<Vec<Program>, String> {
    if !img.starts_with(b"C64") || img.len() < 0x40 {
        return Err("not a T64 tape image".into());
    }
    let u16_at = |o: usize| u16::from_le_bytes([img[o], img[o + 1]]) as usize;
    let mut out = Vec::new();
    for e in img[0x40..].chunks_exact(32).take(u16_at(0x22).max(1)) {
        if e[0] != 1 {
            continue;
        }
        let (start, end) = (u16::from_le_bytes([e[2], e[3]]), u16::from_le_bytes([e[4], e[5]]));
        let at = u32::from_le_bytes([e[8], e[9], e[10], e[11]]) as usize;
        // many tools wrote a wrong end address; the data then runs to the end of the image
        let len = if end > start && at + ((end - start) as usize) <= img.len() { (end - start) as usize } else { img.len().saturating_sub(at) };
        let mut prg = start.to_le_bytes().to_vec();
        prg.extend_from_slice(img.get(at..at + len).ok_or("a T64 entry points past the end")?);
        let mut name = e[16..32].to_vec();
        while matches!(name.last(), Some(0x20 | 0xa0)) {
            name.pop();
        }
        out.push((name, prg));
    }
    if out.is_empty() {
        return Err("the tape holds no program".into());
    }
    Ok(out)
}

/// A 35-track D64 holding `files`, for the virtual drive.
pub fn d64(files: &[Program]) -> Vec<u8> {
    let at = |t: u8, s: usize| ((1..t).map(sectors_in).sum::<usize>() + s) * 256;
    let mut img = vec![0u8; 174_848];
    let bam = at(18, 0);
    img[bam + 0x90..bam + 0xab].copy_from_slice(b"BREADBIN\xa0\xa0\xa0\xa0\xa0\xa0\xa0\xa0\xa0\xa000\xa02A\xa0\xa0\xa0\xa0");
    // data fills the tracks in order around the directory track
    let mut free = (1..=35u8).filter(|&t| t != 18).flat_map(|t| (0..sectors_in(t)).map(move |s| (t, s)));
    let dirs = files.len().div_ceil(8).clamp(1, 18);
    for (i, (name, prg)) in files.iter().take(dirs * 8).enumerate() {
        let chunks: Vec<&[u8]> = prg.chunks(254).collect();
        let secs: Vec<(u8, usize)> = free.by_ref().take(chunks.len()).collect();
        if secs.len() < chunks.len() {
            break; // disk full
        }
        for (k, (c, &(t, s))) in chunks.iter().zip(&secs).enumerate() {
            let o = at(t, s);
            match secs.get(k + 1) {
                Some(&(nt, ns)) => (img[o], img[o + 1]) = (nt, ns as u8),
                None => (img[o], img[o + 1]) = (0, c.len() as u8 + 1),
            }
            img[o + 2..o + 2 + c.len()].copy_from_slice(c);
        }
        let (sec, e) = (1 + i / 8, at(18, 1 + i / 8) + (i % 8) * 32);
        if i % 8 == 0 {
            (img[at(18, sec)], img[at(18, sec) + 1]) = if sec < dirs { (18, sec as u8 + 1) } else { (0, 0xff) };
        }
        img[e + 2] = 0x82;
        (img[e + 3], img[e + 4]) = secs.first().map_or((0, 0), |&(t, s)| (t, s as u8));
        let padded = name.iter().copied().chain(std::iter::repeat(0xa0)).take(16);
        img[e + 5..e + 21].iter_mut().zip(padded).for_each(|(d, c)| *d = c);
        img[e + 30..e + 32].copy_from_slice(&(chunks.len() as u16).to_le_bytes());
    }
    img
}

// ── EasySplit ───────────────────────────────────────────────────────────────

/// "easysplt" in PETSCII, the start of each part EasySplit writes.
const EASYSPLIT: &[u8] = b"easysplt";

/// The EasySplit parts among a disk's files: a cartridge image (usually an
/// EasyFlash .crt) crunched and cut into pieces that fit 1541 disks.
pub fn split_parts(img: &[u8]) -> Vec<Vec<u8>> {
    D64::new(img).map(|d| d.entries().iter().map(|e| d.read(e)).filter(|f| f.starts_with(EASYSPLIT)).collect()).unwrap_or_default()
}

/// Join EasySplit parts (any order) back into the file they were cut from.
/// Each part: "easysplt", the whole file's length (4 bytes), an id shared by
/// the parts (2), its number from 0 (1) and the number of parts (1), then its
/// share of one Exomizer stream.
pub fn easysplit(mut parts: Vec<Vec<u8>>) -> Result<Vec<u8>, String> {
    parts.retain(|p| p.len() > 16 && p.starts_with(EASYSPLIT));
    parts.sort_by_key(|p| p[14]);
    let first = parts.first().ok_or("no EasySplit parts")?;
    let (len, id, count) = (u32::from_le_bytes([first[8], first[9], first[10], first[11]]) as usize, [first[12], first[13]], first[15] as usize);
    if parts.len() != count || parts.iter().enumerate().any(|(i, p)| p[14] as usize != i || p[12..14] != id) {
        return Err(format!("EasySplit: {} of {count} parts", parts.len()));
    }
    let data = crate::exo::decrunch(&parts.iter().flat_map(|p| p[16..].iter().copied()).collect::<Vec<u8>>())?;
    if data.len() != len {
        return Err("EasySplit: the parts don't unpack to the recorded length".into());
    }
    Ok(data)
}

// ── zip ─────────────────────────────────────────────────────────────────────

/// The members of a zip archive as (name, bytes), stored or deflated.
pub fn unzip(z: &[u8]) -> Result<Vec<(String, Vec<u8>)>, String> {
    let u16_at = |o: usize| z.get(o..o + 2).map(|b| u16::from_le_bytes([b[0], b[1]]) as usize).unwrap_or(0);
    let u32_at = |o: usize| z.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize).unwrap_or(0);
    let eocd = (0..z.len().saturating_sub(21)).rev().find(|&i| u32_at(i) == 0x0605_4b50).ok_or("not a zip archive")?;
    let mut p = u32_at(eocd + 16);
    let mut out = Vec::new();
    for _ in 0..u16_at(eocd + 10) {
        if u32_at(p) != 0x0201_4b50 {
            break;
        }
        let (method, size, name_len) = (u16_at(p + 10), u32_at(p + 20), u16_at(p + 28));
        let local = u32_at(p + 42);
        let name = String::from_utf8_lossy(z.get(p + 46..p + 46 + name_len).unwrap_or_default()).into_owned();
        let start = local + 30 + u16_at(local + 26) + u16_at(local + 28);
        let raw = z.get(start..start + size).ok_or("truncated zip")?;
        let data = match method {
            0 => raw.to_vec(),
            8 => miniz_oxide::inflate::decompress_to_vec(raw).map_err(|_| format!("{name}: bad deflate data"))?,
            m => return Err(format!("{name}: unsupported zip method {m}")),
        };
        out.push((name, data));
        p += 46 + name_len + u16_at(p + 30) + u16_at(p + 32);
    }
    Ok(out)
}

// ── choosing what to start ──────────────────────────────────────────────────

pub fn ext(name: &str) -> String {
    name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default()
}

/// Lower is better: cartridges and programs start directly, disks need a drive.
fn ext_rank(name: &str) -> Option<u8> {
    ["crt", "prg", "t64", "d64", "g64", "tap", "zip"].iter().position(|e| *e == ext(name)).map(|i| i as u8)
}

/// Prefer side A / disk 1 of a multi-disk release.
fn side_rank(name: &str) -> u8 {
    let n = name.to_ascii_lowercase().replace(['_', '-'], " ");
    let has = |words: &[&str], marks: &[&str]| words.iter().any(|w| marks.iter().any(|m| n.contains(&format!("{w} {m}")) || n.contains(&format!("{w}{m}"))));
    let words = ["side", "disk", "disc"];
    if has(&words, &["b", "2"]) {
        2
    } else if has(&words, &["c", "d", "3", "4"]) {
        3
    } else if has(&words, &["a", "1"]) {
        0
    } else {
        1
    }
}

/// The best file to boot among `names` (`start` is the archive's own choice).
pub fn pick<'n>(names: &[&'n str], start: &str) -> Option<&'n str> {
    if let Some(n) = names.iter().find(|n| !start.is_empty() && **n == start) {
        return Some(n);
    }
    let mut ok: Vec<&str> = names.iter().copied().filter(|n| ext_rank(n).is_some() && !n.starts_with("__MACOSX")).collect();
    ok.sort_by_key(|n| (side_rank(n), ext_rank(n), n.len()));
    ok.first().copied()
}

/// What the emulator gets: a file to start, or a disk for the virtual drive.
pub struct Prepared {
    pub name: String,
    pub bytes: Vec<u8>,
    /// Serve this D64 with the virtual drive (no drive ROM to mount it with).
    pub vdrive: bool,
    /// Every disk image in the zip, in boot order: the sides to swap in.
    pub disks: Vec<String>,
}

/// Turn a download into something to start. `prefer` names the zip member to
/// use (GameBase64's FileToRun); `can_mount` means the user supplied a 1541 ROM.
pub fn prepare(name: &str, bytes: Vec<u8>, prefer: &str, can_mount: bool) -> Result<Prepared, String> {
    match ext(name).as_str() {
        "zip" => {
            let members = unzip(&bytes)?;
            let base = |n: &str| n.rsplit('/').next().unwrap_or(n).to_ascii_lowercase();
            let names: Vec<&str> = members.iter().map(|(n, _)| n.as_str()).filter(|n| ext(n) != "zip").collect();
            let chosen = names
                .iter()
                .find(|n| !prefer.is_empty() && base(n) == prefer.to_ascii_lowercase())
                .copied()
                .or_else(|| pick(&names, ""))
                .ok_or("nothing runnable inside the zip")?
                .to_string();
            let mut disks: Vec<&str> = names.iter().copied().filter(|n| matches!(ext(n).as_str(), "d64" | "g64")).collect();
            disks.sort_by_key(|n| (side_rank(n), n.to_string()));
            let disks = disks.iter().map(|n| n.rsplit('/').next().unwrap_or(n).to_string()).collect();
            let data = members.into_iter().find(|(n, _)| *n == chosen).map(|(_, d)| d).unwrap_or_default();
            Ok(Prepared { disks, ..prepare(chosen.rsplit('/').next().unwrap_or(&chosen), data, "", can_mount)? })
        }
        "t64" => {
            let files = t64(&bytes)?;
            Ok(Prepared { name: name.replace(['.', ' '], "_") + ".d64", bytes: d64(&files), vdrive: true, disks: vec![] })
        }
        "d64" if !can_mount => {
            if D64::new(&bytes)?.entries().is_empty() {
                return Err("the disk holds no program to start".into());
            }
            Ok(Prepared { name: name.into(), bytes, vdrive: true, disks: vec![] })
        }
        "g64" if !can_mount => Err("this release is a G64 copy-protected disk, which needs your own 1541 ROM (see Settings)".into()),
        e if ext_rank(name).is_some() || e == "d64" || e == "g64" => Ok(Prepared { name: name.into(), bytes, vdrive: false, disks: vec![] }),
        _ => Err(format!("{name} is not a C64 disk, tape, cartridge or program")),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A one-file disk holding a two-sector PRG.
    pub(crate) fn tiny_disk() -> Vec<u8> {
        let mut prg = vec![0x01, 0x08];
        prg.resize(254, 0);
        prg.extend([1, 2, 3]);
        d64(&[(b"GAME".to_vec(), prg)])
    }

    #[test]
    fn reads_directory_and_files() {
        let img = tiny_disk();
        let d = D64::new(&img).unwrap();
        let e = d.find(b"*").unwrap();
        assert_eq!(petscii(&e.name), "GAME");
        let data = d.read(&e);
        assert_eq!(&data[..2], &[0x01, 0x08]);
        assert_eq!(data.len(), 254 + 3);
        assert!(d.find(b"GA*").is_some() && d.find(b"0:GAME").is_some() && d.find(b"0:GAME,P,R").is_some() && d.find(b"NOPE").is_none());
        assert_eq!(&d.directory()[..2], &[0x01, 0x08]);
    }

    /// A stored (uncompressed) zip of `files`.
    fn zip(files: &[(&str, &[u8])]) -> Vec<u8> {
        let (mut z, mut dir) = (Vec::new(), Vec::new());
        for (name, data) in files {
            let (n, len) = (name.len() as u16, data.len() as u32);
            dir.extend([&0x0201_4b50u32.to_le_bytes()[..], &[0; 16], &len.to_le_bytes(), &len.to_le_bytes(), &n.to_le_bytes(), &[0; 12], &(z.len() as u32).to_le_bytes(), name.as_bytes()].concat());
            z.extend([&0x0403_4b50u32.to_le_bytes()[..], &[0; 14], &len.to_le_bytes(), &len.to_le_bytes(), &n.to_le_bytes(), &[0; 2], name.as_bytes(), data].concat());
        }
        let (at, count) = (z.len() as u32, files.len() as u16);
        z.extend(dir.iter());
        z.extend([&0x0605_4b50u32.to_le_bytes()[..], &[0; 6], &count.to_le_bytes(), &(dir.len() as u32).to_le_bytes(), &at.to_le_bytes(), &[0; 2]].concat());
        z
    }

    #[test]
    fn lists_the_sides_of_a_multi_disk_zip() {
        let disk = tiny_disk();
        let z = zip(&[("readme.txt", b"hi"), ("GAME SIDE B.D64", &disk), ("GAME SIDE A.D64", &disk)]);
        let p = prepare("game.zip", z, "", true).unwrap();
        assert_eq!((p.name.as_str(), p.disks), ("GAME SIDE A.D64", vec!["GAME SIDE A.D64".to_string(), "GAME SIDE B.D64".into()]));
    }

    #[test]
    fn tapes_become_disks() {
        let mut img = b"C64S tape image file".to_vec();
        img.resize(0x40, 0);
        img[0x22] = 1;
        let entry = [&[1, 0x82, 0x01, 0x08, 0xc6, 0xc3, 0, 0, 0x60, 0, 0, 0, 0, 0, 0, 0][..], b"HELLO           "].concat();
        img.extend(entry);
        img.extend(vec![7u8; 300]); // the end address is bogus: the data runs to the end
        let files = t64(&img).unwrap();
        assert_eq!(files[0].0, b"HELLO");
        let disk = d64(&files);
        let d = D64::new(&disk).unwrap();
        let data = d.read(&d.find(b"HELLO").unwrap());
        assert_eq!((data.len(), &data[..3]), (302, &[0x01, 0x08, 7][..]));
        assert!(prepare("x.t64", img, "", false).unwrap().vdrive);
    }

    #[test]
    fn finds_sys_stubs() {
        let stub = |line: &[u8]| {
            let mut p = vec![0x01, 0x08];
            let next = 0x0801 + 4 + line.len() as u16 + 1;
            p.extend(next.to_le_bytes());
            p.extend([0x2a, 0x07]);
            p.extend(line);
            p.extend([0, 0, 0]);
            p
        };
        assert_eq!(sys_stub(&stub(b"\x9e(2080) N H SOFT")), Some(2080));
        assert_eq!(sys_stub(&stub(b"\x9e2061")), Some(2061));
        assert_eq!(sys_stub(&stub(b"\x99\"HELLO\"")), None);
    }

    #[test]
    fn picks_side_a_and_programs() {
        assert_eq!(pick(&["game_side_b.d64", "game_side_a.d64"], ""), Some("game_side_a.d64"));
        assert_eq!(pick(&["x.d64", "x.prg", "x_files.xml"], ""), Some("x.prg"));
        assert_eq!(pick(&["a.d64", "b.d64"], "b.d64"), Some("b.d64"));
    }
}
