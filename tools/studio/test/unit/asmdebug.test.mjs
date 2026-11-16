// ACME's listings made into the debugger's line table and the live values'
// variables (asmdebug.js), from a small program assembled by the Studio's
// own ACME (docs/play/acme.mjs, run here in Node).
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import createAcme from '../../../../docs/play/acme.mjs';
import { parseSymbols, parseReport, acmeDebug, asmVars } from '../../../../docs/play/studio/asmdebug.js';
import { DebugInfo } from '../../../../docs/play/studio/debuginfo.js';
import { tunables, setKnob } from '../../../../docs/play/studio/tuning.js';
import { MAIN, MOVE } from '../fixtures/border-asm.mjs';

const WASM = new URL('../../../../docs/play/acme.wasm', import.meta.url);

async function assemble(files, entry) {
  const log = [];
  const a = await createAcme({
    instantiateWasm: (imports, ok) => {
      WebAssembly.instantiate(fs.readFileSync(WASM), imports).then((r) => ok(r.instance));
      return {};
    },
    print: (s) => log.push(s),
    printErr: (s) => log.push(s),
  });
  a.FS.mkdirTree('/src');
  for (const [p, t] of Object.entries(files)) a.FS.writeFile(`/src/${p}`, t);
  a.FS.chdir('/src');
  try {
    a.callMain(['-f', 'cbm', '-o', '/out.prg', '-l', '/labels.txt', '-r', '/report.txt', entry]);
  } catch {}
  const text = (p) => new TextDecoder().decode(a.FS.readFile(p));
  return { symbols: parseSymbols(text('/labels.txt')), report: parseReport(text('/report.txt')), log: log.join('\n') };
}

const built = await assemble({ 'main.asm': MAIN, 'move.asm': MOVE }, 'main.asm');

test('ACME\'s symbols and report: labels, constants, each line\'s address in its own file', () => {
  const { symbols, report } = built;
  assert.equal(symbols.loop, 0x080e);
  assert.equal(symbols.counter, 0xfb);
  assert.equal(symbols.SCREEN, 0x0400);
  const sei = report.find((e) => /\bsei\b/.test(e.text));
  assert.deepEqual([sei.file, sei.line, sei.addr, sei.size], ['main.asm', 6, 0x080d, 1]);
  const adc = report.find((e) => /adc speed/.test(e.text));
  assert.deepEqual([adc.file, adc.line], ['move.asm', 3]);
  // back in main.asm after the !source
  assert.equal(report.filter((e) => e.file === 'main.asm').at(-1).line, 16);
});

test('the debugger\'s table: code lines to addresses and back, routines by their labels', () => {
  const d = new DebugInfo(acmeDebug(built.report, built.symbols));
  assert.deepEqual(d.addrs('main.asm', 7), [built.symbols.loop]);
  assert.deepEqual(d.lineAt(built.symbols.move + 1), { file: 'move.asm', line: 1 });
  assert.deepEqual(d.addrs('move.asm', 8), [], 'data is no line to stop on');
  assert.equal(d.funcAt(built.symbols.move + 3).name, 'move');
  assert.equal(d.funcAt(built.symbols.loop + 2).name, 'loop');
  // move's routine ends where its data begins
  assert.equal(d.funcAt(built.symbols.speed), null);
});

test('the variables: labels on data with their sizes, and zero-page constants the code uses', () => {
  const v = Object.fromEntries(asmVars(built.report, built.symbols).map((x) => [x.name, [x.addr, x.size, x.unit]]));
  assert.deepEqual(v.speed, [built.symbols.speed, 1, 1]);
  assert.deepEqual(v.pos, [built.symbols.pos, 1, 1]);
  assert.deepEqual(v.table, [built.symbols.table, 6, 1]);
  assert.deepEqual(v.counter, [0xfb, 1, 1]);
  assert.ok(!('SCREEN' in v) && !('loop' in v) && !('move' in v), 'constants and code are not variables');
});

test('assembly knobs: a label on one number nothing writes; setKnob keeps the number\'s form', () => {
  const k = tunables({ 'main.asm': MAIN, 'move.asm': MOVE });
  assert.deepEqual(k.map((x) => [x.name, x.value, x.size, x.file, x.line]), [['speed', 3, 1, 'move.asm', 8]]);
  assert.equal(setKnob('speed   !byte 3                 ; how far', k[0], 7), 'speed   !byte 7                 ; how far');
  assert.equal(setKnob('speed !byte $03', k[0], 12), 'speed !byte $0C');
  assert.equal(setKnob('_speed: .byte 3', { name: 'speed' }, 4), '_speed: .byte 4');
  assert.equal(setKnob('unsigned char speed = 3;', { name: 'speed' }, 9), 'unsigned char speed = 9;');
});
