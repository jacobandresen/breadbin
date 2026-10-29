// breadbin-sid-wasm - the SID player behind a plain C ABI, so the browser can load
// it with WebAssembly.instantiate and no generated JS glue. The web page copies
// the .sid bytes in through `bb_alloc`, creates a player, then pulls PCM from it
// in an AudioWorklet and reads the SID register shadow for the visualiser.

use breadbin_sid::{Player, SamplingMethod, NUM_REGS};

/// Allocate `len` bytes in wasm memory for the host to fill.
#[no_mangle]
pub extern "C" fn bb_alloc(len: usize) -> *mut u8 {
    let mut v = Vec::<u8>::with_capacity(len);
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}

/// Free a buffer from [`bb_alloc`].
#[no_mangle]
pub unsafe extern "C" fn bb_free(ptr: *mut u8, len: usize) {
    drop(Vec::from_raw_parts(ptr, 0, len));
}

/// Build a player for `song` (1-based, 0 = the tune's default) from the .sid bytes
/// at `ptr`. `hq` selects reSID's band-limited resampler over interpolation.
/// Returns null if the file can't be played.
#[no_mangle]
pub unsafe extern "C" fn bb_player_new(ptr: *const u8, len: usize, song: u16, rate: u32, hq: u32) -> *mut Player {
    let bytes = std::slice::from_raw_parts(ptr, len);
    let method = if hq != 0 { SamplingMethod::Resample } else { SamplingMethod::Interpolate };
    match Player::with_sampling(bytes, song, rate, method) {
        Ok(p) => Box::into_raw(Box::new(p)),
        Err(_) => std::ptr::null_mut(),
    }
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

/// Render `n` mono i16 samples into `out`.
#[no_mangle]
pub unsafe extern "C" fn bb_player_render(p: *mut Player, out: *mut i16, n: usize) {
    (*p).render(std::slice::from_raw_parts_mut(out, n));
}

/// Copy the current SID register shadow (`NUM_REGS` bytes) into `out`; returns
/// the frame counter.
#[no_mangle]
pub unsafe extern "C" fn bb_player_regs(p: *const Player, out: *mut u8) -> u32 {
    let vis = (*p).snapshot();
    std::ptr::copy_nonoverlapping(vis.regs.as_ptr(), out, NUM_REGS);
    vis.frame as u32
}
