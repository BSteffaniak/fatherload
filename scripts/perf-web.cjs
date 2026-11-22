'use strict';
// Browser frame timing and WASM memory in staged game states (wave 6 QA; memory since wave 7).
//
// Export the states with the desktop build, then time the WASM build in each:
//
//   cargo run --release -p fatherload_game -- --screenshot-scenes target/perf-saves \
//     --export-saves --only night_town,sky_city,moon_colony,crypt_traps,...
//   node scripts/perf-web.cjs target/perf-saves
//
// Each save is written into a fresh browser profile's save storage, the page reloads and the
// title's Continue loads it; the frame work of every animation-frame callback (simulation and
// drawing, as JavaScript sees it) is then sampled for a few seconds. FATHERLOAD_WEB_GPU=1 (and
// PLAYWRIGHT_CHROMIUM_EXECUTABLE=/path/to/chrome) uses the machine's GPU; the default is
// software rendering, as in the acceptance test. Screenshots go to target/web-perf/.
const http = require('node:http');
const fs = require('node:fs/promises');
const path = require('node:path');
const { chromium } = require('../web/node_modules/playwright');

const root = path.resolve(__dirname, '..', process.env.FATHERLOAD_WEB_ROOT || 'target/web');
const saves = path.resolve(process.argv[2] || 'target/perf-saves');
const seconds = Number(process.env.FATHERLOAD_PERF_SECONDS || 5);
const artifacts = path.resolve(__dirname, '../target/web-perf');
const server = http.createServer(async (req, res) => {
  try {
    const name = new URL(req.url, 'http://localhost').pathname;
    const file = name === '/' ? 'index.html' : name.slice(1);
    if (!['index.html', 'fatherload.js', 'fatherload.wasm'].includes(file)) { res.writeHead(404).end(); return; }
    const types = { '.html': 'text/html', '.js': 'application/javascript', '.wasm': 'application/wasm' };
    res.setHeader('Content-Type', types[path.extname(file)]);
    res.end(await fs.readFile(path.join(root, file)));
  } catch (error) { res.writeHead(500).end(String(error)); }
});

const percentile = (sorted, fraction) => sorted[Math.min(sorted.length - 1, Math.round((sorted.length - 1) * fraction))];

(async () => {
  let browser;
  try {
    await fs.mkdir(artifacts, { recursive: true });
    const names = (await fs.readdir(saves)).filter(name => name.endsWith('.json') && !name.startsWith('.')).sort();
    await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
    const url = `http://127.0.0.1:${server.address().port}`;
    browser = await chromium.launch({
      headless: true,
      ...(process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE ? { executablePath: process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE } : {}),
      args: process.env.FATHERLOAD_WEB_GPU
        ? ['--enable-gpu', '--ignore-gpu-blocklist']
        : ['--use-gl=angle', '--use-angle=swiftshader', '--enable-unsafe-swiftshader'],
    });
    const rows = [];
    for (const name of names) {
      const text = await fs.readFile(path.join(saves, name), 'utf8');
      const context = await browser.newContext({ viewport: { width: 1280, height: 720 } });
      await context.addInitScript(() => {
        const requestFrame = window.requestAnimationFrame.bind(window);
        window.frameSamples = [];
        window.requestAnimationFrame = callback => requestFrame(timestamp => {
          const started = performance.now();
          callback(timestamp);
          window.frameSamples.push({ timestamp, work: performance.now() - started });
          if (window.frameSamples.length > 5000) window.frameSamples.shift();
        });
      });
      const page = await context.newPage();
      const errors = [];
      page.on('pageerror', error => errors.push(error.message));
      const playing = async () => {
        await page.waitForFunction(() => document.body.classList.contains('playing'), { timeout: 120000 });
        await page.waitForTimeout(3000);
        await page.locator('#canvas').focus();
      };
      await page.goto(url);
      await playing();
      await page.evaluate(save => {
        FS.writeFile('/fatherload/save-slot-1.json', save);
        Module.fatherloadSaveDirty = true;
        Module.flushFatherloadSaves();
      }, text);
      await page.waitForFunction(() => !Module.fatherloadSaveBusy && !Module.fatherloadSaveDirty);
      await page.reload();
      await playing();
      await page.keyboard.down('Enter'); await page.waitForTimeout(150); await page.keyboard.up('Enter');
      await page.waitForTimeout(3000);
      await page.evaluate(() => { window.frameSamples = []; });
      await page.waitForTimeout(seconds * 1000);
      const frames = await page.evaluate(() => window.frameSamples);
      // WASM linear memory never shrinks, so its size is the page's heap high-water mark.
      const memory = await page.evaluate(() => {
        /* global HEAP8, wasmMemory */
        if (typeof wasmMemory !== 'undefined') return wasmMemory.buffer.byteLength;
        if (typeof HEAP8 !== 'undefined') return HEAP8.buffer.byteLength;
        return Module.HEAP8 ? Module.HEAP8.buffer.byteLength : NaN;
      });
      await page.screenshot({ path: path.join(artifacts, name.replace(/\.json$/, '.png')) });
      await context.close();
      const work = frames.map(frame => frame.work).sort((a, b) => a - b);
      const mean = work.reduce((sum, value) => sum + value, 0) / Math.max(1, work.length);
      rows.push({ scene: name.replace(/\.json$/, ''), frames: frames.length, mean, median: percentile(work, 0.5), p95: percentile(work, 0.95), max: work[work.length - 1], memory, errors: errors.length });
    }
    console.log(`| Scene | Frames in ${seconds} s | Mean work (ms) | Median | p95 | Max | WASM memory (MiB) |`);
    console.log('|---|---|---|---|---|---|---|');
    for (const row of rows) {
      console.log(`| ${row.scene} | ${row.frames} | ${row.mean.toFixed(2)} | ${row.median.toFixed(2)} | ${row.p95.toFixed(2)} | ${row.max.toFixed(2)} | ${(row.memory / 1048576).toFixed(1)} |${row.errors ? ` ${row.errors} page errors` : ''}`);
    }
    console.log(`Screenshots: ${artifacts}`);
  } finally {
    if (browser) await browser.close();
    if (server.listening) await new Promise(resolve => server.close(resolve));
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
