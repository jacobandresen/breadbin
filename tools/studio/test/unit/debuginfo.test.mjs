// The debugger's map of lines and addresses (debuginfo.js), from a small
// debug file in ld65's format: a C file with f() and main(), cc65's own
// assembly of it (left out), and a project assembly file.
import test from 'node:test';
import assert from 'node:assert/strict';
import { DebugInfo } from '../../../../docs/play/studio/debuginfo.js';
import { formatRegs } from '../../../../docs/play/studio/debugger.js';

const DBG = [
  'version\tmajor=2,minor=0',
  'file\tid=0,name="/p/.build/src/main.s",size=1224,mtime=0x6AF73A2C,mod=0',
  'file\tid=1,name="/p/src/main.c",size=94,mtime=0x6AF73A2C,mod=0',
  'file\tid=2,name="/p/src/fast.s",size=90,mtime=0x6AF73A2C,mod=1',
  'file\tid=3,name="/p/gen/bbfast.s",size=90,mtime=0x6AF73A2C,mod=2',
  'line\tid=0,file=0,line=30,span=1',
  'line\tid=1,file=1,line=2,type=1,span=1',
  'line\tid=2,file=1,line=4,type=1,span=16+8',
  'line\tid=3,file=1,line=5,type=1,span=13',
  'line\tid=4,file=1,line=8,type=1,span=22',
  'line\tid=5,file=1,line=9,type=1,span=27',
  'line\tid=6,file=1,line=7,type=1',
  'line\tid=7,file=2,line=3,span=40',
  'line\tid=8,file=3,line=12,span=41',
  'span\tid=1,seg=0,start=0,size=3',
  'span\tid=16,seg=0,start=3,size=6',
  'span\tid=8,seg=0,start=27,size=6',
  'span\tid=13,seg=0,start=9,size=18',
  'span\tid=22,seg=0,start=36,size=5',
  'span\tid=27,seg=0,start=41,size=9',
  'span\tid=18,seg=0,start=0,size=36',
  'span\tid=32,seg=0,start=36,size=20',
  'span\tid=40,seg=1,start=0,size=2',
  'span\tid=41,seg=1,start=2,size=2',
  'seg\tid=0,name="CODE",start=0x000840,size=0x00B5,addrsize=absolute,type=ro',
  'seg\tid=1,name="FAST",start=0x002000,size=0x0004,addrsize=absolute,type=ro',
  'scope\tid=0,name="",mod=0,size=56,span=18+32',
  'scope\tid=1,name="_f",mod=0,type=scope,size=36,parent=0,sym=6,span=18',
  'scope\tid=2,name="_main",mod=0,type=scope,size=20,parent=0,sym=5,span=32',
  'sym\tid=5,name="_main",addrsize=absolute,size=20,scope=0,def=7,val=0x864,seg=0,type=lab',
  'sym\tid=6,name="_f",addrsize=absolute,size=36,scope=0,def=0,val=0x840,seg=0,type=lab',
].join('\n');

const dbg = new DebugInfo(DBG, { work: '/p' });

test('a C line is the addresses it starts at; a for loop has two', () => {
  assert.deepEqual(dbg.addrs('src/main.c', 2), [0x840]);
  assert.deepEqual(dbg.addrs('src/main.c', 4), [0x843, 0x85b]);
  assert.deepEqual(dbg.addrs('src/main.c', 7), [], 'a line without code');
  assert.deepEqual(dbg.codeLines('src/main.c'), [2, 4, 5, 8, 9]);
});

test("cc65's assembly is left out; a project's assembly is in", () => {
  assert.ok(!dbg.files.has('.build/src/main.s'));
  assert.deepEqual(dbg.addrs('src/fast.s', 3), [0x2000]);
  assert.deepEqual(dbg.addrs('gen/bbfast.s', 12), [0x2002]);
});

test('an address is on a line, in a function', () => {
  assert.deepEqual(dbg.lineAt(0x843), { file: 'src/main.c', line: 4 });
  assert.deepEqual(dbg.lineAt(0x850), { file: 'src/main.c', line: 5 }, 'inside the line');
  assert.equal(dbg.lineAt(0x1000), null);
  assert.equal(dbg.funcAt(0x850).name, 'f');
  assert.equal(dbg.funcAt(0x866).name, 'main');
  assert.equal(dbg.funcAt(0x1000), null);
  assert.ok(dbg.isStart(0x85b));
  assert.ok(!dbg.isStart(0x850));
});

test('a breakpoint on a line without code goes to the next that has some', () => {
  assert.equal(dbg.snap('src/main.c', 6), 8);
  assert.equal(dbg.snap('src/main.c', 2), 2);
  assert.equal(dbg.snap('src/main.c', 40), null);
  assert.equal(dbg.snap('src/other.c', 1), null);
});

test("where steps stop: a function's lines, or all (not the runtime's assembly)", () => {
  assert.deepEqual(dbg.lineStarts(dbg.funcAt(0x850)).sort(), [0x840, 0x843, 0x849, 0x85b].sort());
  const all = dbg.lineStarts();
  assert.ok(all.includes(0x2000), 'the project\'s assembly');
  assert.ok(!all.includes(0x2002), 'not gen/*.s');
  assert.ok(all.includes(0x864) && all.includes(0x869));
});

test('an empty or broken debug file is no lines', () => {
  const none = new DebugInfo('');
  assert.equal(none.lineAt(0x840), null);
  assert.deepEqual(none.lineStarts(), []);
  assert.deepEqual(new DebugInfo('nonsense\nline\tid=x').lineStarts(), []);
});

test('the registers, with what is not known as dots', () => {
  assert.equal(formatRegs(0x92c, { a: 0, x: 0x10, y: 0x40, sp: 0xf3 }), 'PC 092C  A 00  X 10  Y 40  SP F3');
  assert.equal(formatRegs(0x92c, { a: null, x: null, y: null, sp: 0xf1 }), 'PC 092C  A ··  X ··  Y ··  SP F1');
});
