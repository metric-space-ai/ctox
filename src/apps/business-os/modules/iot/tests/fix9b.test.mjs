// FIX9b (Shell v4): iot-Detailtitel im schmalen Fenster ohne Ellipse (G4), nur App-CSS.
// Lauf: CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/a8765b9 \
//       node --import ./v4/tools/node/ctox-shared.mjs --test --test-force-exit modules/iot/tests/fix9b.test.mjs
import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { readFile, mkdir } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { dirname, join, normalize, extname } from 'node:path';
import { homedir } from 'node:os';

const APP = 'iot';
const STUB = process.env.CTOX_APP_API_STUB || join(homedir(), '.local/state/shell-v4/stub-pins/a8765b9');
const HERE = dirname(fileURLToPath(import.meta.url));
const BOS = normalize(join(HERE, '../../..'));
const require = createRequire(import.meta.url);
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || join(homedir(), 'Documents/ctox/src/apps/business-os/node_modules/playwright'));
const TYPES = { '.js': 'text/javascript', '.mjs': 'text/javascript', '.css': 'text/css', '.html': 'text/html', '.json': 'application/json', '.svg': 'image/svg+xml' };

const PAGE = `<!doctype html><html lang="de"><head><meta charset="utf-8">
<link rel="stylesheet" href="/ui-contract/v1/workjet-ui-contract.css">
<link rel="stylesheet" href="/app.css">
<link rel="stylesheet" href="/shared/base.css">
<link rel="stylesheet" href="/modules/${APP}/index.css">
<style>html,body{margin:0} #win{position:relative;overflow:hidden;background:var(--bg);container:business-app-window / inline-size}</style>
</head><body><div id="win" style="width:420px;height:860px"></div></body></html>`;

let server, browser, page, base;

