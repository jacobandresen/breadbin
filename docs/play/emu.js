// emu - run a program in vc64web (VirtualC64 compiled to WebAssembly) inside an
// iframe. vc64web boots with the MEGA65 Open ROMs when no Commodore ROMs are
// present, so nothing licensed is needed. The file goes over postMessage,
// following vc64web's own embedding protocol (js/vc64web_player.js).

import { driveScripts } from './drive.js';
import { engineBytes } from './engine.js';
import { PAD_LISTENER } from './touch.js';

const VC64WEB = 'https://vc64web.github.io/';
const ORIGIN = new URL(VC64WEB).origin;
// vc64web reports problems (a jammed CPU, say) with alert(), which would freeze the frame
const ALERTS = "window.alert = (text) => parent.postMessage({ msg: 'breadbin_alert', text: String(text) }, '*');";
// tells the page when the KERNAL at $F71C reads BYTE (the patched one is in) and the C64 has rebooted
const KERNAL_IN = `(async () => {
  for (let i = 0; i < 60 && wasm_peek(0xf71c) !== BYTE; i++) await new Promise((r) => setTimeout(r, 50));
  await new Promise((r) => setTimeout(r, 500));
  parent.postMessage({ msg: 'breadbin_kernal' }, '*');
})();`;
// script lines: wait (up to 10 s) until READY is on the screen; readies() counts them
const WAIT_READY = `const readies = () => { let n = 0; for (let a = 0x400; a < 0x7e3; a++) if (wasm_peek(a) === 0x12 && wasm_peek(a + 1) === 5 && wasm_peek(a + 2) === 1 && wasm_peek(a + 3) === 4 && wasm_peek(a + 4) === 0x19) n++; return n; };
        for (let i = 0; i < 100 && !readies(); i++) await new Promise((r) => setTimeout(r, 100));`;
// vc64web defaults to reSID's fast (aliasing) sampling; resample sounds clean,
// interpolate is the lighter choice for phones
const SID_QUALITY = `wasm_set_sid_engine('ReSID ${matchMedia('(pointer: coarse)').matches ? 'interpolate' : 'resample'}');`;

/**
 * Start `file` ({ name, bytes, vdrive?, boot?, patches?, others? }) in a new
 * vc64web frame inside `host`. With `vdrive` the D64 is served by breadbin's
 * virtual drive instead of being mounted, starting with the file named `boot`
 * (else the first), with live `patches` for the game's loader and the
 * release's `others` disks (drive.js). `roms`
 * optionally carries the user's own { kernal, basic, charset, floppy }; without them, `kernal` is the patched Open ROMs KERNAL. `port` is
 * the joystick port (1 or 2) the controls drive.
 * Returns { frame, send(), stop() }.
 */
