// probe - look inside one title from the command line, built on c64.mjs. Steps
// run in order once the emulator is up; see README.md.
//
//   node probe.mjs gb64:l1/LNINJA2A_04285_02.zip wait:30 pcs dis:4590 shot:ln2.png
//   node probe.mjs gb64:l1/LNINJA2A_04285_02.zip rom:dos.bin wait:15 'type:load"$",8\n' wait:10 drive text
//
// Steps:
//   wait:<s>            let it run
//   type:<text>         auto-type (\n is RETURN)
//   key:<NAME>          hold a key from KEYS (RETURN, SPACE, F1, RUNSTOP, 1...)
//   fire:<port>         press fire
//   joy:<port>:<DIR,..>:<ms>   hold the stick (PULL_RIGHT, PULL_UP...)
//   pcs                 sample the program counter
//   dis:<hex>           disassemble from an address
//   mem:<hex>:<n>       dump memory
//   shell:<command>     a RetroShell command (debugger commands go through debug:)
//   debug:<command>     a command in the debugger, which pauses the C64 meanwhile
//   text | sprites | drive   screen text, sprite positions, vc64web's drive 8
//   sound               the sound level over ~3 s (RMS samples; 0 is silence)
//   shot:<file.png>     screenshot
//   click:<selector>    click a button of the page (#emu-disk: next disk, #emu-port)
//   js:<expression>     evaluate in vc64web's frame (its globals: wasm_peek, $...)
//   page:<expression>   evaluate in breadbin's page (e.g. $('#emu-note').textContent)
//   insert:<file>       hand a local file (.crt, .d64, .prg...) to vc64web, as the page does
// Options (anywhere): rom:<file> installs a 1541 ROM (signed with signDriveRom)
// so the disk goes to vc64web's emulated drive; kernal:, basic:, charset:<file>
// install C64 ROMs (all three together); boot:<file> picks the disk file to start;
// parts:<id>,<id> names the Internet Archive items of a release (games.txt's "a + b");
// patches:<addr:old=new>,... live patches for the game's loader (games.txt's "{...}").

import fs from 'node:fs';
import { launch, open, signDriveRom, sleep } from './c64.mjs';

const [id, ...steps] = process.argv.slice(2);
if (!id) {
  console.log(fs.readFileSync(new URL(import.meta.url), 'utf8').split('\n\nimport')[0]);
  process.exit(1);
}
const opt = (name) => steps.find((s) => s.startsWith(name + ':'))?.slice(name.length + 1);
const read = (k) => opt(k) && fs.readFileSync(opt(k));
const roms = Object.fromEntries(Object.entries({ kernal: read('kernal'), basic: read('basic'), charset: read('charset'), floppy: read('rom') && signDriveRom(read('rom')) }).filter(([, v]) => v));
const browser = await launch();
try {
  const c64 = await open(browser, id, { boot: opt('boot'), parts: opt('parts')?.split(','), patches: opt('patches')?.split(','), roms });
  if (c64.error) throw new Error(c64.error);
  for (const step of steps) {
    const [cmd, ...a] = step.split(':');
    const arg = a.join(':');
    const out = await {
      wait: () => sleep(arg * 1000),
      type: () => c64.type(arg.replace(/\\n/g, '\n')),
      key: () => c64.key(arg),
      fire: () => c64.fire(Number(arg)),
      joy: () => c64.joy(Number(a[0]), a[1].split(','), Number(a[2])),
      pcs: () => c64.pcs(),
      dis: () => c64.disasm(parseInt(arg, 16)),
      mem: () => c64.peek(parseInt(a[0], 16), Number(a[1] || 16)).then((b) => [].concat(b).map((x) => x.toString(16).padStart(2, '0')).join(' ')),
      shell: () => c64.shell(arg),
      debug: () => c64.debug(arg),
      text: () => c64.screenText().then((t) => t.match(/.{40}/g).join('\n')),
      sprites: () => c64.sprites(),
      sound: () => c64.sound().then((v) => v.map((x) => x.toFixed(3)).join(' ')),
      drive: () => c64.driveState(),
      shot: () => c64.shot(arg).then(() => arg),
      click: () => c64.page.click(arg),
      js: () => c64.frame.evaluate(arg),
      page: () => c64.page.evaluate(arg),
      insert: () => c64.page.evaluate((name, bytes) => document.querySelector('.emu-frame').contentWindow.postMessage({ cmd: 'load', file_name: name, file: new Uint8Array(bytes) }, '*'), arg.split('/').pop(), [...fs.readFileSync(arg)]),
      ...Object.fromEntries(['rom', 'kernal', 'basic', 'charset', 'boot', 'parts', 'patches'].map((k) => [k, () => {}])),
    }[cmd]?.();
    if (out !== undefined) console.log(`── ${step}\n${typeof out === 'string' ? out : JSON.stringify(out)}`);
  }
  if (c64.jammed) console.log('── the CPU jammed');
  if (c64.drive.length) console.log('── virtual drive', c64.drive.map((d) => `${d.event} ${d.name || ''}`).join(', '));
} finally {
  await browser.close();
}
