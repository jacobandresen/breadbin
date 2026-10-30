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
const SID_QUALITY = `wasm_set_sid_engine('ReSID ${matchMedia('(pointer: coarse)').matches ? 'interpolate' : 'resample'}');`;

/**
 * Start `file` ({ name, bytes, vdrive? }) in a new vc64web frame inside `host`.
 * With `vdrive` the D64 is served by breadbin's virtual drive instead of being
 * mounted. `roms` optionally carries the user's own { kernal, basic, charset,
 * floppy }. Returns { frame, send(), stop() }.
 */
export function run(host, file, { roms = {}, onState, onDrive, onAlert } = {}) {
  const disk = !file.vdrive && /\.(d64|g64)$/i.test(file.name);
  const config = {
    openROMS: !roms.kernal,
    dialog_on_missing_roms: false,
    // For disks, skip the "insert disk" dialog; LOAD/RUN is typed by the script below.
    ...(disk ? { dialog_on_disk: false } : {}),
    navbar: false,
    port2: true,
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
    if (msg === 'breadbin_drive') {
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
        const load = { cmd: 'load' };
        if (!file.vdrive) Object.assign(load, { file_name: file.name, file: file.bytes });
        if (roms.kernal) Object.assign(load, { kernal_rom: roms.kernal, basic_rom: roms.basic, charset_rom: roms.charset });
        if (roms.floppy) load.floppy_rom = roms.floppy;
        frame.contentWindow.postMessage(load, ORIGIN);
        // these run inside vc64web via its scripting API
        script(PAD_LISTENER);
        if (file.vdrive) {
          engineBytes().then((engine) => driveScripts(engine, file.bytes).forEach(script));
        } else if (disk) {
          script('wasm_auto_type(\'load"*",8,1:\\n\'); await disk_loading_finished(); wasm_auto_type("\\nrun:\\n");');
        }
        frame.focus();
      }
    }
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
