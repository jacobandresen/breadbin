// scroller - a cracktro strip under the player bar for games whose loader
// breadbin patched (games.txt's "{...}"): copper bars and a sine scroller, as
// crack groups signed their work. Still text when motion is reduced.

const COPPER = ['#7a3216', '#c4622d', '#f2a23c', '#ffe7ad', '#f2a23c', '#c4622d', '#7a3216'];
const RAINBOW = ['#ff7777', '#ffa04f', '#eeee77', '#aaff66', '#67b6bd', '#a5a5ff', '#ee77ee'];

/** A scroller canvas saying `text`; `stop()` ends its animation. */
export function scroller(text) {
  const canvas = document.createElement('canvas');
  canvas.className = 'emu-scroller';
  canvas.setAttribute('role', 'img');
  canvas.setAttribute('aria-label', text);
  const g = canvas.getContext('2d');
  const still = matchMedia('(prefers-reduced-motion: reduce)').matches;
  const font = '700 16px "Pixelify Sans", ui-monospace, monospace';
  let x = null, raf = 0, last = 0;

  const frame = (now) => {
    const dpr = devicePixelRatio || 1;
    const w = canvas.clientWidth, h = canvas.clientHeight;
    if (canvas.width !== w * dpr) [canvas.width, canvas.height] = [w * dpr, h * dpr];
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    g.fillStyle = '#000';
    g.fillRect(0, 0, w, h);
    // copper bars rolling through the strip
    for (let b = 0; b < 2; b++) {
      const y = h / 2 + Math.sin(now / 700 + b * Math.PI) * (h / 3) - COPPER.length;
      COPPER.forEach((c, i) => ((g.fillStyle = c), g.fillRect(0, y + i * 2, w, 2)));
    }
    g.font = font;
    g.textBaseline = 'middle';
    if (still) {
      g.fillStyle = '#eeee77';
      g.textAlign = 'center';
      g.fillText(text.split('...')[0].trim(), w / 2, h / 2);
      return;
    }
    const step = g.measureText('M').width;
    if (x === null) x = w;
    x -= ((now - (last || now)) / 1000) * 90;
    last = now;
    if (x < -step * text.length) x = w;
    g.textAlign = 'left';
    for (let i = 0; i < text.length; i++) {
      const cx = x + i * step;
      if (cx < -step || cx > w) continue;
      g.fillStyle = RAINBOW[(i + Math.floor(now / 120)) % RAINBOW.length];
      g.fillText(text[i], cx, h / 2 + Math.sin(now / 260 + i / 3) * (h / 6));
    }
    raf = requestAnimationFrame(frame);
  };
  document.fonts.load(font).finally(() => (raf = requestAnimationFrame(frame)));
  return { el: canvas, stop: () => cancelAnimationFrame(raf) };
}
