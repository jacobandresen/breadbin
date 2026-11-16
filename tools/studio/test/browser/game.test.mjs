// The example games, built in the Studio and played on vc64web (the C64
// emulator the page uses, on the internet: these tests skip when it can't be
// reached). A robot reads the C64's screen and sprites and works the joystick.
import test from 'node:test';
import assert from 'node:assert/strict';
import { serve, launch, openStudio, run, c64, online, sleep, sharedLink } from './harness.mjs';

const skip = !(await online()) && 'vc64web is not reachable';
const server = await serve();
const browser = await launch();
test.after(async () => {
  await browser.close();
  server.close();
});

test('Bubbler: title, a round, running, jumping and blowing a bubble', { skip, timeout: 120000 }, async () => {
  const page = await openStudio(browser, server.url);
  assert.match(await run(page), /^Built in/);
  const c = await c64(page);
  await c.until(async () => /PRESS {2}FIRE/.test(await c.row(21)), 30000);
  assert.match(await c.row(3) + await c.row(9), /A ONE-SCREEN PLATFORMER/);
  await c.press('FIRE');
  await c.until(async () => /SCORE 000000 +ROUND 01 +LIVES 3/.test(await c.row(0)));
  // the player stands at the start, blinking for two seconds (safe from monsters)
  await sleep(2200);
  const start = await c.until(() => c.sprite(0));
  await c.joy('PULL_RIGHT');
  await sleep(700);
  await c.joy('RELEASE_XY');
  const moved = await c.sprite(0);
  assert.ok(moved[0] > start[0] + 10, `ran right: ${start} -> ${moved}`);
  // a jump goes up and comes back down
  await c.joy('PULL_UP');
  await sleep(150);
  const up = await c.sprite(0);
  await c.joy('RELEASE_XY');
  assert.ok(up[1] < moved[1] - 4, `jumped: ${moved} -> ${up}`);
  await c.until(async () => (await c.sprite(0))?.[1] === moved[1], 5000);
  // fire blows a bubble (sprite 1)
  await c.joy('PRESS_FIRE');
  const bubble = await c.until(() => c.sprite(1), 3000);
  await c.joy('RELEASE_FIRE');
  assert.ok(bubble, 'a bubble');
  // and the monsters (sprites 2-7) move by themselves
  const before = await c.peek(0xd004, 12);
  await sleep(800);
  assert.notDeepEqual(await c.peek(0xd004, 12), before, 'the monsters move');
  // the Variables panel reads the game's globals live
  await page.$eval('.studio-vars', (d) => (d.open = true));
  // (a monster may have caught the player by now: lives is 1 to 3)
  await page.waitForFunction(() => [...document.querySelectorAll('.studio-var')].some((v) => v.firstChild.textContent === 'lives' && /^[1-3]$/.test(v.lastChild.textContent)), { timeout: 10000 });
  const vars = await page.$$eval('.studio-var', (els) => Object.fromEntries(els.map((e) => [e.firstChild.textContent, e.lastChild.textContent])));
  assert.equal(vars.round_no, '1');
  assert.match(vars.ekind, /^\[[0-9, ]+\]$/, 'an array of six bytes');
  assert.ok('score' in vars && 'px' in vars, Object.keys(vars).join(' '));
  assert.deepEqual(page.errors, []);
});

test('Bubbler\'s rules: a trapped monster pops into a cherry, a cherry scores, a clear room is the next round', { skip, timeout: 120000 }, async () => {
  const page = await openStudio(browser, server.url);
  assert.match(await run(page), /^Built in/);
  const L = await page.evaluate(() => window.breadbinStudio.lastBuild.labels);
  const c = await c64(page);
  const word = async (addr) => { const [lo, hi] = await c.peek(addr, 2); return lo | (hi << 8); };
  const E = { NONE: 0, TRAPPED: 4 };
  await c.until(async () => /PRESS {2}FIRE/.test(await c.row(21)), 30000);
  await c.press('FIRE');
  await c.until(async () => /ROUND 01/.test(await c.row(0)));
  await sleep(500);
  // the player onto solid floor away from the wall (it starts at the very left,
  // where a trapped monster, which keeps clear of the walls, can't reach it;
  // and not over the hole in the middle, where the cherry would fall through)
  const px = 80;
  const py = await word(L._py_px);
  // ...and monster 0 caught in a bubble right there. In steps: the game copies
  // a monster it moves and writes the copy back, which would undo a poke made
  // in between; a monster that is "none" it leaves alone. So: all monsters
  // none, then the player and monster 0 placed, then monster 0 trapped.
  await c.pokeHeld([[L._estate, [E.NONE, 0, 0, 0, 0, 0]]]);
  await c.pokeHeld([
    [L._px, [px, 0]],
    [L._ex, [px & 255, px >> 8]],
    [L._ey, [(py << 4) & 255, (py << 4) >> 8]],
    [L._eyp, [py & 255, py >> 8]],
  ]);
  await c.poke([[L._estate, [E.TRAPPED]]]);
  // the player touches it: it pops into a cherry (100 points), and the cherry,
  // there on the player, is taken at once (500 more): faster than this test
  // looks, so the score tells the story (it is kept in tens)
  await c.until(async () => (await word(L._score)) >= 10, 5000);
  await c.until(async () => (await word(L._score)) === 60, 8000);
  assert.equal((await c.peek(L._estate))[0], E.NONE, 'the cherry is gone');
  assert.match(await c.row(0), /SCORE 000600/);
  // no monsters left: after a moment, round 2
  await c.until(async () => /ROUND 02/.test(await c.row(0)), 8000);
  assert.equal((await c.peek(L._round_no))[0], 2);
});

