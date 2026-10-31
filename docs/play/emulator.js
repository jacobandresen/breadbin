// emulator - the full-screen emulator view: fetch a game or demo, prepare it
// with the engine and start it in vc64web, with the touch pad on request.
// Launching announces itself with a 'breadbin:launch' event (Tunes pauses).

import * as ia from './ia.js';
import * as emu from './emu.js';
import * as store from './store.js';
import { pick, prepare, patchedKernal, joinSplit } from './engine.js';
import { createPad } from './touch.js';
import { swapScript } from './drive.js';
import { scroller } from './scroller.js';
import { pollGamepads } from './gamepad.js';
import { settings, save, loadRoms } from './settings.js';
import { $, h, pixel } from './ui.js';

let session = null;
let pad = null;
let port = 2; // the joystick port the keys, gamepad and pad drive
let stopGamepads = null;
let swapDisk = null; // puts the next side of a multi-disk release in the drive
let intro = null; // the scroller for a game whose loader breadbin patched

/** The file to boot from an Internet Archive item. */
async function archiveFile(id, progress) {
  const meta = await ia.metadata(id);
  const name = await pick((meta.files || []).map((f) => f.name), meta.metadata?.emulator_start);
  if (!name) throw new Error('this item has no C64 disk, tape or program file');
  return { name, bytes: await ia.download(id, name, progress) };
}

/**
 * A release spread over several Internet Archive items (`item.parts`), all
 * downloaded: a cartridge EasySplit cut across disks, joined into one (an
 * EasyFlash .crt, which needs no drive), or else the sides of a disk game,
 * starting with the first.
 */
async function releaseParts(item, progress) {
  const disks = [];
  for (const [i, id] of item.parts.entries()) disks.push(await archiveFile(id, (got, total) => progress(got + i * total, total * item.parts.length)));
  try {
    return { name: `${item.title.replace(/[^\w]+/g, '_')}.crt`, bytes: await joinSplit(disks.map((d) => d.bytes)) };
  } catch {
    return { ...disks[0], sides: disks };
  }
}

/**
 * breadbin's free 1541 DOS (tools/drive), for what runs code on the drive:
 * Pascual DOS with the job queue and entry points fast loaders use.
 */
async function freeDrive() {
  return new Uint8Array(await (await fetch(new URL('dos1541.rom', import.meta.url))).arrayBuffer());
}

/** The scroller for a game whose loader breadbin patched. */
const hackedBy = (title) =>
  `HACKED BY CLAUDE CODE ... ${title.toUpperCase()} HAD ITS LOADER PATCHED TO RUN ON THE FREE MEGA65 OPEN ROMS AND BREADBIN'S VIRTUAL DRIVE, NO COMMODORE ROMS NEEDED ... GREETINGS TO THE MEGA65 TEAM, VICE, EXOMIZER, EASYFLASH AND THE SCENE THAT KEPT THESE GAMES ALIVE ...      `;

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
  if (item.wip) note.textContent = `Work in progress: this one didn't pass breadbin's playtest yet, so it may not start or play right.`;
  const bar = h('i');
  const label = h('div', {}, `SEARCHING FOR ${item.title.toUpperCase()}`);
  stage.replaceChildren(h('div.emu-loading', {}, label, h('div.bar', {}, bar)));
  $('#emu').hidden = false;
  const progress = (got, total) => {
    label.textContent = 'LOADING';
    bar.style.width = total ? `${Math.round((got / total) * 100)}%` : `${Math.min(95, got / 2000)}%`;
  };
  try {
    const { name, bytes, sides } = item.gb
      ? { name: item.gb.split('/').pop(), bytes: await ia.gb64Game(item.gb, progress) }
      : item.parts
        ? await releaseParts(item, progress)
        : await archiveFile(item.ia || item.id, progress);
    bar.style.width = '100%';
    const roms = await loadRoms();
    // a trackmo or a game whose loader runs on the drive gets a real 1541
    if (item.rom1541 && !roms.floppy) roms.floppy = await freeDrive();
    const kernal = roms.kernal ? null : await patchedKernal(store.cachedBytes, !!roms.floppy);
    const file = { ...(await prepare(name, bytes, { canMount: !!roms.floppy, prefer: item.run || '' })), boot: item.boot, patches: item.patches };
    port = item.port || 2;
    const all = sides || (file.disks.length > 1 && file.disks.map((d) => ({ name: d, zip: [name, bytes] }))) || [];
    setDisks(file, all);
    intro?.stop();
    intro?.el.remove();
    intro = item.patches?.length ? scroller(hackedBy(item.title)) : null;
    if (intro) $('#emu .emu-bar').after(intro.el);
    // the virtual drive also gets the other sides, and turns the disk over itself
    // when a game asks for a file that is on one of them
    if (file.vdrive) file.others = await Promise.all(all.filter((d) => d.name !== file.name).map(async (d) => d.bytes || (await prepare(...d.zip, { prefer: d.name })).bytes));
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

/**
 * With more than one side ({ name, bytes }, or { name, zip: [name, bytes] } for
 * a disk inside the zip), the Disk button puts the next one in drive 8, as a
 * game's "insert side 2" asks: into breadbin's virtual drive, or vc64web's.
 */
function setDisks(file, sides) {
  const button = $('#emu-disk');
  let at = Math.max(0, sides.findIndex((d) => d.name === file.name));
  button.hidden = sides.length < 2;
  const show = () => (button.textContent = `💾 Disk ${at + 1}/${sides.length}`);
  show();
  swapDisk = async () => {
    at = (at + 1) % sides.length;
    show();
    const side = sides[at];
    const bytes = side.bytes || (await prepare(...side.zip, { canMount: true, prefer: side.name })).bytes;
    if (file.vdrive) session?.send({ cmd: 'script', script: swapScript(bytes) });
    else session?.send({ cmd: 'load', file_name: side.name, file: bytes });
  };
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
$('#emu-disk').addEventListener('click', () => {
  swapDisk?.();
  session?.frame.focus();
});
$('#emu-pad').addEventListener('click', () => {
  save({ pad: !settings.pad });
  setPad(settings.pad);
});
$('#emu-close').addEventListener('click', () => {
  intro?.stop();
  intro?.el.remove();
  intro = null;
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
