//! breadbin's engine, compiled to WebAssembly for the browser version
//! (docs/play): the SID player, disk images, the virtual drive, KERNAL
//! patches and the visualisers. JavaScript keeps only the DOM and browser APIs.
//!
//! The ABI is plain C: the host copies bytes in through `bb_alloc`, and
//! results come back through one output buffer (`bb_out_*`) and one text
//! buffer (`bb_text_*`). The drive calls back into the host through the
//! imported `host_peek`, `host_poke` and `host_event`.
//!
//! Safety, for every `unsafe` export: pointers must come from `bb_alloc` (or
//! the engine's own buffers) and be valid for the lengths passed with them.
#![allow(clippy::missing_safety_doc)]

pub mod cpu;
pub mod disk;
pub mod drive;
pub mod exo;
pub mod rom;
pub mod sid;
pub mod viz;

use std::cell::RefCell;

thread_local! {
    static OUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    static SPLIT: RefCell<Vec<Vec<u8>>> = const { RefCell::new(Vec::new()) };
    static TEXT: RefCell<String> = const { RefCell::new(String::new()) };
    static DRIVE: RefCell<Option<drive::Drive>> = const { RefCell::new(None) };
    static VIZ: RefCell<Option<viz::Viz>> = const { RefCell::new(None) };
}

unsafe fn bytes<'a>(ptr: *const u8, len: usize) -> &'a [u8] {
    if len == 0 { &[] } else { std::slice::from_raw_parts(ptr, len) }
}
unsafe fn text<'a>(ptr: *const u8, len: usize) -> &'a str {
    std::str::from_utf8(bytes(ptr, len)).unwrap_or("")
}
fn set_text(s: impl Into<String>) {
    TEXT.with(|t| *t.borrow_mut() = s.into());
}

// ── memory ──────────────────────────────────────────────────────────────────

#[no_mangle]
pub extern "C" fn bb_alloc(len: usize) -> *mut u8 {
    let mut v = Vec::<u8>::with_capacity(len.max(1));
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}

#[no_mangle]
pub unsafe extern "C" fn bb_free(ptr: *mut u8, len: usize) {
    drop(Vec::from_raw_parts(ptr, 0, len.max(1)));
}

#[no_mangle]
pub extern "C" fn bb_out_ptr() -> *const u8 {
    OUT.with(|o| o.borrow().as_ptr())
}
#[no_mangle]
pub extern "C" fn bb_out_len() -> usize {
    OUT.with(|o| o.borrow().len())
}
#[no_mangle]
pub extern "C" fn bb_text_ptr() -> *const u8 {
    TEXT.with(|t| t.borrow().as_ptr())
}
#[no_mangle]
pub extern "C" fn bb_text_len() -> usize {
    TEXT.with(|t| t.borrow().len())
}

// ── disk images ─────────────────────────────────────────────────────────────

/// Prepare a download for the emulator. On success the file is in the output
/// buffer and the text is "<name>\n<1 if for the virtual drive>\n<the zip's
/// disk images, tab-separated>"; on failure
/// the text is the reason. Returns 1 on success.
#[no_mangle]
pub unsafe extern "C" fn bb_prepare(name: *const u8, name_len: usize, data: *const u8, data_len: usize, prefer: *const u8, prefer_len: usize, can_mount: u32) -> u32 {
    match disk::prepare(text(name, name_len), bytes(data, data_len).to_vec(), text(prefer, prefer_len), can_mount != 0) {
        Ok(p) => {
            set_text(format!("{}\n{}\n{}", p.name, p.vdrive as u8, p.disks.join("\t")));
            OUT.with(|o| *o.borrow_mut() = p.bytes);
            1
        }
        Err(e) => {
            set_text(e);
            0
        }
    }
}

/// Choose the file to boot from newline-separated `names` (`start` is the
/// archive's own choice). The text is the name, or empty.
#[no_mangle]
pub unsafe extern "C" fn bb_pick(names: *const u8, names_len: usize, start: *const u8, start_len: usize) {
    let names: Vec<&str> = text(names, names_len).lines().collect();
    set_text(disk::pick(&names, text(start, start_len)).unwrap_or(""));
}

