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
// vc64web's own menu button fades in at any touch on the screen and opens its
// whole interface over the C64, with no way back to breadbin's: breadbin's
// player has the controls, so the menu stays hidden
const NO_MENU = `const s = document.createElement('style'); s.textContent = '#button_show_menu, #navbar { display: none !important; }'; document.head.append(s);`;
const ALERTS = "window.alert = (text) => parent.postMessage({ msg: 'breadbin_alert', text: String(text) }, '*');";
// the C64 restarts on the new KERNAL: give it a moment before anything is typed
// (wasm_peek reads the RAM under the ROM, so the swap can't be watched directly)
const KERNAL_IN = `(async () => {
  await new Promise((r) => setTimeout(r, 3500));
  parent.postMessage({ msg: 'breadbin_kernal' }, '*');
})();`;
// on an iPhone, sound that plays even with the silent switch on, like a video
const AUDIO_SESSION = `if (navigator.audioSession) navigator.audioSession.type = 'playback';`;
/**
 * Script that keeps breadbin's KERNAL (and BASIC) in: vc64web downloads the
 * stock Open ROMs as it starts and installs each when it arrives, which can be
 * after ours went in (the C64 then runs without breadbin's fixes). A late stock
 * KERNAL or BASIC is swapped for ours.
 */
function kernalGuard(kernal, basic) {
  const b64 = (rom) => {
    let bin = '';
    for (const b of rom) bin += String.fromCharCode(b);
    return btoa(bin);
  };
  return `(() => {
    const rom = (s) => Uint8Array.from(atob(s), (c) => c.charCodeAt(0));
    const ours = { kernal: rom('${b64(kernal)}'), basic: ${basic ? `rom('${b64(basic)}')` : 'null'} };
    const load = wasm_loadfile;
    wasm_loadfile = (name, data, len, ...rest) =>
      /kernal_generic/.test(name) ? load('kernal.rom', ours.kernal, ours.kernal.length, ...rest)
      : /basic_generic/.test(name) && ours.basic ? load('basic.rom', ours.basic, ours.basic.length, ...rest)
      : load(name, data, len, ...rest);
  })();`;
}
// With onHotkey (the Studio): keys meant for the page, not the C64, while the
// C64 has the keyboard: Ctrl+Enter (run), Ctrl+S (save), F1 (help), Escape
// (back to the code). Caught before vc64web sees them.
const HOTKEYS = `addEventListener('keydown', (e) => {
  const mod = e.ctrlKey || e.metaKey;
  const key = mod && e.key === 'Enter' ? 'run' : mod && (e.key === 's' || e.key === 'S') ? 'save' : e.key === 'F1' ? 'help' : e.key === 'Escape' ? 'escape' : null;
  if (!key) return;
  e.preventDefault();
  e.stopImmediatePropagation();
  parent.postMessage({ msg: 'breadbin_hotkey', key }, '*');
}, true);`;
const CLEAR_SCREEN = `for (let a = 0x400; a < 0x7e8; a++) wasm_poke(a, 0x20);`;
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
 * optionally carries the user's own { kernal, basic, charset, floppy }; without them, `kernal` and `basic` are the patched Open ROMs. `port` is
 * the joystick port (1 or 2) the controls drive, `border` how much of the
 * C64's border shows (0..1). With `onHotkey(key)`, Ctrl+Enter, Ctrl+S, F1 and
 * Escape pressed while the C64 has the keyboard come to the page instead
 * ('run', 'save', 'help', 'escape').
 * Returns { frame, send(), stop(), reload() }.
 */
