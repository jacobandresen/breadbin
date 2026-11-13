// resource - read one of the Studio's own files (the cc65 tools, the sysroot,
// the runtime library, the examples) by a path relative to this folder. In a
// browser (or worker) that is a fetch; under Node, for the tests, a file read.

const IS_NODE = typeof process !== 'undefined' && !!process.versions?.node && typeof window === 'undefined';

/** The URL of `rel`, relative to docs/play/studio/. */
export const resourceUrl = (rel) => new URL(rel, import.meta.url);

/** The bytes of `rel` (Uint8Array). */
export async function resource(rel) {
  const url = resourceUrl(rel);
  if (IS_NODE && url.protocol === 'file:') {
    const { readFile } = await import('node:fs/promises');
    return new Uint8Array(await readFile(url));
  }
  const res = await fetch(url);
  if (!res.ok) throw new Error(`${rel}: ${res.status} ${res.statusText}`);
  return new Uint8Array(await res.arrayBuffer());
}

/** The text of `rel`. */
export async function resourceText(rel) {
  return new TextDecoder().decode(await resource(rel));
}
