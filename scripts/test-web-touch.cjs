'use strict';
// Phone acceptance test: a touch-only, high-density browser in both orientations.
// Use Playwright Chromium or PLAYWRIGHT_CHROMIUM_EXECUTABLE.
const assert = require('node:assert/strict');
const http = require('node:http');
const fs = require('node:fs/promises');
const path = require('node:path');
const { chromium } = require('../web/node_modules/playwright');

const root = path.resolve(__dirname, '..', process.env.FATHERLOAD_WEB_ROOT || 'target/web');
const artifacts = path.resolve(__dirname, '../target/web-test');
const errors = [];
const server = http.createServer(async (req, res) => {
  try {
    const name = new URL(req.url, 'http://localhost').pathname;
    if (!['/', '/index.html', '/fatherload.js', '/fatherload.wasm'].includes(name) && !/^\/assets\/fatherload\.[a-f0-9]{64}\.(js|wasm)$/.test(name)) {
      res.writeHead(404).end(); return;
    }
    const file = name === '/' ? 'index.html' : name.slice(1);
    const types = { '.html': 'text/html', '.js': 'application/javascript', '.wasm': 'application/wasm' };
    res.setHeader('Content-Type', types[path.extname(file)]);
    res.end(await fs.readFile(path.join(root, file)));
  } catch (error) { res.writeHead(500).end(String(error)); }
});

// A phone held sideways and upright: CSS size and device pixel ratio.
const PHONES = [
  { name: 'landscape', width: 844, height: 390, ratio: 2 },
  { name: 'portrait', width: 390, height: 844, ratio: 2 },
];

