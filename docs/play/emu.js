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
// vc64web defaults to reSID's fast (aliasing) sampling; resample sounds clean,
// interpolate is the lighter choice for phones
// tells the page when the KERNAL at $F71C reads BYTE (the patched one is in) and the C64 has rebooted
const KERNAL_IN = `(async () => {
  for (let i = 0; i < 60 && wasm_peek(0xf71c) !== BYTE; i++) await new Promise((r) => setTimeout(r, 50));
  await new Promise((r) => setTimeout(r, 500));
  parent.postMessage({ msg: 'breadbin_kernal' }, '*');
})();`;
const SID_QUALITY = `wasm_set_sid_engine('ReSID ${matchMedia('(pointer: coarse)').matches ? 'interpolate' : 'resample'}');`;

/**
 * Start `file` ({ name, bytes, vdrive?, boot? }) in a new vc64web frame inside
 * `host`. With `vdrive` the D64 is served by breadbin's virtual drive instead of
 * being mounted, starting with the file named `boot` (else the first). `roms`
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
          frame.contentWindow.postMessage({ cmd: 'load', kernal_rom: kernal }, ORIGIN);
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
    if (roms.floppy) load.floppy_rom = roms.floppy;
    frame.contentWindow.postMessage(load, ORIGIN);
    // these run inside vc64web via its scripting API
    script(PAD_LISTENER);
    frame.contentWindow.postMessage({ bbport: port }, ORIGIN);
    if (file.vdrive) {
      engineBytes().then((engine) => driveScripts(engine, file.bytes, file.boot || '*').forEach(script));
    } else if (/\.tap$/i.test(file.name)) {
      // insert past vc64web's tape dialog, then LOAD from device 1 (the Open ROMs'
      // bare LOAD means the disk drive) and press PLAY
      script(`(async () => {
        for (let i = 0; i < 100 && !$('#modal_file_slot').is(':visible'); i++) await new Promise((r) => setTimeout(r, 100));
        $('#auto_load').prop('checked', false);
        $('#auto_press_play').prop('checked', false);
        $('#button_insert_file').click();
        wasm_auto_type('load"",1\\n');
        setTimeout(wasm_press_play, 800);
      })();`);
    } else if (disk) {
      script('wasm_auto_type(\'load"*",8,1:\\n\'); await disk_loading_finished(); wasm_auto_type("\\nrun:\\n");');
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
