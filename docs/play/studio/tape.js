// tape - the runtime's record of the joystick (bb.c: bb_tape), read from a
// running game and written back into a program, for two things:
//
//   Keep my place  Run reads the tape from the game as it is, writes it into
//                  the new build's PRG and starts that at warp speed: the new
//                  code plays the same input up to where you were (or to the
//                  marked moment), then you play on. Edit and continue.
//   Replay         the marked moment again, with the input played since then
//                  played again: the same jump, every time, to compare
//                  settings rather than thumbs.
//
// The tape is pairs of [joystick bits, frames 1-255], bb_tape_len bytes, and
// the seed bb_init used. Its variables are initialised, so they are in the
// PRG: a program can be given a tape before it starts.

const VARS = ['bb_tape_mode', 'bb_tape_len', 'bb_tape_pos', 'bb_tape_left', 'bb_tape_seed', 'bb_tape_frames', 'bb_tape'];
export const PLAY = 2;

/** Has this build a tape (a Studio runtime game)? */
export const hasTape = (labels) => VARS.every((v) => labels?.[`_${v}`] !== undefined);

/** The [address, length] ranges to peek for readTape(), from a build's labels and the tape's size. */
export function tapeRanges(labels, size) {
  return [['bb_tape_mode', 1], ['bb_tape_len', 2], ['bb_tape_seed', 2], ['bb_tape_frames', 2], ['bb_tape', size]].map(([v, n]) => [labels[`_${v}`], n]);
}

/** The tape from what tapeRanges() peeked: { mode, seed, frames, bytes }. */
export function readTape([mode, len, seed, frames, bytes]) {
  const n = len[0] | (len[1] << 8);
  return { mode: mode[0], seed: seed[0] | (seed[1] << 8), frames: frames[0] | (frames[1] << 8), bytes: bytes.slice(0, n) };
}

/** How many frames the tape's bytes hold. */
export const tapeFrames = (bytes) => {
  let n = 0;
  for (let i = 1; i < bytes.length; i += 2) n += bytes[i];
  return n;
};

/** The tape cut after `frames` frames (the last pair shortened). */
export function cut(bytes, frames) {
  const out = [];
  let left = frames;
  for (let i = 0; i + 1 < bytes.length && left > 0; i += 2) {
    const n = Math.min(bytes[i + 1], left);
    out.push(bytes[i], n);
    left -= n;
  }
  return Uint8Array.from(out);
}

/** Where playing `bytes` stands after `frames` frames: { pos (the next pair's byte), left (frames left of the pair before it) }. */
export function position(bytes, frames) {
  let left = frames;
  for (let i = 0; i + 1 < bytes.length; i += 2) {
    if (left < bytes[i + 1]) return { pos: i + 2, left: bytes[i + 1] - left };
    left -= bytes[i + 1];
  }
  return { pos: bytes.length, left: 0 };
}

const word = (v) => [v & 255, (v >> 8) & 255];

/**
 * The writes ([[address, bytes], ...]) that make a game play `bytes` from
 * frame `from` (0: from bb_init, with `seed`), then record on.
 */
export function playWrites(labels, bytes, { from = 0, seed = null } = {}) {
  const a = (v) => labels[`_${v}`];
  const { pos, left } = from ? position(bytes, from) : { pos: 0, left: 0 };
  return [
    [a('bb_tape'), [...bytes]],
    [a('bb_tape_len'), word(bytes.length)],
    [a('bb_tape_pos'), word(pos)],
    [a('bb_tape_left'), [left]],
    ...(seed === null ? [] : [[a('bb_tape_seed'), word(seed)]]),
    [a('bb_tape_mode'), [PLAY]],
  ];
}

/** `prg` (load address first) with `writes` made into its bytes: a program that starts with them. */
export function patchPrg(prg, writes) {
  const out = Uint8Array.from(prg);
  const at = prg[0] | (prg[1] << 8);
  for (const [addr, bytes] of writes) {
    const i = addr - at + 2;
    if (i < 2 || i + bytes.length > out.length) throw new Error(`the tape is outside the program ($${addr.toString(16)})`);
    out.set(bytes, i);
  }
  return out;
}

/** Was the joystick used after frame `from` on the tape (anything but standing still)? */
export function usedAfter(bytes, from) {
  let at = 0;
  for (let i = 0; i + 1 < bytes.length; i += 2) {
    at += bytes[i + 1];
    if (at > from && bytes[i]) return true;
  }
  return false;
}
