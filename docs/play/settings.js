// settings - the look, sound, touch pad, the user's own ROMs and the CSDb
// relay. Changes to the sound are announced with a 'breadbin:sound' event.

import * as store from './store.js';
import { $, h, toast, closeButton, copyLink } from './ui.js';
import { THEMES, applyTheme, linkLook, currentLook, withLook } from './themes/themes.js';

const DEFAULTS = { theme: 'bakery', scanlines: true, hq: true, width: 0.35, volume: 0.9, relay: '', viz: 0, pad: matchMedia('(pointer: coarse)').matches };
export const settings = { ...DEFAULTS, ...store.load('settings', {}) };

export function save(patch) {
  Object.assign(settings, patch);
  // a look chosen here outlasts the one a link brought: the link's ?look leaves the address
  if (patch.theme && linkLook()) {
    const u = new URL(location.href);
    u.searchParams.delete('look');
    history.replaceState(history.state, '', u.href);
  }
  store.save('settings', settings);
}

applyTheme(linkLook() || settings.theme); // (a shared link's look for this visit, without changing the saved one)
/** Classic's CRT scan lines, on or off (classic.css reads the attribute). */
const scanlines = (on) => document.documentElement.toggleAttribute('data-no-scanlines', !on);
scanlines(settings.scanlines);

/** The looks, side by side, each a small picture of itself: pick one and the page changes at once. */
function themePicker() {
  return h('div.theme-picker', { role: 'radiogroup', 'aria-label': 'Look' }, THEMES.map((t) =>
    h('label.theme-choice', { 'data-look': t.id },
      h('input', { type: 'radio', name: 'theme', value: t.id, checked: currentLook() === t.id, onchange: () => save({ theme: applyTheme(t.id) }) }),
      h('span.theme-swatch', { 'aria-hidden': 'true' }, h('i'), h('i'), h('i')),
      h('b', {}, t.name),
      h('span', {}, t.about))));
}

const ROMS = [
  ['kernal', 'KERNAL (8 KB)', 8192],
  ['basic', 'BASIC (8 KB)', 8192],
  ['charset', 'Character ROM (4 KB)', 4096],
  ['floppy', '1541 drive ROM (16 KB)', 16384],
];

/** The user's ROMs; the system ROMs only as a complete set. */
export async function loadRoms() {
  const roms = {};
  for (const [k] of ROMS) {
    const b = await store.getBlob(`rom-${k}`);
    if (b) roms[k] = b;
  }
  if (!(roms.kernal && roms.basic && roms.charset)) delete roms.kernal;
  return roms;
}

function romRow([key, label, size]) {
  const state = h('span.state');
  const input = h('input', { type: 'file', hidden: true });
  const clear = h('button.key.pill', {}, 'Remove');
  const refresh = async () => {
    const b = await store.getBlob(`rom-${key}`);
    state.textContent = b ? 'installed' : 'not set';
    state.classList.toggle('ok', !!b);
    clear.hidden = !b;
  };
  input.addEventListener('change', async () => {
    const f = input.files[0];
    if (!f) return;
    const b = new Uint8Array(await f.arrayBuffer());
    // PRG-style dumps carry a 2-byte load address
    if (b.length !== size && b.length !== size + 2) return toast(`${f.name} is ${b.length} bytes; a ${label} is ${size}.`);
    await store.putBlob(`rom-${key}`, b.length === size ? b : b.subarray(2));
    refresh();
  });
  clear.addEventListener('click', () => store.deleteBlob(`rom-${key}`).then(refresh));
  refresh();
  return h('div.rom-row', {}, h('span', {}, label), state, h('span', {}, h('button.key.pill', { onclick: () => input.click() }, 'Choose…'), ' ', clear, input));
}

function slider(key, max) {
  const input = h('input', { type: 'range', min: 0, max, step: 0.05, value: settings[key] });
  input.addEventListener('input', () => {
    save({ [key]: Number(input.value) });
    dispatchEvent(new Event('breadbin:sound'));
  });
  return input;
}

