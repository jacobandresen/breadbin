// engine - the Rust engine (crates/breadbin, compiled to breadbin.wasm): disk
// images, the visualisers and, in other contexts, the SID player and virtual
// drive. This wraps its C ABI for the page.

export const WASM_URL = new URL('./breadbin.wasm', import.meta.url);

// The drive's host functions only exist inside the emulator frame.
export const NO_HOST = { env: { host_peek: () => 0, host_poke() {}, host_event() {} } };

let wasm = null;
let bytes = null;

/** The engine's bytes, for the audio worklet and the emulator frame. */
export async function engineBytes() {
  bytes ??= fetch(WASM_URL).then((r) => {
    if (!r.ok) throw new Error('breadbin.wasm is missing (run `make web`)');
    return r.arrayBuffer();
  });
  return bytes;
}

async function engine() {
  wasm ??= engineBytes().then((b) => WebAssembly.instantiate(b, NO_HOST)).then((r) => r.instance.exports);
  return wasm;
}

const enc = new TextEncoder();
const dec = new TextDecoder();

/** Copy `data` into wasm memory; returns [ptr, len, free]. */
function put(w, data) {
  const b = typeof data === 'string' ? enc.encode(data) : data;
  const p = w.bb_alloc(b.length);
  new Uint8Array(w.memory.buffer, p, b.length).set(b);
  return [p, b.length, () => w.bb_free(p, b.length)];
}

const text = (w) => dec.decode(new Uint8Array(w.memory.buffer, w.bb_text_ptr(), w.bb_text_len()));
const out = (w) => new Uint8Array(w.memory.buffer, w.bb_out_ptr(), w.bb_out_len()).slice();

/**
 * Turn a download into something the emulator starts: { name, bytes, vdrive,
 * disks } (`disks`: the zip's disk images, in order, for swapping sides).
 * `prefer` names the zip member to use; `canMount` means a 1541 ROM is present.
 */
export async function prepare(name, data, { prefer = '', canMount = false } = {}) {
  const w = await engine();
  const args = [put(w, name), put(w, data), put(w, prefer)];
  const ok = w.bb_prepare(...args.flatMap(([p, n]) => [p, n]), canMount ? 1 : 0);
  args.forEach(([, , free]) => free());
  if (!ok) throw new Error(text(w));
  const [file, vdrive, disks] = text(w).split('\n');
  return { name: file, bytes: out(w), vdrive: vdrive === '1', disks: disks ? disks.split('\t') : [] };
}

/** The file EasySplit cut across `disks` (D64 images, any order), usually an
 *  EasyFlash cartridge: its bytes, or an error saying what is missing. */
export async function joinSplit(disks) {
  const w = await engine();
  for (const d of disks) {
    const [p, n, free] = put(w, d);
    w.bb_split_add(p, n);
    free();
  }
  if (!w.bb_split_join()) throw new Error(text(w));
  return out(w);
}

/** The MEGA65 Open ROMs KERNAL with breadbin's compatibility patches (rom.rs),
 *  for a real 1541 if `drive` (else breadbin's virtual drive), or null if it
 *  can't be fetched or isn't the build the patches are for. */
export async function patchedKernal(fetchBytes, drive = false) {
  try {
    const w = await engine();
    const [p, n, free] = put(w, await fetchBytes(OPEN_KERNAL));
    const ok = w.bb_patch_kernal(p, n, drive ? 1 : 0);
    const rom = new Uint8Array(w.memory.buffer, p, n).slice();
    free();
    return ok ? rom : null;
  } catch {
    return null;
  }
}
export const OPEN_KERNAL = 'https://mega65.github.io/open-roms/bin/kernal_generic.rom';

/** The file to boot among `names` (`start` is the archive's own choice). */
export async function pick(names, start = '') {
  const w = await engine();
  const args = [put(w, names.join('\n')), put(w, start)];
  w.bb_pick(...args.flatMap(([p, n]) => [p, n]));
  args.forEach(([, , free]) => free());
  return text(w) || null;
}

/** The visualiser: font upload, then one framebuffer per frame. */
export async function visualiser() {
  const w = await engine();
  const regs = w.bb_alloc(32);
  return {
    font(masks) {
      const [p, n, free] = put(w, masks);
      w.bb_viz_font(p, n);
      free();
    },
    /** Render a frame; `rms` < 0 when idle. Returns { pixels, border }. */
    frame(kind, t, rms, sidRegs, title) {
      new Uint8Array(w.memory.buffer, regs, 32).set(sidRegs.subarray(0, 32));
      const [tp, tn, free] = put(w, title);
      const fb = w.bb_viz_frame(kind, t, rms, regs, tp, tn);
      free();
      return { pixels: new Uint8ClampedArray(w.memory.buffer, fb, 320 * 200 * 4), border: w.bb_viz_border() };
    },
  };
}