/// Collect the EasySplit parts on a disk image (see `disk::easysplit`), for
/// `bb_split_join`; returns how many it holds.
#[no_mangle]
pub unsafe extern "C" fn bb_split_add(ptr: *const u8, len: usize) -> u32 {
    let parts = disk::split_parts(bytes(ptr, len));
    let n = parts.len() as u32;
    SPLIT.with(|s| s.borrow_mut().extend(parts));
    n
}

/// Join the collected parts: the file is in the output buffer (returns 1), or
/// the text says why not (0). Either way the collection starts over.
#[no_mangle]
pub extern "C" fn bb_split_join() -> u32 {
    match disk::easysplit(SPLIT.with(|s| s.take())) {
        Ok(file) => {
            OUT.with(|o| *o.borrow_mut() = file);
            1
        }
        Err(e) => {
            set_text(e);
            0
        }
    }
}

// ── KERNAL ──────────────────────────────────────────────────────────────────

/// Patch the Open ROMs KERNAL at `ptr` in place (see `rom`), for a real 1541 if
/// `drive` is 1. Returns 1 if patched.
#[no_mangle]
pub unsafe extern "C" fn bb_patch_kernal(ptr: *mut u8, len: usize, drive: u32) -> u32 {
    if len == 0 {
        return 0;
    }
    rom::patch_kernal(std::slice::from_raw_parts_mut(ptr, len), drive == 1) as u32
}

// ── virtual drive (runs inside the emulator frame) ──────────────────────────

#[cfg(target_arch = "wasm32")]
#[link(wasm_import_module = "env")]
extern "C" {
    fn host_peek(addr: u32) -> u32;
    fn host_poke(addr: u32, value: u32);
    /// kind 1 = loaded (name, addr, end), 2 = file not found (name)
    fn host_event(kind: u32, name: *const u8, name_len: usize, addr: u32, end: u32);
}

#[cfg(target_arch = "wasm32")]
struct Host;
#[cfg(target_arch = "wasm32")]
impl drive::Machine for Host {
    fn peek(&mut self, addr: u16) -> u8 {
        unsafe { host_peek(addr as u32) as u8 }
    }
    fn poke(&mut self, addr: u16, value: u8) {
        unsafe { host_poke(addr as u32, value as u32) }
    }
    fn event(&mut self, event: drive::Event) {
        let (kind, name, addr, end) = match event {
            drive::Event::Load { name, addr, end } => (1, name, addr, end),
            drive::Event::Missing { name } => (2, name, 0, 0),
        };
        unsafe { host_event(kind, name.as_ptr(), name.len(), addr as u32, end as u32) }
    }
}

#[cfg(target_arch = "wasm32")]
/// Attach the drive to the D64 in `img` (call once BASIC shows READY).
#[no_mangle]
pub unsafe extern "C" fn bb_drive_attach(img: *const u8, len: usize) -> u32 {
    match drive::Drive::new(bytes(img, len).to_vec(), &mut Host) {
        Ok(d) => {
            DRIVE.with(|s| *s.borrow_mut() = Some(d));
            1
        }
        Err(e) => {
            set_text(e);
            0
        }
    }
}

/// Add a live patch to the attached drive (see `drive::Drive::patch`): at `at`,
/// the `len` bytes at `ptr` are the old bytes followed by their replacement.
#[no_mangle]
pub unsafe extern "C" fn bb_drive_patch(at: u32, ptr: *const u8, len: usize) {
    let b = bytes(ptr, len);
    let (old, new) = b.split_at(len / 2);
    DRIVE.with(|s| s.borrow_mut().as_mut().map(|d| d.patch(at as u16, old.to_vec(), new.to_vec())));
}

/// Give the attached drive another disk of the release, for files the inserted
/// one lacks. Returns 1 if it took it.
#[no_mangle]
pub unsafe extern "C" fn bb_drive_add_side(img: *const u8, len: usize) -> u32 {
    let img = bytes(img, len).to_vec();
    DRIVE.with(|s| s.borrow_mut().as_mut().map(|d| d.add_side(img).is_ok())).unwrap_or(false) as u32
}

