// kernal - write breadbin's patched Open ROMs KERNAL (crates/breadbin/src/rom.rs,
// the one the site runs) to a file, for other emulators such as VICE. With
// `drive`, the variant for a real 1541 (serial bus fixes) instead of the one
// for breadbin's virtual drive (device 8 channels). With `basic`, the Open ROMs
// BASIC (basic_generic.rom) and its patch instead.
//   node kernal.mjs <kernal_generic.rom> <out.rom> [drive]
//   node kernal.mjs <basic_generic.rom> <out.rom> basic
import fs from 'node:fs';

const [src, out, mode] = process.argv.slice(2);
const wasm = fs.readFileSync(new URL('../../docs/play/breadbin.wasm', import.meta.url));
const stub = new Proxy({}, { get: () => () => 0 }); // host imports are not needed for patching
const { exports: w } = await WebAssembly.instantiate(wasm, { env: stub }).then((r) => r.instance);
const k = fs.readFileSync(src);
const p = w.bb_alloc(k.length);
new Uint8Array(w.memory.buffer, p, k.length).set(k);
const ok = mode === 'basic' ? w.bb_patch_basic(p, k.length) : w.bb_patch_kernal(p, k.length, mode === 'drive' ? 1 : 0);
if (!ok) throw new Error(`not the Open ROMs ${mode === 'basic' ? 'BASIC' : 'KERNAL'} breadbin knows`);
fs.writeFileSync(out, new Uint8Array(w.memory.buffer, p, k.length));
