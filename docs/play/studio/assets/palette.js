// palette - the 16 C64 colours (Philip "Pepto" Timmermann's measured
// palette, the one most emulators use) and their names, in VIC order.

export const PALETTE = [
  '#000000', '#ffffff', '#68372b', '#70a4b2', '#6f3d86', '#588d43', '#352879', '#b8c76f',
  '#6f4f25', '#433900', '#9a6759', '#444444', '#6c6c6c', '#9ad284', '#6c5eb5', '#959595',
];

export const COLOR_NAMES = [
  'black', 'white', 'red', 'cyan', 'purple', 'green', 'blue', 'yellow',
  'orange', 'brown', 'pink', 'dark grey', 'grey', 'light green', 'light blue', 'light grey',
];

/** [r, g, b] of colour `c` (0..15). */
export function rgb(c) {
  const n = parseInt(PALETTE[c & 15].slice(1), 16);
  return [n >> 16, (n >> 8) & 255, n & 255];
}
