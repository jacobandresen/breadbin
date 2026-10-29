// settings - sound, touch pad, the user's own ROMs and the CSDb relay. Changes
// to the sound are announced with a 'breadbin:sound' event.

import * as store from './store.js';
import { $, h, toast, closeButton } from './ui.js';

const DEFAULTS = { hq: true, width: 0.35, volume: 0.9, relay: '', viz: 0, pad: matchMedia('(pointer: coarse)').matches };
export const settings = { ...DEFAULTS, ...store.load('settings', {}) };

export function save(patch) {
  Object.assign(settings, patch);
  store.save('settings', settings);
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
    h('h3', {}, 'Sound'),
    h('label.check', {}, hq, 'High-quality resampling (reSID band-limited; uses more CPU)'),
    h('label.field', {}, 'Stereo width', slider('width', 1)),
    h('label.field', {}, 'Volume', slider('volume', 1.4)),
    h('h3', {}, 'Your own ROMs (optional)'),
    h('p', {}, 'Everything on the shelves runs on the free MEGA65 Open ROMs. If you own the Commodore ROMs (C64 Forever includes them), add them here for titles with their own fast loaders: they stay in this browser and go only to the emulator.'),
    ROMS.map(romRow),
    h('h3', {}, 'CSDb relay'),
    h('p', {}, 'csdb.dk only lets sites on its allowlist read it, so the details shown for demos and tunes go through a relay: r.jina.ai by default, or your own, with {url} where the CSDb address goes (e.g. https://your-worker.workers.dev/?url={url}).'),
    h('label.field', {}, 'Relay URL', relay),
    h('h3', {}, 'Downloads'),
    h('p', {}, 'Games, demos, tunes and catalogue pages are cached so they open again without a download. ', usage),
    h('div.actions', {}, h('button.key.pill', { onclick: clear }, 'Clear cache')),
    h('h3', {}, 'Where everything comes from'),
    h('p', {}, 'Games: GameBase64 and the Internet Archive. Demos: the Internet Archive, ranked by CSDb. Music: the High Voltage SID Collection, ranked by CSDb. Box art: libretro-thumbnails. Emulator: vc64web with the MEGA65 Open ROMs. Engine (SID player, virtual drive, visualisers): breadbin\'s own Rust, compiled to WebAssembly.'),
  ));
  dlg.showModal();
}