/// Put another disk in the attached drive (the next side). Returns 1 if it took it.
#[no_mangle]
pub unsafe extern "C" fn bb_drive_swap(img: *const u8, len: usize) -> u32 {
    let img = bytes(img, len).to_vec();
    match DRIVE.with(|s| s.borrow_mut().as_mut().map(|d| d.swap(img))) {
        Some(Ok(())) => 1,
        Some(Err(e)) => {
            set_text(e);
            0
        }
        None => 0,
    }
}

#[cfg(target_arch = "wasm32")]
#[no_mangle]
pub extern "C" fn bb_drive_tick() {
    DRIVE.with(|s| {
        if let Some(d) = s.borrow_mut().as_mut() {
            d.tick(&mut Host);
        }
    });
}

#[cfg(target_arch = "wasm32")]
/// Loads served so far.
#[no_mangle]
pub extern "C" fn bb_drive_loads() -> u32 {
    DRIVE.with(|s| s.borrow().as_ref().map_or(0, |d| d.loads))
}

#[cfg(target_arch = "wasm32")]
/// The first program's SYS stub address, or 0 (then RUN it).
#[no_mangle]
pub extern "C" fn bb_drive_sys() -> u32 {
    DRIVE.with(|s| s.borrow().as_ref().map_or(0, |d| d.sys as u32))
}

// ── visualisers ─────────────────────────────────────────────────────────────

fn with_viz<R>(f: impl FnOnce(&mut viz::Viz) -> R) -> R {
    VIZ.with(|v| f(v.borrow_mut().get_or_insert_with(Default::default)))
}

/// The font as 96 glyphs (ASCII 32..127) of 8x10 bytes, nonzero = ink.
#[no_mangle]
pub unsafe extern "C" fn bb_viz_font(masks: *const u8, len: usize) {
    with_viz(|v| v.set_font(bytes(masks, len)));
}

/// Render one frame of effect `kind`; `rms` < 0 means nothing is playing.
/// Returns the 320x200 RGBA framebuffer.
#[no_mangle]
pub unsafe extern "C" fn bb_viz_frame(kind: u32, t: f64, rms: f64, regs: *const u8, title: *const u8, title_len: usize) -> *const u32 {
    with_viz(|v| {
        v.update((rms >= 0.0).then_some(rms), bytes(regs, sid::NUM_REGS), t);
        v.draw(kind, text(title, title_len), t);
        v.fb.as_ptr()
    })
}

/// This frame's border colour as 0xRRGGBB.
#[no_mangle]
pub extern "C" fn bb_viz_border() -> u32 {
    with_viz(|v| v.border())
}

// ── SID player ──────────────────────────────────────────────────────────────

use sid::{Player, SamplingMethod};

/// Build a player for `song` (1-based, 0 = default) of the .sid at `ptr`;
/// `hq` selects reSID's band-limited resampler. Null if it can't be played.
#[no_mangle]
pub unsafe extern "C" fn bb_player_new(ptr: *const u8, len: usize, song: u16, rate: u32, hq: u32) -> *mut Player {
    let method = if hq != 0 { SamplingMethod::Resample } else { SamplingMethod::Interpolate };
    Player::with_sampling(bytes(ptr, len), song, rate, method).map_or(std::ptr::null_mut(), |p| Box::into_raw(Box::new(p)))
}

#[no_mangle]
pub unsafe extern "C" fn bb_player_free(p: *mut Player) {
    if !p.is_null() {
        drop(Box::from_raw(p));
    }
}

#[no_mangle]
pub unsafe extern "C" fn bb_player_songs(p: *const Player) -> u32 {
    (*p).songs as u32
}

/// Render `n` stereo frames (2n interleaved i16) into `out`.
#[no_mangle]
pub unsafe extern "C" fn bb_player_render(p: *mut Player, out: *mut i16, n: usize) {
    (*p).render(std::slice::from_raw_parts_mut(out, n));
}

/// Copy the SID register shadow (`NUM_REGS` bytes) into `out`; returns the frame counter.
#[no_mangle]
pub unsafe extern "C" fn bb_player_regs(p: *const Player, out: *mut u8) -> u32 {
    let vis = (*p).snapshot();
    std::ptr::copy_nonoverlapping(vis.regs.as_ptr(), out, sid::NUM_REGS);
    vis.frame as u32
}