before(async () => {
  server = createServer(async (req, res) => {
    const path = decodeURIComponent(new URL(req.url, 'http://x').pathname);
    if (path === '/') { res.writeHead(200, { 'content-type': 'text/html' }); res.end(PAGE); return; }
    const [rootDir, rel] = path.startsWith('/__stub/') ? [STUB, path.slice('/__stub'.length)] : [BOS, path];
    const file = normalize(join(rootDir, rel));
    if (!file.startsWith(rootDir)) { res.writeHead(403); res.end(); return; }
    try {
      let body = await readFile(file);
      // Wie der Service Worker (§16.15): ctox:shared/x.js → Shell-Pfad (Import-Map greift für ctox: nicht).
      if (/\.m?js$/.test(file)) body = Buffer.from(body.toString('utf8').replace(/(['"])ctox:shared\//g, '$1/shared/'));
      res.writeHead(200, { 'content-type': TYPES[extname(file)] || 'application/octet-stream' }); res.end(body);
    } catch { res.writeHead(404); res.end(); }
  });
  await new Promise((r) => server.listen(0, '127.0.0.1', r));
  base = `http://127.0.0.1:${server.address().port}/`;
  browser = await chromium.launch();
  page = await browser.newPage({ viewport: { width: 420, height: 860 } });
  const errors = [];
  page.on('pageerror', (e) => errors.push(e.message));
  page.on('console', (m) => { if (m.type() === 'error') console.error('[page]', m.text()); });
  page.on('response', (r) => { if (r.status() >= 400) console.error('[page] HTTP', r.status(), r.url()); });
  page.__errors = errors;
  await page.goto(base);
  await page.evaluate(async (APP) => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    window.__mod = await import(`/modules/${APP}/index.js`);
    const kernel = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false, actor: { id: 'michael', role: 'admin' } });
    await kernel.ready;
    await kernel.sim.push('business_apps', [{ id: 'iot', title: 'IoT' }]);
    await kernel.sim.push('iot_realms', [{ id: 'master', realm: 'master', name: 'Werk Nord', data: {}, updated_at_ms: 1 }]);
    await kernel.sim.push('iot_assets', [
      { id: 'ast_halle', realm: 'master', asset_type: 'Building', name: 'Halle 3 Nordflügel Klimaanlage Serverraum', parent_id: '', data: {}, updated_at_ms: 3 },
      { id: 'ast_t1', realm: 'master', asset_type: 'Thermostat', name: 'Thermostat Serverraum mit langem Namen', parent_id: 'ast_halle', data: {}, updated_at_ms: 2 },
    ]);
    await kernel.sim.push('iot_attributes', [
      { id: 'ast_t1::temperature', realm: 'master', asset_id: 'ast_t1', attribute_name: 'temperature', value: 23.5, value_type: 'Number', data: {}, updated_at_ms: 2 },
    ]);
    await kernel.sim.push('iot_datapoints', [
      { id: 'dp_1', realm: 'master', asset_id: 'ast_t1', attribute_name: 'temperature', data: { points: [] }, points: [{ t: 1, v: 21 }, { t: 2, v: 22.4 }, { t: 3, v: 23.5 }], updated_at_ms: 2 },
    ]);
    await kernel.sim.push('iot_dashboards', [{ id: 'dash_1', realm: 'master', name: 'Mein Dashboard', updated_at_ms: 1 }]);
    await kernel.sim.push('iot_widgets', [
      { id: 'wid_1', dashboard_id: 'dash_1', realm: 'master', signal_ref: 'ast_t1::temperature', cond_text: 'wenn es länger als 5 Min über 30°C ist', action_prompt: 'Kühlung hochfahren und melden', trigger_status: 'armed', trigger_code: 'x > 30', sort_index: 0, updated_at_ms: 4 },
    ]);
    await kernel.sim.settle();
    window.__k = kernel;
    window.__api = kernel.appApi(APP, '1');
    window.__zone = () => (kernel.status || kernel.statusZone).forApp(APP).get().filter((e) => e.source === 'app');
    window.__commands = (type) => window.__api.query('business_commands', { selector: { app_id: 'iot', ...(type ? { type } : {}) }, sort: [{ created_at: 'asc' }] }).get();
  }, APP);
});

after(async () => { await browser?.close(); await new Promise((r) => server?.close(r)); });

const settle = async () => { await page.evaluate(() => window.__k.sim.settle()); await page.waitForTimeout(220); };
const titleBox = () => page.$eval('.iot-center > .ctox-pane-header .ctox-pane-title', (el) => {
  const cs = getComputedStyle(el);
  return { text: el.textContent, sw: el.scrollWidth, cw: el.clientWidth, sh: el.scrollHeight, ch: el.clientHeight, ellipsis: cs.textOverflow, lines: Math.round(el.getBoundingClientRect().height / parseFloat(cs.lineHeight)) };
});

test('FIX9b (G4): Detailtitel bei 420 ohne Ellipse – bricht höchstens dreizeilig um; bei 1280 einzeilig', async () => {
  await page.evaluate(async () => { window.__app = await window.__mod.mount(document.getElementById('win'), window.__api, {}); });
  await settle();
  const narrow = await titleBox();
  assert.match(narrow.text, /Halle 3 Nordflügel Klimaanlage Serverraum/);
  assert.notEqual(narrow.ellipsis, 'ellipsis', JSON.stringify(narrow));
  assert.ok(narrow.sw <= narrow.cw + 1, `kein seitlicher Abschnitt ${JSON.stringify(narrow)}`);
  assert.ok(narrow.sh <= narrow.ch + 1, `Titel vollständig sichtbar ${JSON.stringify(narrow)}`);
  assert.ok(narrow.lines <= 3, JSON.stringify(narrow));
  await page.setViewportSize({ width: 1280, height: 760 });
  await page.$eval('#win', (el) => { el.style.width = '1280px'; el.style.height = '760px'; });
  await settle();
  const wide = await titleBox();
  assert.equal(wide.lines, 1, JSON.stringify(wide));
  assert.deepEqual(page.__errors, []);
});
