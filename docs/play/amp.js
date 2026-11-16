// amp - the Chicago look's visualiser, a tribute to the one in the MP3 player of
// 1997: a display of 76x16 pixels, scaled up whole, black with a grid of dim
// dots; a spectrum of 19 bars, three pixels wide, coloured by height from
// green through yellow to red, each with a peak that holds, then falls; or the
// wave itself, an oscilloscope. A click on it goes spectrum, scope, off. To
// its left, the time played in big green digits, as the player had it.

const TW = 44; // the time's panel: "m:ss" in 9x13 digits
const W = 76;
const H = 16;
const BARS = 19;
// the bars' colours by row, top to bottom: red, orange, yellow, green
const SPECTRUM = [
  '#ef3110', '#ce2910', '#d65a00', '#d66600', '#d67300', '#c67b08', '#dea518', '#d6b521',
  '#bdde29', '#94de21', '#29ce10', '#32be10', '#39b510', '#319c08', '#299400', '#188408',
];
const SCOPE = ['#fff', '#d6d6de', '#b5bdbd', '#a0aaaf', '#949ca5'];
const DOTS = '#182129';
const PEAK = '#969696';

export class Amp {
  constructor() {
    this.fb = new OffscreenCanvas(W, H);
    this.clock = new OffscreenCanvas(TW, H);
    this.bars = new Float32Array(BARS);
    this.peaks = new Float32Array(BARS);
    this.hold = new Uint8Array(BARS);
    this.freq = null;
    this.mode = 0; // 0 spectrum, 1 scope, 2 off
  }

  click() {
    this.mode = (this.mode + 1) % 3;
  }

  /** One frame onto `ctx` (a w x h canvas): the analyser's spectrum, or `scope` (its wave), or nothing when idle; `elapsed` seconds beside it. */
  draw(ctx, w, h, { analyser, scope, elapsed = -1 }) {
    const x = this.fb.getContext('2d');
    x.fillStyle = '#000';
    x.fillRect(0, 0, W, H);
    x.fillStyle = DOTS; // the display's grid: every other pixel of every other row
    for (let y = 1; y < H; y += 2) for (let i = 1; i < W; i += 2) x.fillRect(i, y, 1, 1);
    if (this.mode === 0) this.spectrum(x, analyser);
    else if (this.mode === 1 && analyser) this.wave(x, scope);
    ctx.fillStyle = '#000';
    ctx.fillRect(0, 0, w, h);
    // the time beside the spectrum, where there is room for both
    const both = w / h > (TW + 4 + W) / H * 0.9;
    const span = both ? TW + 4 + W : W;
    const k = Math.max(1, Math.floor(Math.min(w / span, h / H)));
    const x0 = Math.round((w - span * k) / 2);
    const y0 = Math.round((h - H * k) / 2);
    ctx.imageSmoothingEnabled = false;
    if (both) {
      this.time(elapsed);
      ctx.drawImage(this.clock, x0, y0, TW * k, H * k);
    }
    ctx.drawImage(this.fb, x0 + (both ? (TW + 4) * k : 0), y0, W * k, H * k);
  }

  /** "m:ss" in seven-segment digits, green; nothing playing, a blinking "-:--". */
  time(elapsed) {
    const x = this.clock.getContext('2d');
    x.fillStyle = '#000';
    x.fillRect(0, 0, TW, H);
    const on = elapsed >= 0;
    const s = Math.floor(Math.max(0, elapsed));
    const text = on ? `${Math.min(9, Math.floor(s / 60))}:${String(s % 60).padStart(2, '0')}` : '-:--';
    if (!on && Math.floor(performance.now() / 500) % 2) return;
    let cx = 2;
    for (const ch of text) {
      if (ch === ':') {
        x.fillStyle = '#00e800';
        x.fillRect(cx, 5, 2, 2);
        x.fillRect(cx, 10, 2, 2);
        cx += 4;
        continue;
      }
      digit(x, cx, 1, ch);
      cx += 12;
    }
  }

  spectrum(x, analyser) {
    if (analyser) {
      this.freq ??= new Uint8Array(analyser.frequencyBinCount);
      analyser.getByteFrequencyData(this.freq);
    }
    const n = this.freq?.length || 0;
    for (let b = 0; b < BARS; b++) {
      // bands spread on a log scale over the bins, so the bass doesn't take every bar
      const lo = Math.floor(n ** (b / BARS)) - 1;
      const hi = Math.max(lo + 1, Math.floor(n ** ((b + 1) / BARS)) - 1);
      let v = 0;
      if (analyser) for (let i = Math.max(0, lo); i < hi && i < n; i++) v = Math.max(v, this.freq[i]);
      // the analyser's bytes start at -100 dB, where a SID is always loud: the top two thirds spread over the bars
      const height = Math.max(0, (v - 80) / 175) * H;
      this.bars[b] = Math.max(height, this.bars[b] - 0.7); // bars fall, not drop
      if (this.bars[b] >= this.peaks[b]) (this.peaks[b] = this.bars[b]), (this.hold[b] = 18);
      else if (this.hold[b]) this.hold[b]--;
      else this.peaks[b] = Math.max(0, this.peaks[b] - 0.2);
      const top = H - Math.round(this.bars[b]);
      for (let y = top; y < H; y++) {
        x.fillStyle = SPECTRUM[Math.floor((y * SPECTRUM.length) / H)];
        x.fillRect(b * 4, y, 3, 1);
      }
      const p = H - 1 - Math.round(this.peaks[b]);
      if (this.peaks[b] > 0.5 && p >= 0) (x.fillStyle = PEAK), x.fillRect(b * 4, p, 3, 1);
    }
  }

  wave(x, scope) {
    const mid = H / 2;
    let last = mid;
    for (let i = 0; i < W; i++) {
      const s = scope[Math.floor((i * scope.length) / W)] || 0;
      const y = Math.max(0, Math.min(H - 1, Math.round(mid - s * mid * 1.6)));
      // the line joined up, brighter near the middle
      for (let v = Math.min(last, y); v <= Math.max(last, y); v++) {
        x.fillStyle = SCOPE[Math.min(SCOPE.length - 1, Math.floor((Math.abs(v - mid) * SCOPE.length) / mid))];
        x.fillRect(i, v, 1, 1);
      }
      last = y;
    }
  }
}

// a seven-segment digit, 9x14: which segments each lights (top, top left, top right, middle, bottom left, bottom right, bottom)
const SEGS = { 0: 0b1110111, 1: 0b0010010, 2: 0b1011101, 3: 0b1011011, 4: 0b0111010, 5: 0b1101011, 6: 0b1101111, 7: 0b1010010, 8: 0b1111111, 9: 0b1111011, '-': 0b0001000 };
function digit(x, cx, cy, ch) {
  const on = SEGS[ch] ?? 0;
  const seg = (bit, sx, sy, sw, sh) => {
    x.fillStyle = on & (1 << (6 - bit)) ? '#00e800' : '#0a2a0a'; // the unlit segments faintly there, as on an LCD
    x.fillRect(cx + sx, cy + sy, sw, sh);
  };
  seg(0, 1, 0, 7, 2); // top
  seg(1, 0, 1, 2, 6); // top left
  seg(2, 7, 1, 2, 6); // top right
  seg(3, 1, 6, 7, 2); // middle
  seg(4, 0, 7, 2, 6); // bottom left
  seg(5, 7, 7, 2, 6); // bottom right
  seg(6, 1, 12, 7, 2); // bottom
}
