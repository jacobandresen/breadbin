// kernal - write breadbin's patched Open ROMs KERNAL (crates/breadbin/src/rom.rs,
// the one the site runs) to a file, for other emulators such as VICE. With
// `drive`, the variant for a real 1541 (serial bus fixes) instead of the one
// for breadbin's virtual drive (device 8 channels).
//   node kernal.mjs <kernal_generic.rom> <out.rom> [drive]
import fs from 'node:fs';

const [src, out, mode] = process.argv.slice(2);
const wasm = fs.readFileSync(new URL('../../docs/play/breadbin.wasm', import.meta.url));
const stub = new Proxy({}, { get: () => () => 0 }); // host imports are not needed for patching
const { exports: w } = await WebAssembly.instantiate(wasm, { env: stub }).then((r) => r.instance);
const k = fs.readFileSync(src);
const p = w.bb_alloc(k.length);
new Uint8Array(w.memory.buffer, p, k.length).set(k);
if (!w.bb_patch_kernal(p, k.length, mode === 'drive' ? 1 : 0)) throw new Error('not the Open ROMs KERNAL breadbin knows');
fs.writeFileSync(out, new Uint8Array(w.memory.buffer, p, k.length));
