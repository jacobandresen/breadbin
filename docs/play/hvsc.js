// hvsc - the High Voltage SID Collection, straight from hvsc.de (which sends
// CORS headers and plain-text directory listings). CSDb tunes name their HVSC
// path; the composer shelves list a musician's folder directly.

import { cachedBytes, cachedText } from './store.js';

const BASE = 'https://www.hvsc.de/download/C64Music';

export function sidUrl(path) {
  return BASE + path.split('/').map(encodeURIComponent).join('/');
}

/** The SID file at an HVSC path, cached for good. */
export function sid(path) {
  return cachedBytes(sidUrl(path));
}

/** File names in an HVSC folder ("/MUSICIANS/H/Hubbard_Rob/"). */
export async function list(dir) {
  const text = await cachedText(sidUrl(dir), 30 * 24 * 3600e3);
  if (text.trimStart().startsWith('<')) throw new Error('unexpected directory listing');
  return text.split('\n').map((l) => l.trim()).filter(Boolean);
}

/** "Commando.sid" -> "Commando". */
export function tuneTitle(file) {
  return file.replace(/\.sid$/i, '').replace(/_/g, ' ');
}

/** The giants of C64 music, for the composer shelves. */
export const LEGENDS = [
  ['Rob Hubbard', '/MUSICIANS/H/Hubbard_Rob/'],
  ['Martin Galway', '/MUSICIANS/G/Galway_Martin/'],
  ['Ben Daglish', '/MUSICIANS/D/Daglish_Ben/'],
  ['Jeroen Tel', '/MUSICIANS/T/Tel_Jeroen/'],
  ['Chris Hülsbeck', '/MUSICIANS/H/Huelsbeck_Chris/'],
  ['Tim Follin', '/MUSICIANS/F/Follin_Tim/'],
  ['David Whittaker', '/MUSICIANS/W/Whittaker_David/'],
  ['Matt Gray', '/MUSICIANS/G/Gray_Matt/'],
  ['Jonathan Dunn', '/MUSICIANS/D/Dunn_Jonathan/'],
  ['Laxity', '/MUSICIANS/L/Laxity/'],
  ['Reyn Ouwehand', '/MUSICIANS/O/Ouwehand_Reyn/'],
  ['Jammer', '/MUSICIANS/J/Jammer/'],
];
