// viz - the Tunes visualisers, ported from the native app (crates/breadbin-gui/
// src/viz.rs): three classic demo effects in a real 320x200 framebuffer with the
// Pepto palette, scaled up with hard pixels inside a border that flashes on the
// beat. Text uses Sixtyfour, thresholded to a 1-bit glyph cache so it lands on
// whole framebuffer pixels like the real character ROM would.

export const NAMES = [
  "Twister · for Oxyron's Comaland",
  "Filled Vectors · for Booze Design's Edge of Disgrace",
  "Char Plasma · for Crest & Oxyron's Deus Ex Machina",
];

const W = 320;
const H = 200;
const PEPTO = [
  0x000000, 0xffffff, 0x883932, 0x67b6bd, 0x8b3f96, 0x55a049, 0x40318d, 0xbfce72,
  0x8b5429, 0x574200, 0xb86962, 0x505050, 0x787878, 0x94e089, 0x7869c4, 0x9f9f9f,
];
// ImageData is RGBA in memory; on little-endian a u32 reads as 0xAABBGGRR.
const ABGR = PEPTO.map((c) => (0xff000000 | ((c & 0xff) << 16) | (c & 0xff00) | ((c >> 16) & 0xff)) >>> 0);
const css = (c) => `#${c.toString(16).padStart(6, '0')}`;

const RAMP_BLUE = [0, 6, 11, 4, 14, 3, 13, 1];
const RAMP_RED = [0, 9, 2, 8, 10, 15, 7, 1];
const RAMP_GREEN = [0, 11, 5, 12, 13, 1];
const RAMP_GREY = [0, 11, 12, 15, 1];
const RAMP_FIRE = [0, 9, 2, 8, 10, 7, 1];

const rampIdx = (r, v) => r[Math.floor(Math.min(Math.max(v, 0), 0.999) * r.length)];
const ramp = (r, v) => ABGR[rampIdx(r, v)];

const GREETINGS = 'GREETINGS TO OXYRON * BOOZE DESIGN * CREST * PERFORMERS * CENSOR DESIGN * FAIRLIGHT * BONZAI * TRIAD * ARSENIC * GENESIS PROJECT * ';

// ── 1-bit glyphs ─────────────────────────────────────────────────────────────

const GW = 8;
const GH = 10;
const glyphs = new Map();
let fontReady = false;
document.fonts?.load('8px Sixtyfour').then(() => {
  fontReady = true;
  glyphs.clear();
});

function glyph(ch) {
  let g = glyphs.get(ch);
  if (g) return g;
  const c = new OffscreenCanvas(GW, GH);
  const x = c.getContext('2d', { willReadFrequently: true });
  x.fillStyle = '#fff';
  x.font = fontReady ? '8px Sixtyfour' : 'bold 8px monospace';
  x.textBaseline = 'alphabetic';
  x.fillText(ch, 0, 8);
  const d = x.getImageData(0, 0, GW, GH).data;
  g = new Uint8Array(GW * GH);
  for (let i = 0; i < g.length; i++) g[i] = d[i * 4 + 3] > 110 ? 1 : 0;
  glyphs.set(ch, g);
  return g;
}

export class Viz {
  constructor(canvas) {
    this.canvas = canvas;
    this.ctx = canvas.getContext('2d');
    this.fb = new ImageData(W, H);
    this.px = new Uint32Array(this.fb.data.buffer);
    this.fbCanvas = new OffscreenCanvas(W, H);
    this.fbCtx = this.fbCanvas.getContext('2d');
    this.kind = 0;
    this.playing = false;
    this.level = 0;
    this.avg = 0;
    this.beat = 0;
    this.lastBeat = 0;
    this.voices = [0, 0, 0];
    this.gates = [false, false, false];
    this.lastT = 0;
    this.title = '';
    this.scope = new Float32Array(1024);
  }

  /** Feed one frame: the analyser (or null when idle) and the SID registers. */
  update(analyser, regs, t) {
    const dt = Math.min(Math.max(t - this.lastT, 0), 0.1);
    this.lastT = t;
    this.playing = !!analyser;
    if (!analyser) {
      this.level *= 0.9;
      return;
    }
    analyser.getFloatTimeDomainData(this.scope);
    let sum = 0;
    for (const s of this.scope) sum += s * s;
    const rms = Math.sqrt(sum / this.scope.length);
    this.level = Math.min(rms / 0.27, 1);
    this.avg = this.avg * 0.95 + this.level * 0.05;
    if (this.level > this.avg * 1.3 && this.level > 0.06 && t - this.lastBeat > 0.22) {
      this.beat = 1;
      this.lastBeat = t;
    }
    this.beat *= Math.exp(-dt * 6);
    for (let v = 0; v < 3; v++) {
      const ctrl = regs[v * 7 + 4];
      const gate = (ctrl & 1) !== 0;
      if (gate && !this.gates[v]) this.voices[v] = 1;
      this.gates[v] = gate;
      const floor = gate ? ((regs[v * 7 + 6] >> 4) / 15) * 0.6 : 0;
      this.voices[v] = Math.max(this.voices[v] * Math.exp(-dt * 4), floor);
    }
  }

