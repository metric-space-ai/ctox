// Gemeinsamer Prüfstand der threads-v4-Tests: statischer Server (ctox:shared/ → /shared/ wie der Service Worker,
// §16.15), Chromium headless, Kern-Stub aus dem Pflicht-Pin (CTOX_APP_API_STUB, sonst stub-pins/4b0d0ac).
// Muster: modules/knowledge/tests/v4-api.test.mjs. Playwright aus PLAYWRIGHT_MODULE bzw. ~/Documents/ctox.
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { dirname, join, normalize, extname } from 'node:path';
import { homedir } from 'node:os';
import { realManifestRoutes } from '../../../v4/tools/port/real-manifest.mjs';

export const APP = 'threads';
export const HERE = dirname(fileURLToPath(import.meta.url));
export const STUB = process.env.CTOX_APP_API_STUB || join(homedir(), '.local/state/shell-v4/stub-pins/4b0d0ac');
const BOS = normalize(join(HERE, '../../..'));
const require = createRequire(import.meta.url);
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || join(homedir(), 'Documents/ctox/src/apps/business-os/node_modules/playwright'));
const TYPES = { '.js': 'text/javascript', '.mjs': 'text/javascript', '.css': 'text/css', '.html': 'text/html', '.json': 'application/json', '.svg': 'image/svg+xml' };

const PAGE = `<!doctype html><html lang="de"><head><meta charset="utf-8">
<link rel="stylesheet" href="/ui-contract/v1/workjet-ui-contract.css">
<link rel="stylesheet" href="/app.css">
<link rel="stylesheet" href="/shared/base.css">
<link rel="stylesheet" href="/modules/${APP}/index.css">
<style>html,body{margin:0} #win{position:relative;overflow:hidden;background:var(--bg)}</style>
</head><body><div id="win" style="width:1280px;height:760px"></div></body></html>`;

export async function startHarness() {
  const server = createServer(async (req, res) => {
    const path = decodeURIComponent(new URL(req.url, 'http://x').pathname);
    if (path === '/') { res.writeHead(200, { 'content-type': 'text/html' }); res.end(PAGE); return; }
    const [rootDir, rel] = path.startsWith('/__stub/') ? [STUB, path.slice('/__stub'.length)] : [BOS, path];
    const file = normalize(join(rootDir, rel));
    if (!file.startsWith(rootDir)) { res.writeHead(403); res.end(); return; }
    try {
      let body = await readFile(file);
      if (/\.m?js$/.test(file)) body = Buffer.from(body.toString('utf8').replace(/(['"])ctox:shared\//g, '$1/shared/'));
      res.writeHead(200, { 'content-type': TYPES[extname(file)] || 'application/octet-stream' }); res.end(body);
    } catch { res.writeHead(404); res.end(); }
  });
  await new Promise((r) => server.listen(0, '127.0.0.1', r));
  const base = `http://127.0.0.1:${server.address().port}/`;
  const browser = await chromium.launch();
  const page = await browser.newPage({ viewport: { width: 1280, height: 760 } });
  const errors = [];
  page.on('pageerror', (e) => errors.push(e.message));
  page.on('console', (m) => { if (m.type() === 'error') console.error('[page]', m.text()); });
  page.on('response', (r) => { if (r.status() >= 400) console.error('[page] HTTP', r.status(), r.url()); });
  page.__errors = errors;
  await page.goto(base);
  // Befehlsrouten/Deklarationen der echten threads-Version (v4/manifests/threads.manifest.json); ohne aktive Version
  // lehnt der Kern seit E26/E27 threads.* beim Schreiben mit unknown_command ab.
  await page.evaluate((routes) => { window.__routes = routes; }, realManifestRoutes(APP));
  // Kernel mit der Ops-Fixture (tests/fixtures/ops-fixture.mjs), Akteur michael (admin).
  await page.evaluate(async (APP) => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    const { opsFixture } = await import(`/modules/${APP}/tests/fixtures/ops-fixture.mjs`);
    window.__mod = await import(`/modules/${APP}/index.js`);
    window.__fx = opsFixture();
    const kernel = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false,
      actor: { id: window.__fx.me, display_name: 'Michael Welsch', role: 'admin' } });
    await kernel.ready;
    for (const [name, docs] of Object.entries(window.__fx.collections)) await kernel.sim.push(name, docs);
    // Nach der business_apps-Zeile der Fixture veröffentlichen (sonst fehlt active_ver).
    // FIXT-R: Eigentümer der Fremd-Collections (echte Manifeste, inkl. ctox-platform) vor der eigenen Version.
    for (const o of window.__routes?._fixture_owners ?? []) if (!(await kernel.sim.inspect('business_apps', o.app))?.data?.active_ver) await kernel.sim.publishAppVersion(o.app, o.ver, o.publish);
    await kernel.sim.publishAppVersion(APP, '0.2.4', { title: 'Threads', manifestExtra: window.__routes });
    await kernel.sim.settle();
    window.__k = kernel;
    window.__api = kernel.appApi(APP, '0.2.4');
    window.__zone = () => kernel.statusZone.forApp(APP).get().filter((e) => e.source === 'app');
    window.__commands = () => window.__api.query('business_commands', { selector: { type: { $exists: true } }, sort: [{ created_at: 'asc' }] }).get();
  }, APP);
  const settle = async () => { await page.evaluate(() => window.__k.sim.settle()); await page.waitForTimeout(120); };
  const close = async () => { await browser.close(); await new Promise((r) => server.close(r)); };
  return { page, settle, close, base };
}

export async function mountMain(page, { restored, counting = true } = {}) {
  await page.evaluate(async ({ restored, counting }) => {
    const { countingApi } = await import('/v4/tools/port/test-api.mjs');
    if (counting && !window.__c) window.__c = countingApi(window.__api);
    const api = counting ? window.__c.api : window.__api;
    window.__app = await window.__mod.mount(document.getElementById('win'), api, restored ? { restored } : {});
  }, { restored, counting });
}

export const rowTexts = (page) => page.$$eval('[data-thread-id]', (rows) => rows.map((r) => ({ id: r.dataset.threadId, text: r.innerText.replace(/\s+/g, ' ').trim() })));
