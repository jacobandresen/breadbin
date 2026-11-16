// The Studio in a real browser (headless Chromium): the code editor, the
// asset editors, saving to a folder and to a zip, and building. The games
// themselves run in game.test.mjs (they need vc64web, on the internet).
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { serve, launch, openStudio, run, setCode, sleep, online, sharedLink } from './harness.mjs';

// (the tests that run the C64 need vc64web, on the internet)
const offline = !(await online()) && 'vc64web is not reachable';
const server = await serve();
const browser = await launch();
test.after(async () => {
  await browser.close();
  server.close();
});

test('opens on Bubbler, with C in the retrobox colours', async () => {
  const page = await openStudio(browser, server.url);
  assert.equal(await page.$eval('.studio-project-name', (e) => e.textContent), 'Bubbler');
  assert.equal(await page.$eval('.studio-crumb', (e) => e.textContent), 'src/main.c');
  // retrobox: Normal #1c1c1c on #ebdbb2, keywords #fb5944, types #fabd2f, comments #928374
  await page.evaluate(() => window.breadbinStudio.code.open('src/colours.c', '/* colours */\nint main(void) {\n  if (1) return 0;\n}\n'));
  const colours = await page.evaluate(() => {
    const css = (el) => getComputedStyle(el).color;
    const spans = [...document.querySelectorAll('.cm-line span')];
    const find = (text) => spans.find((s) => s.textContent === text);
    return {
      bg: getComputedStyle(document.querySelector('.cm-editor')).backgroundColor,
      keyword: css(find('return') || find('for') || find('if')),
      type: css(find('unsigned') || find('int')),
      comment: css(spans.find((s) => s.textContent.startsWith('/*'))),
    };
  });
  assert.equal(colours.bg, 'rgb(28, 28, 28)');
  assert.equal(colours.keyword, 'rgb(251, 89, 68)');
  assert.equal(colours.type, 'rgb(250, 189, 47)');
  assert.equal(colours.comment, 'rgb(146, 131, 116)');
  assert.deepEqual(page.errors, []);
});

test('the first visit says what to try; once is enough', async () => {
  const page = await openStudio(browser, server.url);
  await page.waitForSelector('.studio-banner:not([hidden])');
  assert.match(await page.$eval('.studio-banner', (e) => e.textContent), /Welcome to the Studio/);
  await page.click('.studio-banner .studio-tool'); // got it
  assert.ok(await page.$eval('.studio-banner', (e) => e.hidden));
  await page.reload({ waitUntil: 'domcontentloaded' });
  await page.waitForFunction(() => window.breadbinStudio?.files?.['project.json']);
  await sleep(300);
  assert.ok(await page.$eval('.studio-banner', (e) => e.hidden), 'not again');
});

test('the layout: drag the handles, sizes kept; code size; the file list hides; the C64 full screen', async () => {
  const page = await openStudio(browser, server.url, { width: 1400, height: 860 });
  const width = (sel) => page.$eval(sel, (e) => Math.round(e.getBoundingClientRect().width));
  const drag = async (sel, dx) => {
    const b = await page.$eval(sel, (e) => { const r = e.getBoundingClientRect(); return { x: r.x + r.width / 2, y: r.y + r.height / 2 }; });
    await page.mouse.move(b.x, b.y);
    await page.mouse.down();
    await page.mouse.move(b.x + dx, b.y, { steps: 6 });
    await page.mouse.up();
  };
  const side = await width('.studio-side');
  await drag('.studio-main > .studio-split', -150);
  assert.ok(Math.abs((await width('.studio-side')) - (side + 150)) <= 3, 'the C64 panel is 150 wider');
  const files = await width('.studio-files');
  await drag('.studio-pane-code > .studio-split', 60);
  assert.ok(Math.abs((await width('.studio-files')) - (files + 60)) <= 3, 'the file list is 60 wider');
  // bigger code, and no file list
  const font = () => page.$eval('.cm-scroller', (e) => getComputedStyle(e).fontSize);
  await page.keyboard.down('Control');
  await page.keyboard.press('=');
  await page.keyboard.press('=');
  await page.keyboard.up('Control');
  assert.equal(await font(), '15.5px');
  await page.click('.studio-codetools button[title^="Hide or show"]');
  assert.ok((await width('.studio-files')) <= 1, 'the file list is gone (its 1px border aside)');
  // all of it the same after a reload
  await page.reload({ waitUntil: 'domcontentloaded' });
  await page.waitForSelector('.cm-content');
  assert.ok(Math.abs((await width('.studio-side')) - (side + 150)) <= 3);
  assert.equal(await font(), '15.5px');
  assert.ok((await width('.studio-files')) <= 1, 'the file list is gone (its 1px border aside)');
  // a double-click puts a handle back
  await page.$eval('.studio-main > .studio-split', (e) => e.dispatchEvent(new MouseEvent('dblclick', { bubbles: true })));
  assert.ok(Math.abs((await width('.studio-side')) - side) <= 3);
  // the C64's screen full screen, and back
  await page.click('.studio-screen .studio-fullbtn');
  await page.waitForFunction(() => document.fullscreenElement?.classList.contains('studio-screen'));
  await page.evaluate(() => document.exitFullscreen());
  assert.deepEqual(page.errors, []);
});

