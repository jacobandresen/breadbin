// cart - a whole C64 program in a link, readable up to its data:
//
//   https://…/play/#play/Bubbler/<data>
//   <data> = base64url(deflate-raw(PRG bytes))
//
// The Studio's Share makes one (its "A link"), and whoever opens it gets the
// game running in the player. Nothing is uploaded anywhere: the link is the
// game. The title is the link's words (a space is "-").
//
// Links made before (#games/prg:<data>/play, <data> = base64url(gzip([title
// length, title, PRG]))) still open.

const OLD = 'prg:';

const pipe = async (bytes, stream) => new Uint8Array(await new Response(new Blob([bytes]).stream().pipeThrough(stream)).arrayBuffer());
const URL64 = { alphabet: 'base64url', omitPadding: true };
const fromB64 = (s) => Uint8Array.fromBase64(s, URL64);

/** The title as link words: "My Game!" -> "My-Game%21" (letters, digits and . _ ~ stay as they are). */
const words = (title) => encodeURIComponent(String(title).trim().slice(0, 60).replace(/\s+/g, '-') || 'game').replace(/[!'()*]/g, (c) => `%${c.charCodeAt(0).toString(16)}`);

/** The id (after #play/) for `prg` called `title`: "Bubbler/<data>". */
export async function packCart(prg, title) {
  return `${words(title)}/${(await pipe(prg, new CompressionStream('deflate-raw'))).toBase64(URL64)}`;
}

/** The { title, prg } in a link's id: a packCart one, or an old "prg:..." one; throws if it is broken. */
export async function unpackCart(id) {
  let title;
  let prg;
  if (id.startsWith(OLD)) {
    const raw = await pipe(fromB64(id.slice(OLD.length)), new DecompressionStream('gzip'));
    title = new TextDecoder().decode(raw.subarray(1, 1 + raw[0]));
    prg = raw.slice(1 + raw[0]);
  } else {
    const at = id.lastIndexOf('/');
    title = decodeURIComponent(id.slice(0, at)).replace(/-/g, ' ');
    prg = await pipe(fromB64(id.slice(at + 1)), new DecompressionStream('deflate-raw'));
  }
  if (prg.length < 3) throw new Error('the program in the link is empty');
  return { title, prg };
}

/** Is `id` an old-style packed program (#games/prg:...)? */
export const isCart = (id) => String(id).startsWith(OLD);
