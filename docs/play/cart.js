// cart - a whole C64 program in a link:
// the Studio's "Share the game" packs the PRG (and its title) into the id of
// an ordinary breadbin game link, #games/prg:<data>/play, and whoever opens it
// gets the game running in the player. Nothing is uploaded anywhere: the
// link is the game.
//
//   <data> = base64url(gzip([title length, title (UTF-8), PRG bytes]))

const PREFIX = 'prg:';

async function pipe(bytes, stream) {
  return new Uint8Array(await new Response(new Blob([bytes]).stream().pipeThrough(stream)).arrayBuffer());
}

const toB64 = (bytes) => {
  let bin = '';
  for (let i = 0; i < bytes.length; i += 0x8000) bin += String.fromCharCode(...bytes.subarray(i, i + 0x8000));
  return btoa(bin).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
};
const fromB64 = (s) => Uint8Array.from(atob(s.replace(/-/g, '+').replace(/_/g, '/')), (c) => c.charCodeAt(0));

/** The game id for `prg` called `title`: "prg:..." */
export async function packCart(prg, title) {
  const name = new TextEncoder().encode(String(title).slice(0, 60));
  const raw = new Uint8Array(1 + name.length + prg.length);
  raw[0] = name.length;
  raw.set(name, 1);
  raw.set(prg, 1 + name.length);
  return PREFIX + toB64(await pipe(raw, new CompressionStream('gzip')));
}

/** Is `id` a game packed by packCart? */
export const isCart = (id) => String(id).startsWith(PREFIX);

/** The { title, prg } in a packCart id; throws if it is broken. */
export async function unpackCart(id) {
  if (!isCart(id)) throw new Error('not a packed program');
  const raw = await pipe(fromB64(id.slice(PREFIX.length)), new DecompressionStream('gzip'));
  const n = raw[0];
  const title = new TextDecoder().decode(raw.subarray(1, 1 + n));
  const prg = raw.slice(1 + n);
  if (prg.length < 3) throw new Error('the program in the link is empty');
  return { title, prg };
}
