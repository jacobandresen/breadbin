// sid-worklet - runs breadbin's SID player (Rust: 6502 driver + reSID, compiled
// to WebAssembly) on the audio thread. The main thread posts the module bytes and
// a tune; this processor renders PCM in 128-frame quanta and reports the SID
// registers ~60 times a second for the visualiser.

const REPORT_EVERY = 6; // quanta between register reports (~16ms at 48kHz)
const SILENCE_LIMIT = 4; // seconds of silence that count as "the tune ended"

class SidProcessor extends AudioWorkletProcessor {
  constructor() {
    super();
    this.wasm = null;
    this.player = 0;
    this.paused = false;
    this.quanta = 0;
    this.silent = 0;
    this.ended = false;
    this.port.onmessage = (e) => this.onMessage(e.data);
  }

  async onMessage(m) {
    if (m.type === 'init') {
      const { instance } = await WebAssembly.instantiate(m.wasm, {});
      this.wasm = instance.exports;
      this.buf = this.wasm.bb_alloc(256);
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
    const out = outputs[0][0];
    if (!this.player || this.paused) return true;
    const w = this.wasm;
    w.bb_player_render(this.player, this.buf, out.length);
    const pcm = new Int16Array(w.memory.buffer, this.buf, out.length);
    let peak = 0;
    for (let i = 0; i < out.length; i++) {
      const v = pcm[i] / 32768;
      out[i] = v;
      const a = v < 0 ? -v : v;
      if (a > peak) peak = a;
    }
    for (let c = 1; c < outputs[0].length; c++) outputs[0][c].set(out);

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
