// The Tuning panel's knobs: globals declared with a number that the code
// never changes. Bubbler's jump settings are knobs; its spawn point is not.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { tunables, sliderRange } from '../../../../docs/play/studio/tuning.js';

const DIR = new URL('../../../../docs/play/studio/examples/bubbler/', import.meta.url);
const files = Object.fromEntries(JSON.parse(fs.readFileSync(new URL('files.json', DIR), 'utf8')).map((p) => [p, fs.readFileSync(new URL(p, DIR), 'utf8')]));

test('Bubbler\'s knobs: the jump\'s settings, not the spawn point the game sets', () => {
  const k = tunables(files);
  assert.deepEqual(k.map((x) => [x.name, x.value, x.signed, x.size]), [['jump', -62, true, 1], ['gravity', 3, false, 1], ['max_fall', 36, false, 1]]);
  assert.equal(k[0].file, 'src/main.c');
  assert.match(files['src/main.c'].split('\n')[k[0].line - 1], /^signed char jump = -62;/);
});

test('Bouncer\'s knobs: the bat\'s and the serve\'s speeds, not the bat\'s place or width', () => {
  const dir = new URL('../../../../docs/play/studio/examples/bouncer/', import.meta.url);
  const src = { 'src/main.c': fs.readFileSync(new URL('src/main.c', dir), 'utf8') };
  assert.deepEqual(tunables(src).map((x) => x.name), ['bat_speed', 'ball_speed']);
});

test('what the code changes is state: =, +=, ++, --, and taking its address', () => {
  const src = (body) => ({ 'src/a.c': `int speed = 3, lives = 3, x = 1, y = 2, z = 5;\nunsigned char k = 7; /* a knob */\nvoid f(void) {\n${body}\n}\n` });
  const names = (body) => tunables(src(body)).map((x) => x.name);
  assert.deepEqual(names('lives -= 1; ++x; y++; g(&z); if (speed == 3) {}'), ['speed', 'k']);
  assert.deepEqual(names('/* speed = 4; */ // k = 1;'), ['speed', 'lives', 'x', 'y', 'z', 'k']);
  assert.deepEqual(tunables({ 'gen/assets.c': 'int not_mine = 3;' }), []);
});

test('a slider reaches around the value, inside what the type holds', () => {
  assert.deepEqual(sliderRange({ value: 3, signed: false, size: 1 }), [0, 13]);
  assert.deepEqual(sliderRange({ value: -62, signed: true, size: 1 }), [-128, 62]);
  assert.deepEqual(sliderRange({ value: 200, signed: false, size: 1 }), [0, 255]);
  assert.deepEqual(sliderRange({ value: 1000, signed: true, size: 2 }), [0, 3000]);
});