test('a shared link plays the game for whoever opens it', { skip, timeout: 120000 }, async () => {
  const page = await openStudio(browser, server.url);
  await page.evaluate(() => window.breadbinStudio.openExample('bouncer'));
  assert.match(await run(page), /^Built in/);
  const link = await sharedLink(page, server.url);
  const friend = await (await browser.createBrowserContext()).newPage();
  await friend.goto(link, { waitUntil: 'domcontentloaded' });
  const c = await c64(friend, '#emu-stage');
  await c.until(async () => /PRESS FIRE/.test(await c.row(14)), 40000);
  assert.match(await c.row(0), /SCORE 000000 +LEVEL 01/);
  await friend.close();
});

test('while the C64 has the keyboard, Ctrl+Enter still runs and Escape goes back to the code', { skip, timeout: 120000 }, async () => {
  const page = await openStudio(browser, server.url);
  await page.evaluate(() => window.breadbinStudio.openExample('blank'));
  assert.match(await run(page), /^Built in/);
  const c = await c64(page);
  await c.until(async () => /HELLO FROM THE STUDIO!/.test(await c.row(1)), 30000);
  await page.$eval('.studio-buildinfo', (e) => (e.textContent = ''));
  const key = (init) => c.frame.evaluate((i) => dispatchEvent(new KeyboardEvent('keydown', { bubbles: true, cancelable: true, ...i })), init);
  await key({ key: 'Enter', ctrlKey: true });
  await page.waitForFunction(() => /^Built in/.test(document.querySelector('.studio-buildinfo').textContent), { timeout: 20000 });
  await key({ key: 'Escape' });
  await page.waitForFunction(() => document.activeElement?.classList.contains('cm-content'), { timeout: 5000 });
});

test('the control deck under the C64 plays: its FIRE starts Bubbler', { skip, timeout: 120000 }, async () => {
  const page = await openStudio(browser, server.url);
  assert.match(await run(page), /^Built in/);
  const c = await c64(page);
  await c.until(async () => /PRESS {2}FIRE/.test(await c.row(21)), 30000);
  await page.click('.studio-padbtn');
  // the FIRE button, inside the <blip-deck>'s shadow root
  const cap = await page.waitForSelector('.studio-deck:not([hidden]) blip-deck >>> [part~="button"]');
  const box = await cap.boundingBox();
  const fire = { x: box.x + box.width / 2, y: box.y + box.height / 2 };
  await page.mouse.move(fire.x, fire.y);
  await page.mouse.down();
  await sleep(300);
  await page.mouse.up();
  await c.until(async () => /ROUND 01/.test(await c.row(0)), 10000);
});

test('the debugger: a breakpoint stops Bubbler on its line; step over, into, out; run on; pause', { skip, timeout: 180000 }, async () => {
  const page = await openStudio(browser, server.url);
  const st = (f, a) => page.evaluate(f, a);
  // title()'s loop: bb_frame(); then the line that blinks PRESS FIRE
  const loop = await st(() => {
    const t = window.breadbinStudio.files['src/main.c'].split('\n');
    return t.findIndex((l, i) => /^\s+bb_frame\(\);/.test(l) && /for \(i = 0;; \+\+i\)/.test(t[i - 1])) + 1;
  });
  await st((l) => window.breadbinStudio.toggleBreakpoint('src/main.c', l + 1), loop);
  assert.match(await run(page), /^Built in/);
  // where the CPU stopped: "title() src/main.c:82", and the yellow line in the editor
  const stop = async (step) => {
    if (step) await st((s) => window.breadbinStudio.dbg[s](), step);
    await page.waitForFunction(() => document.querySelector('.studio-debug.stopped') && !/…/.test(document.querySelector('.studio-dbg-where').textContent), { timeout: 60000 });
    const where = await page.$eval('.studio-dbg-where', (e) => e.textContent);
    return { where, exec: await st(() => window.breadbinStudio.code.exec), file: await st(() => window.breadbinStudio.path) };
  };
  let s = await stop();
  assert.equal(s.where, `title() src/main.c:${loop + 1}`);
  assert.equal(s.exec, loop + 1);
  assert.match(await page.$eval('.studio-dbg-regs', (e) => e.textContent), /^PC [0-9A-F]{4} {2}A [0-9A-F]{2} {2}X [0-9A-F]{2} {2}Y [0-9A-F]{2} {2}SP [0-9A-F]{2}$/);
  assert.equal(await page.$$eval('.cm-exec-line', (e) => e.length), 1);
  s = await stop('over');
  assert.equal(s.where, `title() src/main.c:${loop + 2}`);
  // round the loop to bb_frame(); and into it: the runtime's C, in gen/bb.c
  for (let i = 0; i < 6 && !s.where.endsWith(`:${loop}`); i++) s = await stop('over');
  assert.equal(s.where, `title() src/main.c:${loop}`);
  s = await stop('into');
  assert.match(s.where, /^bb_frame\(\) gen\/bb\.c:\d+$/);
  assert.equal(s.file, 'gen/bb.c');
  // out: back in title(), on the line after the call
  s = await stop('out');
  assert.equal(s.where, `title() src/main.c:${loop + 1}`);
  // run on: the breakpoint again, a frame later
  const frames = await st(() => window.breadbinStudio.session.peek([[window.breadbinStudio.lastBuild.labels._bb_frames, 1]]));
  s = await stop('cont');
  assert.equal(s.where, `title() src/main.c:${loop + 1}`);
  const later = await st(() => window.breadbinStudio.session.peek([[window.breadbinStudio.lastBuild.labels._bb_frames, 1]]));
  assert.notDeepEqual(later, frames, 'a frame went by');
  // no breakpoints, run on: the game runs (the title blinks); pause stops it in src/
  await st(() => window.breadbinStudio.clearBreakpoints());
  await st(() => window.breadbinStudio.dbg.cont());
  const c = await c64(page);
  await c.until(async () => /PRESS {2}FIRE/.test(await c.row(21)), 10000);
  await c.until(async () => !/PRESS/.test(await c.row(21)), 10000);
  await st(() => window.breadbinStudio.debugToggle());
  s = await stop();
  assert.match(s.where, / src\/\w+\.c:\d+$/);
  const a = await st(() => window.breadbinStudio.session.peek([[window.breadbinStudio.lastBuild.labels._bb_frames, 1]]));
  await sleep(500);
  assert.deepEqual(await st(() => window.breadbinStudio.session.peek([[window.breadbinStudio.lastBuild.labels._bb_frames, 1]])), a, 'stopped: no frames go by');
  // Run starts the program afresh, running
  assert.match(await run(page), /^Built in/);
  await c.until(async () => /PRESS {2}FIRE/.test(await c.row(21)), 30000);
  assert.ok(!(await page.$('.studio-debug.stopped')));
  assert.deepEqual(page.errors, []);
});

