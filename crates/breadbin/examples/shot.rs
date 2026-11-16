//! Run a PRG on the Studio's machine and save a frame as a PPM picture:
//!   cargo run --example shot -- game.prg out.ppm [frames] [joystick bits held from frame 100] [font]
use breadbin::machine::{Machine, HEIGHT, WIDTH};
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let prg = std::fs::read(&a[1]).unwrap();
    let frames: u64 = a.get(3).map_or(150, |s| s.parse().unwrap());
    let joy: u8 = a.get(4).map_or(0, |s| s.parse().unwrap());
    let font = a.get(5).map_or(Vec::new(), |p| std::fs::read(p).unwrap());
    let mut m = Machine::new(&prg, &font, 44100).unwrap();
    let t = std::time::Instant::now();
    for f in 0..frames {
        if f == 100 { m.set_joystick(2, joy); }
        m.run_frame();
    }
    eprintln!("{} frames in {:?}, ended {}, audio {}", frames, t.elapsed(), m.ended(), m.audio.len());
    let mut out = format!("P6 {} {} 255\n", WIDTH, HEIGHT).into_bytes();
    for p in &m.pixels { out.extend([(p >> 16) as u8, (p >> 8) as u8, *p as u8]); }
    std::fs::write(&a[2], out).unwrap();
}
