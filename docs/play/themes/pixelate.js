// pixelate - the Classic theme shows pictures as a C64 would have: on its
// grid of pixels (each as big as the theme's --px; WIDE makes them multicolour
// mode's double-wide ones), in its 16 colours, the shades between them dithered in the ordered
// checkerboard C64 artists used. Box art is far finer than a C64 could draw;
// a screenshot, already the C64's, is only snapped back to its colours.
//
// The C64 version of a picture replaces its src (a blob: URL) and the
// original comes back when the theme goes. Reading a picture's pixels needs
// its host to allow it (CORS): a picture from one that doesn't stays as it is.

/** The C64's 16 colours (Pepto's palette, as studio.css has them). */
const PALETTE = [
  0x000000, 0xffffff, 0x68372b, 0x70a4b2, 0x6f3d86, 0x588d43, 0x352879, 0xb8c76f,
  0x6f4f25, 0x433900, 0x9a6759, 0x444444, 0x6c6c6c, 0x9ad284, 0x6c5eb5, 0x959595,
].map((c) => [c >> 16, (c >> 8) & 255, c & 255]);
/** Bayer's 4x4 order: how strongly each pixel of a 4x4 tile leans to the lighter colour. */
const BAYER = [0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5].map((n) => (n + 0.5) / 16 - 0.5);
const DITHER = 28; // how far a pixel may lean (in 0-255 steps): mixes the shades between colours, leaves a flat colour flat
/** Screen columns to a C64 pixel: 1 is hires bitmap mode (320 across), 2 multicolour's wide pixels (160). Hires keeps a small cover recognisable. */
const WIDE = 1;
const PICTURES = '.art img, .np-shot img';

/** The C64 colour nearest to r, g, b ("redmean": distance weighted as eyes weigh it). */
function nearest(r, g, b) {
  let best = 0;
  let bestD = Infinity;
  for (let i = 0; i < 16; i++) {
    const [R, G, B] = PALETTE[i];
    const m = (r + R) / 2;
    const d = (2 + m / 256) * (r - R) ** 2 + 4 * (g - G) ** 2 + (2 + (255 - m) / 256) * (b - B) ** 2;
    if (d < bestD) (bestD = d), (best = i);
  }
  return PALETTE[best];
}

/** `src` (a loaded, readable image) as `cols` x `rows` C64 pixels, WIDE screen columns each; a canvas. */
function c64ify(src, cols, rows, dither = DITHER) {
  const small = new OffscreenCanvas(cols, rows);
  const sx = small.getContext('2d', { willReadFrequently: true });
  sx.imageSmoothingQuality = 'high'; // each C64 pixel the average of what it covers
  sx.drawImage(src, 0, 0, cols, rows);
  const img = sx.getImageData(0, 0, cols, rows);
  const d = img.data;
  for (let y = 0; y < rows; y++) {
    for (let x = 0; x < cols; x++) {
      const i = (y * cols + x) * 4;
      const lean = BAYER[(y & 3) * 4 + (x & 3)] * dither;
      [d[i], d[i + 1], d[i + 2]] = nearest(d[i] + lean, d[i + 1] + lean, d[i + 2] + lean);
      d[i + 3] = 255;
    }
  }
  sx.putImageData(img, 0, 0);
  // WIDE screen columns a pixel; and at least 48 wide, scaled whole (ui.js takes a picture under 40 for a broken one)
  const k = Math.ceil(48 / (cols * WIDE));
  const out = new OffscreenCanvas(cols * WIDE * k, rows * k);
  const ox = out.getContext('2d');
  ox.imageSmoothingEnabled = false; // the pixels stay hard-edged
  ox.drawImage(small, 0, 0, cols * WIDE * k, rows * k);
  return out;
}