test('Run again after an edit starts the new program in the running C64', { skip, timeout: 120000 }, async () => {
  const page = await openStudio(browser, server.url);
  await page.evaluate(() => window.breadbinStudio.openExample('blank'));
  assert.match(await run(page), /^Built in/);
  const c = await c64(page);
  await c.until(async () => /HELLO FROM THE STUDIO!/.test(await c.row(1)), 30000);
  // the hero walks right
  const start = await c.until(() => c.sprite(0));
  await c.joy('PULL_RIGHT');
  await sleep(600);
  await c.joy('RELEASE_XY');
  assert.ok((await c.sprite(0))[0] > start[0], 'the hero walks');
  // change the greeting, Run: the same C64 (no new frame) shows the new one
  const frame = await page.$('.studio-stage iframe');
  await page.evaluate(() => {
    const s = window.breadbinStudio;
    s.openFile('src/main.c');
    const v = s.code.view;
    const at = v.state.doc.toString().indexOf('HELLO FROM THE STUDIO!');
    v.dispatch({ changes: { from: at, to: at + 22, insert: 'EDITED AND RUN AGAIN!' } });
  });
  assert.match(await run(page), /^Built in/);
  await c.until(async () => /EDITED AND RUN AGAIN!/.test(await c.row(1)), 20000);
  assert.ok(await frame.evaluate((f) => f.isConnected), 'the same emulator frame');
});

test('Bubbler: a monster on the last life is game over; fire is the title again', { skip, timeout: 120000 }, async () => {
  const page = await openStudio(browser, server.url);
  assert.match(await run(page), /^Built in/);
  const L = await page.evaluate(() => window.breadbinStudio.lastBuild.labels);
  const c = await c64(page);
  const word = async (addr) => { const [lo, hi] = await c.peek(addr, 2); return lo | (hi << 8); };
  await c.until(async () => /PRESS {2}FIRE/.test(await c.row(21)), 30000);
  await c.press('FIRE');
  await c.until(async () => /ROUND 01/.test(await c.row(0)));
  await sleep(2300); // past the two seconds a new life is safe
  const px = 80;
  const py = await word(L._py_px);
  // the last life, and a ghost (it ignores the platforms, so it stays put) on the player
  await c.pokeHeld([[L._estate, [0, 0, 0, 0, 0, 0]], [L._lives, [1]]]);
  await c.pokeHeld([[L._px, [px, 0]], [L._ex, [px, 0]], [L._ey, [(py << 4) & 255, (py << 4) >> 8]], [L._eyp, [py & 255, py >> 8]], [L._ekind, [3]]]);
  await c.poke([[L._estate, [3]]]);
  await c.until(async () => /GAME {2}OVER/.test(await c.row(12)), 10000);
  assert.equal((await c.peek(L._palive))[0], 0);
  await c.press('FIRE');
  await c.until(async () => /PRESS {2}FIRE/.test(await c.row(21)), 10000);
});

test('Pixels: shapes in the bitmap, blits beat plots, balls move, peek and poke', { skip, timeout: 150000 }, async () => {
  const page = await openStudio(browser, server.url);
  await page.evaluate(() => window.breadbinStudio.openExample('pixels'));
  assert.match(await run(page), /^Built in/);
  const L = await page.evaluate(() => window.breadbinStudio.lastBuild.labels);
  const c = await c64(page);
  const word = async (addr) => { const [lo, hi] = await c.peek(addr, 2); return lo | (hi << 8); };
  /** Is pixel x, y of the bitmap ($E000, the VIC's layout) set? */
  const pixel = async (x, y) => ((await c.ram(0xe000 + (y >> 3) * 320 + (x & ~7) + (y & 7)))[0] >> (7 - (x & 7))) & 1;
  // 1: shapes, in bitmap mode
  await c.until(async () => (await c.peek(0xd011))[0] & 0x20, 30000);
  // the last things drawn: the disc, then a slot erased from it
  await c.until(async () => (await pixel(272 - 30, 144)) && !(await pixel(272, 144)), 10000);
  assert.equal(await pixel(80, 100), 1, 'the fan of lines meets at 80, 100');
  assert.equal(await pixel(272 - 30, 144), 1, 'the disc\'s left edge');
  assert.equal(await pixel(272, 144), 0, 'the slot erased from the disc');
  assert.equal(await pixel(200 + 35, 144), 1, 'the biggest circle');
  assert.equal(await pixel(200, 144), 0, 'the circles\' empty middle');
  // 2: plot against blit
  await c.until(async () => {
    if ((await c.peek(L._plot_frames, 2)).some(Boolean)) return true;
    await c.press('FIRE', 150);
    return false;
  }, 30000);
  await c.until(async () => (await word(L._blit_frames)) > 0, 30000);
  const plot = await word(L._plot_frames);
  const blit = await word(L._blit_frames);
  assert.ok(blit * 10 < plot, `blits ${blit} frames, plots ${plot}: at least ten times faster`);
  // 3: bouncing balls, and the UFO sprite over the bitmap (fire pressed until
  // it counts: a press while the race still prints its result is missed)
  await c.until(async () => {
    if (await c.sprite(0)) return true;
    await c.press('FIRE', 150);
    return false;
  }, 15000);
  const x0 = await word(L._bx);
  await sleep(500);
  assert.notEqual(await word(L._bx), x0, 'the balls move');
  // 4: peek and poke, in text mode
  await c.until(async () => {
    if (/PEEK AND POKE/.test(await c.row(1))) return true;
    await c.press('FIRE', 150);
    return false;
  }, 15000);
  assert.equal((await c.peek(0xd011))[0] & 0x20, 0, 'back in text mode');
  assert.match(await c.row(4), /BB_PEEK\(0XD012\)/);
});

