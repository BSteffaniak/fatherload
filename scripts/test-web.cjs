'use strict';
// Real browser acceptance test. Use Playwright Chromium or PLAYWRIGHT_CHROMIUM_EXECUTABLE.
const assert = require('node:assert/strict');
const http = require('node:http');
const fs = require('node:fs/promises');
const path = require('node:path');
const { chromium } = require('../web/node_modules/playwright');

const root = path.resolve(__dirname, '..', process.env.FATHERLOAD_WEB_ROOT || 'target/web');
const remote = process.env.FATHERLOAD_WEB_URL;
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

(async () => {
  let browser;
  try {
    await fs.mkdir(artifacts, { recursive: true });
    if (!remote) await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
    browser = await chromium.launch({
      headless: true,
      ...(process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE ? { executablePath: process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE } : {}),
      args: ['--autoplay-policy=document-user-activation-required', '--use-gl=angle', '--use-angle=swiftshader', '--enable-unsafe-swiftshader'],
    });
    const context = await browser.newContext({ viewport: { width: 1280, height: 960 } });
    await context.addInitScript(() => {
      const requestFrame = window.requestAnimationFrame.bind(window);
      window.frameSamples = [];
      window.requestAnimationFrame = callback => requestFrame(timestamp => {
        const started = performance.now();
        callback(timestamp);
        window.frameSamples.push({ timestamp, work: performance.now() - started });
        if (window.frameSamples.length > 600) window.frameSamples.shift();
      });
    });
    const page = await context.newPage();
    page.on('pageerror', error => errors.push(error.message));
    page.on('console', message => { if (message.type() === 'error') errors.push(message.text()); });
    page.on('response', response => { if (response.status() >= 400) errors.push(`${response.status()} ${response.url()}`); });
    const key = async name => {
      await page.keyboard.down(name); await page.waitForTimeout(100);
      await page.keyboard.up(name); await page.waitForTimeout(120);
    };
    const start = async () => {
      await page.waitForFunction(() => document.body.classList.contains('playing'), { timeout: 60000 });
      await page.waitForTimeout(5000);
      await page.locator('#canvas').focus();
    };
    const shot = name => page.screenshot({ path: path.join(artifacts, `${name}.png`) });
    const files = () => page.evaluate(() => FS.readdir('/fatherload'));
    const save = async () => {
      await key('Escape'); await key('ArrowDown'); await key('Enter'); await key('Enter');
      await page.waitForFunction(() => FS.analyzePath('/fatherload/save-slot-1.json').exists);
      await page.waitForFunction(() => !Module.fatherloadSaveBusy && !Module.fatherloadSaveDirty);
      return page.evaluate(() => JSON.parse(FS.readFile('/fatherload/save-slot-1.json', { encoding: 'utf8' })));
    };
    const state = save => save.save.world;
    await page.goto(remote || `http://127.0.0.1:${server.address().port}`);
    assert.equal(await page.locator('#launch, #start').count(), 0, 'no interstitial start screen');
    await page.evaluate(() => {
      window.startupCanvasSizes = [];
      const canvas = document.getElementById('canvas');
      new MutationObserver(() => {
        window.startupCanvasSizes.push(`${canvas.width}x${canvas.height}`);
      }).observe(canvas, { attributes: true, attributeFilter: ['width', 'height'] });
    });
    await start(); await shot('title');
    const assertViewport = async () => {
      assert.deepEqual(await page.locator('#canvas').evaluate(canvas => {
        const rect = canvas.getBoundingClientRect();
        return [rect.x, rect.y, rect.width, rect.height, canvas.width, canvas.height];
      }), [0, 0, 1280, 960, 1280, 960], 'canvas fills viewport with matching render buffer');
      assert(await page.evaluate(() => document.documentElement.scrollHeight === innerHeight), 'no page scrolling');
    };
    await assertViewport();
    await page.evaluate(async () => {
      for (const device of window.miniaudio.devices) {
        if (device?.webaudio) await device.webaudio.suspend();
      }
    });
    await key('Shift');
    await page.waitForFunction(() => window.miniaudio?.devices.some(device => device?.webaudio?.state === 'running'));
    assert(await page.locator('#canvas').isVisible(), 'original title renders without activation');
    await page.getByRole('button', { name: 'Help', exact: true }).click();
    await shot('help');
    await page.getByRole('button', { name: 'Back', exact: true }).click();
    await page.waitForFunction(() => document.activeElement === document.getElementById('canvas'));
    await page.getByRole('button', { name: 'Fullscreen', exact: true }).click();
    await page.waitForFunction(() => document.fullscreenElement?.id === 'game');
    assert(await page.locator('#tools').isVisible(), 'fullscreen retains web controls');
    await page.getByRole('button', { name: 'Exit fullscreen', exact: true }).click();
    await page.waitForFunction(() => !document.fullscreenElement);
    await page.waitForTimeout(500);
    await assertViewport();
    const startupSizes = await page.evaluate(() => [...new Set(window.startupCanvasSizes)]);
    assert(startupSizes.length <= 3, `startup resize feedback loop: ${startupSizes.join(', ')}`);
    await key('Enter');
    await page.waitForFunction(() => window.miniaudio?.devices.some(device => device?.webaudio?.state === 'running'));
    for (let i = 0; i < 6; i++) await key('ArrowDown');
    await key('Enter'); await page.waitForTimeout(3000); await shot('single-player');
    await page.evaluate(() => { window.frameSamples = []; });
    await page.waitForTimeout(2000);
    const frames = await page.evaluate(() => window.frameSamples);
    assert(frames.length >= 10, 'game must keep scheduling animation frames');
    const intervals = frames.slice(1).map((frame, i) => frame.timestamp - frames[i].timestamp).sort((a,b) => a-b);
    const work = frames.map(frame => frame.work).sort((a,b) => a-b);
    console.log(`RAF timing (headless/software GPU): ${frames.length} frames; median interval ${intervals[Math.floor(intervals.length/2)].toFixed(2)} ms; median work ${work[Math.floor(work.length/2)].toFixed(2)} ms`);
    await page.keyboard.down('d'); await page.waitForTimeout(600); await page.keyboard.up('d');
    const original = await save();
    assert.equal(Object.keys(state(original).players).length, 1, 'single-player roster');
    const savedText = await page.evaluate(() => FS.readFile('/fatherload/save-slot-1.json', { encoding: 'utf8' }));
    assert((await files()).includes('save-slot-1.json'));
    await page.reload();
    await page.waitForFunction(() => document.body.classList.contains('playing'), { timeout: 60000 });
    assert.equal(await page.evaluate(() => FS.readFile('/fatherload/save-slot-1.json', { encoding: 'utf8' })), savedText, 'IndexedDB restores the exact save before main');
    await start(); await shot('restored-title');
    await key('Enter'); await page.waitForTimeout(2000); await shot('resumed');
    // The real visibility listener requests a checkpoint; verify the Rust loop consumes it.
    await page.evaluate(() => { Module.fatherloadSuspendRequested = true; });
    await page.waitForFunction(() => FS.analyzePath('/fatherload/save.json').exists);
    await page.waitForFunction(() => !Module.fatherloadSaveBusy && !Module.fatherloadSaveDirty);
    // Fresh browser storage: select local split-screen, start it, save its two-seat world.
    const splitContext = await browser.newContext({ viewport: { width: 1280, height: 960 } });
    const split = await splitContext.newPage();
    split.on('pageerror', error => errors.push(error.message));
    split.on('console', message => { if (message.type() === 'error') errors.push(message.text()); });
    const splitKey = async name => {
      await split.keyboard.down(name); await split.waitForTimeout(100);
      await split.keyboard.up(name); await split.waitForTimeout(120);
    };
    await split.goto(page.url());
    await split.waitForFunction(() => document.body.classList.contains('playing'), { timeout: 60000 });
    await split.waitForTimeout(5000);
    await splitKey('ArrowDown'); await splitKey('Enter');
    for (let i = 0; i < 6; i++) await splitKey('ArrowDown');
    await splitKey('Enter'); await split.waitForTimeout(3000);
    await split.screenshot({ path: path.join(artifacts, 'split-screen.png') });
    await splitKey('Escape'); await splitKey('s'); await splitKey('Enter'); await splitKey('Enter');
    await split.waitForFunction(() => FS.analyzePath('/fatherload/save-slot-1.json').exists);
    const roster = await split.evaluate(() => JSON.parse(FS.readFile('/fatherload/save-slot-1.json', { encoding: 'utf8' })).save.world.players);
    assert.equal(Object.keys(roster).length, 2, 'split-screen roster');
    await split.setViewportSize({ width: 960, height: 720 }); await split.waitForTimeout(500);
    assert.equal(await split.locator('#canvas').evaluate(canvas => canvas.width), 960, 'render buffer resizes');
    assert.equal(await split.locator('#canvas').evaluate(canvas => canvas.height), 720, 'render buffer uses full height');
    await page.evaluate(() => Module.setStatus('Saved in this browser.'));
    await page.waitForTimeout(3700);
    assert.equal(await page.locator('#status').textContent(), '', 'save notification dismisses');
    await page.evaluate(() => Module.setStatus('Could not persist saves. Keep this tab open and retry saving.'));
    await page.waitForTimeout(3700);
    assert.match(await page.locator('#status').textContent(), /Could not persist/, 'storage errors stay visible');
    await page.setViewportSize({ width: 390, height: 844 });
    await page.waitForTimeout(500);
    assert.deepEqual(await page.locator('#canvas').evaluate(canvas => [canvas.width, canvas.height]), [390, 844], 'portrait viewport fits');
    await shot('portrait');
    assert.deepEqual(errors, [], 'browser console must not contain runtime errors');
    console.log('PASS: gameplay, audio initialization, exact IndexedDB save/reload, resume, suspension checkpoint, two-seat split-screen and resize.');
    console.log(`Screenshots: ${artifacts}`);
  } finally {
    if (browser) await browser.close();
    if (server.listening) await new Promise(resolve => server.close(resolve));
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