test('breakpoints: a click in the margin sets one (on the next line with code after a build), they move with the code and are kept', async () => {
  const page = await openStudio(browser, server.url);
  await page.evaluate(() => window.breadbinStudio.openFile('src/main.c'));
  const marks = () => page.$$eval('.cm-bp-gutter .cm-bp-bp', (els) => els.length);
  const lines = () => page.evaluate(() => [...(window.breadbinStudio.breaks.get('src/main.c') || [])]);
  // a click on line 3's margin
  const y = await page.evaluate(() => {
    const v = window.breadbinStudio.code.view;
    const r = v.coordsAtPos(v.state.doc.line(3).from);
    return (r.top + r.bottom) / 2;
  });
  const x = await page.$eval('.cm-bp-gutter', (e) => { const r = e.getBoundingClientRect(); return r.x + r.width / 2; });
  await page.mouse.click(x, y);
  assert.deepEqual(await lines(), [3]);
  assert.equal(await marks(), 1);
  // a line above it: the breakpoint goes down with its line
  await page.evaluate(() => window.breadbinStudio.code.view.dispatch({ changes: { from: 0, insert: '\n' } }));
  assert.deepEqual(await lines(), [4]);
  await page.evaluate(() => window.breadbinStudio.code.view.dispatch({ changes: { from: 0, to: 1 } }));
  assert.deepEqual(await lines(), [3]);
  // after a build, a line without code (a blank one) gives the next line that has some
  await page.evaluate(() => window.breadbinStudio.build());
  await page.evaluate(() => window.breadbinStudio.toggleBreakpoint('src/main.c', 3));
  const blank = await page.evaluate(() => {
    const st = window.breadbinStudio;
    const text = st.files['src/main.c'].split('\n');
    return text.findIndex((l, i) => l === '' && i > 0 && !st.lineInfo.addrs('src/main.c', i + 1).length && st.lineInfo.addrs('src/main.c', i + 2).length) + 1;
  });
  assert.ok(blank > 0);
  await page.evaluate((l) => window.breadbinStudio.toggleBreakpoint('src/main.c', l), blank);
  assert.deepEqual(await lines(), [blank + 1], 'the next line with code');
  // kept after a reload
  await page.evaluate(() => window.breadbinStudio.saveNow());
  await page.reload({ waitUntil: 'domcontentloaded' });
  await page.waitForFunction(() => window.breadbinStudio?.files?.['project.json']);
  await page.evaluate(() => window.breadbinStudio.openFile('src/main.c'));
  assert.deepEqual(await lines(), [blank + 1]);
  await page.waitForSelector('.cm-bp-gutter .cm-bp-bp');
  assert.deepEqual(page.errors, []);
});