test('Bouncer\'s rules: the W pickup widens the bat; no bricks left is the next level', { skip, timeout: 120000 }, async () => {
  const page = await openStudio(browser, server.url);
  await page.evaluate(() => window.breadbinStudio.openExample('bouncer'));
  assert.match(await run(page), /^Built in/);
  const L = await page.evaluate(() => window.breadbinStudio.lastBuild.labels);
  const W = Number(/#define SPR_PICKUP_W (\d+)/.exec(await page.evaluate(() => window.breadbinStudio.files['gen/assets.h']))[1]);
  const c = await c64(page);
  await c.until(async () => /PRESS FIRE/.test(await c.row(14)), 30000);
  await c.press('FIRE');
  await c.until(async () => !/PRESS FIRE/.test(await c.row(14)));
  assert.equal(await c.sprite(1), null, 'one bat sprite');
  // serve: pickups fall only while the ball is in play (a press straight after
  // the last one can come in the same frame as its release, and not count)
  await c.until(async () => {
    if ((await c.peek(L._served))[0]) return true;
    await c.press('FIRE', 200);
    return false;
  }, 10000);
  // a W pickup falling onto the bat
  // (in steps, as above: an inactive pickup is left alone, so it is placed first, then switched on)
  const [lo, hi] = await c.peek(L._batx, 2);
  await c.pokeHeld([[L._pickon, [0, 0, 0]]]);
  await c.pokeHeld([[L._pickx, [lo, hi]], [L._picky, [176, 0]], [L._pickimg, [W]]]);
  await c.poke([[L._pickon, [1]]]);
  await c.until(async () => (await c.peek(L._batw))[0] === 48, 5000);
  assert.ok(await c.until(() => c.sprite(1), 2000), 'the bat is two sprites wide');
  // the last brick gone: level 2 (if the ball was lost meanwhile, the game
  // waits for the next serve before it looks: fire serves)
  await c.poke([[L._bricks, [0]]]);
  await c.press('FIRE');
  await c.until(async () => /LEVEL 02/.test(await c.row(0)), 8000);
});

test('Bouncer: serve the ball, move the bat', { skip, timeout: 120000 }, async () => {
  const page = await openStudio(browser, server.url);
  await page.evaluate(() => window.breadbinStudio.openExample('bouncer'));
  assert.match(await run(page), /^Built in/);
  const c = await c64(page);
  await c.until(async () => /PRESS FIRE/.test(await c.row(14)), 30000);
  assert.match(await c.row(0), /SCORE 000000 +LEVEL 01 +LIVES 3/);
  await c.press('FIRE');
  await c.until(async () => !/PRESS FIRE/.test(await c.row(14)));
  const bat = await c.until(() => c.sprite(0));
  await c.joy('PULL_LEFT');
  await sleep(500);
  await c.joy('RELEASE_XY');
  assert.ok((await c.sprite(0))[0] < bat[0] - 10, 'the bat moved left');
  // serve: the ball (sprite 2) leaves the bat, going up (pressed again if a
  // press came too soon after the last to count)
  const L = await page.evaluate(() => window.breadbinStudio.lastBuild.labels);
  const onBat = await c.sprite(2);
  await c.until(async () => {
    if ((await c.peek(L._served))[0]) return true;
    await c.press('FIRE', 200);
    return false;
  }, 10000);
  await sleep(300);
  const flying = await c.sprite(2);
  assert.ok(flying && flying[1] < onBat[1] - 8, `the ball flies: ${onBat} -> ${flying}`);
});

test('live values: the code shows the running game\'s variables; a drag changes the game, and on its declaration the code', { skip, timeout: 120000 }, async () => {
  const page = await openStudio(browser, server.url);
  await page.evaluate(() => window.breadbinStudio.openFile('src/main.c'));
  assert.match(await run(page), /^Built in/);
  const c = await c64(page);
  // (0 until the program is in the C64's memory)
  await page.waitForFunction(() => [...document.querySelectorAll('.cm-live-item')].some((x) => x.textContent === 'gravity3'), { timeout: 60000 });
  const chip = async (name) => page.evaluate((name) => {
    const el = [...document.querySelectorAll('.cm-live-item')].find((x) => x.firstChild.textContent === name).lastChild;
    el.scrollIntoView({ block: 'center' });
    const r = el.getBoundingClientRect();
    return { x: r.x + r.width / 2, y: r.y + r.height / 2, text: el.textContent, keep: el.classList.contains('keep') };
  }, name);
  const g = await chip('gravity');
  assert.equal(g.text, '3');
  assert.ok(g.keep, 'gravity = 3 is its declaration: a tweak is kept in the code');
  // drag right: 4 steps of 5 pixels
  await page.mouse.move(g.x, g.y);
  await page.mouse.down();
  for (let i = 1; i <= 4; i++) {
    await page.mouse.move(g.x + i * 5 + 1, g.y);
    await sleep(40);
  }
  await page.mouse.up();
  const v = await page.evaluate(() => window.breadbinStudio.vars.find((x) => x.name === 'gravity'));
  await c.until(async () => (await c.peek(v.addr))[0] === 7, 3000);
  assert.match(await page.evaluate(() => window.breadbinStudio.files['src/main.c']), /^unsigned char gravity = 7;/m);
  // the Tuning panel: Bubbler's three knobs; a slider moved changes the game, let go the code
  assert.deepEqual(await page.$$eval('.studio-tuning-name', (e) => e.map((x) => x.textContent)), ['jump', 'gravity', 'max_fall']);
  await page.$eval('.studio-tuning input[aria-label=jump]', (i) => {
    i.value = -40;
    i.dispatchEvent(new Event('input'));
    i.dispatchEvent(new Event('change'));
  });
  const j = await page.evaluate(() => window.breadbinStudio.vars.find((x) => x.name === 'jump'));
  await c.until(async () => (await c.peek(j.addr))[0] === 256 - 40, 3000);
  assert.match(await page.evaluate(() => window.breadbinStudio.files['src/main.c']), /^signed char jump = -40;/m);
  // a graph of another, from its name
  await page.evaluate(() => [...document.querySelectorAll('.cm-live-name')].find((x) => x.textContent === 'tick').dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, button: 0 })));
  await page.waitForFunction(() => /tick\s*\d+/.test(document.querySelector('.studio-graph-head')?.textContent || ''), { timeout: 10000 });
  assert.deepEqual(page.errors, []);
});

