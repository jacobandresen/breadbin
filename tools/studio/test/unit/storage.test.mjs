// FolderStore: a project in a folder on disk, through the File System Access
// API (here an in-memory fake with the same interface).
import test from 'node:test';
import assert from 'node:assert/strict';
import { FolderStore } from '../../../../docs/play/studio/storage.js';
import { FakeDir } from './fake-fs.mjs';

test('writes files into subfolders and reads them back', async () => {
  const dir = new FakeDir('bubbler');
  const store = new FolderStore(dir);
  await store.write({ 'project.json': '{"name":"Bubbler"}', 'src/main.c': 'int main(void){}', 'gen/bb.h': '/* gen */' });
  assert.equal(store.label, 'bubbler/');
  const files = await new FolderStore(dir).read();
  assert.deepEqual(Object.keys(files).sort(), ['gen/bb.h', 'project.json', 'src/main.c']);
  assert.equal(files['src/main.c'], 'int main(void){}');
});

test('skips .git and build folders when reading', async () => {
  const dir = new FakeDir();
  await dir.put('src/main.c', 'x');
  await dir.put('.git/HEAD', 'ref');
  await dir.put('build/game.prg', 'p');
  assert.deepEqual(Object.keys(await new FolderStore(dir).read()), ['src/main.c']);
});

test('binary files stay bytes, text becomes strings', async () => {
  const dir = new FakeDir();
  const store = new FolderStore(dir);
  await store.write({ 'assets/logo.bin': new Uint8Array([0, 1, 255]), 'README.md': '# hi' });
  const files = await store.read();
  assert.deepEqual([...files['assets/logo.bin']], [0, 1, 255]);
  assert.equal(files['README.md'], '# hi');
});

test('changes() reports edits made outside the Studio, once', async () => {
  const dir = new FakeDir();
  const store = new FolderStore(dir);
  await store.write({ 'src/main.c': 'one', 'src/util.c': 'u' });
  await store.read();
  assert.deepEqual(await store.changes(), {}, 'nothing changed yet');
  await dir.put('src/main.c', 'two'); // e.g. saved from neovim
  await dir.put('src/new.h', 'new');
  assert.deepEqual(await store.changes(), { 'src/main.c': 'two', 'src/new.h': 'new' });
  assert.deepEqual(await store.changes(), {}, 'reported only once');
  await (await dir.getDirectoryHandle('src')).removeEntry('util.c');
  assert.deepEqual(await store.changes(), { 'src/util.c': null });
});

test('the Studio\'s own writes are not reported as outside changes', async () => {
  const dir = new FakeDir();
  const store = new FolderStore(dir);
  await store.read();
  await store.write({ 'src/main.c': 'mine' });
  assert.deepEqual(await store.changes(), {});
});

test('gen/ is never reported: the build writes it', async () => {
  const dir = new FakeDir();
  const store = new FolderStore(dir);
  await store.read();
  await dir.put('gen/assets.h', 'x');
  assert.deepEqual(await store.changes(), {});
});

test('null deletes a file', async () => {
  const dir = new FakeDir();
  const store = new FolderStore(dir);
  await store.write({ 'src/a.c': 'a', 'src/b.c': 'b' });
  await store.write({ 'src/a.c': null });
  assert.deepEqual(Object.keys(await store.read()), ['src/b.c']);
});

test('a native make in the folder is not an edit: its outputs are not read', async () => {
  const dir = new FakeDir();
  const store = new FolderStore(dir);
  await store.write({ 'src/main.c': 'x' });
  await store.read();
  await dir.put('game.prg', 'P');
  await dir.put('game.map', 'M');
  await dir.put('src/main.o', 'O');
  await dir.put('data/music.prg', 'an ACME include');
  assert.deepEqual(Object.keys(await store.changes()), ['data/music.prg']);
  assert.deepEqual(Object.keys(await new FolderStore(dir).read()).sort(), ['data/music.prg', 'src/main.c']);
});
