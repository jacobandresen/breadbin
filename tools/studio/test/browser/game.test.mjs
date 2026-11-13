// The example games, built in the Studio and played on vc64web (the C64
// emulator the page uses, on the internet: these tests skip when it can't be
// reached). A robot reads the C64's screen and sprites and works the joystick.
import test from 'node:test';
import assert from 'node:assert/strict';
import { serve, launch, openStudio, run, c64, online, sleep } from './harness.mjs';

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
  await page.evaluate(() => (navigator.clipboard.writeText = async (t) => (window.__copied = t)));
  await page.click('.studio-share');
  const link = await page.waitForFunction(() => window.__copied, { timeout: 20000 }).then((h) => h.jsonValue());
  const friend = await (await browser.createBrowserContext()).newPage();
  await friend.goto(link.replace(/^https?:\/\/[^/]+/, new URL(server.url).origin), { waitUntil: 'domcontentloaded' });
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
  await page.waitForSelector('.studio-deck:not([hidden]) #fire-buttons .arcade-btn');
  const fire = await page.$eval('.studio-deck #fire-buttons .arcade-btn', (e) => { const r = e.getBoundingClientRect(); return { x: r.x + r.width / 2, y: r.y + r.height / 2 }; });
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
