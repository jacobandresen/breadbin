// sign - make a 1541 ROM acceptable to vc64web; run as a script it signs a
// file in place: node tools/drive/sign.mjs dos.bin out.rom

import fs from 'node:fs';
import { fileURLToPath } from 'node:url';

/**
 * Make a 1541 ROM acceptable to vc64web. VirtualC64 (VCCore/Media/RomFile.cpp)
 * only takes a drive ROM whose first three bytes are those of a Commodore or
 * Dolphin ROM ($97 $AA $AA...); a ROM written from scratch usually starts with
 * code. This stamps $97 $AA $AA at $C000, moves the overwritten instructions to
 * free space ($FF bytes) near the top, and points the reset vector and every
 * JMP $C000 there. The instruction at $C000 must be 3 bytes or end on byte 3
 * (checked: `firstLen`). Returns the patched copy.
 */
export function signDriveRom(rom, { firstLen = 4 } = {}) {
  const b = Uint8Array.from(rom);
  if (b.length !== 0x4000) throw new Error('a 1541 ROM is 16 KB');
  if (b[0] === 0x97) return b;
  const at = 0x3fe8; // $FFE8: firstLen bytes + JMP
  if (!b.subarray(at, at + firstLen + 3).every((x) => x === 0xff)) throw new Error('no free space at $FFE8');
  const reset = b[0x3ffc] | (b[0x3ffd] << 8);
  if (reset !== 0xc000) throw new Error(`reset vector is $${reset.toString(16)}, not $C000`);
  const tramp = 0xc000 + at;
  b.set(b.subarray(0, firstLen), at);
  b.set([0x4c, firstLen, 0xc0], at + firstLen); // JMP $C000+firstLen
  for (let i = 0; i < b.length - 2; i++) if (b[i] === 0x4c && b[i + 1] === 0 && b[i + 2] === 0xc0) b.set([tramp & 255, tramp >> 8], i + 1);
  b.set([tramp & 255, tramp >> 8], 0x3ffc);
  b.set([0x97, 0xaa, 0xaa], 0);
  return b;
}

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const [from, to = from] = process.argv.slice(2);
  fs.writeFileSync(to, signDriveRom(fs.readFileSync(from)));
}
