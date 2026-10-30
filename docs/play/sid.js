// sid - the browser's SID player: an AudioWorklet running the engine's player, a
// gentle output chain (DC blocker, a soft limiter and an optional Haas-style
// stereo widening for headphones), and an analyser the visualiser reads.

import { engineBytes } from './engine.js';

const WORKLET_URL = new URL('./sid-worklet.js', import.meta.url);

export class SidPlayer extends EventTarget {
  constructor() {
    super();
    this.ctx = null;
    this.ready = null;
    this.regs = new Uint8Array(32);
    this.frame = 0;
    this.playing = false;
    this.paused = false;
    this.songs = 1;
  }

  /** Create the audio graph; must run from a user gesture the first time. */
  async init() {
    this.ready ??= (async () => {
      const ctx = new AudioContext({ latencyHint: 'playback' });
      this.ctx = ctx;
      const [engine] = await Promise.all([engineBytes(), ctx.audioWorklet.addModule(WORKLET_URL)]);
      const wasm = engine.slice(0);
      this.node = new AudioWorkletNode(ctx, 'sid-processor', { numberOfInputs: 0, outputChannelCount: [2] });
      await new Promise((resolve, reject) => {
        this.node.port.onmessage = (e) => this.onMessage(e.data, resolve, reject);
        this.node.port.postMessage({ type: 'init', wasm }, [wasm]);
      });

      // DC blocker -> master gain -> soft limiter -> (dry + widened) -> out.
      const hp = new BiquadFilterNode(ctx, { type: 'highpass', frequency: 18, Q: 0.5 });
      this.gain = new GainNode(ctx, { gain: 0.9 });
      const limiter = new DynamicsCompressorNode(ctx, { threshold: -10, knee: 8, ratio: 4, attack: 0.004, release: 0.2 });
      this.analyser = new AnalyserNode(ctx, { fftSize: 1024, smoothingTimeConstant: 0.6 });
      this.node.connect(hp).connect(this.gain).connect(limiter);
      limiter.connect(this.analyser);

      // Dry stereo (2SID and 3SID tunes are panned in the engine), plus a short
      // delayed copy panned apart for width.
      limiter.connect(ctx.destination);
      this.wide = new GainNode(ctx, { gain: 0, channelCount: 1, channelCountMode: 'explicit' });
      const delayL = new DelayNode(ctx, { delayTime: 0.011 });
      const delayR = new DelayNode(ctx, { delayTime: 0.017 });
      const tone = new BiquadFilterNode(ctx, { type: 'lowpass', frequency: 5200 });
      limiter.connect(tone).connect(this.wide);
      this.wide.connect(delayL).connect(new StereoPannerNode(ctx, { pan: -0.9 })).connect(ctx.destination);
      this.wide.connect(delayR).connect(new StereoPannerNode(ctx, { pan: 0.9 })).connect(ctx.destination);
    })();
    await this.ready;
    if (this.ctx.state !== 'running') await this.ctx.resume();
  }

  onMessage(m, resolve, reject) {
    if (m.type === 'ready') resolve?.();
    else if (m.type === 'vis') {
      this.regs = m.regs;
      this.frame = m.frame;
    } else if (m.type === 'loaded') {
      this.songs = m.songs;
      this.pending?.resolve();
    } else if (m.type === 'error') {
      this.pending?.reject(new Error(m.message));
      reject?.(new Error(m.message));
    } else if (m.type === 'ended') {
      this.dispatchEvent(new Event('ended'));
    }
  }

  /** Play `song` (1-based, 0 = default) of the .sid in `bytes`. */
  async play(bytes, song = 0, { hq = true } = {}) {
    await this.init();
    const done = new Promise((resolve, reject) => (this.pending = { resolve, reject }));
    this.node.port.postMessage({ type: 'load', bytes, song, hq });
    await done;
    this.playing = true;
    this.paused = false;
    this.dispatchEvent(new Event('state'));
  }

  setPaused(paused) {
    if (!this.playing) return;
    this.paused = paused;
    this.node.port.postMessage({ type: 'pause', paused });
    this.dispatchEvent(new Event('state'));
  }

  stop() {
    this.node?.port.postMessage({ type: 'stop' });
    this.playing = false;
    this.paused = false;
    this.dispatchEvent(new Event('state'));
  }

  /** 0 = off, 1 = full; ~0.35 is a subtle room. */
  setWidth(amount) {
    if (this.wide) this.wide.gain.value = amount * 0.5;
  }
  setVolume(v) {
    if (this.gain) this.gain.gain.value = v;
  }
}

/** Fields of a PSID/RSID header, for the now-playing card. */
export function sidHeader(b) {
  const str = (o) => new TextDecoder('latin1').decode(b.subarray(o, o + 32)).replace(/\0.*$/s, '').trim();
  const be16 = (o) => (b[o] << 8) | b[o + 1];
  const version = be16(4);
  const flags = version >= 2 ? be16(0x76) : 0;
  return {
    magic: new TextDecoder().decode(b.subarray(0, 4)),
    name: str(0x16),
    author: str(0x36),
    released: str(0x56),
    songs: be16(0x0e) || 1,
    startSong: be16(0x10) || 1,
    model: ['', '6581', '8580', '6581/8580'][(flags >> 4) & 3],
    clock: ['', 'PAL', 'NTSC', 'PAL/NTSC'][(flags >> 2) & 3],
  };
}
