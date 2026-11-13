// The guide's tutorial ("Your first change: coins to collect", in
// tools/studio/README.md), done to the Empty game's files the way the editors
// would do it, then built: the steps and the code in the guide keep working.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { Toolchain } from '../../../../docs/play/studio/toolchain.js';
import { generate } from '../../../../docs/play/studio/codegen.js';
import { setCell } from '../../../../docs/play/studio/assets/map.js';
import { parseStep, formatStep, parseNote } from '../../../../docs/play/studio/assets/music.js';
import { pretty } from '../../../../docs/play/studio/project.js';

const DIR = new URL('../../../../docs/play/studio/examples/blank/', import.meta.url);
const GUIDE = fs.readFileSync(new URL('../../README.md', import.meta.url), 'utf8');

test('the coins tutorial builds', async () => {
  const files = Object.fromEntries(JSON.parse(fs.readFileSync(new URL('files.json', DIR), 'utf8')).map((p) => [p, fs.readFileSync(new URL(p, DIR), 'utf8')]));

  // 1. TILES: a yellow coin in slot 66, named, with the pickup flag
  const tiles = JSON.parse(files['assets/tiles.json']);
  assert.equal(tiles.tiles[66], null, 'slot 66 is empty, as the guide says');
  const pickup = tiles.flags.indexOf('pickup');
  assert.ok(pickup >= 0, 'the Empty game has a pickup flag');
  tiles.tiles[66] = { name: 'coin', multi: false, color: 7, flags: 1 << pickup, rows: ['..1111..', '.111111.', '11.11.11', '11.11.11', '11.11.11', '11.11.11', '.111111.', '..1111..'] };
  files['assets/tiles.json'] = `${pretty(tiles)}\n`;

  // 2. MAP: coins above the ground
  const map = JSON.parse(files['assets/map.json']);
  for (const x of [4, 10, 20, 28]) setCell(map, map.rooms[0], x, 21, 66);
  files['assets/map.json'] = `${pretty(map)}\n`;

  // 3. MUSIC: a "coin" pattern, speed 2: Q E T and 1, as typed in octave 4 (C-5 E-5 G-5, release)
  const music = JSON.parse(files['assets/music.json']);
  const steps = Array(8).fill('--- . ...');
  ['C-5', 'E-5', 'G-5'].forEach((n, i) => (steps[i] = formatStep({ ...parseStep(steps[i]), note: parseNote(n), ins: 0 })));
  steps[3] = '=== . ...';
  music.patterns.push({ name: 'coin', speed: 2, steps });
  files['assets/music.json'] = `${pretty(music)}\n`;

  // 4. CODE: the guide's own snippet, into the loop after the movement
  // (in the guide it is indented, inside a numbered list)
  const block = /( *)```c\n((?:\1.*\n)*?)\1```/.exec(GUIDE.slice(GUIDE.indexOf('### Your first change')));
  const snippet = block[2].split('\n').map((l) => l.slice(block[1].length)).join('\n');
  assert.match(snippet, /^\/\* a coin under the hero's middle/);
  let main = files['src/main.c'];
  main = main.replace('unsigned char left, walk;', 'unsigned char left, walk;\nunsigned char coins;');
  main = main.replace('    if (bb_btnp(BB_FIRE)) bb_sfx(SFX_BLIP);', `${snippet.replace(/^/gm, '    ').trimEnd()}\n\n    if (bb_btnp(BB_FIRE)) bb_sfx(SFX_BLIP);`);
  assert.match(main, /FLAG_PICKUP/);
  files['src/main.c'] = main;

  // 5. Run: it builds, without a warning, and the names the code uses exist
  const gen = await generate(files);
  assert.match(gen['gen/assets.h'], /#define TILE_COIN 66/);
  assert.match(gen['gen/assets.h'], /#define FLAG_PICKUP 0x08/);
  assert.match(gen['gen/assets.h'], /#define SFX_COIN \d+/);
  const tc = await Toolchain.load();
  const r = await tc.build({ ...files, ...gen });
  assert.equal(r.ok, true, r.diagnostics.map((d) => `${d.file}:${d.line}: ${d.message}`).join('\n'));
  assert.deepEqual(r.diagnostics, []);
});