test('slow motion: the speed button runs the C64 at half and a quarter of its speed, and back', { skip, timeout: 120000 }, async (t) => {
  const page = await openStudio(browser, server.url);
  assert.match(await run(page), /^Built in/);
  const c = await c64(page);
  await c.until(async () => /PRESS {2}FIRE/.test(await c.row(21)), 30000);
  // the CPU's cycles in a second and a half
  const rate = async () => {
    const a = await c.frame.evaluate(() => Number(wasm_get_cpu_cycles()));
    await sleep(1500);
    return (await c.frame.evaluate(() => Number(wasm_get_cpu_cycles()))) - a;
  };
  // measured against the C64's own speed (985248 cycles a second), not the
  // full-speed run: with other tests running a busy machine may not reach full speed
  const real = 985248 * 1.5;
  const full = await rate();
  // a machine too busy to run the C64 at near its speed can't show it slower (the labels are still checked)
  const busy = full < real * 0.8;
  if (busy) t.diagnostic(`the C64 ran at ${Math.round((full / real) * 100)}% here: speeds not measured`);
  const speed = () => page.$eval('.studio-dbtn.speed', (b) => b.textContent);
  assert.equal(await speed(), '1×');
  await page.click('.studio-dbtn.speed');
  assert.equal(await speed(), '0.5×');
  await sleep(300);
  const half = await rate();
  assert.ok(busy || (half / real > 0.35 && half / real < 0.62), `half speed: ${half} cycles in 1.5 s (full: ${full})`);
  await page.click('.studio-dbtn.speed');
  assert.equal(await speed(), '0.25×');
  await sleep(300);
  const quarter = await rate();
  assert.ok(busy || (quarter / real > 0.15 && quarter / real < 0.32), `quarter speed: ${quarter} cycles in 1.5 s`);
  await page.click('.studio-dbtn.speed');
  assert.equal(await speed(), '1×');
  await sleep(300);
  const again = await rate();
  assert.ok(busy || again > half * 1.25, `faster again: ${again} after ${half} at half`);
  await page.close();
});

test('a marked moment: ◉ marks it, ⟲ goes back to it with the Tuning settings as the code has them now', { skip, timeout: 120000 }, async () => {
  const page = await openStudio(browser, server.url, { keepPlace: true });
  assert.match(await run(page), /^Built in/);
  const c = await c64(page);
  await c.until(async () => /PRESS {2}FIRE/.test(await c.row(21)), 30000);
  assert.ok(await page.$eval('.studio-dbtn.again', (b) => b.disabled), 'nothing to go back to yet');
  await c.press('FIRE');
  await c.until(async () => /ROUND 01/.test(await c.row(0)));
  await sleep(2200);
  const start = await c.until(() => c.sprite(0));
  await page.click('.studio-dbtn.mark');
  await page.waitForFunction(() => !document.querySelector('.studio-dbtn.again').disabled, { timeout: 5000 });
  await c.joy('PULL_RIGHT');
  await sleep(800);
  await c.joy('RELEASE_XY');
  const moved = await c.sprite(0);
  assert.ok(moved[0] > start[0] + 10, `ran right: ${start} -> ${moved}`);
  // a heavier gravity, by the slider: in the game and in the code
  await page.$eval('.studio-tuning input[aria-label=gravity]', (i) => {
    i.value = 6;
    i.dispatchEvent(new Event('input'));
    i.dispatchEvent(new Event('change'));
  });
  const g = await page.evaluate(() => window.breadbinStudio.vars.find((x) => x.name === 'gravity'));
  await c.until(async () => (await c.peek(g.addr))[0] === 6, 3000);
  // back: the player where it stood, the gravity still 6 (the snapshot had 3)
  await page.click('.studio-dbtn.again');
  await c.until(async () => Math.abs((await c.sprite(0))[0] - start[0]) <= 2, 5000);
  await c.until(async () => (await c.peek(g.addr))[0] === 6, 3000);
  // a Run: the new build plays the tape back to the marked moment (Keep my place), and marks it again
  assert.match(await run(page), /^Built in/);
  await page.waitForFunction(() => window.breadbinStudio.placed >= 1 && !document.querySelector('.studio-dbtn.again').disabled, { timeout: 30000 });
  const there = await c.until(() => c.sprite(0), 5000);
  assert.ok(Math.abs(there[0] - start[0]) <= 2 && there[1] === start[1], `back at the mark: ${start} -> ${there}`);
  assert.deepEqual(page.errors, []);
});

