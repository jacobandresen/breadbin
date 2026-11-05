// look - play a title through probe-style steps and save a screenshot of the
// whole page after each "shot" step (probe.mjs's shot: grabs only the frame,
// which comes out blank in some runs). For checking a playtest by eye.
//
//   node look.mjs <id> <out-prefix> [profile:<name>] [rom:<file>] steps...
//   steps: wait:<s> until:<text> key:<NAME> fire:<port> joy:<port>:<DIR,..>:<ms> click:<selector> shot
import fs from 'node:fs';
import { launch, open, signDriveRom, sleep } from './c64.mjs';

const [id, prefix, ...steps] = process.argv.slice(2);
const opt = (name) => steps.find((s) => s.startsWith(name + ':'))?.slice(name.length + 1);
const roms = opt('rom') ? { floppy: signDriveRom(fs.readFileSync(opt('rom'))) } : {};
const browser = await launch({ profile: opt('profile') });
try {
  const c64 = await open(browser, id, { roms });
  if (c64.error) throw new Error(c64.error);
  let n = 0;
  for (const step of steps) {
    const [cmd, ...a] = step.split(':');
    if (cmd === 'wait') await sleep(a[0] * 1000);
    else if (cmd === 'until') for (let t = 0; t < 120 && !new RegExp(a[0], 'i').test(await c64.screenText()); t++) await sleep(500);
    else if (cmd === 'key') await c64.key(a[0]);
    else if (cmd === 'fire') await c64.fire(Number(a[0]));
    else if (cmd === 'joy') await c64.joy(Number(a[0]), a[1].split(','), Number(a[2]));
    else if (cmd === 'click') await c64.page.click(a.join(':'));
    else if (cmd === 'shot') {
      const file = `${prefix}${n++}.png`;
      await c64.page.screenshot({ path: file });
      console.log(file);
    }
  }
} finally {
  await browser.close();
}