test('a compile error shows in the problems list and the editor; clicking it goes to the line', async () => {
  const page = await openStudio(browser, server.url);
  await page.evaluate(() => window.breadbinStudio.openFile('src/main.c'));
  const text = await page.evaluate(() => window.breadbinStudio.files['src/main.c']);
  await setCode(page, text.replace('  bb_init();', '  bb_init();\n  oops = 1;'));
  assert.match(await run(page), /Didn't build: 1 error/);
  const problem = await page.$eval('.studio-problem.error', (e) => e.textContent);
  assert.match(problem, /main\.c:\d+.*Undeclared identifier 'oops'/);
  await page.waitForSelector('.cm-lintRange-error');
  // the editor went to the error's line
  const line = await page.evaluate(() => {
    const s = window.breadbinStudio.code.view.state;
    return s.doc.lineAt(s.selection.main.head).text;
  });
  assert.equal(line.trim(), 'oops = 1;');
  // fixed, it builds, and the memory meter fills in
  await setCode(page, text);
  assert.match(await run(page), /^Built in \d+ ms · no problems/);
  assert.match(await page.$eval('.studio-meter', (e) => e.textContent), /KB of 46\.0 KB used/);
  // gen/ was written, and the editor completes the asset names
  assert.ok(await page.evaluate(() => 'gen/assets.h' in window.breadbinStudio.files));
  assert.ok(await page.evaluate(() => window.breadbinStudio.code.assets.some((a) => a.name === 'SPR_BUB_STAND')));
});

test('completion offers the runtime and the assets, with their docs', async () => {
  const page = await openStudio(browser, server.url);
  await page.evaluate(() => {
    const s = window.breadbinStudio;
    s.openFile('src/main.c');
    const v = s.code.view;
    v.dispatch({ selection: { anchor: v.state.doc.length } });
    v.focus();
  });
  await page.keyboard.type('\nbb_sp');
  await page.waitForSelector('.cm-tooltip-autocomplete');
  const options = await page.$$eval('.cm-tooltip-autocomplete li', (els) => els.map((e) => e.textContent));
  assert.ok(options.some((o) => o.startsWith('bb_spr')), options.join(', '));
  await page.waitForSelector('.cm-completionInfo');
  assert.match(await page.$eval('.cm-completionInfo', (e) => e.textContent), /bb_spr|sprite/i);
});

test('F12 on a name goes to its definition, in another file', async () => {
  const page = await openStudio(browser, server.url);
  await page.evaluate(() => {
    const s = window.breadbinStudio;
    s.openFile('src/main.c');
    const v = s.code.view;
    const at = v.state.doc.toString().indexOf('player_update();');
    v.dispatch({ selection: { anchor: at + 3 } });
    v.focus();
  });
  await page.keyboard.press('F12');
  // (the cursor moves to the line a moment after the file opens)
  await page.waitForFunction(() => {
    const s = window.breadbinStudio;
    const st = s.code.view.state;
    return s.path === 'src/player.c' && st.doc.lineAt(st.selection.main.head).text === 'void player_update(void) {';
  }, { timeout: 5000 });
});

test('completion knows cc65\'s library, from the headers the file includes', async () => {
  const page = await openStudio(browser, server.url);
  await page.evaluate(() => {
    const s = window.breadbinStudio;
    s.openFile('src/main.c');
    const v = s.code.view;
    v.dispatch({ changes: { from: 0, insert: '#include <string.h>\n' } });
  });
  await page.waitForFunction(() => window.breadbinStudio.code.library.some((e) => e.name === 'memcpy'), { timeout: 15000 });
  await page.evaluate(() => {
    const v = window.breadbinStudio.code.view;
    v.dispatch({ selection: { anchor: v.state.doc.length } });
    v.focus();
  });
  await page.keyboard.type('\nmemc');
  await page.waitForSelector('.cm-tooltip-autocomplete');
  const options = await page.$$eval('.cm-tooltip-autocomplete li', (els) => els.map((e) => e.textContent));
  assert.ok(options.some((o) => /^memcpy.*<string\.h>/.test(o)), options.join(' | '));
});

test('SPRITES: painting a pixel changes sprites.json; undo puts it back', async () => {
  const page = await openStudio(browser, server.url);
  await page.click('.studio-mode[data-mode=sprites]');
  await page.waitForSelector('.studio-sprites .studio-paint');
  const before = await page.evaluate(() => window.breadbinStudio.files['assets/sprites.json']);
  const box = await page.$eval('.studio-sprites .studio-paint', (c) => { const r = c.getBoundingClientRect(); return { x: r.x, y: r.y, w: r.width, h: r.height }; });
  // the top-left pixel: see-through in bub stand; paint it with the sprite's colour (slot 2)
  await page.keyboard.press('2');
  await page.mouse.click(box.x + 3, box.y + 3);
  await sleep(100);
  const after = JSON.parse(await page.evaluate(() => window.breadbinStudio.files['assets/sprites.json']));
  assert.equal(after.sprites[0].rows[0][0], '1');
  assert.notEqual(JSON.stringify(after), JSON.stringify(JSON.parse(before)));
  await page.focus('.studio-sprites');
  await page.keyboard.down('Control');
  await page.keyboard.press('z');
  await page.keyboard.up('Control');
  await sleep(100);
  assert.equal(JSON.parse(await page.evaluate(() => window.breadbinStudio.files['assets/sprites.json'])).sprites[0].rows[0][0], '.');
  // O: the onion skin shows the sprite before this one underneath
  await page.keyboard.press(']');
  await page.keyboard.press('o');
  assert.deepEqual(await page.evaluate(() => window.breadbinStudio.editors.sprites.canvas.under?.rows), JSON.parse(before).sprites[0].rows);
});

test('SPRITES: export the sheet as a PNG, import it back: the same pixels', async () => {
  const page = await openStudio(browser, server.url);
  await page.click('.studio-mode[data-mode=sprites]');
  await page.waitForSelector('.studio-sprites .studio-paint');
  // the export, as the ⇩ PNG button makes it
  const png = await page.evaluate(async () => {
    const { sheetPng } = await import('./studio/pixels.js');
    const ed = window.breadbinStudio.editors.sprites;
    const blob = await sheetPng(ed.sheet.sprites.map((s) => ({ rows: s.rows, multi: s.multi, colors: { ...ed.colors(s), bg: null } })), { w: 24, h: 21, perRow: 8 });
    return [...new Uint8Array(await blob.arrayBuffer())];
  });
  const file = path.join(os.tmpdir(), `studio-sprites-${process.pid}.png`);
  fs.writeFileSync(file, Buffer.from(png));
  const before = JSON.parse(await page.evaluate(() => window.breadbinStudio.files['assets/sprites.json'])).sprites;
  const [chooser] = await Promise.all([page.waitForFileChooser(), page.click('.studio-sprites .studio-listtools button[title^="Sprites from a picture"]')]);
  await chooser.accept([file]);
  await page.waitForFunction((n) => JSON.parse(window.breadbinStudio.files['assets/sprites.json']).sprites.length === 2 * n, {}, before.length);
  const after = JSON.parse(await page.evaluate(() => window.breadbinStudio.files['assets/sprites.json'])).sprites;
  const imported = after.slice(1, 1 + before.length); // in after the selected (first) one
  for (let i = 0; i < before.length; i++) assert.deepEqual(imported[i].rows, before[i].rows, `${before[i].name} came back the same`);
  fs.rmSync(file);
});

test('TILES: ticking a flag and renaming it changes tiles.json and the constant', async () => {
  const page = await openStudio(browser, server.url);
  await page.click('.studio-mode[data-mode=tiles]');
  await page.waitForSelector('.studio-flags input[type=checkbox]');
  // tile 64 (wall a) is solid and wall; tick flag 2 too
  await page.$$eval('.studio-flags input[type=checkbox]', (els) => els[2].click());
  const set = JSON.parse(await page.evaluate(() => window.breadbinStudio.files['assets/tiles.json']));
  assert.equal(set.tiles[64].flags, 7);
  await page.$$eval('.studio-flagname', (els) => {
    els[2].value = 'slippery';
    els[2].dispatchEvent(new Event('change'));
  });
  await run(page);
  assert.match(await page.evaluate(() => window.breadbinStudio.files['gen/assets.h']), /#define FLAG_SLIPPERY 0x04/);
});

test('MAP: painting a cell changes that room\'s row in map.json', async () => {
  const page = await openStudio(browser, server.url);
  await page.click('.studio-mode[data-mode=map]');
  await page.waitForSelector('.studio-room');
  await page.$$eval('.studio-roomitem', (els) => els[1].click()); // round 1
  await page.$$eval('.studio-paltile', (els) => els[1].click()); // block a
  // the screen inside the canvas's border
  const box = await page.$eval('.studio-room', (c) => { const r = c.getBoundingClientRect(); return { x: r.x + c.clientLeft, y: r.y + c.clientTop, w: c.clientWidth, h: c.clientHeight }; });
  // cell 20, 5: empty in round 1
  await page.mouse.click(box.x + (20.5 / 40) * box.w, box.y + (5.5 / 25) * box.h);
  await sleep(100);
  const map = JSON.parse(await page.evaluate(() => window.breadbinStudio.files['assets/map.json']));
  assert.equal(map.legend[map.rooms[1].rows[5][20]], 65);
});

test('MUSIC: the keyboard piano writes notes into the pattern, and Space plays it', async () => {
  const page = await openStudio(browser, server.url);
  await page.click('.studio-mode[data-mode=music]');
  await page.waitForSelector('.studio-tracker');
  await page.click('.studio-tracker .studio-cell.note');
  await page.keyboard.press('Delete'); // step 0 empty, the cursor on step 1
  await page.keyboard.press('ArrowUp');
  await page.keyboard.press('q'); // C, an octave up from the octave shown (4): C-5
  await sleep(100);
  const music = JSON.parse(await page.evaluate(() => window.breadbinStudio.files['assets/music.json']));
  assert.equal(music.patterns[0].steps[0], 'C-5 0 ...');
  await page.focus('.studio-tracker');
  await page.keyboard.press(' ');
  await page.waitForFunction(() => window.breadbinStudio.sid.playing, { timeout: 20000 });
  await page.keyboard.press(' ');
  await page.waitForFunction(() => !window.breadbinStudio.sid.playing);
  // a quick double press stops it too, even before the sound has started
  await page.keyboard.press(' ');
  await page.keyboard.press(' ');
  await sleep(1500);
  assert.equal(await page.evaluate(() => window.breadbinStudio.sid.playing), false);
});

test('MUSIC: ⇩ .sid saves the songs as a PSID file', async () => {
  const page = await openStudio(browser, server.url);
  await page.click('.studio-mode[data-mode=music]');
  await page.waitForSelector('.studio-sidout');
  const file = await page.evaluate(async () => {
    const got = new Promise((resolve) => {
      const orig = URL.createObjectURL;
      URL.createObjectURL = (b) => (b.arrayBuffer().then((a) => resolve([...new Uint8Array(a)])), orig(b));
    });
    document.querySelector('.studio-sidout').click();
    return got;
  });
  const text = (a, n) => String.fromCharCode(...file.slice(a, a + n)).replace(/\0.*$/, '');
  assert.equal(text(0, 4), 'PSID');
  assert.equal(text(0x16, 32), 'Bubbler');
  assert.equal((file[14] << 8) | file[15], 1, 'one song');
  assert.ok(file.length > 0x7c + 1000);
});

test('save to a folder: the files go to disk, edits follow, and changes made there come back', async () => {
  const page = await openStudio(browser, server.url);
  await page.evaluate(() => (window.__pickName = 'bubbler-on-disk'));
  page.on('dialog', (d) => d.accept());
  await page.evaluate(() => window.breadbinStudio.saveToFolder());
  await page.waitForFunction(() => window.breadbinStudio.store?.kind === 'folder');
  assert.match(await page.$eval('.studio-saved', (e) => e.textContent), /bubbler-on-disk\//);
  const onDisk = (path) => page.evaluate(async (p) => {
    let dir = await (await navigator.storage.getDirectory()).getDirectoryHandle('bubbler-on-disk');
    const parts = p.split('/');
    for (const d of parts.slice(0, -1)) dir = await dir.getDirectoryHandle(d);
    return (await (await dir.getFileHandle(parts.at(-1))).getFile()).text();
  }, path);
  assert.match(await onDisk('src/main.c'), /Bubbler/);
  assert.match(await onDisk('Makefile'), /cl65/);
  // an edit in the Studio is saved to the folder
  await page.evaluate(() => window.breadbinStudio.openFile('src/main.c'));
  await page.evaluate(() => {
    const v = window.breadbinStudio.code.view;
    v.dispatch({ changes: { from: 0, insert: '/* edited in the Studio */\n' } });
  });
  await page.evaluate(() => window.breadbinStudio.saveNow());
  assert.match(await onDisk('src/main.c'), /^\/\* edited in the Studio \*\//);
  // another program changes a file: the Studio takes it in
  await page.evaluate(async () => {
    const dir = await (await (await navigator.storage.getDirectory()).getDirectoryHandle('bubbler-on-disk')).getDirectoryHandle('src');
    const w = await (await dir.getFileHandle('extra.c', { create: true })).createWritable();
    await w.write('/* from my own editor */\n');
    await w.close();
  });
  await page.waitForFunction(() => window.breadbinStudio.files['src/extra.c']?.includes('own editor'), { timeout: 10000 });
  // and after a reload, the folder is the project again
  await page.reload({ waitUntil: 'domcontentloaded' });
  await page.waitForFunction(() => window.breadbinStudio?.store?.kind === 'folder', { timeout: 30000 });
  assert.ok(await page.evaluate(() => 'src/extra.c' in window.breadbinStudio.files));
  // saved to the same folder again: still remembered once
  await page.evaluate(() => (window.__pickName = 'bubbler-on-disk'));
  await page.evaluate(() => window.breadbinStudio.saveToFolder());
  await page.waitForFunction(() => window.breadbinStudio.store?.kind === 'folder');
  const remembered = await page.evaluate(async () => (await (await import('./studio/storage.js')).rememberedFolders()).map((f) => f.handle.name));
  assert.deepEqual(remembered, ['bubbler-on-disk']);
  // another project now; the projects menu offers the folder to go back to
  await page.evaluate(() => window.breadbinStudio.openExample('blank'));
  await page.waitForFunction(() => window.breadbinStudio.store?.kind === 'browser');
  await page.click('.studio-project');
  await page.waitForSelector('.studio-picker[open]');
  const folderButton = await page.waitForSelector('.studio-own-open[title*="bubbler-on-disk"]');
  await folderButton.click();
  await page.waitForFunction(() => window.breadbinStudio.store?.kind === 'folder' && window.breadbinStudio.meta.name === 'Bubbler');
});

test('Share: a link with the game in it starts it in breadbin\'s player', async () => {
  const page = await openStudio(browser, server.url);
  assert.match(await run(page), /^Built in/);
  // the link goes to the clipboard (caught here)
  const link = await sharedLink(page, server.url);
  // readable up to the data: #play/<Title>/<data>
  assert.match(link, /#play\/Bubbler\/[A-Za-z0-9_-]+$/);
  // a friend opens it, in another browser profile: the player starts with the game
  const friend = await (await browser.createBrowserContext()).newPage();
  await friend.goto(link, { waitUntil: 'domcontentloaded' });
  await friend.waitForFunction(() => !document.querySelector('#emu').hidden, { timeout: 30000 });
  assert.equal(await friend.$eval('#emu-title', (e) => e.textContent), 'Bubbler');
  await friend.waitForSelector('#emu-stage iframe', { timeout: 30000 });
  // a link of the old form (#games/prg:<gzip of title and PRG>/play) still opens
  const old = await friend.evaluate(async () => {
    const prg = new Uint8Array([1, 8, 0x60]);
    const title = new TextEncoder().encode('Old Game');
    const raw = new Uint8Array([title.length, ...title, ...prg]);
    const gz = new Uint8Array(await new Response(new Blob([raw]).stream().pipeThrough(new CompressionStream('gzip'))).arrayBuffer());
    return `prg:${btoa(String.fromCharCode(...gz)).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '')}`;
  });
  await friend.evaluate(() => document.querySelector('#emu-close').click());
  await friend.evaluate((id) => (location.hash = `#games/${encodeURIComponent(id)}/play`), old);
  await friend.waitForFunction(() => document.querySelector('#emu-title').textContent === 'OLD GAME' || document.querySelector('#emu-title').textContent === 'Old Game', { timeout: 30000 });
  await friend.close();
});

test('without folder access (Firefox, Safari): zips instead, said so, nothing broken', async () => {
  const page = await openStudio(browser, server.url, { folder: false });
  assert.equal(await page.evaluate(() => typeof window.showDirectoryPicker), 'undefined');
  // the welcome offers no folder
  assert.doesNotMatch(await page.$eval('.studio-banner', (e) => e.textContent), /folder/i);
  await page.click('.studio-project');
  await page.waitForSelector('.studio-picker[open]');
  const picker = await page.$eval('.studio-picker', (e) => e.textContent);
  assert.doesNotMatch(picker, /Save to a folder/);
  assert.match(picker, /can't reach folders on your disk/);
  assert.ok(await page.$eval('.studio-picker button[title="Needs Chrome, Edge or Opera"]', (b) => b.disabled), 'Open a folder is off');
  // Export .zip downloads
  const blobs = await page.evaluate(() => {
    const made = [];
    const orig = URL.createObjectURL;
    URL.createObjectURL = (b) => (made.push(b.size), orig(b));
    [...document.querySelectorAll('.studio-picker button')].find((b) => /Export \.zip/.test(b.textContent)).click();
    return made;
  });
  assert.equal(blobs.length, 1);
  assert.ok(blobs[0] > 20000, `a zip of ${blobs[0]} bytes`);
  assert.deepEqual(page.errors, []);
});

test('a plain cc65 project from disk: opens, builds, and can start the assets', async () => {
  const page = await openStudio(browser, server.url);
  // a folder with a C file in it, and no project.json
  await page.evaluate(async () => {
    const dir = await (await navigator.storage.getDirectory()).getDirectoryHandle('hello', { create: true });
    const w = await (await dir.getFileHandle('hello.c', { create: true })).createWritable();
    await w.write('#include <stdio.h>\n\nint main(void) {\n  puts("HELLO");\n  return 0;\n}\n');
    await w.close();
    window.__pickName = 'hello';
  });
  await page.evaluate(() => window.breadbinStudio.openFolder());
  await page.waitForFunction(() => window.breadbinStudio.meta.name === 'hello' && window.breadbinStudio.path === 'hello.c');
  assert.match(await run(page), /^Built in/);
  assert.ok(!(await page.evaluate(() => 'gen/bb.h' in window.breadbinStudio.files)), 'no runtime for plain cc65');
  // the sprite editor offers to start the assets
  await page.click('.studio-mode[data-mode=sprites]');
  await page.waitForSelector('.studio-noassets button');
  await page.click('.studio-noassets button');
  await page.waitForSelector('.studio-sprites:not([hidden]) .studio-paint');
  assert.ok(await page.evaluate(() => ['assets/sprites.json', 'assets/tiles.json', 'assets/map.json', 'assets/music.json'].every((p) => p in window.breadbinStudio.files)));
  assert.match(await run(page), /^Built in/);
  assert.ok(await page.evaluate(() => 'gen/bb.h' in window.breadbinStudio.files), 'now with the runtime');
});

test('export and import a zip', async () => {
  const page = await openStudio(browser, server.url);
  const bytes = await page.evaluate(async () => {
    const { zip } = await import('./studio/zip.js');
    return [...zip(window.breadbinStudio.files)];
  });
  await page.evaluate(async (b) => {
    const files = new File([new Uint8Array(b)], 'bubbler.zip');
    // a renamed copy, so it is a project of its own
    const { unzip, zip } = await import('./studio/zip.js');
    const f = await unzip(new Uint8Array(await files.arrayBuffer()));
    f['project.json'] = new TextEncoder().encode(JSON.stringify({ name: 'Imported', lang: 'c', entry: 'src/main.c' }));
    await window.breadbinStudio.importZip(new File([zip(f)], 'imported.zip'));
  }, bytes);
  await page.waitForFunction(() => window.breadbinStudio.meta.name === 'Imported');
  assert.ok(await page.evaluate(() => Object.keys(window.breadbinStudio.files).length > 10));
});

test('the projects menu and the cheat sheet open', async () => {
  const page = await openStudio(browser, server.url);
  await page.click('.studio-project');
  await page.waitForSelector('.studio-picker[open]');
  const cards = await page.$$eval('.studio-picker .studio-card b', (els) => els.map((e) => e.textContent));
  assert.deepEqual(cards.slice(0, 4), ['Bubbler', 'Bouncer', 'Pixels', 'Empty game']);
  await page.keyboard.press('Escape');
  await page.keyboard.press('F1');
  await page.waitForSelector('.studio-help[open]');
  assert.match(await page.$eval('.studio-help', (e) => e.textContent), /bb_spr\(unsigned char n/);
});

test('every example builds in the browser\'s worker', async () => {
  const page = await openStudio(browser, server.url);
  for (const id of ['bouncer', 'blank', 'bubbler']) {
    await page.evaluate((id) => window.breadbinStudio.openExample(id), id);
    assert.match(await run(page), /^Built in/, id);
    await page.evaluate(() => window.breadbinStudio.stop());
  }
  assert.deepEqual(page.errors, []);
});

test('Share & export: the web page is one file that plays the game without the Studio', async () => {
  const page = await openStudio(browser, server.url);
  await page.evaluate(() => (window.breadbinStudio.download = (name, bytes) => (window.__dl = { name, text: new TextDecoder().decode(bytes) })));
  await page.click('.studio-share');
  await page.waitForSelector('.studio-export[open] .studio-card');
  const cards = await page.$$eval('.studio-export .studio-card b', (b) => b.map((x) => x.textContent));
  assert.deepEqual(cards, ['A link', 'Web page', 'C64 program']);
  await (await page.$$('.studio-export .studio-card'))[1].click();
  const dl = await (await page.waitForFunction(() => window.__dl, { timeout: 30000 })).jsonValue();
  assert.equal(dl.name, 'bubbler.html');
  // opened on its own, away from the Studio: the game's title screen
  const p2 = await (await browser.createBrowserContext()).newPage();
  const errors = [];
  p2.on('pageerror', (e) => errors.push(e.message));
  await p2.setContent(dl.text);
  assert.equal(await p2.title(), 'Bubbler');
  await p2.click('.start');
  // the title's yellow-green BUBBLER (colour 7) is on the canvas
  await p2.waitForFunction(() => {
    const d = document.querySelector('canvas').getContext('2d').getImageData(0, 0, 384, 272).data;
    for (let i = 0; i < d.length; i += 4) if (d[i] === 0xb8 && d[i + 1] === 0xc7 && d[i + 2] === 0x6f) return true;
    return false;
  }, { timeout: 15000 });
  assert.deepEqual(errors, []);
  await p2.close();
});

test('a lesson: the jump made a setting, run, tuned higher, step by step', { skip: offline, timeout: 120000 }, async () => {
  const page = await openStudio(browser, server.url);
  await page.evaluate(() => window.breadbinStudio.startLesson('jump'));
  await page.waitForSelector('.studio-lesson:not([hidden])');
  const n = () => page.$eval('.studio-lesson-n', (e) => e.textContent);
  const next = async () => {
    await page.waitForSelector('.studio-lesson.done .studio-lesson-next:not([disabled])', { timeout: 60000 });
    await page.click('.studio-lesson-next');
  };
  assert.equal(await n(), '1 / 3');
  await page.evaluate(() => [...document.querySelectorAll('.studio-lesson button')].find((b) => b.textContent === 'Show me').click());
  await next();
  assert.equal(await n(), '2 / 3');
  assert.match(await run(page), /^Built in/);
  await next();
  assert.equal(await n(), '3 / 3');
  await page.waitForSelector('.studio-tuning input[aria-label=jump]', { timeout: 60000 });
  await page.$eval('.studio-tuning input[aria-label=jump]', (i) => {
    i.value = -10;
    i.dispatchEvent(new Event('input'));
    i.dispatchEvent(new Event('change'));
  });
  await next();
  await page.waitForFunction(() => /That's the lesson/.test(document.querySelector('.studio-lesson').textContent));
  assert.match(await page.evaluate(() => window.breadbinStudio.files['src/main.c']), /^signed char jump = -10;/m);
  assert.deepEqual(page.errors, []);
});

test('History: each Run keeps a version; restore puts the project back as it was', { timeout: 120000 }, async () => {
  const page = await openStudio(browser, server.url);
  await page.evaluate(() => window.breadbinStudio.openExample('blank'));
  assert.match(await run(page), /^Built in/);
  const before = await page.evaluate(() => window.breadbinStudio.files['src/main.c']);
  await page.evaluate(() => window.breadbinStudio.edited('src/main.c', window.breadbinStudio.files['src/main.c'].replace('int y = 100;', 'int y = 90;')));
  assert.match(await run(page), /^Built in/);
  await page.evaluate(() => window.breadbinStudio.openPicker());
  await page.waitForSelector('.studio-picker-history summary');
  await page.click('.studio-picker-history summary');
  await page.waitForFunction(() => document.querySelectorAll('.studio-history-row').length >= 2, { timeout: 10000 });
  assert.match(await page.$eval('.studio-history-row', (r) => r.textContent), /main\.c/);
  page.on('dialog', (d) => d.accept());
  await page.evaluate(() => [...document.querySelectorAll('.studio-history-row')][1].querySelector('button').click());
  await page.waitForFunction((b) => window.breadbinStudio.files['src/main.c'] === b, { timeout: 10000 }, before);
  assert.deepEqual(page.errors, []);
});

test('on a phone: code and C64 take turns (Run shows the C64), code keys above the keyboard type', { skip: offline, timeout: 120000 }, async () => {
  const page = await (await browser.createBrowserContext()).newPage();
  page.errors = [];
  page.on('pageerror', (e) => page.errors.push(e.message));
  await page.setViewport({ width: 390, height: 844, isMobile: true, hasTouch: true });
  await page.goto(`${server.url}#hack`, { waitUntil: 'domcontentloaded' });
  await page.waitForFunction(() => window.breadbinStudio?.files?.['project.json']);
  const visible = (sel) => page.$eval(sel, (e) => e.offsetParent !== null);
  assert.ok(await visible('.studio-phonebar'));
  assert.ok(await visible('.studio-code'), 'the code first');
  assert.equal(await visible('.studio-stage'), false, 'the C64 waits');
  // the key bar: shown while the editor has the focus; ; typed at the cursor
  await page.evaluate(() => window.breadbinStudio.openFile('src/main.c', { line: 1 }));
  await page.tap('.cm-content');
  await page.waitForSelector('.studio-keybar:not([hidden])');
  const before = await page.evaluate(() => window.breadbinStudio.code.view.state.doc.length);
  await page.evaluate(() => [...document.querySelectorAll('.studio-key')].find((k) => k.textContent === ';').dispatchEvent(new PointerEvent('pointerdown', { bubbles: true })));
  assert.equal(await page.evaluate(() => window.breadbinStudio.code.view.state.doc.length), before + 1);
  await page.evaluate(() => [...document.querySelectorAll('.studio-key')].find((k) => k.textContent === '↶').dispatchEvent(new PointerEvent('pointerdown', { bubbles: true })));
  await page.waitForFunction((n) => window.breadbinStudio.code.view.state.doc.length === n, {}, before);
  // ⋯: the completion list, without a Ctrl+Space (after "bb_" it offers the runtime's calls)
  await page.evaluate(() => window.breadbinStudio.code.view.dispatch(window.breadbinStudio.code.view.state.replaceSelection('bb_')));
  await page.evaluate(() => [...document.querySelectorAll('.studio-key')].find((k) => k.textContent === '⋯').dispatchEvent(new PointerEvent('pointerdown', { bubbles: true })));
  await page.waitForFunction(() => /bb_spr/.test(document.querySelector('.cm-tooltip-autocomplete')?.textContent || ''), { timeout: 5000 });
  await page.keyboard.press('Escape');
  await page.evaluate(() => [...document.querySelectorAll('.studio-key')].find((k) => k.textContent === '↶').dispatchEvent(new PointerEvent('pointerdown', { bubbles: true })));
  await page.waitForFunction((n) => window.breadbinStudio.code.view.state.doc.length === n, {}, before);
  // ▶ Run in the bar: the C64 takes the screen; Code brings the code back
  await page.evaluate(() => document.activeElement.blur());
  await page.evaluate(() => [...document.querySelectorAll('.studio-phonebar button')].find((b) => /Run/.test(b.textContent)).click());
  await page.waitForFunction(() => document.querySelector('.studio-stage')?.offsetParent && !document.querySelector('.studio-code').offsetParent, { timeout: 60000 });
  await page.click('.studio-phoneview[data-view=work]');
  assert.ok(await visible('.studio-code'));
  assert.deepEqual(page.errors, []);
});

test('the cheat sheet: a call tapped goes into the code at the cursor', async () => {
  const page = await openStudio(browser, server.url);
  await page.evaluate(() => window.breadbinStudio.openFile('src/main.c', { line: 1 }));
  await page.evaluate(() => window.breadbinStudio.openHelp());
  await page.waitForSelector('.studio-help[open] .studio-help-sig');
  await page.evaluate(() => [...document.querySelectorAll('.studio-help-sig')].find((b) => b.textContent.includes('bb_spr(')).click());
  const at = await page.evaluate(() => {
    const st = window.breadbinStudio.code.view.state;
    return st.doc.sliceString(st.selection.main.head - 7, st.selection.main.head + 1);
  });
  assert.equal(at, 'bb_spr()', 'name(), the cursor between the brackets');
  assert.equal(await page.$('.studio-help[open]'), null, 'the sheet closed');
  assert.deepEqual(page.errors, []);
});

test('the looks: Settings switches the theme at once, a reload keeps it, the code follows', async () => {
  const page = await openStudio(browser, server.url);
  const look = () => page.evaluate(() => ({
    theme: document.documentElement.dataset.theme || 'bakery',
    page: getComputedStyle(document.body).backgroundColor,
    code: getComputedStyle(document.querySelector('.cm-editor')).backgroundColor,
    font: getComputedStyle(document.body).fontFamily,
  }));
  const pick = async (id) => {
    await page.click('#open-settings');
    await page.click(`.theme-choice[data-look="${id}"]`);
    await page.keyboard.press('Escape');
  };
  const bakery = await look();
  assert.equal(bakery.theme, 'bakery');
  assert.equal(bakery.code, 'rgb(28, 28, 28)', 'retrobox: the code on #1c1c1c');

  await pick('classic');
  const classic = await look();
  assert.equal(classic.theme, 'classic');
  assert.equal(classic.page, 'rgb(53, 40, 121)', 'the C64\'s blue screen');
  assert.equal(classic.code, 'rgb(53, 40, 121)', 'the code on it too, without rebuilding the editor');
  assert.match(classic.font, /PXL 64/);
  assert.ok(await page.evaluate(() => document.fonts.ready.then(() => document.fonts.check('16px "PXL 64"'))), 'the C64 type loaded');

  await page.reload({ waitUntil: 'domcontentloaded' });
  await page.waitForFunction(() => window.breadbinStudio?.files?.['project.json']);
  assert.equal((await look()).theme, 'classic', 'a reload keeps the look');

  await pick('geos');
  const geos = await look();
  assert.equal(geos.theme, 'geos');
  assert.match(geos.font, /Micro 5/);
  assert.ok(await page.$('.theme-clock'), 'the desktop\'s clock in the menu bar');

  await pick('chicago');
  const chicago = await look();
  assert.equal(chicago.page, 'rgb(0, 128, 128)', 'the teal desk');
  assert.equal(chicago.code, 'rgb(255, 255, 255)', 'the code on white');
  assert.match(chicago.font, /Arimo/);
  assert.ok(await page.$('.theme-clock'), 'the clock in the taskbar\'s tray');

  await pick('bakery');
  assert.deepEqual(await look(), bakery, 'back to the bakery, exactly');
  assert.equal(await page.$('.theme-clock'), null);
  assert.deepEqual(page.errors, []);
});

test('the console (Chicago): ` drops it down, commands run, ` puts it away; other looks have none', async () => {
  const page = await openStudio(browser, server.url);
  const look = (id) => page.evaluate(async (id) => (await import('./themes/themes.js')).applyTheme(id), id);
  await page.click('.cm-content');
  await page.keyboard.press('Backquote');
  assert.ok(await page.$eval('.studio-console', (e) => e.hidden), 'none in the bakery: ` is a character there');
  await look('chicago');
  await page.click('.cm-content');
  await page.keyboard.press('Backquote');
  assert.equal(await page.$eval('.studio-console', (e) => e.hidden), false, 'down');
  for (const cmd of ['ls', 'mode sprites', 'nonsense']) {
    await page.keyboard.type(cmd);
    await page.keyboard.press('Enter');
  }
  assert.equal(await page.evaluate(() => window.breadbinStudio.mode), 'sprites');
  const log = await page.$eval('.studio-console-lines', (e) => e.textContent);
  assert.match(log, /src\/main\.c/, 'ls lists the files');
  assert.match(log, /Unknown command "nonsense"/);
  await page.keyboard.press('Backquote');
  assert.ok(await page.$eval('.studio-console', (e) => e.hidden), 'up again');
  await look('bakery');
  assert.deepEqual(page.errors, []);
});
