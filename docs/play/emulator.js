// emulator - the full-screen emulator view: fetch a game or demo, prepare it
// with the engine and start it in vc64web, with the touch pad on request.
// Launching announces itself with a 'breadbin:launch' event (Tunes pauses).

import * as ia from './ia.js';
import * as emu from './emu.js';
import * as store from './store.js';
import { pick, prepare } from './engine.js';
import { createPad } from './touch.js';
import { settings, save, loadRoms } from './settings.js';
import { $, h, pixel } from './ui.js';

let session = null;
let pad = null;

/** The file to boot from an Internet Archive item. */
async function archiveFile(id, progress) {
  const meta = await ia.metadata(id);
  const name = await pick((meta.files || []).map((f) => f.name), meta.metadata?.emulator_start);
  if (!name) throw new Error('this item has no C64 disk, tape or program file');
  return { name, bytes: await ia.download(id, name, progress) };
}

/** Start `item` (a game from GameBase64 or the archive, or a demo). */
export async function launch(kind, item) {
  $('#detail').close();
  dispatchEvent(new Event('breadbin:launch'));
  const [stage, status, note] = [$('#emu-stage'), $('#emu-status'), $('#emu-note')];
  $('#emu-title').textContent = pixel(item.title);
  const online = $('#emu-online');
  online.hidden = kind !== 'game' || !ia.onlineAt(item);
  if (!online.hidden) online.href = ia.onlineAt(item);
  note.textContent = status.textContent = '';
  const bar = h('i');
  const label = h('div', {}, `SEARCHING FOR ${item.title.toUpperCase()}`);
  stage.replaceChildren(h('div.emu-loading', {}, label, h('div.bar', {}, bar)));
  $('#emu').hidden = false;
  const progress = (got, total) => {
    label.textContent = 'LOADING';
    bar.style.width = total ? `${Math.round((got / total) * 100)}%` : `${Math.min(95, got / 2000)}%`;
  };
  try {
    const { name, bytes } = item.gb
      ? { name: item.gb.split('/').pop(), bytes: await ia.gb64Game(item.gb, progress) }
      : await archiveFile(item.ia || item.id, progress);
    bar.style.width = '100%';
    const roms = await loadRoms();
    const file = await prepare(name, bytes, { canMount: !!roms.floppy, prefer: item.run || '' });
    note.textContent = 'Joystick in port 2: cursor keys + space, a gamepad or the pad.';
    store.recordPlay({ kind, id: item.id, item });
    session = emu.run(stage, file, {
      roms,
      onState: (s) => (status.textContent = s.running ? (s.warping ? 'warp' : 'running') : 'paused'),
      onDrive: (d) => {
        if (d.event === 'load') status.textContent = `loaded ${d.name}`;
        if (d.event === 'missing') note.textContent = `The program asked the drive for "${d.name}", which isn't on this disk.`;
      },
      onAlert: (text) => {
        note.textContent = /jammed/.test(text) ? 'The program crashed the C64 (its CPU jammed), so the emulator restarted it.' : text;
      },
    });
    status.textContent = 'starting…';
    setPad(settings.pad);
  } catch (e) {
    stage.replaceChildren(h('div.emu-error', {}, h('b', {}, '?LOAD ERROR'), e.message));
  }
}

/** Show or hide the on-screen joystick for the running session. */
function setPad(on) {
  pad?.destroy();
  pad = null;
  $('#emu').classList.toggle('touch', on);
  $('#emu-pad').setAttribute('aria-pressed', String(on));
  if (on && session) pad = createPad($('#emu'), (m) => session.send(m));
}

$('#emu-pad').addEventListener('click', () => {
  save({ pad: !settings.pad });
  setPad(settings.pad);
});
$('#emu-close').addEventListener('click', () => {
  setPad(false);
  session?.stop();
  session = null;
  $('#emu').hidden = true;
  if (document.fullscreenElement) document.exitFullscreen();
});
$('#emu-full').addEventListener('click', () => {
  if (document.fullscreenElement) document.exitFullscreen();
  else $('#emu').requestFullscreen?.().then(() => session?.frame.focus());
});
