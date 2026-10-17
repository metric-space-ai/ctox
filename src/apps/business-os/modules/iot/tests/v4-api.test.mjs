// iot gegen den eingefrorenen App-API-4.0-Kern-Stub (createStubKernel) in echtem Chromium.
// Muster: modules/knowledge/tests/v4-api.test.mjs. Stub-Pfad aus CTOX_APP_API_STUB (Pflicht-Pin
// ~/.local/state/shell-v4/stub-pins/4b0d0ac), read-only unter /__stub/ ausgeliefert.
// Lauf: CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/4b0d0ac node --test src/apps/business-os/modules/iot/tests/v4-api.test.mjs
// Playwright: aus PLAYWRIGHT_MODULE, sonst ~/Documents/ctox/src/apps/business-os/node_modules/playwright.
// Screenshots (1280 und 420) landen in tests/shots/.
import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { readFile, mkdir } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { dirname, join, normalize, extname } from 'node:path';
import { homedir } from 'node:os';
import { realManifestRoutes } from '../../../v4/tools/port/real-manifest.mjs';

const APP = 'iot';
// Pin 8265bd7: E48 – der Stub veröffentlicht ctox-platform selbst; Fremdlesen braucht die eigene Deklaration (echtes Manifest).
const STUB = process.env.CTOX_APP_API_STUB || join(homedir(), '.local/state/shell-v4/stub-pins/4b0d0ac');
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
<style>html,body{margin:0} #win{position:relative;overflow:hidden;background:var(--bg)}</style>
</head><body><div id="win" style="width:1280px;height:760px"></div></body></html>`;

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
  page = await browser.newPage({ viewport: { width: 1280, height: 760 } });
  const errors = [];
  page.on('pageerror', (e) => errors.push(e.message));
  page.on('console', (m) => { if (m.type() === 'error') console.error('[page]', m.text()); });
  page.on('response', (r) => { if (r.status() >= 400) console.error('[page] HTTP', r.status(), r.url()); });
  page.__errors = errors;
  await page.goto(base);
  await page.evaluate((routes) => { window.__routes = routes; }, realManifestRoutes(APP));
  await page.evaluate(async (APP) => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    window.__mod = await import(`/modules/${APP}/index.js`);
    const kernel = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false, actor: { id: 'michael', role: 'admin' } });
    await kernel.ready;
    await kernel.sim.push('business_apps', [{ id: 'iot', title: 'IoT' }]);
    // E48: echtes Manifest (foreign_collections business_commands) nach der business_apps-Zeile veröffentlichen.
    for (const o of window.__routes?._fixture_owners ?? []) if (!(await kernel.sim.inspect('business_apps', o.app))?.data?.active_ver) await kernel.sim.publishAppVersion(o.app, o.ver, o.publish);
    // Der Stub lädt das Handler-Artefakt der veröffentlichten Route auch dann, wenn sim.onCommand den Befehl übernimmt
    // (ctox-app-api-stub.mjs runCommand: readArtifactContent vor dem sim-Handler). Für den per sim.onCommand simulierten
    // Webhook-Befehl daher ein Platzhalter-Artefakt; es wirft, falls der Simulator einmal fehlt.
    const SIMULATED = ['ctox.iot.webhook.register'];
    const files = Object.fromEntries(SIMULATED.map((t) => [window.__routes.handlers[t], `export default () => { throw new Error('Platzhalter ${t}: sim.onCommand fehlt'); };`]));
    await kernel.sim.publishAppVersion(APP, '1', { title: 'IoT', files, manifestExtra: window.__routes });
    await kernel.sim.push('iot_realms', [{ id: 'master', realm: 'master', name: 'Werk Nord', data: {}, updated_at_ms: 1 }]);
    await kernel.sim.push('iot_assets', [
      { id: 'ast_halle', realm: 'master', asset_type: 'Building', name: 'Halle 3', parent_id: '', data: {}, updated_at_ms: 3 },
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
const text = (sel) => page.$eval(sel, (el) => el.textContent);


// Shell-Menü-Antwort festlegen (Stub liefert spec.default ?? null) – neue Hülle statt Prototyp (Falle 3).
async function mountWith({ menu = null, dialog = undefined, restored = undefined } = {}) {
  await page.evaluate(async ({ menu, dialog, restored }) => {
    if (window.__app) await window.__app.unmount();
    const base = window.__c.api;
    const shell = Object.freeze({ ...base.shell,
      menu: async (spec) => { window.__menus = [...(window.__menus || []), spec]; return menu; },
      ...(dialog !== undefined ? { dialog: async (spec) => { window.__k.sim.dialogs.push?.(spec); window.__dialogs = [...(window.__dialogs || []), spec]; return dialog; } } : {}),
    });
    const api = Object.freeze({ ...base, shell });
    window.__app = await window.__mod.mount(document.getElementById('win'), api, restored ? { restored } : {});
  }, { menu, dialog, restored });
  await settle();
}
const clickEl = (sel) => page.evaluate((sel) => document.querySelector(sel).click(), sel);

test('mount: Baum zeigt Assets live, gescoptes Root, Titel über api.shell', async () => {
  await page.evaluate(async () => {
    const { countingApi } = await import('/v4/tools/port/test-api.mjs');
    window.__c = countingApi(window.__api);
  });
  await mountWith();
  assert.match(await text('[data-iot-tree]'), /Halle 3/);
  assert.equal(await page.$eval('#win', (el) => el.dataset.app), APP);
  assert.equal(await page.evaluate(() => window.__k.shell.window('iot').state.get().title), 'IoT');
  assert.equal(await page.evaluate(() => window.__c.subscribers('iot_assets')), 1);
  assert.deepEqual(await page.evaluate(() => window.__zone()), []);
  assert.deepEqual(page.__errors, []);
});

test('Live-Änderung erscheint ohne Neuladen, Löschung verschwindet', async () => {
  await page.evaluate(() => window.__k.sim.push('iot_assets', { id: 'ast_live', realm: 'master', asset_type: 'Site', name: 'Live-Standort', parent_id: '', data: {}, updated_at_ms: 9 }));
  await settle();
  assert.match(await text('[data-iot-tree]'), /Live-Standort/);
  await page.evaluate(() => window.__k.sim.push('iot_assets', { id: 'ast_live', _deleted: true }));
  await settle();
  assert.doesNotMatch(await text('[data-iot-tree]'), /Live-Standort/);
});

test('leere Collections rendern keinen Zustandstext und keine Zone', async () => {
  const empty = await page.evaluate(async () => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    const k2 = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false, instanceId: 'empty' });
    await k2.ready;
    const host = document.createElement('div');
    host.style.cssText = 'width:900px;height:500px';
    document.body.append(host);
    const app = await window.__mod.mount(host, k2.appApi('iot', '1'), {});
    await new Promise((r) => setTimeout(r, 300));
    const out = {
      tree: host.querySelector('[data-iot-tree]').textContent.trim(),
      center: host.querySelector('[data-iot-center] .ctox-pane-body')?.textContent.trim() ?? '',
      stateBox: Boolean(host.querySelector('.ctox-syncing, .ctox-empty, [role="status"]')),
      zone: (k2.status || k2.statusZone).forApp('iot').get().map((e) => e.code),
    };
    await app.unmount();
    host.remove();
    await k2.close();
    return out;
  });
  assert.equal(empty.tree, '');
  assert.equal(empty.center, '');
  assert.equal(empty.stateBox, false);
  assert.deepEqual(empty.zone, []);
  await mountWith(); // Modulzustand ist modulweit (V4-PORT.md OFFEN)
});

test('Schreiben: api.put erscheint live und wird vom Core angenommen', async () => {
  const result = await page.evaluate(async () => {
    const h = window.__api.put('iot_assets', { id: 'ast_put', realm: 'master', asset_type: 'Sensor', name: 'Sensor Put', parent_id: '', data: {}, updated_at_ms: 10 });
    await h.local;
    return h.result;
  });
  assert.equal(result.status, 'accepted');
  await settle();
  assert.match(await text('[data-iot-tree]'), /Sensor Put/);
});

test('Anlegen bei leerem Suchergebnis: „Asset anlegen“ zeigt das Formular (Rundgang RUND2)', async () => {
  await mountWith();
  await settle();
  await page.fill('#win [data-pg-search]', 'zzzz-kein-treffer');
  await page.dispatchEvent('#win [data-pg-search]', 'input');
  await settle();
  await page.click('#win [data-action="new"]');
  await settle();
  assert.equal(await page.evaluate(() => Boolean(document.querySelector('#win [data-iot-tree] form[data-form="create"]'))), true);
  await page.fill('#win [data-pg-search]', '');
  await page.dispatchEvent('#win [data-pg-search]', 'input');
  await settle();
});

test('Befehl: Pausieren über api.shell.menu legt business_commands-Dokument an, failed → genau ein Zoneneintrag', async () => {
  await mountWith({ menu: 'pause' });
  await clickEl('[data-sel-kind="asset"][data-asset-id="ast_halle"]');
  await settle();
  assert.match(await text('[data-iot-center]'), /Kühlung hochfahren/);
  await page.evaluate(() => window.__k.sim.failCommand('ctox.iot.widget.pause', 'Agent offline'));
  await clickEl('[data-act="widget-menu"][data-id="wid_1"]');
  await settle();
  const menu = await page.evaluate(() => window.__menus.at(-1));
  assert.ok(menu.items.some((i) => i.id === 'editor'));
  const cmd = await page.evaluate(() => window.__commands('ctox.iot.widget.pause').at(-1));
  assert.equal(cmd.payload.widget_id, 'wid_1');
  assert.equal(cmd.payload.paused, true);
  assert.equal(cmd.record_id, 'wid_1');
  assert.equal(cmd.status, 'failed');
  const zone = await page.evaluate(() => window.__zone());
  assert.equal(zone.length, 1);
  assert.equal(zone[0].code, 'iot.command.ctox.iot.widget.pause');
  assert.match(zone[0].text, /Agent offline/);
  assert.equal(await page.evaluate(() => document.querySelectorAll('body > .ctox-context-menu, body > .ctox-modal').length), 0, 'nichts auf Dokumentebene');
});

test('Löschen fragt über api.shell.dialog: ohne Ja kein Befehl', async () => {
  await mountWith({ menu: 'delete' });
  const before = await page.evaluate(() => window.__commands('ctox.iot.widget.delete').length);
  await clickEl('[data-act="widget-menu"][data-id="wid_1"]');
  await settle();
  assert.equal(await page.evaluate(() => window.__commands('ctox.iot.widget.delete').length), before);
  assert.ok(await page.evaluate(() => window.__k.sim.dialogs.length) >= 1);
});

test('Webhook: Ergebnis des Befehls erscheint als Dialog im Fenster (nie Warten im Klick)', async () => {
  await page.evaluate(() => window.__k.sim.onCommand('ctox.iot.webhook.register', () => ({ ingest_path: '/ctox/iot/webhook/wh_1', token: 'tok-123' })));
  await mountWith({ dialog: true });
  await clickEl('[data-act="toggle"][data-asset-id="ast_halle"]');
  await settle();
  await clickEl('[data-act="toggle"][data-asset-id="ast_t1"]');
  await settle();
  await clickEl('[data-sel-kind="signal"][data-asset-id="ast_t1"]');
  await settle();
  await clickEl('[data-act="signal-webhook"]');
  await settle();
  const dlg = await page.evaluate(() => (window.__dialogs || []).at(-1));
  assert.equal(dlg.kind, 'alert');
  assert.match(dlg.text, /tok-123/);
  assert.match(dlg.text, /\/ctox\/iot\/webhook\/wh_1/);
});

test('snapshot/restored: offener Editor-Entwurf, Auswahl und Filter überleben den Remount', async () => {
  await mountWith({ menu: 'editor' });
  await clickEl('[data-sel-kind="asset"][data-asset-id="ast_halle"]');
  await settle();
  await clickEl('[data-act="widget-menu"][data-id="wid_1"]');
  await settle();
  const inside = await page.evaluate(() => {
    const ed = document.querySelector('[data-iot-editor]');
    const win = document.getElementById('win').getBoundingClientRect();
    const r = ed.getBoundingClientRect();
    return { contained: document.getElementById('win').contains(ed), fits: r.left >= win.left && r.right <= win.right + 0.5 && r.top >= win.top && r.bottom <= win.bottom + 0.5 };
  });
  assert.deepEqual(inside, { contained: true, fits: true });
  await page.fill('[data-iot-editor] [data-ed-field="cond_text"]', 'halb fertige Bedingung');
  const snap = await page.evaluate(() => structuredClone(window.__app.snapshot()));
  assert.equal(snap.editor.widgetId, 'wid_1');
  assert.equal(snap.editor.fields.cond_text, 'halb fertige Bedingung');
  assert.equal(snap.selection.assetId, 'ast_halle');
  assert.ok(JSON.stringify(snap).length < 1024 * 1024);
  await mountWith({ restored: snap });
  const after = await page.evaluate(() => ({
    cond: document.querySelector('[data-iot-editor] [data-ed-field="cond_text"]')?.value,
    selected: window.__app.snapshot().selection.assetId,
  }));
  assert.equal(after.cond, 'halb fertige Bedingung');
  assert.equal(after.selected, 'ast_halle');
  assert.equal(await page.evaluate(() => window.__c.subscribers('iot_widgets')), 1, 'alte Abos abgemeldet');
  await page.evaluate(() => document.querySelector('[data-iot-editor] [data-ed="close"]').click());
  assert.deepEqual(page.__errors, []);
});

test('Screenshots 1280 und 420 gegen den Stub', async () => {
  await mkdir(join(HERE, 'shots'), { recursive: true });
  await settle();
  await page.screenshot({ path: join(HERE, 'shots', `${APP}-1280.png`) });
  await page.setViewportSize({ width: 420, height: 860 });
  await page.$eval('#win', (el) => { el.style.width = '420px'; el.style.height = '860px'; });
  await page.waitForTimeout(250);
  await page.screenshot({ path: join(HERE, 'shots', `${APP}-420.png`) });
  const overflow = await page.$eval('#win', (el) => el.scrollWidth - el.clientWidth);
  assert.ok(overflow <= 0, `kein horizontaler Überlauf bei 420 (scrollWidth-clientWidth=${overflow})`);
  await page.setViewportSize({ width: 1280, height: 760 });
  await page.$eval('#win', (el) => { el.style.width = '1280px'; el.style.height = '760px'; });
});

test('unmount meldet alle Abos ab und leert root', async () => {
  const res = await page.evaluate(async () => {
    await window.__app.unmount();
    await window.__k.sim.push('iot_assets', { id: 'ast_late', realm: 'master', asset_type: 'Site', name: 'Late', data: {}, updated_at_ms: 1 });
    await window.__k.sim.settle();
    return {
      subs: ['iot_realms', 'iot_assets', 'iot_attributes', 'iot_datapoints', 'iot_alarms', 'iot_dashboards', 'iot_widgets'].map((c) => window.__c.subscribers(c)),
      children: document.getElementById('win').children.length,
    };
  });
  assert.deepEqual(res.subs, [0, 0, 0, 0, 0, 0, 0]);
  assert.equal(res.children, 0);
  assert.deepEqual(page.__errors, []);
});