async function play(browser, url, phone) {
  const context = await browser.newContext({
    viewport: { width: phone.width, height: phone.height },
    deviceScaleFactor: phone.ratio,
    hasTouch: true,
    isMobile: true,
  });
  const page = await context.newPage();
  const touch = await context.newCDPSession(page);
  page.on('pageerror', error => errors.push(`${phone.name}: ${error.message}`));
  page.on('console', message => { if (message.type() === 'error') errors.push(`${phone.name}: ${message.text()}`); });
  const shot = name => page.screenshot({ path: path.join(artifacts, `touch-${phone.name}-${name}.png`) });
  // Software rendering at phone density is slow: inputs are held for whole frames, not
  // milliseconds, so the game always samples them.
  const frames = (count = 3) => page.evaluate(count => new Promise(resolve => {
    const step = left => (left ? requestAnimationFrame(() => step(left - 1)) : resolve());
    step(count);
  }), count);
  const key = async name => {
    await page.keyboard.down(name); await frames();
    await page.keyboard.up(name); await frames();
  };
  const finger = (type, points) => touch.send('Input.dispatchTouchEvent', {
    type, touchPoints: points.map(([x, y], id) => ({ x, y, id })),
  });
  const tap = async (x, y) => {
    await finger('touchStart', [[x, y]]); await frames();
    await finger('touchEnd', []); await frames();
  };
  const inMenu = () => page.evaluate(() => document.body.classList.contains('menu'));
  const save = async () => {
    // Remove the last save so the wait below sees this one.
    await page.evaluate(() => { if (FS.analyzePath('/fatherload/save-slot-1.json').exists) FS.unlink('/fatherload/save-slot-1.json'); });
    // The pause menu remembers its row: go to the top, then down to Save Game.
    await key('Escape'); for (let i = 0; i < 3; i++) await key('ArrowUp');
    await key('ArrowDown'); await key('e'); await key('e');
    await page.waitForFunction(() => FS.analyzePath('/fatherload/save-slot-1.json').exists);
    await page.waitForFunction(() => !Module.fatherloadSaveBusy && !Module.fatherloadSaveDirty);
    const text = await page.evaluate(() => FS.readFile('/fatherload/save-slot-1.json', { encoding: 'utf8' }));
    // Close the slot picker, then the pause menu.
    for (let i = 0; i < 3 && await inMenu(); i++) await key('Escape');
    assert(!(await inMenu()), 'back in play after saving');
    return Object.values(JSON.parse(text).save.world.players)[0];
  };

  await page.goto(url);
  await page.waitForFunction(() => document.body.classList.contains('playing'), { timeout: 60000 });
  await page.waitForTimeout(4000);
  assert(await page.evaluate(() => document.body.classList.contains('touch')), 'a phone starts in touch mode');
  assert.deepEqual(await page.locator('#canvas').evaluate(canvas => {
    const rect = canvas.getBoundingClientRect();
    return [rect.width, rect.height, canvas.width, canvas.height];
  }), [phone.width, phone.height, phone.width * phone.ratio, phone.height * phone.ratio],
  'the render buffer is the page in device pixels');
  assert(await page.evaluate(() => document.documentElement.scrollHeight === innerHeight), 'no page scrolling');
  assert(await inMenu(), 'the title is a menu');
  await shot('title');

  // World setup is driven with keys (a phone can have a keyboard too); everything after it
  // is touch, checked through what the page and the save report.
  await page.locator('#canvas').focus();
  await key('Shift');
  await key('e');
  for (let i = 0; i < 7; i++) await key('ArrowDown');
  await shot('setup');
  await key('e');
  await page.waitForFunction(() => !document.body.classList.contains('menu'), { timeout: 20000 });
  await page.waitForTimeout(2500);
  // A touch anywhere hands the prompts and controls back to the touch screen.
  await tap(phone.width * 0.5, phone.height * 0.35);
  await page.waitForTimeout(300);
  await shot('playing');
  assert(!(await page.locator('#tools').isVisible()), 'page buttons stay out of the way while playing');

  // The floating stick: a thumb planted bottom-left and pushed right drives the rig right.
  const before = await save();
  await tap(phone.width * 0.5, phone.height * 0.35);
  const base = [phone.width * 0.2, phone.height * 0.8];
  await finger('touchStart', [base]); await page.waitForTimeout(80);
  for (let step = 1; step <= 6; step++) {
    await finger('touchMove', [[base[0] + step * 12, base[1]]]); await page.waitForTimeout(30);
  }
  await shot('stick');
  await page.waitForTimeout(2000);
  await finger('touchEnd', []); await page.waitForTimeout(300);
  const after = await save();
  assert(after.x > before.x + 8, `the stick drives the rig right: ${before.x} -> ${after.x}`);

  // Menus are tapped: open the pause menu and tap down its middle until Resume takes it.
  await tap(phone.width * 0.5, phone.height * 0.35);
  await key('Escape');
  await page.waitForFunction(() => document.body.classList.contains('menu'));
  await page.waitForTimeout(400);
  await shot('pause');
  assert(await page.locator('#tools').isVisible(), 'page buttons return in menus');
  let resumed = false;
  for (let y = 0.15; y < 0.9 && !resumed; y += 0.03) {
    await tap(phone.width * 0.5, phone.height * y);
    resumed = !(await inMenu());
  }
  assert(resumed, 'tapping the Resume row closes the pause menu');
  await context.close();
}

(async () => {
  let browser;
  try {
    await fs.mkdir(artifacts, { recursive: true });
    await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
    browser = await chromium.launch({
      headless: true,
      ...(process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE ? { executablePath: process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE } : {}),
      // FATHERLOAD_WEB_GPU=1 renders on the machine's GPU (local runs: software rendering can
      // be too slow to hold key presses across frames); CI uses software rendering.
      args: [...(process.env.FATHERLOAD_WEB_GPU
        ? ['--enable-gpu', '--ignore-gpu-blocklist']
        : ['--use-gl=angle', '--use-angle=swiftshader', '--enable-unsafe-swiftshader'])],
    });
    const url = `http://127.0.0.1:${server.address().port}`;
    for (const phone of PHONES) await play(browser, url, phone);
    assert.deepEqual(errors, [], 'no page errors');
    console.log('Phone touch test passed.');
  } catch (error) {
    console.error(error);
    process.exitCode = 1;
  } finally {
    if (browser) await browser.close();
    server.close();
  }
})();
