// sid-worklet - runs the engine's SID player (Rust: 6502 driver + reSID,
// compiled to WebAssembly) on the audio thread. The main thread posts the module bytes and
// a tune; this processor renders PCM in 128-frame quanta and reports the SID
// registers ~60 times a second for the visualiser.

const REPORT_EVERY = 6; // quanta between register reports (~16ms at 48kHz)
const SILENCE_LIMIT = 4; // seconds of silence that count as "the tune ended"
// Auto-level: tunes differ a lot in loudness, so each is eased toward a common
// level over a few seconds (never boosted more than 3x, held during silence).
const TARGET_RMS = 0.13;
const LEVEL_RATE = 0.001; // per quantum: averages over ~3 s
const GAIN_RATE = 0.002;

class SidProcessor extends AudioWorkletProcessor {
  constructor() {
    super();
    this.wasm = null;
    this.player = 0;
    this.paused = false;
    this.quanta = 0;
    this.silent = 0;
    this.ended = false;
    this.level = TARGET_RMS ** 2;
    this.gain = 1;
    this.port.onmessage = (e) => this.onMessage(e.data);
  }

  async onMessage(m) {
    if (m.type === 'init') {
      // the drive's host functions only exist inside the emulator frame
      const env = { host_peek: () => 0, host_poke() {}, host_event() {} };
      const { instance } = await WebAssembly.instantiate(m.wasm, { env });
      this.wasm = instance.exports;
      this.buf = this.wasm.bb_alloc(1024); // stereo i16 frames for one 128-frame quantum
      this.regs = this.wasm.bb_alloc(32);
      this.port.postMessage({ type: 'ready' });
    } else if (m.type === 'load') {
      this.unload();
      const w = this.wasm;
      const ptr = w.bb_alloc(m.bytes.length);
      new Uint8Array(w.memory.buffer, ptr, m.bytes.length).set(m.bytes);
      this.player = w.bb_player_new(ptr, m.bytes.length, m.song, sampleRate, m.hq ? 1 : 0);
      w.bb_free(ptr, m.bytes.length);
      this.paused = false;
      this.silent = 0;
      this.ended = false;
      this.level = TARGET_RMS ** 2;
      this.gain = 1;
      this.port.postMessage(
        this.player
          ? { type: 'loaded', songs: w.bb_player_songs(this.player) }
          : { type: 'error', message: 'this SID could not be played' },
      );
    } else if (m.type === 'pause') {
      this.paused = m.paused;
    } else if (m.type === 'stop') {
      this.unload();
    }
  }

  unload() {
    if (this.player) this.wasm.bb_player_free(this.player);
    this.player = 0;
  }

  process(_inputs, outputs) {
    const [left, right = left] = outputs[0];
    if (!this.player || this.paused) return true;
    const w = this.wasm;
    w.bb_player_render(this.player, this.buf, left.length);
    const pcm = new Int16Array(w.memory.buffer, this.buf, left.length * 2);
    let peak = 0;
    let power = 0;
    const g = this.gain / 32768;
    for (let i = 0; i < left.length; i++) {
      const l = pcm[2 * i];
      const r = pcm[2 * i + 1];
      left[i] = l * g;
      right[i] = r * g;
      peak = Math.max(peak, Math.abs(l), Math.abs(r));
      power += l * l + r * r;
    }
    peak /= 32768;
    power /= 2 * left.length * 32768 * 32768;
    if (power > 1e-5) {
      this.level += (power - this.level) * LEVEL_RATE;
      const want = Math.min(3, Math.max(0.5, TARGET_RMS / Math.sqrt(this.level)));
      this.gain += (want - this.gain) * GAIN_RATE;
    }

    this.silent = peak < 0.002 ? this.silent + out.length : 0;
    if (!this.ended && this.silent > sampleRate * SILENCE_LIMIT) {
      this.ended = true;
      this.port.postMessage({ type: 'ended' });
    }
    if (++this.quanta % REPORT_EVERY === 0) {
      const frame = w.bb_player_regs(this.player, this.regs);
      const regs = new Uint8Array(w.memory.buffer, this.regs, 32).slice();
      this.port.postMessage({ type: 'vis', regs, frame });
    }
    return true;
  }
}

registerProcessor('sid-processor', SidProcessor);
