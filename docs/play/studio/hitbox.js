// hitbox - the ▢ overlay over the C64: each sprite's box, and the boxes the
// game tests with bb_overlap() (the runtime keeps the last 8 calls' boxes in
// bb_hits, bbfast.s), red where two of them overlap. "Why did that hit me?"
// answered on the screen.
//
// Where a VIC pixel lands: vc64web draws the C64's picture into a 384x270
// canvas (at the Studio's border, 0.1), the VIC's sprite position (x, y)
// at canvas pixel (x + 8, y - 15). Measured with a solid sprite at (100, 100)
// and the screen's edges (sprite position 24, 50 for the top left corner).
// The screen coordinates bb_spr and bb_overlap take are the VIC's minus
// (24, 50).

const CANVAS = { w: 384, h: 270, dx: 8, dy: -15 };
const RECENT = 10; // frames: a bb_overlap box older than that is not drawn

/** The boxes to draw from the VIC's registers ($D000-$D02E) and the hits ring: { sprites, tests }. */
export function boxes(vic, hits = null, hitsN = 0, frame = 0) {
  const sprites = [];
  for (let n = 0; n < 8; n++) {
    if (!(vic[0x15] & (1 << n))) continue;
    const x = vic[n * 2] + (vic[0x10] & (1 << n) ? 256 : 0);
    const y = vic[n * 2 + 1];
    sprites.push({ n, x, y, w: vic[0x1d] & (1 << n) ? 48 : 24, h: vic[0x17] & (1 << n) ? 42 : 21 });
  }
  const tests = [];
  if (hits) {
    for (let k = 0; k < 8; k++) {
      const i = ((hitsN + k) & 7) * 16; // oldest first
      const age = (frame - hits[i + 12]) & 255;
      if (age > RECENT || hits.slice(i, i + 13).every((b) => b === 0)) continue;
      const w16 = (lo) => {
        const v = hits[i + lo] | (hits[i + lo + 1] << 8);
        return v > 32767 ? v - 65536 : v;
      };
      const a = { x: w16(9) + 24, y: w16(7) + 50, w: hits[i + 6], h: hits[i + 5] };
      const b = { x: w16(3) + 24, y: w16(1) + 50, w: hits[i], h: hits[i + 11] };
      const hit = a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;
      tests.push({ a, b, hit });
    }
  }
  return { sprites, tests };
}

export class HitboxOverlay {
  constructor() {
    this.canvas = document.createElement('canvas');
    this.canvas.className = 'studio-hitbox';
    this.canvas.hidden = true;
    this.on = false;
    this.session = null;
    this.labels = null;
  }

  attach(session, labels) {
    this.session = session;
    this.labels = labels;
  }

  set(on) {
    this.on = on;
    this.canvas.hidden = !on;
    cancelAnimationFrame(this.raf);
    if (on) this.loop();
  }

  loop() {
    this.raf = requestAnimationFrame(async () => {
      if (!this.on) return;
      if (!this.busy) {
        this.busy = true;
        try {
          await this.draw();
        } catch {}
        this.busy = false;
      }
      this.loop();
    });
  }

  async draw() {
    const s = this.session;
    const frame = s?.frame;
    const ctx = this.canvas.getContext('2d');
    const host = this.canvas.parentElement;
    if (!s?.started || !frame || !host) return ctx.clearRect(0, 0, this.canvas.width, this.canvas.height);
    const l = this.labels || {};
    const ranges = [[0xd000, 0x2f]];
    const ring = l._bb_hits !== undefined && l._bb_hits_n !== undefined && l._bb_frames !== undefined;
    if (ring) ranges.push([l._bb_hits, 128], [l._bb_hits_n, 1], [l._bb_frames, 1]);
    const got = await s.peek(ranges);
    if (!got) return;
    const { sprites, tests } = boxes(got[0], ring ? got[1] : null, ring ? got[2][0] : 0, ring ? got[3][0] : 0);
    // the canvas over the iframe's picture (its canvas starts a pixel down)
    const f = frame.getBoundingClientRect();
    const hb = host.getBoundingClientRect();
    const dpr = devicePixelRatio || 1;
    const W = Math.round(f.width * dpr);
    const H = Math.round(f.height * dpr);
    if (this.canvas.width !== W || this.canvas.height !== H) [this.canvas.width, this.canvas.height] = [W, H];
    Object.assign(this.canvas.style, { left: `${f.left - hb.left}px`, top: `${f.top - hb.top}px`, width: `${f.width}px`, height: `${f.height}px` });
    const sx = (f.width / CANVAS.w) * dpr;
    const sy = ((f.height - 2) / CANVAS.h) * dpr;
    const rect = (x, y, w, h) => [(x + CANVAS.dx) * sx, (y + CANVAS.dy) * sy + dpr, w * sx, h * sy];
    ctx.clearRect(0, 0, W, H);
    ctx.lineWidth = Math.max(1, dpr);
    ctx.font = `${Math.round(10 * dpr)}px monospace`;
    for (const sp of sprites) {
      const [x, y, w, h] = rect(sp.x, sp.y, sp.w, sp.h);
      ctx.strokeStyle = 'rgba(131, 165, 152, 0.9)';
      ctx.strokeRect(x + 0.5, y + 0.5, w, h);
      ctx.fillStyle = 'rgba(131, 165, 152, 0.95)';
      ctx.fillText(String(sp.n), x + 2 * dpr, y - 2 * dpr);
    }
    for (const t of tests) {
      for (const [box, colour] of [[t.a, '#fabd2f'], [t.b, '#fe8019']]) {
        const [x, y, w, h] = rect(box.x, box.y, box.w, box.h);
        ctx.strokeStyle = colour;
        ctx.strokeRect(x + 0.5, y + 0.5, w, h);
      }
      if (t.hit) {
        const x0 = Math.max(t.a.x, t.b.x);
        const y0 = Math.max(t.a.y, t.b.y);
        const [x, y, w, h] = rect(x0, y0, Math.min(t.a.x + t.a.w, t.b.x + t.b.w) - x0, Math.min(t.a.y + t.a.h, t.b.y + t.b.h) - y0);
        ctx.fillStyle = 'rgba(251, 73, 52, 0.55)';
        ctx.fillRect(x, y, w, h);
      }
    }
  }
}
