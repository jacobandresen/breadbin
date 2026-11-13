// codegen - the gen/ folder of a C project, written before every build from
// the asset editors' files and the runtime library:
//
//   gen/assets.h   SPR_, TILE_, FLAG_, ROOM_, SONG_ and SFX_ names for the code
//   gen/assets.c   the sprite images, the character set, tile colours and
//                  flags, and the rooms, run-length coded
//   gen/music.s    the instruments, patterns and songs for the player
//   gen/bb.h ...   the runtime (bb.h, bb.c, bbfast.s, bbirq.s, bbsound.s, and the
//                  pixels, bbpixel.c and bbdraw.s, for a project that draws them)
//
// gen/ is output, like a build folder: the editors are where assets change.
// It is written to the project folder too, so `make` with a native cc65
// builds the same program.

import { images, encodeSprite } from './assets/sprites.js';
import { encodeCharset, tileColors, tileFlags, FONT } from './assets/tiles.js';
import { roomTiles, rle } from './assets/map.js';
import { musicSource } from './assets/music.js';
import { resourceText } from './resource.js';

export const RUNTIME = ['bb.h', 'bb.c', 'bbfast.s', 'bbirq.s', 'bbsound.s'];
/** The pixel code: in gen/ for a project that draws pixels, else empty stand-ins (see generate). */
export const PIXELS = ['bbpixel.c', 'bbdraw.s'];
const PIXEL_CALLS = /\bbb_(bitmap|textmode|plot|pget|line|rect|fill|circle|disc|ink|btext|blit|grab|pen)\b/;