test('an assembly project: live values, a Tuning knob kept in the code, breakpoints and steps, slow motion', { skip, timeout: 150000 }, async (t) => {
  const { MAIN, MOVE } = await import('../fixtures/border-asm.mjs');
  const page = await openStudio(browser, server.url);
  const st = (f, a) => page.evaluate(f, a);
  await st(async ([main, move]) => {
    const s = window.breadbinStudio;
    const store = new s.store.constructor(`asm-test-${Date.now()}`);
    await store.write({ 'project.json': '{ "name": "Border", "lang": "acme", "entry": "main.asm" }\n', 'main.asm': main, 'move.asm': move });
    await s.openStore(store, `browser:${store.id}`);
    s.openFile('move.asm');
  }, [MAIN, MOVE]);
  assert.match(await run(page), /^Built in/);
  const c = await c64(page);
  // the variables beside the code: speed (a knob, on its declaration) and pos
  await page.waitForFunction(() => [...document.querySelectorAll('.cm-live-item')].some((x) => x.textContent === 'speed3'), { timeout: 60000 });
  assert.ok(await page.evaluate(() => [...document.querySelectorAll('.cm-live-item')].filter((x) => x.textContent === 'speed3').some((x) => x.lastChild.classList.contains('keep'))), 'speed !byte 3 is its declaration');
  assert.ok(await page.evaluate(() => [...document.querySelectorAll('.cm-live-name')].some((x) => x.textContent === 'pos')), await page.evaluate(() => [...document.querySelectorAll('.cm-live-item')].map((x) => x.textContent).join(' ')));
  // the Tuning slider: in the game at once, in move.asm when let go
  assert.deepEqual(await page.$$eval('.studio-tuning-name', (e) => e.map((x) => x.textContent)), ['speed']);
  await page.$eval('.studio-tuning input[aria-label=speed]', (i) => {
    i.value = 5;
    i.dispatchEvent(new Event('input'));
    i.dispatchEvent(new Event('change'));
  });
  const speed = await st(() => window.breadbinStudio.vars.find((x) => x.name === 'speed').addr);
  await c.until(async () => (await c.peek(speed))[0] === 5, 3000);
  assert.match(await st(() => window.breadbinStudio.files['move.asm']), /^speed {3}!byte 5 /m);
  // a breakpoint on move's first line; step over, step out (back after the jsr)
  await st(() => window.breadbinStudio.toggleBreakpoint('move.asm', 1));
  const stop = async (step) => {
    if (step) await st((s) => window.breadbinStudio.dbg[s](), step);
    await page.waitForFunction(() => document.querySelector('.studio-debug.stopped') && !/…/.test(document.querySelector('.studio-dbg-where').textContent), { timeout: 30000 });
    return page.$eval('.studio-dbg-where', (e) => e.textContent);
  };
  assert.equal(await stop(), 'move move.asm:1');
  assert.equal(await stop('over'), 'move move.asm:2');
  assert.equal(await stop('out'), 'wait main.asm:12');
  await st(() => window.breadbinStudio.clearBreakpoints());
  await st(() => window.breadbinStudio.dbg.cont());
  await page.waitForFunction(() => !document.querySelector('.studio-debug.stopped'), { timeout: 10000 });
  // slow motion: the frame counter (counter, at $fb) goes half as fast
  const frames = async () => {
    const a = (await c.peek(0xfb))[0];
    await sleep(2000);
    return ((await c.peek(0xfb))[0] - a + 256) % 256;
  };
  // (against the C64's 50 frames a second, not the full-speed count: a busy machine may not reach it)
  const full = await frames();
  await page.click('.studio-dbtn.speed');
  await sleep(300);
  const half = await frames();
  if (full < 80) t.diagnostic(`the C64 ran ${full} frames in 2 s here: slow motion not measured`);
  else assert.ok(half > 35 && half < 62, `counter: ${half} in 2 s at half speed (full: ${full}), 50 expected`);
  assert.deepEqual(page.errors, []);
});

