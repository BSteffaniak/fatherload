'use strict';
// Browser frame timing of the space renderer (Wave 11 space-render).
//
// The WASM build shows a staged space view instead of the game when the page is opened with
// `?space=<name>` (the names of `rendering::space::fixtures::NAMES`, the same views as the
// `space_*` screenshot scenes). This samples the frame work of every animation-frame callback
// in each view, like scripts/perf-web.cjs does for saved game states:
//
//   nix develop .#web --command bash scripts/build-web.sh
//   node scripts/perf-web-space.cjs [space_black_hole,space_asteroid_belt,...]
//
// FATHERLOAD_WEB_GPU=1 (and PLAYWRIGHT_CHROMIUM_EXECUTABLE=/path/to/chrome) uses the machine's
// GPU; the default is software rendering (SwiftShader), as in the acceptance test.
// Screenshots go to target/web-perf-space/.
const http = require('node:http');
const fs = require('node:fs/promises');
const path = require('node:path');
const { chromium } = require('../web/node_modules/playwright');

const root = path.resolve(__dirname, '..', process.env.FATHERLOAD_WEB_ROOT || 'target/web');
const seconds = Number(process.env.FATHERLOAD_PERF_SECONDS || 5);
const artifacts = path.resolve(__dirname, '../target/web-perf-space');
const all = [
  'space_deep_starfield', 'space_star_system', 'space_planet_gallery', 'space_approach_moon',
  'space_approach_planet', 'space_moon_hover', 'space_moon_surface', 'space_moon_night',
  'space_moon_rolled', 'space_planet_surface', 'space_core_shaft', 'space_polar_quads',
  'space_nebula', 'space_black_hole', 'space_asteroid_belt', 'space_comet',
  'space_station_derelict', 'space_wormhole', 'space_warp',
];
const names = process.argv[2] ? process.argv[2].split(',') : all;
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
      const logs = [];
      page.on('pageerror', error => errors.push(error.message));
      page.on('console', message => { if (/space:|shader/i.test(message.text())) logs.push(message.text()); });
      await page.goto(`${url}/?space=${name}`);
      await page.waitForFunction(() => window.frameSamples.length > 30, { timeout: 120000 });
      await page.waitForTimeout(2000);
      await page.evaluate(() => { window.frameSamples = []; });
      await page.waitForTimeout(seconds * 1000);
      const frames = await page.evaluate(() => window.frameSamples);
      await page.screenshot({ path: path.join(artifacts, `${name}.png`) });
      await context.close();
      const work = frames.map(frame => frame.work).sort((a, b) => a - b);
      const mean = work.reduce((sum, value) => sum + value, 0) / Math.max(1, work.length);
      const span = frames.length > 1 ? (frames[frames.length - 1].timestamp - frames[0].timestamp) / 1000 : seconds;
      rows.push({ scene: name, frames: frames.length, fps: (frames.length - 1) / Math.max(span, 0.001), mean, median: percentile(work, 0.5), p95: percentile(work, 0.95), max: work[work.length - 1], errors: errors.length, logs });
    }
    console.log(`| Scene | Frames in ${seconds} s | fps | Mean work (ms) | Median | p95 | Max |`);
    console.log('|---|---|---|---|---|---|---|');
    for (const row of rows) {
      console.log(`| ${row.scene} | ${row.frames} | ${row.fps.toFixed(1)} | ${row.mean.toFixed(2)} | ${row.median.toFixed(2)} | ${row.p95.toFixed(2)} | ${row.max.toFixed(2)} |${row.errors ? ` ${row.errors} page errors` : ''}${row.logs.length ? ` ${row.logs.join('; ')}` : ''}`);
    }
    console.log(`Screenshots: ${artifacts}`);
  } finally {
    if (browser) await browser.close();
    if (server.listening) await new Promise(resolve => server.close(resolve));
  }
})().catch(error => { console.error(error); process.exitCode = 1; });
