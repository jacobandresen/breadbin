// tar - read a (gzipped) ustar archive into { path: bytes }. The cc65 sysroot
// (headers, the C64 library, the linker config) travels as one .tar.gz.

/** Inflate gzip `bytes` with the platform's DecompressionStream. */
export async function gunzip(bytes) {
  const stream = new Blob([bytes]).stream().pipeThrough(new DecompressionStream('gzip'));
  return new Uint8Array(await new Response(stream).arrayBuffer());
}

const field = (b, at, len) => new TextDecoder().decode(b.subarray(at, at + len)).replace(/\0.*$/s, '');

/** Regular files of the ustar archive `b`, by path (directories skipped). */
export function untar(b) {
  const files = {};
  for (let at = 0; at + 512 <= b.length; ) {
    const name = field(b, at, 100);
    if (!name) break; // two zero blocks end the archive
    const size = parseInt(field(b, at + 124, 12).trim() || '0', 8);
    const type = String.fromCharCode(b[at + 156] || 48);
    const prefix = field(b, at + 345, 155);
    const path = (prefix ? `${prefix}/` : '') + name;
    if (type === '0' || type === '\0') files[path.replace(/^\.\//, '')] = b.slice(at + 512, at + 512 + size);
    at += 512 + Math.ceil(size / 512) * 512;
  }
  return files;
}
