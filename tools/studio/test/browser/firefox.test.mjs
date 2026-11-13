// The Studio in Firefox (FIREFOX, else /usr/bin/firefox; skipped without
// one): it opens, builds in its worker, every editor shows, and it offers
// zips, as Firefox has no folder access.
import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import puppeteer from 'puppeteer-core';
import { serve, sleep } from './harness.mjs';

const FIREFOX = process.env.FIREFOX || '/usr/bin/firefox';
const skip = !fs.existsSync(FIREFOX) && 'no Firefox';

test('the Studio works in Firefox', { skip, timeout: 120000 }, async () => {
  const server = await serve();
  const browser = await puppeteer.launch({ browser: 'firefox', executablePath: FIREFOX, headless: true });
  try {
    const page = await browser.newPage();
    const errors = [];
    page.on('pageerror', (e) => errors.push(e.message));
    await page.setViewport({ width: 1400, height: 900 });
    await page.goto(`${server.url}#hack`);
    await page.waitForFunction(() => window.breadbinStudio?.files?.['project.json'], { timeout: 60000 });
    assert.equal(await page.evaluate(() => typeof window.showDirectoryPicker), 'undefined', 'Firefox has no folder access');
    await page.evaluate(() => document.querySelector('.studio-run').click());
    await page.waitForFunction(() => /^(Built|Didn't)/.test(document.querySelector('.studio-buildinfo').textContent), { timeout: 60000 });
    assert.match(await page.$eval('.studio-buildinfo', (e) => e.textContent), /^Built in/);
    for (const m of ['sprites', 'tiles', 'map', 'music', 'code']) {
      await page.evaluate((m) => document.querySelector(`.studio-mode[data-mode=${m}]`).click(), m);
      await sleep(300);
      assert.equal(await page.evaluate((m) => document.querySelector(`.studio-pane[data-mode=${m}]`).hidden, m), false, `${m} shows`);
    }
    assert.ok(await page.$('.cm-editor .cm-line'), 'the code editor');
    assert.deepEqual(errors, []);
  } finally {
    await browser.close();
    server.close();
  }
});