  draw(t) {
    if (!this.playing) this.readyScreen(t);
    else if (this.kind === 0) this.twister(t);
    else if (this.kind === 1) this.vectors(t);
    else this.plasma(t);
    this.blit();
  }

  blit() {
    const c = this.canvas;
    const dpr = devicePixelRatio || 1;
    const w = Math.round(c.clientWidth * dpr);
    const h = Math.round(c.clientHeight * dpr);
    if (c.width !== w || c.height !== h) {
      c.width = w;
      c.height = h;
    }
    const ctx = this.ctx;
    const border = this.playing ? rampIdx(RAMP_BLUE, 0.55 + this.beat * 0.45) : 14;
    ctx.fillStyle = css(PEPTO[border]);
    ctx.fillRect(0, 0, w, h);
    const margin = Math.max(Math.min(w, h) * 0.07, 8);
    let scale = Math.min((w - 2 * margin) / W, (h - 2 * margin) / H);
    if (scale >= 1) scale = Math.floor(scale * 2) / 2; // keep pixels even where possible
    if (scale <= 0) return;
    this.fbCtx.putImageData(this.fb, 0, 0);
    ctx.imageSmoothingEnabled = false;
    ctx.drawImage(this.fbCanvas, Math.round((w - W * scale) / 2), Math.round((h - H * scale) / 2), W * scale, H * scale);
  }

  // ── drawing helpers on the framebuffer ────────────────────────────────────

  fillRow(y, c) {
    if (y >= 0 && y < H) this.px.fill(c, y * W, y * W + W);
  }

  text(x, baseY, s, color) {
    x = Math.round(x);
    const top = Math.round(baseY) - 8;
    for (const ch of s) {
      if (x > -GW && x < W && ch !== ' ') {
        const g = glyph(ch);
        const c = typeof color === 'function' ? color(x) : color;
        for (let gy = 0; gy < GH; gy++) {
          const y = top + gy;
          if (y < 0 || y >= H) continue;
          for (let gx = 0; gx < GW; gx++) {
            const px = x + gx;
            if (g[gy * GW + gx] && px >= 0 && px < W) this.px[y * W + px] = c;
          }
        }
      }
      x += GW;
    }
  }

  // ── idle: the power-on screen ─────────────────────────────────────────────

  readyScreen(t) {
    this.px.fill(ABGR[6]);
    const lines = [
      '    **** BREADBIN SID PLAYER ****',
      '',
      ' 64K RAM SYSTEM  38911 BASIC BYTES FREE',
      '',
      'READY.',
      'LOAD"*",8,1',
    ];
    lines.forEach((l, i) => this.text(0, 20 + i * 10, l, ABGR[14]));
    if (Math.floor(t * 1.6) % 2 === 0) {
      const y0 = 20 + lines.length * 10 - 8;
      for (let y = y0; y < y0 + 9; y++) this.px.fill(ABGR[14], y * W, y * W + 8);
    }
  }

  // ── 1. Twister ────────────────────────────────────────────────────────────

  twister(t) {
    const { px, level, beat, voices } = this;
    px.fill(ABGR[0]);
    [RAMP_RED, RAMP_GREEN, RAMP_BLUE].forEach((r, v) => {
      const cy = 92 + Math.sin(t * (0.9 + v * 0.37) + v * 2.1) * 70;
      const half = 4 + voices[v] * 9;
      for (let y = Math.max(0, Math.floor(cy - half)); y < Math.min(H - 16, Math.floor(cy + half)); y++) {
        const d = 1 - Math.abs(y - cy) / half;
        this.fillRow(y, ramp(r, d * (0.4 + voices[v] * 0.6)));
      }
    });
    const faces = [RAMP_RED, RAMP_BLUE, RAMP_GREEN, RAMP_GREY];
    const r = 28 + beat * 10;
    for (let y = 0; y < H - 16; y++) {
      const a = t * 1.3 + Math.sin(y * 0.011 + t * 0.8) * (1.2 + level * 2.5);
      const cx = 160 + Math.sin(y * 0.017 + t * 0.6) * (20 + level * 50);
      for (let i = 0; i < 4; i++) {
        const x0 = cx + r * Math.sin(a + (i * Math.PI) / 2);
        const x1 = cx + r * Math.sin(a + ((i + 1) * Math.PI) / 2);
        if (x1 <= x0) continue;
        const c = ramp(faces[i], 0.25 + ((x1 - x0) / (2 * r)) * 0.8);
        px.fill(c, y * W + Math.max(0, Math.floor(x0)), y * W + Math.min(W, Math.floor(x1)));
      }
    }
    this.fillRow(H - 17, ABGR[1]);
    this.sineScroller(`NOW PLAYING: ${this.title.toUpperCase()}  ***  ${GREETINGS}`, t, H - 4);
  }

