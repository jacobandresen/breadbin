// psid - the MUSIC editor's sound as a PSID file: the game's own player
// (runtime/bbsound.s) and the music data (gen/music.s), assembled at $1000
// and played by breadbin's SID engine (sid.js). What the editor plays is
// exactly what the game plays.
//
//   $1000  JMP bbs_init     A = the song (PSID songs count from 1, A from 0)
//   $1003  JMP bbs_play     once a frame
//
// A preview of one pattern (or one note) is a song of its own: see
// previewMusic().

import { musicSource } from './assets/music.js';
import { resourceText } from './resource.js';

export const LOAD = 0x1000;

const STUB = `        .import bbs_init, bbs_play
        .segment "STARTUP"
        jmp bbs_init
        jmp bbs_play
`;
// for the tests: start the song, then sound effect `n` at once
const SFX_STUB = (n) => `        .import bbs_init, bbs_play, bbs_sfx
        .segment "STARTUP"
        jmp init
        jmp bbs_play
init:   jsr bbs_init
        lda #${n}
        jmp bbs_sfx
`;
const CFG = `MEMORY { RAM: start = $1000, size = $8000, file = %O; }
SEGMENTS {
  STARTUP: load = RAM, type = ro;
  CODE:    load = RAM, type = ro;
  RODATA:  load = RAM, type = ro;
  DATA:    load = RAM, type = rw;
  BSS:     load = RAM, type = bss, define = yes;
}
`;

let player = null;

/**
 * Assemble `music` into the player's code and data (bytes from $1000), with
 * `tc` (a Toolchain). `sfx` also starts that sound effect with the song.
 */
export async function assemblePlayer(tc, music, { sfx = null } = {}) {
  player ??= resourceText('runtime/bbsound.s');
  const files = { 'stub.s': sfx == null ? STUB : SFX_STUB(sfx), 'bbsound.s': await player, 'music.s': musicSource(music), 'psid.cfg': CFG };
  const objects = {};
  const log = [];
  for (const s of ['stub.s', 'bbsound.s', 'music.s']) {
    const r = await tc.run('ca65', ['--color', 'off', s, '-o', s.replace('.s', '.o')], Object.fromEntries(Object.entries(files).map(([k, v]) => [`/p/${k}`, v])));
    log.push(r.log);
    if (r.code !== 0) throw new Error(`the music didn't assemble: ${log.join('\n').trim()}`);
    objects[`/p/${s.replace('.s', '.o')}`] = r.read(`/p/${s.replace('.s', '.o')}`);
  }
  const r = await tc.run('ld65', ['--color', 'off', '-C', 'psid.cfg', '-o', 'tune.bin', ...Object.keys(objects)], { '/p/psid.cfg': CFG, ...objects });
  if (r.code !== 0) throw new Error(`the music didn't link: ${r.log.trim()}`);
  return r.read('/p/tune.bin');
}

/** A PSID v2 file around `code` (loaded at $1000) with `songs` songs. */
export function psidFile(code, { songs = 1, start = 1, name = '', author = '', released = '' } = {}) {
  const head = new Uint8Array(0x7c);
  const be16 = (at, v) => ((head[at] = (v >> 8) & 255), (head[at + 1] = v & 255));
  const str = (at, s) => head.set(new TextEncoder().encode(String(s).replace(/[^\x20-\x7e]/g, '').slice(0, 31)), at);
  head.set([0x50, 0x53, 0x49, 0x44], 0); // "PSID"
  be16(4, 2); // version
  be16(6, 0x7c); // data offset
  be16(8, LOAD); // load address
  be16(10, LOAD); // init
  be16(12, LOAD + 3); // play
  be16(14, Math.max(1, songs));
  be16(16, Math.max(1, start));
  // speed: 0 for every song = the vertical blank, 50 times a second
  str(0x16, name);
  str(0x36, author);
  str(0x56, released);
  be16(0x76, 0x0014); // PAL, MOS 6581
  const out = new Uint8Array(head.length + code.length);
  out.set(head);
  out.set(code, head.length);
  return out;
}

/** The whole of `music` as a PSID (each song a sub-tune). */
export async function musicPsid(tc, music, meta = {}) {
  const code = await assemblePlayer(tc, music, meta);
  return psidFile(code, { songs: music.songs?.length || 1, ...meta });
}

/**
 * Music that plays only `what`: { pattern } loops one pattern on the first
 * voice; { song } plays from a song row; { note, ins } sounds one note.
 */
export function previewMusic(music, what) {
  if (what.note != null) {
    const step = `${['C-', 'C#', 'D-', 'D#', 'E-', 'F-', 'F#', 'G-', 'G#', 'A-', 'A#', 'B-'][what.note % 12]}${Math.floor(what.note / 12)} ${(what.ins ?? 0).toString(16)} ...`;
    const steps = [step, ...Array(what.hold ?? 15).fill('--- . ...'), '=== . ...', ...Array(15).fill('--- . ...')];
    return { ...music, patterns: [{ name: 'note', speed: 1, steps }], songs: [{ name: 'note', loop: -1, rows: [[0, -1, -1]] }] };
  }
  if (what.pattern != null) {
    return { ...music, songs: [{ name: 'pattern', loop: what.loop === false ? -1 : 0, rows: [[what.pattern, -1, -1]] }] };
  }
  const s = music.songs[what.song ?? 0];
  return { ...music, songs: [{ ...s, rows: s.rows.slice(what.row ?? 0), loop: s.loop >= (what.row ?? 0) ? s.loop - (what.row ?? 0) : s.loop }] };
}