export function run(host, file, { roms = {}, kernal = null, port = 2, onState, onDrive, onAlert } = {}) {
  const disk = !file.vdrive && /\.(d64|g64)$/i.test(file.name);
  const config = {
    openROMS: !roms.kernal,
    dialog_on_missing_roms: false,
    // For disks, skip the "insert disk" dialog; LOAD/RUN is typed by the script below.
    ...(disk ? { dialog_on_disk: false } : {}),
    navbar: false,
    [`port${port}`]: true, // the cursor keys + space
    wide: false,
    border: 0.3,
    dark: true,
  };
  const frame = document.createElement('iframe');
  frame.className = 'emu-frame';
  frame.allow = 'autoplay; fullscreen; gamepad; keyboard-map';
  frame.title = 'C64 emulator';
  frame.src = `${VC64WEB}#${encodeURIComponent(JSON.stringify(config))}`;
  host.replaceChildren(frame);

  let sent = false;
  let poll = 0;
  const script = (s) => frame.contentWindow.postMessage({ cmd: 'script', script: s }, ORIGIN);
  const onMessage = (ev) => {
    if (ev.origin !== ORIGIN || ev.source !== frame.contentWindow) return;
    const msg = ev.data?.msg;
    if (msg === 'breadbin_kernal') {
      start();
    } else if (msg === 'breadbin_drive') {
      onDrive?.(ev.data);
    } else if (msg === 'breadbin_alert') {
      onAlert?.(ev.data.text);
    } else if (msg === 'render_run_state') {
      onState?.({ running: ev.data.value, warping: ev.data.is_warping });
      // vc64web reports "running" once its ROMs are in; only then inject.
      if (ev.data.value && !sent) {
        sent = true;
        clearInterval(poll);
        script(ALERTS);
        script(SID_QUALITY);
        // a patched Open ROMs KERNAL resets the C64; the program goes in once the
        // patch shows in the ROM and the C64 has booted again
        if (kernal && !roms.kernal) {
          // a drive ROM goes in the same message, ahead of the KERNAL: vc64web
          // reloads its stored ROMs after taking one, which would put the
          // stock KERNAL back if it came later
          frame.contentWindow.postMessage({ cmd: 'load', kernal_rom: kernal, ...(roms.floppy && { floppy_rom: roms.floppy }) }, ORIGIN);
          script(KERNAL_IN.replace('BYTE', kernal[0xf71c - 0xe000]));
          setTimeout(start, 4000); // in case the message never comes
        } else start();
      }
    }
  };
  let started = false;
  const start = () => {
    if (started) return;
    started = true;
    const load = { cmd: 'load' };
    if (!file.vdrive) Object.assign(load, { file_name: file.name, file: file.bytes });
    if (roms.kernal) Object.assign(load, { kernal_rom: roms.kernal, basic_rom: roms.basic, charset_rom: roms.charset });
    if (roms.floppy && !kernal) load.floppy_rom = roms.floppy; // else sent with the KERNAL
    frame.contentWindow.postMessage(load, ORIGIN);
    // these run inside vc64web via its scripting API
    script(PAD_LISTENER);
    frame.contentWindow.postMessage({ bbport: port }, ORIGIN);
    if (file.vdrive) {
      engineBytes().then((engine) => driveScripts(engine, file.bytes, file.boot || '*', file.patches, file.others).forEach(script));
    } else if (/\.tap$/i.test(file.name)) {
      // insert past vc64web's tape dialog (without its reset), then LOAD from
      // device 1 (the Open ROMs' bare LOAD means the disk drive), press PLAY,
      // and RUN if READY comes back (a program that starts itself never does)
      script(`(async () => {
        wasm_set_warp(1);
        for (let i = 0; i < 100 && !$('#modal_file_slot').is(':visible'); i++) await new Promise((r) => setTimeout(r, 100));
        $('#auto_load').prop('checked', false);
        $('#auto_press_play').prop('checked', false);
        $('#auto_reset').prop('checked', false);
        // a native click (vc64web listens natively, which jQuery's click() misses),
        // repeated: the dialog ignores it while it is still fading in
        for (let i = 0; i < 20 && $('#modal_file_slot').is(':visible'); i++) {
          document.querySelector('#button_insert_file').click();
          await new Promise((r) => setTimeout(r, 300));
        }
        ${WAIT_READY}
        const before = readies();
        wasm_auto_type('load"",1\\n');
        setTimeout(wasm_press_play, 800);
        for (let i = 0; i < 3000 && readies() <= before; i++) await new Promise((r) => setTimeout(r, 200));
        if (readies() > before) wasm_auto_type('run:\\n');
      })();`);
    } else if (disk) {
      // The emulated 1541 loads at its real speed (minutes for a big game), so
      // warp while it works (vc64web's WARP_AUTO, off unless switched on), for
      // the first program only: a trackmo goes on loading while it plays, and
      // warping then would race its music.
      // Type once READY shows (right after a KERNAL swap the C64 is still
      // booting and the first keys are lost: "OAD") and the drive has had a
      // moment to start on the ROM just sent: a LOAD before that waits forever.
      script(`(async () => {
        wasm_set_warp(1);
        ${WAIT_READY}
        await new Promise((r) => setTimeout(r, 3000));
        wasm_auto_type('load"*",8,1:\\n');
        await disk_loading_finished();
        wasm_set_warp(0);
        wasm_auto_type('\\nrun:\\n');
      })();`);
    }
    frame.focus();
  };
  addEventListener('message', onMessage);
  frame.addEventListener('load', () => {
    clearInterval(poll);
    poll = setInterval(() => frame.contentWindow?.postMessage('poll_state', ORIGIN), 400);
  });

  return {
    frame,
    send: (m) => frame.contentWindow?.postMessage(m, ORIGIN),
    stop() {
      clearInterval(poll);
      removeEventListener('message', onMessage);
      frame.remove();
    },
  };
}
