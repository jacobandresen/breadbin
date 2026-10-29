// media - turn whatever a download is into something the emulator can start.
// With only the free Open ROMs there is no 1541 drive ROM, so a disk image
// can't be mounted. Instead breadbin's virtual drive (drive.js) serves the
// disk's files to the KERNAL's LOAD. Given the user's own 1541 ROM, disks mount
// in the emulated drive as-is.

const EXT_RANK = { crt: 0, prg: 1, t64: 2, d64: 3, g64: 4, tap: 5, zip: 6 };

export function ext(name) {
  const m = /\.([a-z0-9]+)$/i.exec(name);
  return m ? m[1].toLowerCase() : '';
}

/** Prefer side A / disk 1 of a multi-disk release. */
function sideRank(name) {
  const n = name.toLowerCase();
  if (/(side|disk|disc)[ _-]*(b|2)\b|_b\.\w+$/.test(n)) return 2;
  if (/(side|disk|disc)[ _-]*(c|d|3|4)\b/.test(n)) return 3;
  if (/(side|disk|disc)[ _-]*(a|1)\b/.test(n)) return 0;
  return 1;
}

/** Pick the file to boot from an archive item's metadata. */
export function pickFile(meta) {
  const files = (meta.files || []).map((f) => f.name).filter((n) => n in EXT_RANK || ext(n) in EXT_RANK);
  const start = meta.metadata?.emulator_start;
  if (start && files.includes(start)) return start;
  files.sort((a, b) => sideRank(a) - sideRank(b) || EXT_RANK[ext(a)] - EXT_RANK[ext(b)] || a.length - b.length);
  return files[0] || null;
}

// ── ZIP ──────────────────────────────────────────────────────────────────────

/** List and lazily inflate the members of a zip archive. */
export function unzip(bytes) {
  const dv = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  let eocd = -1;
  for (let i = bytes.length - 22; i >= Math.max(0, bytes.length - 65557); i--) {
    if (dv.getUint32(i, true) === 0x06054b50) {
      eocd = i;
      break;
    }
  }
  if (eocd < 0) throw new Error('not a zip archive');
  const count = dv.getUint16(eocd + 10, true);
  let p = dv.getUint32(eocd + 16, true);
  const out = [];
  for (let n = 0; n < count && dv.getUint32(p, true) === 0x02014b50; n++) {
    const method = dv.getUint16(p + 10, true);
    const csize = dv.getUint32(p + 20, true);
    const nameLen = dv.getUint16(p + 28, true);
    const extraLen = dv.getUint16(p + 30, true);
    const commentLen = dv.getUint16(p + 32, true);
    const local = dv.getUint32(p + 42, true);
    const name = new TextDecoder().decode(bytes.subarray(p + 46, p + 46 + nameLen));
    const start = local + 30 + dv.getUint16(local + 26, true) + dv.getUint16(local + 28, true);
    const raw = bytes.subarray(start, start + csize);
    out.push({ name, read: () => (method === 0 ? Promise.resolve(raw) : inflate(raw)) });
    p += 46 + nameLen + extraLen + commentLen;
  }
  return out;
}

async function inflate(raw) {
  const stream = new Blob([raw]).stream().pipeThrough(new DecompressionStream('deflate-raw'));
  return new Uint8Array(await new Response(stream).arrayBuffer());
}

// ── D64 ──────────────────────────────────────────────────────────────────────

const sectorsIn = (t) => (t <= 17 ? 21 : t <= 24 ? 19 : t <= 30 ? 18 : 17);
const TRACK_START = [0];
for (let t = 1; t <= 42; t++) TRACK_START[t] = (TRACK_START[t - 1] || 0) + (t > 1 ? sectorsIn(t - 1) : 0);

function sector(img, t, s) {
  const o = (TRACK_START[t] + s) * 256;
  if (t < 1 || t > 42 || s >= sectorsIn(t) || o + 256 > img.length) return null;
  return img.subarray(o, o + 256);
}

/** PETSCII filename -> readable text. */
function petscii(bytes) {
  let s = '';
  for (const b of bytes) {
    if (b === 0xa0) break;
    const c = b >= 0xc1 && b <= 0xda ? b - 0x80 : b;
    s += c >= 0x20 && c < 0x7f ? String.fromCharCode(c) : '?';
  }
  return s;
}

function readChain(img, t, s) {
  const parts = [];
  const seen = new Set();
  while (t !== 0) {
    const key = t * 64 + s;
    const sec = sector(img, t, s);
    if (!sec || seen.has(key)) break;
    seen.add(key);
    if (sec[0] === 0) {
      parts.push(sec.subarray(2, Math.max(2, sec[1] + 1)));
      break;
    }
    parts.push(sec.subarray(2));
    [t, s] = [sec[0], sec[1]];
  }
  const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
  let o = 0;
  for (const p of parts) {
    out.set(p, o);
    o += p.length;
  }
  return out;
}

/** The disk's title and its PRG files, in directory order. */
export function d64(img) {
  const bam = sector(img, 18, 0);
  if (!bam) throw new Error('not a D64 disk image');
  const title = petscii(bam.subarray(0x90, 0xa0));
  const files = [];
  let [t, s] = [18, 1];
  const seen = new Set();
  while (t !== 0 && !seen.has(t * 64 + s)) {
    seen.add(t * 64 + s);
    const sec = sector(img, t, s);
    if (!sec) break;
    for (let e = 0; e < 8; e++) {
      const ent = sec.subarray(e * 32, e * 32 + 32);
      const type = ent[2];
      if ((type & 0x07) !== 2 || !(type & 0x80)) continue; // closed PRG only
      files.push({ name: petscii(ent.subarray(5, 21)), data: () => readChain(img, ent[3], ent[4]) });
    }
    [t, s] = [sec[0], sec[1]];
  }
  return { title, files };
}

/**
 * Prepare a downloaded file for vc64web. Returns { name, bytes, note? }.
 * `canMountDisks` is true once the user has supplied a 1541 ROM.
 */
export async function prepare(name, bytes, { canMountDisks = false } = {}) {
  const e = ext(name);
  if (e === 'zip') {
    const members = unzip(bytes).filter((m) => ext(m.name) in EXT_RANK && ext(m.name) !== 'zip' && !m.name.startsWith('__MACOSX'));
    members.sort((a, b) => sideRank(a.name) - sideRank(b.name) || EXT_RANK[ext(a.name)] - EXT_RANK[ext(b.name)]);
    if (!members.length) throw new Error('nothing runnable inside the zip');
    const m = members[0];
    return prepare(m.name.split('/').pop(), await m.read(), { canMountDisks });
  }
  if ((e === 'd64' || e === 'g64') && canMountDisks) return { name, bytes };
  if (e === 'g64') throw new Error('this release is a G64 copy-protected disk, which needs your own 1541 ROM (see Settings)');
  if (e === 'd64') {
    const disk = d64(bytes);
    if (!disk.files.length) throw new Error('the disk holds no program to start');
    return {
      name,
      bytes,
      vdrive: true,
      note: 'Disk served by breadbin\'s virtual drive. Titles with their own fast loader need a 1541 drive ROM (see Settings).',
    };
  }
  return { name, bytes };
}
