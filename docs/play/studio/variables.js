// variables - the Variables panel: a running game's global variables, read
// from the C64 a few times a second. The linker's label file (toolchain.js
// `labels`) says where each global is; its size is the distance to the next
// label in the same segment. Types aren't known: one byte reads as 0..255
// (and as -128..127 if it would be negative), two as a 16-bit number, more
// as a list of bytes.

const MAX_BYTES = 8;

/**
 * The game's globals in `labels` ({ name: address }): C names (a leading "_"),
 * not the runtime's (bb_) nor the C library's, in the BSS or DATA segments of
 * `memory` (toolchain.js memoryUse). `sizes` (toolchain.js dataSizes) gives
 * each one's size where cc65's assembly said; otherwise it reaches to the next
 * label. [{ name, addr, size, unit }], by address.
 */
export function globals(labels, memory, sizes = {}, { hidden = [], types = {} } = {}) {
  const skip = new Set(hidden);
  const segs = (memory?.segments || []).filter((s) => /^(BSS|DATA)$/.test(s.name));
  const inSeg = (a) => segs.find((s) => a >= s.start && a < s.start + s.size);
  const all = Object.entries(labels || {}).map(([name, addr]) => ({ name, addr })).sort((a, b) => a.addr - b.addr);
  const out = [];
  for (let i = 0; i < all.length; i++) {
    const { name, addr } = all[i];
    const seg = inSeg(addr);
    if (!seg || !/^_[a-z]/i.test(name) || /^_bb_/.test(name) || /^__/.test(name) || skip.has(name.slice(1))) continue;
    // the next label, of any kind, ends it (or the segment's end)
    let next = seg.start + seg.size;
    for (let j = i + 1; j < all.length; j++) {
      if (all[j].addr > addr) {
        next = Math.min(next, all[j].addr);
        break;
      }
    }
    const known = sizes[name.slice(1)];
    out.push({ name: name.slice(1), addr, size: known?.size || Math.max(1, next - addr), unit: types[name.slice(1)] || known?.unit || 1 });
  }
  return out;
}

/**
 * The size of one item of each array or variable the C `files` declare at
 * file level: { name: 1 (char) or 2 (int, short, pointer) }. The assembly
 * only says how many bytes a variable has; the C says what is in them.
 */
export function cTypes(files) {
  const out = {};
  // a declaration: a type, then names (with sizes, initial values in { }), then ';'; no ( ): not a function
  const decl = /^\s*(?:static\s+|volatile\s+|const\s+)*((?:unsigned|signed)\s+)?(char|int|short|long|void)\b\s*(\*?)\s*([^;()]+);/;
  for (const [path, text] of Object.entries(files)) {
    if (!/\.(c|h)$/.test(path) || typeof text !== 'string') continue;
    for (const line of text.split('\n')) {
      const m = decl.exec(line);
      if (!m) continue;
      const base = m[2] === 'char' ? 1 : m[2] === 'long' ? 4 : 2;
      // "a, *b, c[4] = {...}": each name, a pointer is 2 bytes whatever it points to
      for (const part of m[4].split(',')) {
        const v = /^\s*(\*?)\s*([A-Za-z_]\w*)/.exec(part);
        if (v) out[v[2]] = m[3] || v[1] ? 2 : base;
      }
    }
  }
  return out;
}

const word = (lo, hi) => {
  const v = lo | (hi << 8);
  return v > 32767 ? `${v} (${v - 65536})` : `${v}`;
};

/** The value of `bytes` (a global's first bytes) for a global of `size` bytes, in `unit`s (2: ints). */
export function format(bytes, size, unit = 1) {
  if (!bytes?.length) return '?';
  if (size === 1) return bytes[0] > 127 ? `${bytes[0]} (${bytes[0] - 256})` : `${bytes[0]}`;
  if (size === 2) return word(bytes[0], bytes[1]);
  if (unit === 2) {
    const items = [];
    for (let i = 0; i + 1 < bytes.length; i += 2) items.push((bytes[i] | (bytes[i + 1] << 8)).toString());
    return `[${items.join(', ')}${size > bytes.length ? ', …' : ''}]`;
  }
  const shown = bytes.slice(0, Math.min(size, MAX_BYTES));
  return `[${shown.join(', ')}${size > MAX_BYTES ? ', …' : ''}]`;
}

/** The [address, length] ranges to read for `vars`. */
export const ranges = (vars) => vars.map((v) => [v.addr, Math.min(v.size, MAX_BYTES)]);
