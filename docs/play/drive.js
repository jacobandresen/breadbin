// drive - attach breadbin's virtual drive (crates/breadbin/src/drive.rs) to the
// emulator. The engine is instantiated inside the vc64web frame (through its
// "script" message) with vc64web's peek/poke as the C64, then ticked every few
// milliseconds; it serves the D64's files to the KERNAL's LOAD.

/** Runs inside the vc64web frame; sent as source, so self-contained. */
async function attach(engineB64, diskB64) {
  const bin = (b64) => Uint8Array.from(atob(b64), (c) => c.charCodeAt(0));
  const say = (m) => window.parent.postMessage({ msg: 'breadbin_drive', ...m }, '*');
  const dec = new TextDecoder();
  let w;
  const env = {
    host_peek: (a) => wasm_peek(a),
    host_poke: (a, v) => wasm_poke(a, v),
    host_event: (kind, p, n, addr, end) =>
      say({ event: kind === 1 ? 'load' : 'missing', name: dec.decode(new Uint8Array(w.memory.buffer, p, n)), addr, end }),
  };
  w = (await WebAssembly.instantiate(bin(engineB64), { env })).instance.exports;

  // the KERNAL resets its vectors while booting, so wait for READY
  const ready = () => {
    for (let a = 0x400; a < 0x7e3; a++) {
      if (wasm_peek(a) === 0x12 && wasm_peek(a + 1) === 5 && wasm_peek(a + 2) === 1 && wasm_peek(a + 3) === 4 && wasm_peek(a + 4) === 0x19) return true;
    }
    return false;
  };
  for (let i = 0; i < 600 && !ready(); i++) await new Promise((r) => setTimeout(r, 100));

  const disk = bin(diskB64);
  const p = w.bb_alloc(disk.length);
  new Uint8Array(w.memory.buffer, p, disk.length).set(disk);
  if (!w.bb_drive_attach(p, disk.length)) return say({ event: 'error' });
  setInterval(() => w.bb_drive_tick(), 4);
  window.breadbinDrive = w;
  say({ event: 'ready' });
}

/**
 * Types LOAD"*",8,1 (or the named file) once the drive is attached, then, as a
 * person would, RUN (or the SYS a one-line crack stub was going to do) if READY
 * comes back.
 * Autostarting loaders never return to READY, so they're left alone.
 */
async function boot(file) {
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  const readies = () => {
    let n = 0;
    for (let a = 0x400; a < 0x7e3; a++) {
      if (wasm_peek(a) === 0x12 && wasm_peek(a + 1) === 5 && wasm_peek(a + 2) === 1 && wasm_peek(a + 3) === 4 && wasm_peek(a + 4) === 0x19 && wasm_peek(a + 5) === 0x2e) n++;
    }
    return n;
  };
  for (let i = 0; i < 600 && !window.breadbinDrive; i++) await sleep(100);
  const drive = window.breadbinDrive;
  const before = readies();
  wasm_auto_type(`load"${file.toLowerCase()}",8,1:\n`); // unshifted letters type PETSCII capitals
  for (let i = 0; i < 300 && !drive.bb_drive_loads(); i++) await sleep(100);
  for (let i = 0; i < 30; i++) {
    await sleep(100);
    if (readies() > before) {
      const sys = drive.bb_drive_sys();
      wasm_auto_type(sys ? `sys${sys}:\n` : 'run:\n');
      break;
    }
  }
}

const b64 = (bytes) => {
  let s = '';
  for (let i = 0; i < bytes.length; i += 0x8000) s += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(s);
};

/** Script sources for vc64web's {cmd: "script"}: attach the drive, then boot. */
export function driveScripts(engine, disk, file = '*') {
  return [`(${attach})(${JSON.stringify(b64(new Uint8Array(engine)))}, ${JSON.stringify(b64(disk))});`, `(${boot})(${JSON.stringify(file)});`];
}
