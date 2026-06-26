// audio - live SID playback: a cpal output stream fed by a generator thread that
// runs the pure-Rust SID engine (src/sid.rs). GUI-agnostic; the UI reads snapshots
// (`Vis`) and the oscilloscope buffer to drive visualisers. Extracted verbatim from
// the original tunes.rs so the audio path is unchanged.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SizedSample};

use crate::sid::{Player, Vis, NUM_REGS};

/// The shared, lock-protected PCM ring the audio callback drains.
type Ring = Arc<Mutex<VecDeque<i16>>>;

/// Number of points in the oscilloscope snapshot.
pub const SCOPE_PTS: usize = 256;

/// Owns the live playback: the audio stream, the generator thread, and the
/// snapshots the visualiser reads. Dropping it stops the sound.
pub struct Audio {
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
    let err_fn = |e| eprintln!("breadbin audio: stream error: {e}");
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
    pub fn start(sid_bytes: Vec<u8>, song: u16) -> Result<Audio, String> {
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
        let vis = Arc::new(Mutex::new(Vis { regs: [0u8; NUM_REGS], frame: 0 }));
        let scope = Arc::new(Mutex::new(vec![0i16; SCOPE_PTS]));

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
        Ok(Audio { _stream: stream, stop, paused, vis, scope, handle: Some(handle) })
    }

    pub fn toggle_pause(&self) {
        let now = !self.paused.load(Ordering::Relaxed);
        self.paused.store(now, Ordering::Relaxed);
    }
    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }
    pub fn snapshot(&self) -> Vis {
        self.vis.lock().unwrap().clone()
    }
    pub fn scope(&self) -> Vec<i16> {
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