/** What breadbin is built from and on, with the licence of what it runs. */
const CREDITS = [
  ['vc64web', 'https://github.com/vc64web/virtualc64web', 'by mithrendal, on VirtualC64 by Dirk W. Hoffmann: the emulator (GPL-3.0).'],
  ['MEGA65 Open ROMs', 'https://github.com/MEGA65/open-roms', 'the free KERNAL, BASIC and character set, patched by breadbin (LGPL-3.0).'],
  ['Pascual DOS', 'https://github.com/Pascual-Candel-Palazon/Pascual_DOS-1541', 'by Pascual Candel Palazón: the free 1541 DOS, with breadbin\'s job queue for trackmos (MIT).'],
  ['resid-rs', 'https://github.com/digitalstreamio/resid-rs', 'Rust port of reSID by Dag Lem: the SID chip of the player (GPL-3.0).'],
  ['Exomizer', 'https://bitbucket.org/magli143/exomizer', 'by Magnus Lind: its decruncher, ported to unpack EasySplit files (zlib-style licence).'],
  ['EasyFlash', 'https://skoe.de/easyflash/', 'by Thomas Giesel (skoe): the cartridge and EasySplit formats.'],
  ['blip-deck', 'https://github.com/jacobandresen/blip-deck', 'the control deck of the touch pad: its arcade stick and button (MIT).'],
  ['ACME', 'https://github.com/meonwax/acme', 'by Marco Baye: the cross-assembler of the Hack tab (GPL-2.0).'],
  ['GameBase64', 'https://www.gb64.com/', 'the games and their facts.'],
  ['Internet Archive', 'https://archive.org/details/softwarelibrary_c64', 'the games, demos and HVSC copy everything downloads from.'],
  ['CSDb', 'https://csdb.dk/', 'the scene database: ratings, groups and parties.'],
  ['High Voltage SID Collection', 'https://www.hvsc.de/', 'the music.'],
  ['libretro-thumbnails', 'https://github.com/libretro-thumbnails/Commodore_-_64', 'the box art.'],
  ['Pixelify Sans, Silkscreen, Figtree, Micro 5, VT323, Noto Emoji, Arimo, Cousine', 'https://fonts.google.com/', 'the type (SIL Open Font License; Arimo and Cousine, Apache 2.0).'],
  ['PXL font', 'https://github.com/MEGA65/open-roms', 'by Retrofan, from the MEGA65 Open ROMs: the Classic theme\'s type (LGPL-3.0).'],
];

export function openSettings() {
  const dlg = $('#settings');
  const hq = h('input', { type: 'checkbox', checked: settings.hq, onchange: () => save({ hq: hq.checked }) });
  const relay = h('input', { type: 'url', value: settings.relay, placeholder: 'https://r.jina.ai (default)', onchange: () => save({ relay: relay.value.trim() }) });
  const usage = h('span', {}, '…');
  store.cacheUsage().then((n) => (usage.textContent = `${(n / 1048576).toFixed(1)} MB in this browser.`));
  const clear = async () => {
    await store.clearCaches();
    toast('Cache cleared.');
    dlg.close();
  };
  dlg.replaceChildren(closeButton(dlg), h('div.sheet-body', {},
    h('h2', {}, 'Settings'),
    h('h3', {}, 'Look'),
    themePicker(),
    // (the Share buttons' links carry the look as well: this is the page you're on)
    h('p.theme-link', {}, h('button.key.pill.small', { type: 'button', onclick: () => copyLink(withLook(location.href), `🔗 Link to this page in ${THEMES.find((t) => t.id === currentLook()).name} copied.`) }, '🔗 Copy a link to this page in this look')),
    h('label.check', {}, h('input', { type: 'checkbox', checked: settings.scanlines, onchange: (e) => { save({ scanlines: e.target.checked }); scanlines(e.target.checked); } }), 'Scan lines on the Classic screen (off reads more crisply)'),
    h('h3', {}, 'Sound'),
    h('label.check', {}, hq, 'High-quality resampling (reSID band-limited; uses more CPU)'),
    h('label.field', {}, 'Stereo width', slider('width', 1)),
    h('label.field', {}, 'Volume', slider('volume', 1.4)),
    h('h3', {}, 'Your own ROMs (optional)'),
    h('p', {}, 'Everything on the shelves runs on free ROMs: the MEGA65 Open ROMs in the C64 and, for trackmos, Pascual DOS in the 1541. If you own the Commodore ROMs (C64 Forever includes them), you can add them here instead: they stay in this browser and go only to the emulator.'),
    ROMS.map(romRow),
    h('h3', {}, 'CSDb relay'),
    h('p', {}, 'csdb.dk only lets sites on its allowlist read it, so the details shown for tunes go through a relay: r.jina.ai by default, or your own, with {url} where the CSDb address goes (e.g. https://your-worker.workers.dev/?url={url}).'),
    h('label.field', {}, 'Relay URL', relay),
    h('h3', {}, 'Downloads'),
    h('p', {}, 'Games, demos, tunes and catalogue pages are cached so they open again without a download. ', usage),
    h('div.actions', {}, h('button.key.pill', { onclick: clear }, 'Clear cache')),
    h('h3', {}, 'Credits'),
    h('ul.credits', {}, CREDITS.map(([name, url, what]) => h('li', {}, h('a', { href: url, target: '_blank', rel: 'noopener' }, name), h('span', {}, what[0].toUpperCase() + what.slice(1))))),
    h('p', {}, 'And the coders, musicians, graphicians and crackers whose work this is: each card names them, from GameBase64 and CSDb.'),
  ));
  dlg.showModal();
}
