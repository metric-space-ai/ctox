#!/usr/bin/env node
// Real module + WebGL regression gegen den App-API-4.0-Stub (gepinnt). Fixtures sind nur Plattform-Dokumente
// business_apps/business_app_versions; keine Produktionsdaten, kein Tenant.
import assert from 'node:assert/strict';
import http from 'node:http';
import { readFileSync, mkdirSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { realManifestRoutes } from '../../../v4/tools/port/real-manifest.mjs';

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
// v4: Kern-Stub (gepinnt, CTOX_APP_API_STUB) unter /__stub/; ctox:shared/ wird wie vom Service Worker umgeschrieben.
const stubDir = path.resolve(process.env.CTOX_APP_API_STUB || path.join(process.env.HOME || '', '.local/state/shell-v4/stub-pins/64acc2c'));
// PIN-ALL (E48): echte Routen/Fremd-Collections aus v4/manifests/app-store.manifest.json; ohne aktive app-store-Version
// liefert der Kern business_apps/business_app_versions nicht an die App.
const routes = realManifestRoutes('app-store');
const publishedExtraApps = ['app-store', ...(routes._fixture_owners ?? []).map((o) => o.app).filter((id) => id !== 'ctox-platform' && id !== 'shell')];
const html = `<!doctype html><html lang="de" data-theme="dark" data-shell-style="ctox">
<meta charset="utf-8"><link rel="stylesheet" href="/app.css"><link rel="stylesheet" href="/shared/base.css">
<link rel="stylesheet" href="/modules/app-store/index.css">
<style>body{margin:0;background:#0b0e13}#open{position:absolute;right:8px;top:8px}
#window{position:absolute;left:20px;top:50px;width:1180px;height:760px;display:flex;flex-direction:column;overflow:hidden}</style>
<button id="open">App Store öffnen</button><div id="desktop"></div>
<script type="module">
import { mount } from '/modules/app-store/index.js';
import { createStubKernel } from '/__stub/ctox-app-api-stub.mjs';
const base = [
  { id:'fixture-calendar', title:'Cached Calendar', category:'Business', description:'Cached calendar', version:'1.0.0' },
  { id:'fixture-mail', title:'Cached Mail', category:'Business', description:'Cached mail', version:'1.0.0' },
  { id:'fixture-notes', title:'Cached Notes', category:'Business', description:'Cached notes', version:'1.0.0' },
];
const apps = [...base];
while (apps.length < ${catalogueSize}) {
  const index = apps.length;
  apps.push({ ...base[index % 3], id:'fixture-extra-'+index, title:'Cached App '+index });
}
const kernel = createStubKernel({ storage:'memory', latencyMs:5, multiTab:false, actor:{ role:'owner' } });
await kernel.ready;
// Zeilen nach den Plattform-Schemas (business_app_versions ohne Zusatzfelder, business_apps.migrated_at_seq als string).
await kernel.sim.push('business_app_versions', apps.map((app) => ({ id: app.id + '@1', app_id: app.id, app_ver: '1',
  manifest: { version: app.version, description: app.description, layout: { category: app.category }, provenance: {} } })));
await kernel.sim.push('business_apps', apps.map((app) => ({ id: app.id, title: app.title, active_ver: '1', migrated_at_seq: '0' })));
const routes = ${JSON.stringify(routes)};
for (const o of routes._fixture_owners ?? []) if (!(await kernel.sim.inspect('business_apps', o.app))?.data?.active_ver) await kernel.sim.publishAppVersion(o.app, o.ver, o.publish);
await kernel.sim.publishAppVersion('app-store', '1', { title: 'App Store', manifestExtra: routes });
await kernel.sim.settle();
const api = kernel.appApi('app-store', '1');
let handle, frame;
document.querySelector('#open').onclick = async () => {
  window.openStarted = performance.now();
  window.fixtureArtwork = [];
  frame = document.createElement('section');
  frame.id = 'window';
  frame.innerHTML = '<button id="close" aria-label="Schließen" style="position:fixed;left:8px;top:8px">×</button><div id="root" style="flex:1;min-height:0;display:flex;flex-direction:column"></div>';
  document.querySelector('#desktop').append(frame);
  frame.querySelector('#close').onclick = () => { handle?.unmount(); frame.remove(); };
  handle = await mount(frame.querySelector('#root'), api, { restored: { v: 1, scope: 'installed' } });
};
window.fixtureReady = true;
</script></html>`;
const mime = { '.js':'text/javascript', '.mjs':'text/javascript', '.css':'text/css',
  '.html':'text/html', '.json':'application/json', '.svg':'image/svg+xml', '.png':'image/png' };
const server = http.createServer((req, res) => {
  const pathname = new URL(req.url, 'http://localhost').pathname;
  if (pathname === '/') { res.setHeader('content-type', 'text/html'); return res.end(html); }
  const file = artworkSource && /^\/modules\/app-store\/vendor\/store-shelf\/(?:store-shelf|box-art)\.mjs$/.test(pathname)
    ? path.join(artworkSource, path.basename(pathname))
    : pathname.startsWith('/__stub/') ? path.resolve(stubDir, '.' + pathname.slice('/__stub'.length))
    : path.resolve(root, '.' + pathname);
  if (!file.startsWith(root + path.sep) && !file.startsWith(stubDir + path.sep)
    && !(artworkSource && file.startsWith(artworkSource + path.sep))) { res.writeHead(403); return res.end(); }
  try {
    res.setHeader('content-type', mime[path.extname(file)] || 'application/octet-stream');
    let body = readFileSync(oldSource && pathname === '/modules/app-store/index.js' ? oldSource : file);
    // Wie der Service Worker (§16.15, OFFEN-F): ctox:shared/x.js → /shared/x.js.
    if (/\.m?js$/.test(file)) body = Buffer.from(body.toString('utf8').replace(/(['"])ctox:shared\//g, '$1/shared/'));
    res.end(body);
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
  // Erwartet: die Katalog-Apps der Fixture plus jede weitere echte App, die sie veröffentlicht (app-store selbst und die
  // Eigentümer seiner Fremd-Collections, E48); Plattform/Shell sind keine Apps und erscheinen nie.
  assert.equal(await page.locator('[data-apps-grid] [data-app-id="ctox-platform"], [data-apps-grid] [data-app-id="shell"]').count(), 0);
  assert.equal(await page.locator('[data-apps-grid] [data-app-id]').count(), catalogueSize + publishedExtraApps.length);
  assert.equal(await page.locator('[data-apps-grid]').getByText('Cached Calendar', {exact:true}).count(), 1);
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
