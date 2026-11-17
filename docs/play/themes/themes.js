// themes - breadbin's looks. A theme is a stylesheet of tokens (and, for
// more than colours, rules in the `theme` layer) switched on by
// <html data-theme>; index.html sets the attribute before the first paint,
// from the saved settings, so a page never flashes the other look.

export const THEMES = [
  { id: 'bakery', name: 'Midnight bakery', about: 'breadbin\'s own: plum night, warm toast, retrobox code', color: '#120f1c' },
  { id: 'classic', name: 'Classic', about: 'breadbin on a C64: light blue on blue, the C64\'s own 8x8 type, big pixels', color: '#6c5eb5', pixels: 'c64' },
  { id: 'geos', name: 'GEOS', about: 'the desktop of a 1986 C64: grey dither, striped title bars, bitmap type', color: '#e3e3e3', clock: 'geos', desktop: 'geos' },
  { id: 'chicago', name: 'Chicago', about: 'a tribute to the desktop of 1995: teal, grey bevels, navy title bars, a Start menu, a taskbar with a clock', color: '#008080', clock: 'tray', desktop: 'chicago' },
  { id: 'cupertino', name: 'Cupertino', about: 'a tribute to the first desktops of 1984: black on white, a grey desk, striped title bars, pictures in dots', color: '#ffffff', clock: 'tray', pixels: 'mac', desktop: 'cupertino' },
];

let pixelating = false;
let desktop = false;

/** Put theme `id` on the page: the attribute, the browser's bar colour, a 'breadbin:theme' event. */
/** The look a shared link asks for (?look=chicago), if it names one; it wins over the saved look while it's in the address. */
export function linkLook() {
  const id = new URLSearchParams(location.search).get('look');
  return THEMES.some((t) => t.id === id) ? id : null;
}

/** The look showing now. */
export const currentLook = () => document.documentElement.dataset.theme || THEMES[0].id;

/** `url` (a string) carrying the look showing now, so whoever opens it sees the page as it looks here; the default look needs none. */
export function withLook(url) {
  const u = new URL(url, location.href);
  if (currentLook() === THEMES[0].id) u.searchParams.delete('look');
  else u.searchParams.set('look', currentLook());
  return u.href;
}

export function applyTheme(id) {
  const theme = THEMES.find((t) => t.id === id) || THEMES[0];
  const root = document.documentElement;
  if (theme.id === THEMES[0].id) delete root.dataset.theme;
  else root.dataset.theme = theme.id;
  document.querySelector('meta[name="theme-color"]')?.setAttribute('content', theme.color);
  clock(theme.clock);
  // Classic draws pictures the C64's way, Cupertino in black and white dots (pixelate.js, loaded only for them)
  if (theme.pixels) import('./pixelate.js').then((m) => m.start(theme.pixels));
  else if (pixelating) import('./pixelate.js').then((m) => m.stop());
  pixelating = Boolean(theme.pixels);
  // Chicago's Start menu, desk icons and little programs, Cupertino's and GEOS's menus and desk accessories (desktop.js)
  if (theme.desktop || desktop) import('./desktop.js').then((m) => (theme.desktop ? m.start(theme.desktop) : m.stop()));
  desktop = Boolean(theme.desktop);
  dispatchEvent(new CustomEvent('breadbin:theme', { detail: theme.id }));
  return theme.id;
}

// A desktop's clock in the header (the theme's CSS puts it in place): GEOS's
// "11/16/26  10:24 AM" in its menu bar, or the time alone, "10:24 PM", in a taskbar's tray.
const CLOCKS = {
  geos: (d, time) => `${two(d.getMonth() + 1)}/${two(d.getDate())}/${two(d.getFullYear() % 100)}  ${two(d.getHours() % 12 || 12)}:${time}`,
  tray: (d, time) => `${d.getHours() % 12 || 12}:${time}`,
};
const two = (n) => String(n).padStart(2, '0');
let tick = 0;
function clock(kind) {
  let el = document.querySelector('.theme-clock');
  clearInterval(tick);
  if (!CLOCKS[kind]) return el?.remove();
  if (!el) {
    el = document.createElement('time');
    el.className = 'theme-clock';
    document.querySelector('.case-tools')?.append(el);
  }
  const show = () => {
    const d = new Date();
    el.textContent = CLOCKS[kind](d, `${two(d.getMinutes())} ${d.getHours() < 12 ? 'AM' : 'PM'}`);
    el.dateTime = d.toISOString();
  };
  show();
  tick = setInterval(show, 10e3);
}
