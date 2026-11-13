// A game in a link (docs/play/cart.js): the Studio's Share packs the built
// program into a breadbin game link.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { packCart, unpackCart, isCart } from '../../../../docs/play/cart.js';
import { Toolchain } from '../../../../docs/play/studio/toolchain.js';
import { generate } from '../../../../docs/play/studio/codegen.js';

test('a program and its title go into an id and come back the same', async () => {
  const prg = Uint8Array.from({ length: 5000 }, (_, i) => (i * 7) & 255);
  const id = await packCart(prg, 'Mÿ Gäme');
  assert.ok(isCart(id));
  assert.match(id, /^prg:[A-Za-z0-9_-]+$/, 'only characters a link keeps as they are');
  const back = await unpackCart(id);
  assert.equal(back.title, 'Mÿ Gäme');
  assert.deepEqual([...back.prg], [...prg]);
  await assert.rejects(unpackCart('prg:AAAA'));
  assert.equal(isCart('gb64:x'), false);
});

test('Bubbler fits in a link of a sensible length', async () => {
  const DIR = new URL('../../../../docs/play/studio/examples/bubbler/', import.meta.url);
  const files = Object.fromEntries(JSON.parse(fs.readFileSync(new URL('files.json', DIR), 'utf8')).map((p) => [p, fs.readFileSync(new URL(p, DIR), 'utf8')]));
  const r = await (await Toolchain.load()).build({ ...files, ...(await generate(files)) });
  const id = await packCart(r.prg, 'Bubbler');
  assert.ok(id.length < 16000, `${id.length} characters for a ${r.prg.length}-byte program`);
});
