// viz - the Tunes visualisers. The effects are rendered by the Rust engine into
// a 320x200 framebuffer; this blits it with hard pixels inside a border that
// flashes on the beat, and gives the engine Silkscreen as 1-bit glyphs.

import { visualiser } from './engine.js';

export const NAMES = [
  "Twister · for Oxyron's Comaland",
  "Filled Vectors · for Booze Design's Edge of Disgrace",
  "Char Plasma · for Crest & Oxyron's Deus Ex Machina",
];

/** Silkscreen at 8px, ASCII 32..127, thresholded to 8x10 masks. */
function glyphMasks() {
  const c = new OffscreenCanvas(8, 10);
  const x = c.getContext('2d', { willReadFrequently: true });
  x.font = '8px Silkscreen';
  const masks = new Uint8Array(96 * 80);
  for (let i = 0; i < 96; i++) {
    x.clearRect(0, 0, 8, 10);
    x.fillStyle = '#fff';
    x.fillText(String.fromCharCode(32 + i), 0, 8);
    const d = x.getImageData(0, 0, 8, 10).data;
    for (let p = 0; p < 80; p++) masks[i * 80 + p] = d[p * 4 + 3] > 110 ? 1 : 0;
  }
  return masks;
}

export class Viz {
  constructor(canvas) {
    this.canvas = canvas;
    this.ctx = canvas.getContext('2d');
    this.fb = new OffscreenCanvas(320, 200);
    this.image = new ImageData(320, 200);
    this.kind = 0;
    this.title = '';
    this.rms = -1;
    this.scope = new Float32Array(1024);
    this.ready = visualiser().then(async (engine) => {
      this.engine = engine;
      engine.font(glyphMasks());
      await document.fonts?.load('8px Silkscreen');
      engine.font(glyphMasks());
    });
  }

  /** Feed the output audio (null when idle) and the SID registers. */
  update(analyser, regs) {
    this.regs = regs;
    if (!analyser) {
      this.rms = -1;
      return;
    }
    analyser.getFloatTimeDomainData(this.scope);
    let sum = 0;
    for (const s of this.scope) sum += s * s;
    this.rms = Math.sqrt(sum / this.scope.length);
  }

  draw(t) {
    if (!this.engine) return;
    const { pixels, border } = this.engine.frame(this.kind, t, this.rms, this.regs, this.title);
    this.image.data.set(pixels);
    this.fb.getContext('2d').putImageData(this.image, 0, 0);

    const c = this.canvas;
    const dpr = devicePixelRatio || 1;
    const [w, h] = [Math.round(c.clientWidth * dpr), Math.round(c.clientHeight * dpr)];
    if (c.width !== w || c.height !== h) [c.width, c.height] = [w, h];
    const ctx = this.ctx;
    ctx.fillStyle = `#${border.toString(16).padStart(6, '0')}`;
    ctx.fillRect(0, 0, w, h);
    const margin = Math.max(Math.min(w, h) * 0.07, 8);
    let scale = Math.min((w - 2 * margin) / 320, (h - 2 * margin) / 200);
    if (scale >= 1) scale = Math.floor(scale * 2) / 2; // keep pixels even where possible
    if (scale <= 0) return;
    ctx.imageSmoothingEnabled = false;
    ctx.drawImage(this.fb, Math.round((w - 320 * scale) / 2), Math.round((h - 200 * scale) / 2), 320 * scale, 200 * scale);
  }
}
