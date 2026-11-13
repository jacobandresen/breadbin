// worker - the Studio's builds, off the page's thread: cc65, ca65 and ld65
// run here, so typing never waits for a compile.
//
//   { id, type: 'build', files }          -> { id, result }  (toolchain.js build())
//   { id, type: 'psid', music, meta }     -> { id, bytes }   (psid.js musicPsid())
//   { id, type: 'headers', names }        -> { id, headers } (cc65's headers' text, by name)
//   any failure                           -> { id, error }

import { Toolchain } from './toolchain.js';
import { musicPsid } from './psid.js';

let tc = null;

self.onmessage = async ({ data: m }) => {
  try {
    tc ??= Toolchain.load();
    const t = await tc;
    if (m.type === 'build') {
      const result = await t.build(m.files, m.options);
      self.postMessage({ id: m.id, result }, result.prg ? [result.prg.buffer] : []);
    } else if (m.type === 'psid') {
      const bytes = await musicPsid(t, m.music, m.meta);
      self.postMessage({ id: m.id, bytes }, [bytes.buffer]);
    } else if (m.type === 'headers') {
      // the text of cc65's headers named in m.names, for the editor's completion
      const headers = {};
      for (const n of m.names) {
        const b = t.sysroot[`include/${n}`];
        if (b) headers[n] = new TextDecoder().decode(b);
      }
      self.postMessage({ id: m.id, headers });
    } else if (m.type === 'warm') {
      self.postMessage({ id: m.id, ok: true });
    }
  } catch (e) {
    self.postMessage({ id: m.id, error: e?.message || String(e) });
  }
};
