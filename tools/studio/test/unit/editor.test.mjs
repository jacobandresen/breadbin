// What the editors are built on: the API help read from bb.h, the asset
// files' layout, paint tools, undo, labels and ACME's messages.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { parseApi, parseAssetNames } from '../../../../docs/play/studio/api.js';
import { pretty } from '../../../../docs/play/studio/project.js';
import { parseLabels, memoryUse } from '../../../../docs/play/studio/toolchain.js';
import { parseAcme } from '../../../../docs/play/studio/acme-diag.js';
import { flood, line, rect, shift, flipH, flipV, History } from '../../../../docs/play/studio/pixels.js';
import { nextName } from '../../../../docs/play/studio/sprite-editor.js';
import { generateAssets } from '../../../../docs/play/studio/codegen.js';
import { newTileset } from '../../../../docs/play/studio/assets/tiles.js';
import { newMusic } from '../../../../docs/play/studio/assets/music.js';

const BB_H = fs.readFileSync(new URL('../../../../docs/play/studio/runtime/bb.h', import.meta.url), 'utf8');

test('the API help names every function bb.h declares, each with its comment', () => {
  const api = parseApi(BB_H);
  const declared = [...BB_H.matchAll(/\b(bb_\w+)\s*\(/g)].map((m) => m[1]);
  for (const name of new Set(declared)) {
    const e = api.find((x) => x.name === name);
    assert.ok(e, `${name} is in the help`);
    if (e.kind === 'function') assert.ok(e.doc.length > 10, `${name} has a comment`);
  }
  const spr = api.find((e) => e.name === 'bb_spr');
  assert.equal(spr.signature, 'void bb_spr(unsigned char n, unsigned char img, int x, int y)');
  assert.match(spr.doc, /top-left at x, y/);
  assert.equal(api.filter((e) => / \(a C64 colour\)$/.test(e.signature)).length, 16);
  assert.equal(api.find((e) => e.name === 'BB_FIRE').signature, 'BB_FIRE = 0x10');
  assert.match(api.find((e) => e.name === 'BB_STOP').doc, /stops the music/);
});

test('asset names come from gen/assets.h, grouped', () => {
  const names = parseAssetNames('/* sprite images, for bb_spr() */\n#define SPR_HERO 0\n#define SPR_COUNT 1\n/* rooms, for bb_room() */\n#define ROOM_START 0\n');
  assert.deepEqual(names, [
    { name: 'SPR_HERO', value: '0', group: 'sprite images' },
    { name: 'SPR_COUNT', value: '1', group: 'sprite images' },
    { name: 'ROOM_START', value: '0', group: 'rooms' },
  ]);
});

test('asset JSON: rows one per line, numbers inline, small objects on a line, nulls together', () => {
  const text = pretty({ rows: ['..11', '1111'], nums: [1, 2, 3], song: [[0, 1, -1], [2, 3, 4]], ins: { name: 'x', a: 1 }, tiles: [null, null, null, { a: 1 }] });
  assert.equal(text, `{
  "rows": [
    "..11",
    "1111"
  ],
  "nums": [1, 2, 3],
  "song": [[0, 1, -1], [2, 3, 4]],
  "ins": { "name": "x", "a": 1 },
  "tiles": [
    null, null, null,
    { "a": 1 }
  ]
}`);
  const v = { a: [1, { b: [null, 'x'] }], c: 'd' };
  assert.deepEqual(JSON.parse(pretty(v)), v);
});

test('labels and memory from the linker\'s files', () => {
  assert.deepEqual(parseLabels('al 000810 .__STARTUP_RUN__\nal 00C03A ._score\n'), { __STARTUP_RUN__: 0x810, _score: 0xc03a });
  const map = 'Segment list:\n-------------\nName                   Start     End    Size  Align\n----------------------------------------------------\nCODE                  00080D  001FFF  0017F3  00001\nBSS                   002000  0021FF  000200  00001\n\n';
  const m = memoryUse(map, new Uint8Array(0x1800), 0xc000);
  assert.deepEqual(m.segments.map((s) => s.name), ['CODE', 'BSS']);
  assert.equal(m.used, 0x2200 - 0x0801);
});

test('ACME\'s messages', () => {
  assert.deepEqual(parseAcme('Error - File code/main.asm, line 12 (Zone <untitled>): Value not defined (foo).\nWarning - File a.asm, line 3: Label name starts with a shift-space.'), [
    { file: 'code/main.asm', line: 12, severity: 'error', message: 'Value not defined (foo).', tool: 'acme' },
    { file: 'a.asm', line: 3, severity: 'warning', message: 'Label name starts with a shift-space.', tool: 'acme' },
  ]);
});

test('paint tools: lines, boxes, fills, shifts, flips', () => {
  assert.deepEqual(line(0, 0, 3, 1), [[0, 0], [1, 0], [2, 1], [3, 1]]);
  assert.equal(rect(0, 0, 2, 2).length, 8);
  assert.equal(rect(2, 2, 0, 0, true).length, 9);
  const grid = ['..#.', '..#.', '####'].map((r) => [...r]);
  flood(grid, 0, 0, 'x');
  assert.deepEqual(grid.map((r) => r.join('')), ['xx#.', 'xx#.', '####']);
  assert.deepEqual(shift(['ab', 'cd'], 1, 0), ['ba', 'dc']);
  assert.deepEqual(shift(['ab', 'cd'], 0, 1), ['cd', 'ab']);
  assert.deepEqual(flipH(['ab']), ['ba']);
  assert.deepEqual(flipV(['a', 'b']), ['b', 'a']);
});

test('undo and redo', () => {
  const h = new History(3);
  h.push({ v: 1 });
  h.push({ v: 2 });
  assert.deepEqual(h.undo({ v: 3 }), { v: 2 });
  assert.deepEqual(h.redo({ v: 2 }), { v: 3 });
  for (let i = 0; i < 10; i++) h.push({ v: i });
  assert.equal(h.past.length, 3, 'a limited history');
  assert.equal(new History().undo({}), null);
});

test('copies get the next free name', () => {
  assert.equal(nextName('walk 1', ['walk 1']), 'walk 2');
  assert.equal(nextName('walk 1', ['walk 1', 'walk 2']), 'walk 3');
  assert.equal(nextName('hero', []), 'hero 2');
});

test('a multicolour tile in a colour above 7 is warned about', () => {
  const tiles = newTileset();
  tiles.tiles[64] = { name: 'lava', multi: true, color: 8, flags: 0, rows: Array(8).fill('1111') };
  const g = generateAssets({ sprites: { sprites: [] }, tiles, map: { legend: {}, rooms: [] }, music: newMusic() });
  assert.equal(g.warnings.length, 1);
  assert.match(g.warnings[0], /tile 64 \(lava\).*0-7/);
  assert.ok(!Object.keys(g).includes('warnings'), 'not a file');
});

test('the Variables panel: the game\'s globals and their sizes, from the labels', async () => {
  const { globals, format, ranges } = await import('../../../../docs/play/studio/variables.js');
  const memory = { segments: [{ name: 'CODE', start: 0x0810, size: 0x1000 }, { name: 'BSS', start: 0x2000, size: 0x20 }] };
  const labels = { _main: 0x0900, _score: 0x2000, _lives: 0x2002, _ex: 0x2003, _bb_frames: 0x200f, __BSS_RUN__: 0x2000, _last: 0x201e };
  assert.deepEqual(globals(labels, memory), [
    { name: 'score', addr: 0x2000, size: 2, unit: 1 },
    { name: 'lives', addr: 0x2002, size: 1, unit: 1 },
    { name: 'ex', addr: 0x2003, size: 12, unit: 1 }, // up to bb_frames: the runtime's labels still end it
    { name: 'last', addr: 0x201e, size: 2, unit: 1 }, // to the end of the segment
  ]);
  assert.equal(format([3], 1), '3');
  assert.equal(format([255], 1), '255 (-1)');
  assert.equal(format([0x10, 0x27], 2), '10000');
  assert.equal(format([0xff, 0xff], 2), '65535 (-1)');
  assert.equal(format([1, 2, 3, 4, 5, 6, 7, 8], 12), '[1, 2, 3, 4, 5, 6, 7, 8, …]');
  assert.equal(format(null, 1), '?');
  assert.deepEqual(ranges([{ addr: 5, size: 40 }]), [[5, 8]]);
});

test('C variables\' sizes, from cc65\'s assembly', async () => {
  const { dataSizes } = await import('../../../../docs/play/studio/toolchain.js');
  const { globals, format } = await import('../../../../docs/play/studio/variables.js');
  const asm = `.segment\t"DATA"\n_score:\n\t.word\t$0005\n_table:\n\t.word\t$0001\n\t.word\t$0002\n\t.word\t$0003\n_name:\n\t.addr\tS0001\n.segment\t"RODATA"\nS0001:\n\t.byte\t$48,$49,$00\n.segment\t"BSS"\n_lives:\n\t.res\t1,$00\n_after:\n\t.res\t6,$00\n.segment\t"CODE"\n.proc\t_main: near\n\tlda _lives\n.endproc\n`;
  const sizes = dataSizes(asm);
  assert.deepEqual(sizes, { score: { size: 2, unit: 2 }, table: { size: 6, unit: 2 }, name: { size: 2, unit: 2 }, lives: { size: 1, unit: 1 }, after: { size: 6, unit: 1 } });
  // with sizes, a variable followed by unnamed statics is no longer stretched to the next label
  const memory = { segments: [{ name: 'BSS', start: 0x2000, size: 0x40 }] };
  assert.deepEqual(globals({ _lives: 0x2000, _far: 0x2010 }, memory, sizes)[0], { name: 'lives', addr: 0x2000, size: 1, unit: 1 });
  assert.equal(format([1, 0, 2, 0, 0xff, 0xff], 6, 2), '[1, 2, 65535]');
});

test('C headers: functions and constants for completion, with their comments', async () => {
  const { parseHeader, systemIncludes } = await import('../../../../docs/play/studio/api.js');
  const header = `/*****************************************/
/* Copyright notice                       */
/*****************************************/
#ifndef _THING_H
#define _THING_H

/* The biggest thing */
#define THING_MAX 255
#define COLOR_RED 2  /* a colour */

/* Copy n bytes */
void* __fastcall__ memcpy (void* dest, const void* src,
                           size_t count);
int _internal (void);
typedef unsigned char thing_t;
unsigned char thing_count (void);   /* how many */
#define thing_first() (0)
#endif
`;
  const e = parseHeader(header, 'thing.h');
  assert.deepEqual(e.map((x) => [x.name, x.kind]), [['THING_MAX', 'constant'], ['COLOR_RED', 'constant'], ['memcpy', 'function'], ['thing_count', 'function'], ['thing_first', 'function']]);
  assert.equal(e[0].doc, 'The biggest thing');
  assert.equal(e[1].doc, 'a colour');
  assert.equal(e[2].signature, 'void* memcpy(void* dest, const void* src, size_t count)');
  assert.equal(e[2].doc, 'Copy n bytes');
  assert.equal(e[3].doc, 'how many');
  assert.equal(e[2].from, 'thing.h');
  assert.deepEqual(systemIncludes('#include <stdlib.h>\n#include "bb.h"\n  # include <c64.h>\n'), ['stdlib.h', 'c64.h']);
});
