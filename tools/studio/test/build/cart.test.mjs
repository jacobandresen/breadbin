// A game in a link (docs/play/cart.js): the Studio's Share packs the built
// program into a breadbin link, readable up to its data: #play/<Title>/<data>.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { packCart, unpackCart, isCart } from '../../../../docs/play/cart.js';
import { Toolchain } from '../../../../docs/play/studio/toolchain.js';
import { generate } from '../../../../docs/play/studio/codegen.js';

test('a program and its title go into a readable id and come back the same', async () => {
  const prg = Uint8Array.from({ length: 5000 }, (_, i) => (i * 7) & 255);
  const id = await packCart(prg, 'Mÿ Gäme (2)');
  // the title as link words, then the data: only characters a link keeps as they are
  assert.match(id, /^M%C3%BF-G%C3%A4me-%282%29\/[A-Za-z0-9_-]+$/);
  assert.equal(isCart(id), false, 'not the old form');
  const back = await unpackCart(id);
  assert.equal(back.title, 'Mÿ Gäme (2)');
  assert.deepEqual([...back.prg], [...prg]);
  assert.equal((await unpackCart(await packCart(prg, 'Bubbler'))).title, 'Bubbler');
  await assert.rejects(unpackCart('Broken/AAAA'));
});

test('a link of the old form (prg:<gzip of title and PRG>) still opens', async () => {
  const raw = new Uint8Array([3, ...new TextEncoder().encode('Old'), 1, 8, 0x60]);
  const gz = new Uint8Array(await new Response(new Blob([raw]).stream().pipeThrough(new CompressionStream('gzip'))).arrayBuffer());
  const id = `prg:${Buffer.from(gz).toString('base64url')}`;
  assert.ok(isCart(id));
  assert.deepEqual(await unpackCart(id), { title: 'Old', prg: new Uint8Array([1, 8, 0x60]) });
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
