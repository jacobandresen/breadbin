// "Go to definition" over a real project: Bubbler's sources, and its gen/.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { findDefinition } from '../../../../docs/play/studio/definitions.js';
import { generate } from '../../../../docs/play/studio/codegen.js';

const DIR = new URL('../../../../docs/play/studio/examples/bubbler/', import.meta.url);
const files = Object.fromEntries(JSON.parse(fs.readFileSync(new URL('files.json', DIR), 'utf8')).map((p) => [p, fs.readFileSync(new URL(p, DIR), 'utf8')]));
Object.assign(files, await generate(files));

const at = (name, prefer) => {
  const d = findDefinition(files, name, prefer);
  return d && `${d.file}: ${files[d.file].split('\n')[d.line - 1].trim()}`;
};

test('functions: to their bodies, not their calls or prototypes', () => {
  assert.equal(at('player_update', 'src/main.c'), 'src/player.c: void player_update(void) {');
  assert.equal(at('add_score', 'src/enemies.c'), 'src/main.c: void add_score(unsigned int points) {');
  assert.equal(at('body_fall'), 'src/body.c: void body_fall(void) {');
  assert.equal(at('take', 'src/enemies.c'), 'src/enemies.c: static void take(unsigned char i) {');
});

test('variables: where they are defined, not declared extern', () => {
  assert.equal(at('estate'), 'src/enemies.c: unsigned char ekind[MAX_ENEMIES], estate[MAX_ENEMIES];');
  assert.equal(at('byp_'), 'src/body.c: int bx_, by_, byp_;');
  assert.equal(at('lives'), 'src/main.c: unsigned char lives, round_no;');
});

test('#defines and enum members', () => {
  assert.equal(at('MAX_ENEMIES'), 'src/game.h: #define MAX_ENEMIES 6');
  assert.equal(at('E_TRAPPED'), 'src/game.h: enum { E_NONE, E_WALK, E_HOP, E_GHOST, E_TRAPPED, E_FRUIT };');
  assert.equal(at('NEAR'), 'src/game.h: #define NEAR(dx, dy, w, h) ((unsigned int)((dx) + (w)) < 2 * (w) && (unsigned int)((dy) + (h)) < 2 * (h))');
});

test('the runtime and the assets, in gen/', () => {
  assert.equal(at('SPR_BUB_STAND'), 'gen/assets.h: #define SPR_BUB_STAND 0');
  assert.equal(at('FLAG_SOLID'), 'gen/assets.h: #define FLAG_SOLID 0x01');
  assert.match(at('bb_spr'), /^gen\/bb\.h: void __fastcall__ bb_spr\(unsigned char n/);
  assert.equal(at('bb_init'), 'gen/bb.c: void bb_init(void) {');
  assert.equal(at('bbs_play'), 'gen/bbsound.s: bbs_play:');
});

test('names that are nowhere, or not names', () => {
  assert.equal(findDefinition(files, 'no_such_thing'), null);
  assert.equal(findDefinition(files, '42'), null);
  assert.equal(findDefinition(files, 'a b'), null);
});