/** The grid `img` shows its picture on: how many C64 pixels across and down, as it is laid out. */
function grid(img) {
  const px = parseFloat(getComputedStyle(img).getPropertyValue('--px')) || 3;
  const box = img.getBoundingClientRect();
  const fit = getComputedStyle(img).objectFit;
  const scale = (fit === 'contain' ? Math.min : Math.max)(box.width / img.naturalWidth, box.height / img.naturalHeight);
  return { cols: Math.max(4, Math.round((img.naturalWidth * scale) / px / WIDE)), rows: Math.max(4, Math.round((img.naturalHeight * scale) / px)) };
}

/** How `img` is redrawn: { cols, rows, screen } on its grid, or null when it is let be. */
function plan(img) {
  // a C64 screen (a screenshot, often a blurred thumbnail of one, up to 403x284 with its border): its colours are
  // the C64's already, give or take the JPEG - snapped back to them, no dither, and never more pixels than it has
  const screen = img.naturalWidth <= 403 && img.naturalHeight <= 284 && img.naturalWidth >= img.naturalHeight;
  const { cols, rows } = grid(img);
  if (screen && cols * WIDE > img.naturalWidth) return { screen, cols: Math.floor(img.naturalWidth / WIDE), rows: Math.round((rows * img.naturalWidth) / (cols * WIDE)) };
  if (!screen && img.naturalWidth <= cols * WIDE * 1.5) return null; // no finer than the grid
  return { screen, cols, rows };
}

async function convert(img) {
  if (img.dataset.c64 || !img.naturalWidth || !img.isConnected) return;
  const how = plan(img);
  if (!how) return done(img, 'as-is');
  const original = img.currentSrc || img.src;
  img.dataset.c64 = original;
  try {
    const copy = new Image();
    copy.crossOrigin = 'anonymous'; // pixels can be read only from a host that allows it
    copy.referrerPolicy = 'no-referrer';
    copy.src = readable(original);
    await copy.decode();
    const blob = await c64ify(copy, how.cols, how.rows, how.screen ? 0 : DITHER).convertToBlob();
    // the theme went, or another picture went in meanwhile (ui.js tries the next when one fails): that one gets its own turn
    if (img.dataset.c64 !== original || (img.currentSrc || img.src) !== original || !on) return;
    img.src = URL.createObjectURL(blob);
    done(img);
  } catch {
    done(img, 'unreadable'); // shown as it came, rather than not at all
  }
}

/** `img` may show now (classic.css keeps a picture hidden until then: no flash of the fine original). */
function done(img, why) {
  if (why) img.dataset.c64 = why;
  img.dataset.c64Done = '';
}

/** Where `url`'s pixels can be read: archive.org's thumbnails come with no CORS, the same file through /cors/ does. */
function readable(url) {
  const thumb = /^https:\/\/archive\.org\/services\/img\/([^/?#]+)/.exec(url);
  return thumb ? `https://archive.org/cors/${thumb[1]}/__ia_thumb.jpg` : url;
}

let on = false;
function onLoad({ target: img }) {
  if (!on || !img.matches?.(PICTURES) || img.src.startsWith('blob:')) return;
  // another picture in it now (ui.js falls back down its list): this one is judged afresh
  if (img.dataset.c64 && img.dataset.c64 !== img.src) delete img.dataset.c64, delete img.dataset.c64Done;
  convert(img);
}

/** Classic on: every picture, now and as they load, the C64's way. */
export function start() {
  if (on) return;
  on = true;
  // a load doesn't bubble, but it can be caught on its way down - from the document: a "load" never reaches window
  document.addEventListener('load', onLoad, true);
  for (const img of document.querySelectorAll(PICTURES)) if (img.complete) convert(img);
}

/** Classic off: the pictures as they came. */
export function stop() {
  on = false;
  document.removeEventListener('load', onLoad, true);
  for (const img of document.querySelectorAll('img[data-c64]')) {
    const was = img.dataset.c64;
    delete img.dataset.c64;
    delete img.dataset.c64Done;
    if (img.src.startsWith('blob:')) URL.revokeObjectURL(img.src), (img.src = was);
  }
}
