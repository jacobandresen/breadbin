// hvsc - SID files from the High Voltage SID Collection. First the Internet
// Archive's copy of HVSC 79, read file by file out of its 7z through
// archive.org/cors (CORS headers, always up), then hvsc.de for anything newer.
// Verified tunes know their HVSC path; tools/playtest fetches the same way.

import { cachedBytes } from './store.js';

const enc = (path) => path.split('/').map(encodeURIComponent).join('/');
/** Where an HVSC path ("/MUSICIANS/H/Hubbard_Rob/Commando.sid") can be read, in order. */
export const sources = (path) => [
  `https://archive.org/cors/hvsc-79-all-of-them.-7z/HVSC_79-all-of-them.7z/C64Music${enc(path)}`,
  `https://www.hvsc.de/download/C64Music${enc(path)}`,
];

/** The SID at an HVSC path, cached for good. */
export async function sid(path) {
  let error;
  for (const url of sources(path)) {
    try {
      const bytes = await cachedBytes(url);
      if (/^[PR]SID/.test(String.fromCharCode(...bytes.subarray(0, 4)))) return bytes;
      error = new Error('not a SID file');
    } catch (e) {
      error = e;
    }
  }
  throw error;
}
