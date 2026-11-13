// Zip files, compiler messages, project folders and the generated C.
import test from 'node:test';
import assert from 'node:assert/strict';
import { deflateRawSync } from 'node:zlib';
import { zip, unzip, crc32 } from '../../../../docs/play/studio/zip.js';
import { parseDiagnostics, summary } from '../../../../docs/play/studio/diagnostics.js';
import { normalise, meta, sortPaths, slug } from '../../../../docs/play/studio/project.js';
import { constName, generateAssets } from '../../../../docs/play/studio/codegen.js';
import { newTileset } from '../../../../docs/play/studio/assets/tiles.js';
import { newMusic } from '../../../../docs/play/studio/assets/music.js';

test('crc32 matches the standard check value', () => {
  assert.equal(crc32(new TextEncoder().encode('123456789')), 0xcbf43926);
});

test('a project zips and unzips unchanged', async () => {
  const files = { 'project.json': '{}', 'src/main.c': 'int main(void) { return 0; }\n', 'assets/x.bin': new Uint8Array([0, 255, 7]) };
  const back = await unzip(zip(files));
  assert.deepEqual(Object.keys(back).sort(), Object.keys(files).sort());
  assert.equal(new TextDecoder().decode(back['src/main.c']), files['src/main.c']);
  assert.deepEqual([...back['assets/x.bin']], [0, 255, 7]);
});

test('unzip reads deflated members, as other zip tools write them', async () => {
  const text = 'hello hello hello hello hello\n';
  const stored = zip({ 'a.txt': text });
  // turn the stored member into a deflated one, by hand
  const body = deflateRawSync(Buffer.from(text));
  const name = Buffer.from('a.txt');
  const local = Buffer.alloc(30);
  local.writeUInt32LE(0x04034b50, 0); local.writeUInt16LE(8, 8);
  local.writeUInt32LE(crc32(Buffer.from(text)), 14); local.writeUInt32LE(body.length, 18); local.writeUInt32LE(text.length, 22);
  local.writeUInt16LE(name.length, 26);
  const central = Buffer.alloc(46);
  central.writeUInt32LE(0x02014b50, 0); central.writeUInt16LE(8, 10);
  central.writeUInt32LE(body.length, 20); central.writeUInt32LE(text.length, 24); central.writeUInt16LE(name.length, 28);
  const end = Buffer.from(stored.subarray(stored.length - 22));
  end.writeUInt32LE(46 + name.length, 12);
  end.writeUInt32LE(30 + name.length + body.length, 16);
  const z = Buffer.concat([local, name, body, central, name, end]);
  assert.equal(new TextDecoder().decode((await unzip(new Uint8Array(z)))['a.txt']), text);
  await assert.rejects(unzip(new Uint8Array(100)), /not a zip/);
});

test('a zipped folder loses its top folder; .git is skipped; text becomes strings', () => {
  const f = normalise({ 'bubbler/project.json': new TextEncoder().encode('{}'), 'bubbler/src/main.c': 'x', 'bubbler/.git/HEAD': 'y' }, { unwrap: true });
  assert.deepEqual(Object.keys(f).sort(), ['project.json', 'src/main.c']);
  assert.equal(f['project.json'], '{}');
  assert.deepEqual(Object.keys(normalise({ 'src/a.c': 'x' })), ['src/a.c'], 'without unwrap, src/ stays');
});

test('project.json defaults and the language guessed from the files', () => {
  assert.equal(meta({ 'src/main.c': '' }).lang, 'c');
  assert.equal(meta({ 'main.asm': '' }).entry, 'main.asm');
  assert.equal(meta({ 'project.json': '{"name":"X","lang":"acme","entry":"a.asm"}' }).name, 'X');
  assert.equal(meta({ 'project.json': 'not json', 'src/a.c': '' }).name, 'untitled');
  assert.deepEqual(sortPaths(['gen/bb.h', 'assets/map.json', 'Makefile', 'src/main.c', 'project.json']),
    ['project.json', 'src/main.c', 'assets/map.json', 'Makefile', 'gen/bb.h']);
  assert.equal(slug('My Game!'), 'my-game');
});