  sineScroller(msg, t, baseY) {
    const chars = [...msg];
    const span = chars.length * 8;
    const offset = (t * 60) % span;
    chars.forEach((ch, i) => {
      for (const x of [i * 8 - offset, i * 8 - offset + span]) {
        if (x < -8 || x >= W) continue;
        const y = baseY + Math.sin(x * 0.04 + t * 4) * 2.5;
        const c = ramp(RAMP_FIRE, 0.45 + 0.5 * (Math.sin(x * 0.02 + t * 2) * 0.5 + 0.5));
        this.text(x, y, ch, c);
      }
    });
  }

  // ── 2. Filled vectors ─────────────────────────────────────────────────────

  vectors(t) {
    const { px, level, beat } = this;
    for (let y = 0; y < 110; y++) this.fillRow(y, ramp(RAMP_BLUE, (y / 110) * 0.75));
    for (let y = 110; y < H; y++) {
      const z = 2400 / (y - 104);
      const shade = (1 - Math.min(z / 300, 1)) * 0.7;
      const row = Math.floor((z + t * 120) / 24);
      const grey = ramp(RAMP_GREY, shade * 0.9);
      const blue = ramp(RAMP_BLUE, shade * 0.9);
      for (let x = 0; x < W; x++) {
        const wx = ((x - 160) * z) / 160;
        px[y * W + x] = ((Math.floor(wx / 24) + row) & 1) === 0 ? grey : blue;
      }
    }

    const g = (1 + Math.sqrt(5)) / 2;
    const verts = [
      [-1, g, 0], [1, g, 0], [-1, -g, 0], [1, -g, 0], [0, -1, g], [0, 1, g],
      [0, -1, -g], [0, 1, -g], [g, 0, -1], [g, 0, 1], [-g, 0, -1], [-g, 0, 1],
    ];
    const FACES = [
      [0, 11, 5], [0, 5, 1], [0, 1, 7], [0, 7, 10], [0, 10, 11], [1, 5, 9], [5, 11, 4], [11, 10, 2], [10, 7, 6], [7, 1, 8],
      [3, 9, 4], [3, 4, 2], [3, 2, 6], [3, 6, 8], [3, 8, 9], [4, 9, 5], [2, 4, 11], [6, 2, 10], [8, 6, 7], [9, 8, 1],
    ];
    const ax = t * 0.7 + beat * 0.4;
    const ay = t * 0.45;
    const [sx, cx, sy, cy] = [Math.sin(ax), Math.cos(ax), Math.sin(ay), Math.cos(ay)];
    const world = verts.map(([x, y0, z0]) => {
      const y = y0 * cx - z0 * sx;
      const z = y0 * sx + z0 * cx;
      return [x * cy + z * sy, y, -x * sy + z * cy];
    });
    const size = 30 * (1 + beat * 0.25 + level * 0.15);
    const bob = Math.sin(t * 1.7) * 10;
    const project = (p) => {
      const k = 4.5 / (4.5 + p[2]);
      return [160 + p[0] * size * k, 80 + bob + p[1] * size * k];
    };
    const ln = Math.hypot(0.4, -0.6, -0.7);
    const light = [0.4 / ln, -0.6 / ln, -0.7 / ln];
    const faceRamp = [RAMP_FIRE, RAMP_BLUE, RAMP_GREEN][Math.floor(t / 8) % 3];
    const visible = [];
    for (const f of FACES) {
      const [a, b, c] = f.map((i) => world[i]);
      const u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
      const v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
      const n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
      const len = Math.hypot(...n);
      if (n[2] / len > 0) continue;
      const lit = (n[0] * light[0] + n[1] * light[1] + n[2] * light[2]) / len;
      visible.push([a[2] + b[2] + c[2], [project(a), project(b), project(c)], Math.max(lit, 0)]);
    }
    visible.sort((p, q) => q[0] - p[0]);

    // Floor shadow, then the faces, rasterised straight into the framebuffer.
    const rx = size * 1.6;
    const ry = size * 0.25;
    for (let y = Math.ceil(150 - ry); y <= 150 + ry; y++) {
      const hw = rx * Math.sqrt(Math.max(0, 1 - ((y - 150) / ry) ** 2));
      px.fill(ABGR[0], y * W + Math.max(0, Math.round(160 - hw)), y * W + Math.min(W, Math.round(160 + hw)));
    }
    for (const [, pts, lit] of visible) this.triangle(pts, ramp(faceRamp, 0.15 + lit * 0.85 + beat * 0.15));
    this.text(8, 12, 'BREADBIN  FILLED VECTORS', ABGR[1]);
  }

