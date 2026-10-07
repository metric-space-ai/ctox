#!/usr/bin/env node
// Real module + WebGL regression. Only the shell-owned cached DB handle and
// pending sync bridge are fixtures; no production records or tenant are used.
import assert from 'node:assert/strict';
import http from 'node:http';
import { readFileSync, mkdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../..');
const output = process.argv[process.argv.indexOf('--output-dir') + 1];
assert.ok(process.argv.includes('--output-dir') && output, '--output-dir is required');
mkdirSync(output, { recursive: true });
const { chromium } = await import(process.env.PLAYWRIGHT_MODULE_PATH
  ? pathToFileURL(path.join(process.env.PLAYWRIGHT_MODULE_PATH, 'index.mjs')).href : 'playwright');
const oldSource = process.env.CTOX_APP_STORE_MOUNT_SOURCE;
const artworkArg = process.argv.indexOf('--artwork-source-dir');
const artworkSource = artworkArg < 0 ? null : path.resolve(process.argv[artworkArg + 1]);
const countArg = process.argv.indexOf('--catalogue-size');
const catalogueSize = countArg < 0 ? 3 : Number(process.argv[countArg + 1]);
assert.ok(Number.isInteger(catalogueSize) && catalogueSize >= 3 && catalogueSize <= 48, 'catalogue-size must be 3..48');
const html = `<!doctype html><html lang="de" data-theme="dark" data-shell-style="ctox">
<meta charset="utf-8"><link rel="stylesheet" href="/app.css"><link rel="stylesheet" href="/shared/base.css">
<style>body{margin:0;background:#0b0e13}#open{position:absolute;right:8px;top:8px}
.shell-window{position:absolute;left:20px;top:50px;width:1180px;height:760px}</style>
<button id="open">App Store öffnen</button><div id="desktop"></div>
<script type="module">
import { mount } from '/modules/app-store/index.js';
const catalog = { modules: [], templates: [], marketplace: [
  { id:'fixture-calendar', title:'Offline Calendar', category:'Business', description:'Cached calendar', version:'1.0.0', download_url:'/fixture-unused.zip', status:'installed' },
  { id:'fixture-mail', title:'Offline Mail', category:'Business', description:'Cached mail', version:'1.0.0', download_url:'/fixture-unused.zip', status:'installed' },
  { id:'fixture-notes', title:'Offline Notes', category:'Business', description:'Cached notes', version:'1.0.0', download_url:'/fixture-unused.zip', status:'installed' },
]};
while (catalog.marketplace.length < ${catalogueSize}) {
  const index = catalog.marketplace.length;
  catalog.marketplace.push({ ...catalog.marketplace[index % 3], id:'fixture-extra-'+index, title:'Cached App '+index });
}
let cleanup, frame;
document.querySelector('#open').onclick = async () => {
  window.openStarted = performance.now();
  window.fixtureArtwork = [];
  frame = document.createElement('section');
  frame.className = 'shell-window is-focused';
  frame.dataset.shellContract = 'v2';
  frame.dataset.shellWindowChrome = 'shared-v2';
  frame.innerHTML = '<header class="shell-window-header"></header><div class="shell-window-controls"><button id="close" aria-label="Schließen">×</button></div><div class="shell-window-content"><div class="module-root shell-window-module-root"><main class="module-content"></main></div></div>';
  document.querySelector('#desktop').append(frame);
  frame.querySelector('#close').onclick = () => { cleanup?.(); frame.remove(); };
  cleanup = await mount({
    host: frame.querySelector('main'), locale:'de', modules:[],
    session:{ user:{ id:'fixture-owner', role:'owner' } },
    sync:{ startCollection:() => new Promise(() => {}) },
    db:{ collection:() => ({ findOne:() => ({
      exec:async () => ({ toJSON:() => catalog }),
      $:{ subscribe:() => ({ unsubscribe() {} }) },
    }) }) },
  });
};
window.fixtureReady = true;
</script></html>`;
const mime = { '.js':'text/javascript', '.mjs':'text/javascript', '.css':'text/css',
  '.html':'text/html', '.json':'application/json', '.svg':'image/svg+xml', '.png':'image/png' };
const server = http.createServer((req, res) => {
  const pathname = new URL(req.url, 'http://localhost').pathname;
  if (pathname === '/') { res.setHeader('content-type', 'text/html'); return res.end(html); }
  const file = artworkSource && /^\/vendor\/store-shelf\/(?:store-shelf|box-art)\.mjs$/.test(pathname)
    ? path.join(artworkSource, path.basename(pathname))
    : path.resolve(root, '.' + pathname);
  if (!file.startsWith(root + path.sep) && !(artworkSource && file.startsWith(artworkSource + path.sep))) { res.writeHead(403); return res.end(); }
  try {
    res.setHeader('content-type', mime[path.extname(file)] || 'application/octet-stream');
    res.end(readFileSync(oldSource && pathname === '/modules/app-store/index.js' ? oldSource : file));
  } catch { res.writeHead(404); res.end(); }
});
await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
let browser, context, page;
const pageErrors = [], consoleMessages = [];
try {
  browser = await chromium.launch({ headless:true, chromiumSandbox:true,
    args:['--disable-gpu', '--enable-unsafe-swiftshader'] });
  context = await browser.newContext({ viewport:{width:1280,height:900}, reducedMotion:'no-preference' });
  page = await context.newPage();
  page.on('console', message => { if (message.type() === 'warning' || message.type() === 'error') consoleMessages.push(message.text()); });
  page.on('pageerror', error => pageErrors.push(error.message));
  // Preserve the actual framebuffer solely to measure rendered box pixels.
  // Shadows are black; non-black opaque pixels demonstrate actual box art,
  // rather than an accessible name attached to an empty canvas.
  await page.addInitScript(() => {
    window.fixtureArtwork = [];
    const Canvas = globalThis.OffscreenCanvas;
    if (Canvas) globalThis.OffscreenCanvas = class extends Canvas {
      constructor(width, height) {
        super(width, height);
        window.fixtureArtwork.push({ width, height });
      }
    };
    const original = HTMLCanvasElement.prototype.getContext;
    HTMLCanvasElement.prototype.getContext = function(type, options) {
      const gl = original.call(this, type, /^webgl/.test(type)
        ? {...options, preserveDrawingBuffer:true} : options);
      if (/^webgl/.test(type) && gl) this.fixtureGl = gl;
      return gl;
    };
    window.boxPixels = () => {
      const canvas = document.querySelector('[data-shelf-canvas]');
      const gl = canvas?.fixtureGl;
      if (!gl || !canvas.isConnected || !gl.drawingBufferWidth) return 0;
      const pixels = new Uint8Array(gl.drawingBufferWidth * gl.drawingBufferHeight * 4);
      gl.readPixels(0,0,gl.drawingBufferWidth,gl.drawingBufferHeight,gl.RGBA,gl.UNSIGNED_BYTE,pixels);
      let count = 0;
      for (let i=0;i<pixels.length;i+=32) {
        if (pixels[i+3]>128 && pixels[i]+pixels[i+1]+pixels[i+2]>90) count++;
      }
      return count;
    };
  });
  await page.goto('http://127.0.0.1:' + server.address().port);
  await page.waitForFunction(() => window.fixtureReady);
  await page.getByRole('button', {name:'App Store öffnen', exact:true}).click();
  await page.waitForFunction(() => window.boxPixels() > 200, null, {timeout:10000, polling:'raf'});
  const first = await page.evaluate(() => {
    window.firstCanvas = document.querySelector('[data-shelf-canvas]');
    return { milliseconds:performance.now()-window.openStarted, pixels:window.boxPixels() };
  });
  await page.screenshot({path:path.join(output,'first.png')});
  await page.getByRole('button', {name:'Schließen', exact:true}).click();
  assert.equal(await page.evaluate(() => window.firstCanvas.isConnected), false);
  await page.getByRole('button', {name:'App Store öffnen', exact:true}).click();
  await page.waitForFunction(() => window.boxPixels() > 200, null, {timeout:5000, polling:'raf'});
  const reopened = await page.evaluate(() => ({
    milliseconds:performance.now()-window.openStarted, pixels:window.boxPixels(),
    freshCanvas:window.firstCanvas !== document.querySelector('[data-shelf-canvas]'),
    rawArtworkBytes:window.fixtureArtwork.reduce((sum, image) => sum + image.width * image.height * 4, 0),
  }));
  assert.equal(reopened.freshCanvas, true);
  assert.ok(reopened.rawArtworkBytes <= 80 * 1024 ** 2, JSON.stringify(reopened));
  assert.ok(reopened.milliseconds < 1000, JSON.stringify(reopened));
  await page.screenshot({path:path.join(output,'reopened.png')});
  await context.setOffline(true);
  await page.getByRole('button', {name:'Als Liste anzeigen', exact:true}).click();
  assert.equal(await page.locator('[data-apps-grid] [data-app-id]').count(), catalogueSize);
  assert.equal(await page.locator('[data-apps-grid]').getByText('Offline Calendar', {exact:true}).count(), 1);
  await page.screenshot({path:path.join(output,'offline.png')});
  assert.deepEqual(pageErrors, []);
  writeFileSync(path.join(output,'result.json'),JSON.stringify({first,reopened,offlineCachedItems:catalogueSize,pageErrors},null,2)+'\n');
  console.log('SHELF_BROWSER_PASS ' + JSON.stringify({first,reopened,offlineCachedItems:catalogueSize}));
} catch (error) {
  await page?.screenshot({path:path.join(output,'failure.png')});
  const surface = await page?.evaluate(() => {
    const canvas = document.querySelector('[data-shelf-canvas]');
    return { text:document.body.innerText.slice(0,1500), canvasRect:canvas?.getBoundingClientRect().toJSON(),
      gl:!!canvas?.fixtureGl, pixels:window.boxPixels?.() };
  }).catch(() => null);
  const failure = { error:error.message, pageErrors, consoleMessages, surface };
  writeFileSync(path.join(output,'failure.json'),JSON.stringify(failure,null,2)+'\n');
  console.log('SHELF_BROWSER_FAILURE '+JSON.stringify(failure));
  throw error;
} finally {
  await context?.close();
  await browser?.close();
  await new Promise(resolve => server.close(resolve));
}