test('the player\'s Tools: the cheat finder finds the lives, freezes them, finds and patches the code; the monitor breaks and steps', { skip, timeout: 150000 }, async () => {
  const page = await openStudio(browser, server.url);
  assert.match(await run(page), /^Built in/);
  const labels = await page.evaluate(() => window.breadbinStudio.lastBuild.labels);
  const lives = labels._lives;
  const hex = (a) => a.toString(16).toUpperCase().padStart(4, '0');
  const link = await sharedLink(page, server.url);
  // Bubbler in the player, as anyone would play it, a round started
  const friend = await (await browser.createBrowserContext()).newPage();
  friend.errors = [];
  friend.on('pageerror', (e) => friend.errors.push(e.message));
  await friend.goto(link, { waitUntil: 'domcontentloaded' });
  const c = await c64(friend, '#emu-stage');
  await c.until(async () => /PRESS {2}FIRE/.test(await c.row(21)), 40000);
  await c.press('FIRE');
  await c.until(async () => /LIVES 3/.test(await c.row(0)));
  await friend.click('#emu-tools');
  await friend.waitForSelector('.emu-tools:not([hidden])');
  const button = (text) => friend.evaluateHandle((t) => [...document.querySelectorAll('.emu-tools button')].find((b) => b.textContent === t), text);
  const count = () => friend.$eval('.tools-count', (e) => e.textContent);
  // the search: it is 3; a life lost (2), another (1); it is 1
  await (await button('● Start a search')).click();
  await friend.waitForFunction(() => /candidates: play/.test(document.querySelector('.tools-count').textContent));
  await friend.type('.tools-num', '3');
  await (await button('Is')).click();
  await friend.waitForFunction(() => /after 1 step/.test(document.querySelector('.tools-count').textContent));
  for (const n of [2, 1]) {
    await c.poke([[lives, [n]]]);
    await sleep(100);
    await (await button('Went down')).click();
    await friend.waitForFunction((r) => new RegExp(`after ${r} steps`).test(document.querySelector('.tools-count').textContent), {}, n === 2 ? 2 : 3);
  }
  await friend.$eval('.tools-num', (i) => (i.value = '1'));
  await (await button('Is')).click();
  await friend.waitForFunction(() => /after 4 steps/.test(document.querySelector('.tools-count').textContent));
  const row = await friend.waitForFunction((a) => [...document.querySelectorAll('.tools-item')].find((r) => r.querySelector('code').textContent === a), { timeout: 5000 }, `$${hex(lives)}`);
  assert.ok(row, `the lives' address in: ${await count()}`);
  // freeze: the game can't change it
  await row.evaluate((r) => [...r.querySelectorAll('button')].find((b) => b.textContent === 'freeze').click());
  await friend.waitForFunction(() => /frozen at 1/.test(document.querySelector('.emu-tools').textContent));
  await c.poke([[lives, [0]]]);
  await sleep(150);
  assert.equal((await c.peek(lives))[0], 1, 'frozen at 1');
  // who writes it: Bubbler's --lives, among them; patched out, it is NOPs
  await row.evaluate((r) => [...r.querySelectorAll('button')].find((b) => b.textContent === 'who writes?').click());
  await friend.waitForFunction(() => /Code that writes/.test(document.querySelector('.emu-tools').textContent));
  const writers = await friend.$$eval('.tools-list .tools-item', (rows) => rows.map((r) => r.textContent).filter((t) => /patch out/.test(t)));
  assert.ok(writers.some((t) => t.includes(`DEC $${hex(lives)}`) || t.includes(`STA $${hex(lives)}`)), writers.join(' | '));
  const at = parseInt(writers[0].slice(1, 5), 16);
  await (await button('patch out')).click();
  await c.until(async () => (await c.peek(at, 1))[0] === 0xea, 3000);
  assert.match(await friend.$eval('.emu-tools', (e) => e.textContent), /→ NOP/);
  // the monitor: a breakpoint at bb_frame (every frame), a step, run on
  await friend.$eval('.emu-tools details:last-of-type', (d) => (d.open = true));
  await friend.type('.tools-addr[aria-label="Breakpoint address"]', hex(labels._bb_frame));
  await (await button('Set')).click();
  await friend.waitForFunction((pc) => document.querySelector('.tools-regs').textContent.startsWith(`Stopped · PC ${pc}`), { timeout: 20000 }, hex(labels._bb_frame));
  // (·· where RetroShell had not printed them yet, on a busy machine)
  assert.match(await friend.$eval('.tools-regs', (e) => e.textContent), /A ([0-9A-F]{2}|··) {2}X ([0-9A-F]{2}|··)/);
  await friend.waitForFunction((pc) => document.querySelector('.tools-mem').textContent.includes(`▸${pc}`), { timeout: 5000 }, hex(labels._bb_frame));
  await (await button('Step')).click();
  await friend.waitForFunction((pc) => /^Stopped · PC [0-9A-F]{4}/.test(document.querySelector('.tools-regs').textContent) && !document.querySelector('.tools-regs').textContent.includes(`PC ${pc}`), { timeout: 10000 }, hex(labels._bb_frame));
  await friend.click('.tools-break');
  await (await button('▶ Run')).click();
  await friend.waitForFunction(() => document.querySelector('.tools-regs').textContent === 'Running', { timeout: 10000 });
  const t = (await c.peek(labels._tick))[0];
  await sleep(300);
  assert.notEqual((await c.peek(labels._tick))[0], t, 'the game runs again');
  assert.deepEqual(friend.errors, []);
  await friend.close();
});

test('Keep my place: Run plays what you did again with the new build; ▷ replays it from the mark', { skip, timeout: 150000 }, async () => {
  const page = await openStudio(browser, server.url, { keepPlace: true });
  assert.match(await run(page), /^Built in/);
  const c = await c64(page);
  await c.until(async () => /PRESS {2}FIRE/.test(await c.row(21)), 30000);
  await c.press('FIRE');
  await c.until(async () => /ROUND 01/.test(await c.row(0)));
  await sleep(2200);
  await c.joy('PULL_RIGHT');
  await sleep(700);
  await c.joy('RELEASE_XY');
  await sleep(300);
  const here = await c.until(() => c.sprite(0)); // (the player blinks at a round's start: wait for it to show)
  // Run (the same code: a new build all the same): back in round 1, the player where it was
  assert.match(await run(page), /^Built in/);
  await page.waitForFunction(() => window.breadbinStudio.placed >= 1, { timeout: 30000 });
  assert.match(await c.row(0), /ROUND 01/, 'in the game, not at its title');
  const back = await c.until(() => c.sprite(0), 5000);
  assert.ok(Math.abs(back[0] - here[0]) <= 3 && back[1] === here[1], `where it was: ${here} -> ${back}`);
  // mark, walk left, ↺: back at the mark; ▷: the walk again, by itself
  await page.click('.studio-dbtn.mark');
  await page.waitForFunction(() => !document.querySelector('.studio-dbtn.replay').disabled, { timeout: 5000 });
  const mark = await c.until(() => c.sprite(0), 5000);
  await c.joy('PULL_LEFT');
  await sleep(600);
  await c.joy('RELEASE_XY');
  await sleep(400);
  const walked = await c.sprite(0);
  assert.ok(walked[0] < mark[0] - 10, `walked left: ${mark} -> ${walked}`);
  await page.click('.studio-dbtn.again');
  await c.until(async () => Math.abs((await c.sprite(0))[0] - mark[0]) <= 2, 5000);
  await page.click('.studio-dbtn.replay');
  await sleep(1500);
  const replayed = await c.sprite(0);
  assert.ok(Math.abs(replayed[0] - walked[0]) <= 3, `the walk again: ${walked} -> ${replayed}`);
  assert.deepEqual(page.errors, []);
});