export function run(host, file, { roms = {}, kernal = null, basic = null, port = 2, border = 0.3, onState, onDrive, onAlert, onAudio, onHotkey } = {}) {
  const disk = !file.vdrive && /\.(d64|g64)$/i.test(file.name);
  const config = {
    openROMS: !roms.kernal,
    dialog_on_missing_roms: false,
    // For disks, skip the "insert disk" dialog; LOAD/RUN is typed by the script below.
    ...(disk ? { dialog_on_disk: false } : {}),
    navbar: false,
    [`port${port}`]: true, // the cursor keys + space
    wide: false,
    border, // how much of the C64's border shows (0..1)
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
  let peeks = 0;
  const peekWaiting = new Map();
  // the frame may already be gone (the player closed while a timer was waiting)
  const script = (s) => frame.contentWindow?.postMessage({ cmd: 'script', script: s }, ORIGIN);
  const onMessage = (ev) => {
    if (ev.origin !== ORIGIN || ev.source !== frame.contentWindow) return;
    const msg = ev.data?.msg;
    if (msg === 'breadbin_kernal') {
      start();
    } else if (msg === 'breadbin_drive') {
      onDrive?.(ev.data);
    } else if (msg === 'breadbin_peek') {
      peekWaiting.get(ev.data.id)?.(ev.data.bytes);
      peekWaiting.delete(ev.data.id);
    } else if (msg === 'breadbin_hotkey') {
      onHotkey?.(ev.data.key);
    } else if (msg === 'breadbin_alert') {
      onAlert?.(ev.data.text);
    } else if (msg === 'render_current_audio_state') {
      onAudio?.(ev.data.value); // 'suspended' until a tap inside vc64web on Safari
    } else if (msg === 'render_run_state') {
      onState?.({ running: ev.data.value, warping: ev.data.is_warping });
      // vc64web reports "running" once its ROMs are in; only then inject.
      if (ev.data.value && !sent) {
        sent = true;
        clearInterval(poll);
        script(ALERTS);
        script(NO_MENU);
        script(SID_QUALITY);
        script(AUDIO_SESSION);
        if (onHotkey) script(HOTKEYS);
        // a patched Open ROMs KERNAL resets the C64; the program goes in once the
        // patch shows in the ROM and the C64 has booted again
        if (kernal && !roms.kernal) {
          // a drive ROM goes in the same message, ahead of the KERNAL: vc64web
          // reloads its stored ROMs after taking one, which would put the
          // stock KERNAL back if it came later
          // blank the screen first: the READY still on it would otherwise pass
          // for the new boot's, and keys typed before it are lost
          script(CLEAR_SCREEN);
          script(kernalGuard(kernal, basic));
          frame.contentWindow?.postMessage({ cmd: 'load', kernal_rom: kernal, ...(basic && { basic_rom: basic }), ...(roms.floppy && { floppy_rom: roms.floppy }) }, ORIGIN);
          script(KERNAL_IN);
          setTimeout(start, 4000); // in case the message never comes
        } else start();
      }
    }
  };
  let started = false;
  const start = () => {
    if (started || !frame.contentWindow) return;
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
        const before = readies();
        wasm_auto_type('load"*",8,1:\\n');
        // RUN once the LOAD is over, that is when its READY shows. vc64web's
        // disk_loading_finished() can come before the drive has even started
        // (RUN typed during the LOAD is lost: Edge of Disgrace's one-block boot
        // file, Soiled Legacy). A program that starts itself shows no READY and
        // needs no RUN.
        while (readies() <= before) await new Promise((r) => setTimeout(r, 200));
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
    /** Has the first program gone in? (Until then reload() can't be used.) */
    get started() {
      return started;
    },
    /**
     * Start another PRG in the running C64, without booting a new frame (the
     * Studio's Run, ~1.5 s instead of ~7): reset, wait for BASIC's READY,
     * copy the program into RAM at its load address, set BASIC's end-of-
     * program pointers and type RUN. The ROMs stay as they are (breadbin's
     * patched KERNAL included).
     */
    /**
     * Read the C64's memory: `ranges` is [[address, length], ...]; resolves to
     * one array of bytes per range (null if the emulator doesn't answer). The
     * Studio's Variables panel shows a running game's globals with it.
     */
    peek(ranges) {
      const id = ++peeks;
      return new Promise((resolve) => {
        peekWaiting.set(id, resolve);
        script(`parent.postMessage({ msg: 'breadbin_peek', id: ${id}, bytes: ${JSON.stringify(ranges)}.map(([a, n]) => Array.from({ length: n }, (_, i) => wasm_peek(a + i))) }, '*');`);
        setTimeout(() => peekWaiting.has(id) && (peekWaiting.delete(id), resolve(null)), 1000);
      });
    },
    reload(bytes) {
      let bin = '';
      for (const b of bytes) bin += String.fromCharCode(b);
      script(`(async () => {
        const data = Uint8Array.from(atob('${btoa(bin)}'), (c) => c.charCodeAt(0));
        // blank the old screen first: its READY would pass for the new boot's
        for (let a = 0x400; a < 0x7e8; a++) wasm_poke(a, 0x20);
        wasm_reset();
        ${WAIT_READY}
        await new Promise((r) => setTimeout(r, 100));
        const at = data[0] | (data[1] << 8);
        for (let i = 2; i < data.length; i++) wasm_poke(at + i - 2, data[i]);
        const end = at + data.length - 2;
        for (const z of [0x2d, 0x2f, 0x31]) { wasm_poke(z, end & 255); wasm_poke(z + 1, end >> 8); } // VARTAB, ARYTAB, STREND
        wasm_auto_type('run\\n');
      })();`);
      frame.focus();
    },
    stop() {
      clearInterval(poll);
      removeEventListener('message', onMessage);
      frame.remove();
    },
  };
}
