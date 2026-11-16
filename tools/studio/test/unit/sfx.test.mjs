// Made-up sound effects (sfx.js): every kind gives a pattern that ends with a
// release, plays its own instrument, and goes into the music source.
import test from 'node:test';
import assert from 'node:assert/strict';
import { KINDS, makeSfx } from '../../../../docs/play/studio/sfx.js';
import { newMusic, patternBytes, musicSource, parseStep } from '../../../../docs/play/studio/assets/music.js';

test('each kind: a pattern named after it, its instrument, a release; a second take is another pattern', () => {
  let seed = 1;
  const rnd = () => ((seed = (seed * 16807) % 2147483647) / 2147483647);
  const m = newMusic();
  for (const kind of Object.keys(KINDS)) {
    const { pattern, instrument } = makeSfx(m, kind, rnd);
    const p = m.patterns[pattern];
    assert.equal(p.name, kind);
    assert.equal(m.instruments[instrument].name, `sfx ${kind}`);
    assert.ok(p.steps.some((s) => s.startsWith('===')), `${kind} ends`);
    const first = parseStep(p.steps[0]);
    assert.equal(first.ins, instrument, `${kind} plays its instrument`);
    assert.ok(patternBytes(p).length > 0);
  }
  const again = makeSfx(m, 'jump', rnd);
  assert.equal(m.patterns[again.pattern].name, 'jump 2');
  assert.equal(m.instruments.filter((i) => i.name === 'sfx jump').length, 1, 'the instrument is remade, not added again');
  assert.match(musicSource(m), /\S/);
});