test('the hitbox overlay: ▢ draws the sprites\' boxes over the C64; the frame meter', { skip, timeout: 120000 }, async () => {
  const page = await openStudio(browser, server.url);
  assert.match(await run(page), /^Built in/);
  const c = await c64(page);
  await c.until(async () => /PRESS {2}FIRE/.test(await c.row(21)), 30000);
  await page.click('.studio-boxbtn');
  assert.equal(await page.$eval('.studio-boxbtn', (b) => b.getAttribute('aria-pressed')), 'true');
  // the title's sprite: a box drawn (some pixels of the canvas painted), the canvas over the frame
  await page.waitForFunction(() => {
    const cv = document.querySelector('.studio-hitbox');
    if (!cv || cv.hidden || !cv.width) return false;
    const d = cv.getContext('2d').getImageData(0, 0, cv.width, cv.height).data;
    for (let i = 3; i < d.length; i += 4) if (d[i]) return true;
    return false;
  }, { timeout: 10000 });
  const [cv, fr] = await page.evaluate(() => [document.querySelector('.studio-hitbox'), document.querySelector('.studio-stage iframe')].map((e) => {
    const r = e.getBoundingClientRect();
    return [Math.round(r.x), Math.round(r.y), Math.round(r.width)];
  }));
  assert.deepEqual(cv, fr, 'the overlay lies on the C64\'s picture');
  await page.click('.studio-boxbtn');
  assert.ok(await page.$eval('.studio-hitbox', (e) => e.hidden));
  // the frame meter: Bubbler's title takes some of a frame, well under all of it
  await page.waitForFunction(() => /^\d+% of a frame/.test(document.querySelector('.studio-cpu:not([hidden]) .studio-meter-text')?.textContent || ''), { timeout: 10000 });
  const pct = Number((await page.$eval('.studio-cpu .studio-meter-text', (e) => e.textContent)).match(/^(\d+)%/)[1]);
  assert.ok(pct > 0 && pct < 90, `the title uses ${pct}% of a frame`);
  assert.deepEqual(page.errors, []);
});

test('live editing: a sprite\'s colour and a tile\'s pixels go into the running game, no Run', { skip, timeout: 120000 }, async () => {
  const page = await openStudio(browser, server.url);
  assert.match(await run(page), /^Built in/);
  const c = await c64(page);
  await c.until(async () => /PRESS {2}FIRE/.test(await c.row(21)), 30000);
  const labels = await page.evaluate(() => window.breadbinStudio.lastBuild.labels);
  const charset = await c.peek(0xc800 + 64 * 8, 8);
  // every sprite red, and tile 64 (a wall) upside down, as the editors would commit them
  await page.evaluate(() => {
    const s = window.breadbinStudio;
    const sheet = JSON.parse(s.files['assets/sprites.json']);
    for (const sp of sheet.sprites) sp.color = 2;
    s.commit('sprites', sheet);
    const set = JSON.parse(s.files['assets/tiles.json']);
    set.tiles[64].rows.reverse();
    s.commit('tiles', set);
  });
  await c.until(async () => (await c.peek(0xd027))[0] % 16 === 2, 5000);
  assert.equal((await c.peek(labels._bb_sprite_color))[0], 2);
  await c.until(async () => (await c.peek(0xc800 + 64 * 8, 8)).join() === [...charset].reverse().join(), 5000);
  assert.deepEqual(page.errors, []);
});

test('the Studio\'s 🔧 Tools: the monitor shows the program\'s memory; a search starts', { skip, timeout: 120000 }, async () => {
  const page = await openStudio(browser, server.url);
  assert.match(await run(page), /^Built in/);
  const c = await c64(page);
  await c.until(async () => /PRESS {2}FIRE/.test(await c.row(21)), 30000);
  await page.click('.studio-toolsbtn');
  await page.waitForSelector('.studio-side .emu-tools:not([hidden])');
  // the monitor at $0801: the BASIC line that starts the program (SYS 2061)
  await page.$eval('.studio-side .emu-tools details:last-of-type', (d) => (d.open = true));
  await page.waitForFunction(() => /^0801 {2}0B 08/.test(document.querySelector('.studio-side .tools-mem').textContent), { timeout: 5000 });
  const button = (t) => page.evaluateHandle((t) => [...document.querySelectorAll('.studio-side .emu-tools button')].find((b) => b.textContent === t), t);
  await (await button('● Start a search')).click();
  await page.waitForFunction(() => /candidates: play/.test(document.querySelector('.studio-side .tools-count').textContent), { timeout: 10000 });
  assert.deepEqual(page.errors, []);
});
