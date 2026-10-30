// emulator - the full-screen emulator view: fetch a game or demo, prepare it
// with the engine and start it in vc64web, with the touch pad on request.
// Launching announces itself with a 'breadbin:launch' event (Tunes pauses).

import * as ia from './ia.js';
import * as emu from './emu.js';
import * as store from './store.js';
import { pick, prepare, patchedKernal } from './engine.js';
import { createPad } from './touch.js';
import { pollGamepads } from './gamepad.js';
import { settings, save, loadRoms } from './settings.js';
import { $, h, pixel } from './ui.js';

let session = null;
let pad = null;
let port = 2; // the joystick port the keys, gamepad and pad drive
let stopGamepads = null;

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
    const kernal = roms.kernal ? null : await patchedKernal(store.cachedBytes);
    const file = { ...(await prepare(name, bytes, { canMount: !!roms.floppy, prefer: item.run || '' })), boot: item.boot };
    port = item.port || 2;
    store.recordPlay({ kind, id: item.id, item });
    session = emu.run(stage, file, {
      roms,
      kernal,
      port,
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
    stopGamepads?.();
    stopGamepads = pollGamepads((m) => session?.send(m), () => port);
    setPort(port);
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
  if (on && session) pad = createPad($('#emu'), (m) => session.send(m), { port });
}

/** Point the keys, gamepad and pad at joystick port `n`. */
function setPort(n) {
  port = n;
  session?.send({ bbport: n });
  pad?.setPort(n);
  $('#emu-port').textContent = `Port ${n}`;
  $('#emu-note').textContent = `Joystick in port ${n}: cursor keys + space, a gamepad or the pad. Doesn't respond? Try Port ${3 - n}.`;
}

$('#emu-port').addEventListener('click', () => {
  setPort(3 - port);
  session?.frame.focus();
});
$('#emu-pad').addEventListener('click', () => {
  save({ pad: !settings.pad });
  setPad(settings.pad);
});
$('#emu-close').addEventListener('click', () => {
  setPad(false);
  stopGamepads?.();
  stopGamepads = null;
  session?.stop();
  session = null;
  $('#emu').hidden = true;
  if (document.fullscreenElement) document.exitFullscreen();
});
$('#emu-full').addEventListener('click', () => {
  if (document.fullscreenElement) document.exitFullscreen();
  else $('#emu').requestFullscreen?.().then(() => session?.frame.focus());
});