test('diagnostics: severities, quotes and tool-wide errors', () => {
  const d = parseDiagnostics('src/a.c:3: Warning: Variable ‘x’ is defined but never used\nsrc/a.c:9: Fatal: Cannot open\n2 errors and 1 warning generated.\nld65: Error: Memory area overflow');
  assert.deepEqual(d.map((x) => [x.file, x.line, x.severity]), [['src/a.c', 3, 'warning'], ['src/a.c', 9, 'error'], ['', 0, 'error']]);
  assert.equal(d[0].message, "Variable 'x' is defined but never used");
  assert.equal(summary(d), '2 errors, 1 warning');
  assert.equal(summary([]), 'no problems');
});

test('constant names are C identifiers, unique', () => {
  const taken = new Set();
  assert.equal(constName('SPR', 'player run', taken), 'SPR_PLAYER_RUN');
  assert.equal(constName('SPR', 'player-run', taken), 'SPR_PLAYER_RUN_2');
  assert.equal(constName('ROOM', '', taken), 'ROOM_UNNAMED');
  assert.equal(constName('SFX', 'Ünïcode ½', taken), 'SFX_N_CODE');
});

test('generated assets name everything the code uses', () => {
  const tiles = newTileset();
  tiles.tiles[64] = { name: 'brick', multi: false, color: 2, flags: 1, rows: Array(8).fill('11111111') };
  const g = generateAssets({
    sprites: { mc1: 1, mc2: 2, sprites: [{ name: 'hero', multi: true, color: 5, flip: true, rows: [] }] },
    tiles,
    map: { legend: { ' ': 32, '#': 64 }, rooms: [{ name: 'round 1', rows: ['#'.repeat(40)] }] },
    music: newMusic(),
  });
  const h = g['gen/assets.h'];
  for (const line of ['#define SPR_HERO 0', '#define SPR_HERO_FLIP 1', '#define SPR_COUNT 2', '#define TILE_BRICK 64', '#define FLAG_SOLID 0x01', '#define ROOM_ROUND_1 0', '#define SONG_SONG_0 0', '#define SFX_PATTERN_0 0']) {
    assert.ok(h.includes(line), line);
  }
  assert.ok(!/#define TILE_FONT/.test(h), 'the font tiles get no names');
  assert.match(g['gen/assets.c'], /bb_sprite_data\[128\]/);
  assert.match(g['gen/assets.c'], /room0\[\] = \{\n {2}0xff, 0x28, 0x40,/);
  assert.throws(() => generateAssets({ sprites: { sprites: Array(65).fill({ rows: [] }) }, tiles, map: { legend: {}, rooms: [] }, music: newMusic() }), /64/);
});

test('a plain cc65 project: no project.json, main.c anywhere, no assets', async () => {
  const { hasAssets, hasSources } = await import('../../../../docs/play/studio/project.js');
  const files = { 'hello.c': 'int main(void) { return 0; }', 'lib/util.c': '', 'Makefile': '' };
  assert.deepEqual(meta(files, { name: 'hello' }), { name: 'hello', entry: 'hello.c', lang: 'c' });
  assert.equal(meta({ 'game/main.c': '', 'a.c': '' }).entry, 'game/main.c');
  assert.equal(hasAssets(files), false);
  assert.equal(hasAssets({ 'assets/map.json': '{}' }), true);
  assert.equal(hasSources(files), true);
  assert.equal(hasSources({ 'notes.txt': '' }), false);
});

test('the pixel code comes only with projects that draw pixels', async () => {
  const { drawsPixels, PIXELS } = await import('../../../../docs/play/studio/codegen.js');
  assert.deepEqual(PIXELS, ['bbpixel.c', 'bbdraw.s']);
  assert.equal(drawsPixels({ 'src/main.c': 'bb_spr(0, 1, 2, 3);' }), false);
  assert.equal(drawsPixels({ 'src/main.c': 'bb_bitmap(1, 0); bb_line(0, 0, 9, 9);' }), true);
  assert.equal(drawsPixels({ 'src/draw.h': '#define X bb_blit' }), true);
  assert.equal(drawsPixels({ 'gen/bb.h': 'void bb_plot(int x, int y);', 'src/a.c': '' }), false, 'gen/ (bb.h itself) does not count');
  assert.equal(drawsPixels({ 'src/a.c': 'int bb_platform;' }), false, 'whole names only');
});