/** Does the project's code (not gen/) use the pixel functions? */
export const drawsPixels = (files) => Object.entries(files).some(([p, t]) => /\.(c|h)$/.test(p) && !/^gen\//.test(p) && typeof t === 'string' && PIXEL_CALLS.test(t));
export { ASSET_PATHS as ASSET_FILES } from './project.js';
import { ASSET_PATHS as ASSET_FILES } from './project.js';

/** "player run" -> "PLAYER_RUN", unique within `taken`. */
export function constName(prefix, name, taken) {
  let base = `${prefix}_${String(name || 'unnamed').toUpperCase().replace(/[^A-Z0-9]+/g, '_').replace(/^_+|_+$/g, '') || 'UNNAMED'}`;
  let n = base;
  for (let i = 2; taken.has(n); i++) n = `${base}_${i}`;
  taken.add(n);
  return n;
}

const cbytes = (b) => {
  const out = [];
  for (let i = 0; i < b.length; i += 16) out.push(`  ${[...b.subarray(i, i + 16)].map((v) => `0x${v.toString(16).padStart(2, '0')}`).join(', ')},`);
  return out.join('\n');
};
const comment = (s) => String(s ?? '').replace(/\*\//g, '* /').replace(/[^\x20-\x7e]/g, '?');

/** { 'gen/assets.h': text, 'gen/assets.c': text, 'gen/music.s': text } from parsed asset JSON. */
export function generateAssets({ sprites, tiles, map, music }) {
  const taken = new Set();
  const warnings = [];
  tiles.tiles.forEach((t, i) => {
    if (t?.multi && t.color > 7) warnings.push(`tile ${i}${t.name ? ` (${t.name})` : ''}: a multicolour tile's own colour can only be 0-7; ${t.color} shows as ${t.color & 7}`);
  });
  const h = [
    '/* gen/assets.h - written by the breadbin Studio from the assets/ folder.',
    ' * Change the assets in the Studio\'s editors: this file is made again on every build. */',
    '#ifndef ASSETS_H',
    '#define ASSETS_H',
    '',
  ];
  const c = ['/* gen/assets.c - written by the breadbin Studio from the assets/ folder. */', ''];

  // sprites
  const imgs = images(sprites);
  if (imgs.length > 64) throw new Error(`${imgs.length} sprite images: the runtime has room for 64 (at $D000)`);
  h.push('/* sprite images, for bb_spr() */');
  imgs.forEach((s, i) => h.push(`#define ${constName('SPR', s.constName, taken)} ${i}`));
  h.push(`#define SPR_COUNT ${imgs.length}`, '');
  const data = new Uint8Array(Math.max(1, imgs.length) * 64);
  imgs.forEach((s, i) => data.set(encodeSprite(s), i * 64));
  c.push(`const unsigned char bb_sprite_count = ${imgs.length};`);
  c.push(`const unsigned char bb_sprite_data[${data.length}] = {`, cbytes(data), '};');
  c.push(`const unsigned char bb_sprite_color[${Math.max(1, imgs.length)}] = { ${imgs.map((s) => s.color & 15).join(', ') || 0} };`);
  c.push(`const unsigned char bb_sprite_multi[${Math.max(1, imgs.length)}] = { ${imgs.map((s) => (s.multi ? 1 : 0)).join(', ') || 0} };`);
  c.push(`const unsigned char bb_sprite_mc[2] = { ${sprites.mc1 & 15}, ${sprites.mc2 & 15} };`, '');

  // tiles
  h.push('/* tiles (0..63 are the font, in screen-code order), for bb_set_tile() */');
  tiles.tiles.forEach((t, i) => {
    if (i >= FONT && t && t.name) h.push(`#define ${constName('TILE', t.name, taken)} ${i}`);
  });
  h.push('', '/* tile flags, for bb_fget() and bb_flags_at() */');
  (tiles.flags || []).slice(0, 8).forEach((f, i) => h.push(`#define ${constName('FLAG', f, taken)} 0x${(1 << i).toString(16).padStart(2, '0')}`));
  h.push('');
  const col = tiles.colors || {};
  c.push(`const unsigned char bb_screen_colors[4] = { ${col.border & 15}, ${col.bg & 15}, ${col.mc1 & 15}, ${col.mc2 & 15} };`);
  c.push(`const unsigned char bb_multicolor_tiles = ${tiles.tiles.some((t) => t?.multi) ? 1 : 0};`);
  c.push('const unsigned char bb_charset[2048] = {', cbytes(encodeCharset(tiles)), '};');
  c.push('const unsigned char bb_tile_color[256] = {', cbytes(Uint8Array.from(tileColors(tiles))), '};');
  c.push('const unsigned char bb_tile_flags[256] = {', cbytes(Uint8Array.from(tileFlags(tiles))), '};', '');

  // rooms
  h.push('/* rooms, for bb_room() */');
  const rooms = map.rooms || [];
  rooms.forEach((r, i) => {
    h.push(`#define ${constName('ROOM', r.name, taken)} ${i}`);
    c.push(`/* ${comment(r.name)} */`, `static const unsigned char room${i}[] = {`, cbytes(rle(roomTiles(map, r))), '};');
  });
  h.push(`#define ROOM_COUNT ${rooms.length}`, '');
  c.push(`const unsigned char bb_room_count = ${rooms.length};`);
  c.push(`const unsigned char *const bb_rooms[${Math.max(1, rooms.length)}] = { ${rooms.map((_, i) => `room${i}`).join(', ') || '0'} };`, '');

  // music
  h.push('/* songs, for bb_music() */');
  (music.songs || []).forEach((s, i) => h.push(`#define ${constName('SONG', s.name, taken)} ${i}`));
  h.push('', '/* patterns, for bb_sfx() */');
  (music.patterns || []).forEach((p, i) => h.push(`#define ${constName('SFX', p.name, taken)} ${i}`));
  h.push('', '#endif', '');

  const out = { 'gen/assets.h': h.join('\n'), 'gen/assets.c': c.join('\n'), 'gen/music.s': musicSource(music) };
  // (not a file: what the Studio shows as warnings about the assets)
  Object.defineProperty(out, 'warnings', { value: warnings, enumerable: false });
  return out;
}

let runtime = null;
/** The runtime library's files, as gen/<name>. */
export async function runtimeFiles(pixels = true) {
  runtime ??= Promise.all([...RUNTIME, ...PIXELS].map(async (f) => [`gen/${f}`, await resourceText(`runtime/${f}`)])).then(Object.fromEntries);
  const all = await runtime;
  if (pixels) return all;
  // a project that draws no pixels gets the files without the code: 4 KB less
  // program, and gen/ still holds the same files (a native make builds alike)
  return {
    ...Object.fromEntries(RUNTIME.map((f) => [`gen/${f}`, all[`gen/${f}`]])),
    'gen/bbpixel.c': '/* gen/bbpixel.c - empty: this project calls none of the pixel functions\n * (bb_bitmap, bb_plot, bb_blit...), so the Studio leaves their code out. */\ntypedef int bb_no_pixels;\n',
    'gen/bbdraw.s': '; gen/bbdraw.s - empty: this project calls none of the pixel functions, so the\n; Studio leaves their code out (see gen/bbpixel.c).\n',
  };
}

/** Parse a project's asset files (path -> text); a missing or broken one is reported by name. */
export function readAssets(files) {
  const out = {};
  for (const path of ASSET_FILES) {
    const key = path.replace(/^assets\/|\.json$/g, '');
    if (!(path in files)) throw new Error(`${path} is missing`);
    try {
      out[key] = JSON.parse(files[path]);
    } catch (e) {
      throw new Error(`${path}: ${e.message}`);
    }
  }
  return out;
}

/** Everything gen/ holds for the project `files`. */
export async function generate(files) {
  const assets = generateAssets(readAssets(files));
  const out = { ...(await runtimeFiles(drawsPixels(files))), ...assets };
  Object.defineProperty(out, 'warnings', { value: assets.warnings, enumerable: false });
  return out;
}