  triangle(pts, c) {
    const [a, b, d] = [...pts].sort((p, q) => p[1] - q[1]);
    const edge = (p, q, y) => (q[1] === p[1] ? p[0] : p[0] + ((q[0] - p[0]) * (y - p[1])) / (q[1] - p[1]));
    for (let y = Math.max(0, Math.ceil(a[1])); y <= Math.min(H - 1, Math.floor(d[1])); y++) {
      const xa = edge(a, d, y);
      const xb = y < b[1] ? edge(a, b, y) : edge(b, d, y);
      const [l, r] = xa < xb ? [xa, xb] : [xb, xa];
      this.px.fill(c, y * W + Math.max(0, Math.round(l)), y * W + Math.min(W, Math.round(r) + 1));
    }
  }

  // ── 3. Char plasma ────────────────────────────────────────────────────────

  plasma(t) {
    const PATTERNS = [
      [0, 0, 0, 0, 0, 0, 0, 0],
      [0x88, 0, 0x22, 0, 0x88, 0, 0x22, 0],
      [0xaa, 0x55, 0xaa, 0x55, 0xaa, 0x55, 0xaa, 0x55],
      [0x77, 0xff, 0xdd, 0xff, 0x77, 0xff, 0xdd, 0xff],
      [0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff],
    ];
    const CYCLES = [
      [0, 6, 4, 14, 3, 13, 1, 13, 3, 14, 4, 6],
      [0, 9, 2, 8, 10, 7, 1, 7, 10, 8, 2, 9],
    ];
    const { px, level, beat, voices } = this;
    const cycle = CYCLES[Math.floor(t / 12) % CYCLES.length];
    const mx = 20 + Math.sin(t * 0.6) * 12;
    const my = 12.5 + Math.cos(t * 0.8) * 8;
    for (let cy = 0; cy < 25; cy++) {
      for (let cx = 0; cx < 40; cx++) {
        const d = Math.hypot(cx - mx, cy - my);
        let v = Math.sin(cx * 0.16 + t) + Math.sin(cy * 0.21 - t * 1.3) + Math.sin((cx + cy) * 0.11 + t * 0.7) + Math.sin(d * (0.3 + level * 0.25) - t * 2);
        v = Math.min(Math.max(v / 8 + 0.5, 0), 0.999);
        const pos = v * cycle.length * 1.5 + t * 1.5;
        const ip = Math.floor(pos);
        const fg = ABGR[cycle[(ip + 1) % cycle.length]];
        const bg = ABGR[cycle[ip % cycle.length]];
        const pat = PATTERNS[Math.floor((pos - ip) * PATTERNS.length)];
        for (let row = 0; row < 8; row++) {
          const bits = pat[row];
          const base = (cy * 8 + row) * W + cx * 8;
          for (let bit = 0; bit < 8; bit++) px[base + bit] = bits & (0x80 >> bit) ? fg : bg;
        }
      }
    }
    [RAMP_RED, RAMP_GREEN, RAMP_BLUE].forEach((r, v) => {
      const cy = 56 + v * 48;
      const half = 1 + voices[v] * 10;
      for (let y = Math.floor(cy - half); y < Math.floor(cy + half); y++) {
        this.fillRow(y, ramp(r, 0.3 + (1 - Math.abs(y - cy) / half) * 0.7));
      }
    });
    px.fill(ABGR[0], 0, 12 * W);
    this.fillRow(12, ramp(RAMP_FIRE, 0.6 + beat * 0.4));
    const label = this.title.toUpperCase();
    const tw = [...label].length * 8;
    const x = tw < W ? (W - tw) / 2 : 4 - ((t * 30) % (tw - W + 40));
    this.text(x, 9, label, ramp(RAMP_GREY, 0.7 + beat * 0.3));
  }
}
