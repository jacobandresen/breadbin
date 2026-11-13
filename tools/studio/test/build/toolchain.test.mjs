// The cc65 toolchain in WebAssembly, as the Studio's build worker runs it:
// it builds C into a PRG, reports errors where they are, and measures memory.
import test from 'node:test';
import assert from 'node:assert/strict';
import { Toolchain } from '../../../../docs/play/studio/toolchain.js';

const tc = await Toolchain.load();

test('a C program builds into a PRG that loads at $0801', async () => {
  const r = await tc.build({ 'src/main.c': '#include <stdio.h>\nint main(void) { puts("HELLO"); return 0; }\n' });
  assert.equal(r.ok, true, r.log);
  assert.equal(r.prg[0] | (r.prg[1] << 8), 0x0801);
  // the BASIC stub: 10 SYS 2061
  assert.match(String.fromCharCode(...r.prg.subarray(2, 20)), /\x9e2061/);
  assert.ok(r.memory.used > 500 && r.memory.used < 4000, `used ${r.memory.used}`);
  assert.equal(r.memory.total, 0xc000 - 0x0801);
});

test('compile errors point at the file and line', async () => {
  const r = await tc.build({ 'src/main.c': 'int main(void) {\n  int a = b;\n  return 0\n}\n' });
  assert.equal(r.ok, false);
  assert.deepEqual(r.diagnostics.map((d) => [d.file, d.line, d.severity]), [['src/main.c', 2, 'error'], ['src/main.c', 4, 'error']]);
  assert.match(r.diagnostics[0].message, /Undeclared identifier 'b'/);
});

test('a missing function is reported at the C line that calls it', async () => {
  const r = await tc.build({ 'src/main.c': 'void jump(void);\n\nint main(void) {\n  jump();\n  return 0;\n}\n' });
  assert.equal(r.ok, false);
  assert.equal(r.diagnostics.length, 1, JSON.stringify(r.diagnostics));
  assert.deepEqual([r.diagnostics[0].file, r.diagnostics[0].line], ['src/main.c', 4]);
  assert.match(r.diagnostics[0].message, /'jump' is used but never defined/);
});

test('C and ca65 assembly link together, headers from any project folder', async () => {
  const r = await tc.build({
    'src/main.c': '#include "add.h"\nunsigned char x;\nint main(void) { x = add3(4); return x; }\n',
    'src/add.h': 'unsigned char __fastcall__ add3(unsigned char a);\n',
    'src/add.s': '.export _add3\n_add3: clc\n adc #3\n ldx #0\n rts\n',
  });
  assert.equal(r.ok, true, r.log);
});

test('warnings do not stop the build', async () => {
  const r = await tc.build({ 'src/main.c': 'int main(void) { int unused; return 0; }\n' }, { defines: {} });
  assert.equal(r.ok, true);
});

test('cc65\'s own headers read into completion entries', async () => {
  const { parseHeader } = await import('../../../../docs/play/studio/api.js');
  const text = (n) => new TextDecoder().decode(tc.sysroot[`include/${n}`]);
  const names = (n) => parseHeader(text(n), n).map((e) => e.name);
  for (const [h, want] of [['stdlib.h', ['rand', 'srand', 'malloc', 'abs']], ['string.h', ['memcpy', 'memset', 'strlen']], ['c64.h', ['COLOR_BLACK', 'COLOR_WHITE']], ['conio.h', ['clrscr', 'gotoxy', 'cputs']]]) {
    for (const w of want) assert.ok(names(h).includes(w), `${w} from <${h}>`);
  }
});
