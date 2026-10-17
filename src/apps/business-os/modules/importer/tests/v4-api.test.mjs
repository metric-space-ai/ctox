// importer gegen den eingefrorenen App-API-4.0-Kern-Stub (createStubKernel) in echtem Chromium.
// Muster: modules/knowledge/tests/v4-api.test.mjs. Stub-Pfad aus CTOX_APP_API_STUB (Pflicht-Pin
// ~/.local/state/shell-v4/stub-pins/64acc2c), read-only unter /__stub/ ausgeliefert.
// Lauf: CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/64acc2c node --test src/apps/business-os/modules/importer/tests/v4-api.test.mjs
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

const APP = 'importer';
const STUB = process.env.CTOX_APP_API_STUB || join(homedir(), '.local/state/shell-v4/stub-pins/64acc2c');
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
  // FIXT-R: echte Routen/Rechte/Fremd-Collections von importer (v4/manifests/importer.manifest.json).
  await page.evaluate((routes) => { window.__routes = routes; }, realManifestRoutes(APP));
  await page.evaluate(async (APP) => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    window.__mod = await import(`/modules/${APP}/index.js`);
    const kernel = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false, actor: { id: 'michael', role: 'admin' } });
    await kernel.ready;
    await kernel.sim.push('business_apps', [{ id: 'importer', title: 'App Importer' }, { id: 'radio-atlas', title: 'Radio Atlas' }]);
    await kernel.sim.settle();
    // FIXT-R: Eigentümer der Fremd-Collections (ctox-platform, desktop) vor der eigenen Version, diese nach der
    // business_apps-Zeile.
    for (const o of window.__routes?._fixture_owners ?? []) if (!(await kernel.sim.inspect('business_apps', o.app))?.data?.active_ver) await kernel.sim.publishAppVersion(o.app, o.ver, o.publish);
    await kernel.sim.publishAppVersion(APP, '1', { title: 'App Importer', manifestExtra: window.__routes });
    window.__k = kernel;
    window.__api = kernel.appApi(APP, '1');
    window.__zone = () => (kernel.status || kernel.statusZone).forApp(APP).get().filter((e) => e.source === 'app');
    window.__commands = (type) => window.__api.query('business_commands', { selector: { app_id: 'importer', ...(type ? { type } : {}) }, sort: [{ created_at: 'asc' }] }).get();
  }, APP);
});

after(async () => { await browser?.close(); await new Promise((r) => server?.close(r)); });

const settle = async () => { await page.evaluate(() => window.__k.sim.settle()); await page.waitForTimeout(220); };
const text = (sel) => page.$eval(sel, (el) => el.textContent);


async function remount(restored) {
  await page.evaluate(async (restored) => {
    if (window.__app) await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, restored ? { restored } : {});
  }, restored ?? null);
  await settle();
}
const visibleStage = () => page.evaluate(() => document.querySelector('[data-importer-root]').dataset.impStage);

test('mount: Quellenansicht, gescoptes Root, Titel über api.shell, keine Zustandstexte, keine Zone', async () => {
  await page.evaluate(async () => {
    const { countingApi } = await import('/v4/tools/port/test-api.mjs');
    window.__c = countingApi(window.__api);
  });
  await remount();
  assert.equal(await visibleStage(), 'source');
  assert.equal(await page.$eval('#win', (el) => el.dataset.app), APP);
  assert.equal(await page.evaluate(() => window.__k.shell.window('importer').state.get().title), 'App Importer');
  assert.equal(await page.evaluate(() => Boolean(document.querySelector('.ctox-syncing, [data-imp-notice], [role="status"]'))), false);
  assert.deepEqual(await page.evaluate(() => window.__zone()), []);
  assert.deepEqual(page.__errors, []);
});

test('ungültige GitHub-URL → genau ein Zoneneintrag, kein Befehl', async () => {
  await page.fill('[data-imp-github-url]', 'https://gitlab.com/x/y');
  await page.click('[data-imp-github-btn]');
  await settle();
  const zone = await page.evaluate(() => window.__zone());
  assert.equal(zone.length, 1);
  assert.equal(zone[0].code, 'importer.github.invalid');
  assert.equal(await page.evaluate(() => window.__commands().length), 0);
});

test('Befehl ohne Warten: GitHub-Import → app.create, Fortschritt live, Ergebnis live, Öffnen navigiert ohne Neuladen', async () => {
  await page.evaluate(() => window.__k.sim.onCommand('ctox.business_os.app.create', () => ({ live: true, validation_status: 'passed', smoke_status: 'passed', source_revision: 'abc123', asset_revision: 'a1', catalog_revision: 'c1' })));
  await page.fill('[data-imp-github-url]', 'https://github.com/AksharP5/radio-atlas');
  await page.click('[data-imp-github-btn]');
  assert.equal(await visibleStage(), 'progress', 'sofort im Fortschritt, ohne auf Annahme zu warten');
  assert.deepEqual(await page.evaluate(() => window.__zone()), [], 'Hinweis der ungültigen URL geräumt');
  await settle();
  const cmd = await page.evaluate(() => window.__commands('ctox.business_os.app.create').at(-1));
  assert.equal(cmd.record_id, 'radio-atlas');
  assert.equal(cmd.payload.module_id, 'radio-atlas');
  assert.deepEqual(cmd.payload.import_source, { kind: 'github', repository_url: 'https://github.com/AksharP5/radio-atlas', ref: 'HEAD' });
  assert.deepEqual(cmd.payload.required_skills, ['business-os-app-module-development']);
  assert.equal(cmd.payload.client_context.actor.id, 'michael');
  assert.equal(await visibleStage(), 'done');
  assert.match(await text('[data-imp-done]'), /abc123/);
  assert.equal(await page.$eval('[data-imp-open]', (el) => el.hidden), false);
  await page.click('[data-imp-open]');
  const nav = await page.evaluate(() => window.__k.sim.navigations.at(-1));
  assert.equal(nav.to || nav.target || nav.app_id, 'radio-atlas');
});

test('failed: Projektion zeigt das Jobergebnis, Quelle wieder wählbar', async () => {
  await page.click('[data-imp-reset]');
  await page.evaluate(() => window.__k.sim.failCommand('ctox.business_os.app.create', 'Validierung rot'));
  await page.fill('[data-imp-github-url]', 'https://github.com/owner/broken-app');
  await page.click('[data-imp-github-btn]');
  await settle();
  assert.equal(await visibleStage(), 'done');
  assert.match(await text('[data-imp-done]'), /Validierung rot/);
  assert.equal(await page.$eval('[data-imp-open]', (el) => el.hidden), true);
  await page.click('[data-imp-reset]');
  assert.equal(await visibleStage(), 'source');
});

test('Schreiben: Datei-Import legt Snapshot-Dokumente per api.put an (vom Core angenommen) und hängt sie als Abhängigkeiten an', async () => {
  await page.evaluate(() => window.__k.sim.onCommand('ctox.business_os.app.create', () => ({ live: false })));
  await page.setInputFiles('[data-imp-pick-files-input]', { name: 'radio.html', mimeType: 'text/html', buffer: Buffer.from('<!doctype html><title>Retro Radio</title><p>x</p>') });
  await page.waitForFunction(() => window.__api.query('business_commands', { selector: { type: 'ctox.business_os.app.create' } }).get().length >= 3);
  await settle();
  const res = await page.evaluate(async () => {
    const files = window.__api.query('desktop_files', {}).get();
    const chunks = window.__api.query('desktop_file_chunks', {}).get();
    const cmd = window.__commands('ctox.business_os.app.create').at(-1);
    return { files, chunks, cmd, outbox: (await window.__k.sim.outbox()).length };
  });
  assert.equal(res.files.filter((f) => f.kind === 'folder').length, 1);
  const file = res.files.find((f) => f.kind === 'file');
  assert.equal(file.name, 'radio.html');
  assert.equal(res.chunks.length, 1);
  assert.equal(res.chunks[0].file_id, file.id);
  assert.equal(res.outbox, 0, 'alle Schreibvorgänge bestätigt');
  assert.equal(res.cmd.record_id, 'retro-radio');
  assert.equal(res.cmd.payload.import_source.profile, 'standalone-html');
  assert.deepEqual(res.cmd.payload.dependencies.map((d) => d.record_id), [file.id]);
  assert.deepEqual(await page.evaluate(() => window.__zone().map((e) => e.code)), [], 'Fortschrittseintrag geräumt');
  await page.click('[data-imp-reset]');
});

test('snapshot/restored: offene URL-Eingabe und verfolgter Auftrag überleben den Remount', async () => {
  await page.fill('[data-imp-github-url]', 'https://github.com/halb/fertig');
  const snap = await page.evaluate(() => structuredClone(window.__app.snapshot()));
  assert.equal(snap.githubUrl, 'https://github.com/halb/fertig');
  await remount(snap);
  assert.equal(await page.$eval('[data-imp-github-url]', (el) => el.value), 'https://github.com/halb/fertig');
  // Laufender Auftrag: Remount folgt ihm weiter.
  const cmdId = await page.evaluate(() => window.__commands('ctox.business_os.app.create').at(-2).id);
  await remount({ v: 1, githubUrl: '', category: 'entertainment', commandId: cmdId, moduleId: 'broken-app' });
  assert.equal(await visibleStage(), 'done');
  assert.match(await text('[data-imp-done]'), /Validierung rot/);
  assert.deepEqual(page.__errors, []);
});

test('Screenshots 1280 und 420 gegen den Stub', async () => {
  await remount();
  await mkdir(join(HERE, 'shots'), { recursive: true });
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
  const cmdId = await page.evaluate(() => window.__commands('ctox.business_os.app.create').at(-1).id);
  await remount({ v: 1, commandId: cmdId, moduleId: 'retro-radio' });
  const res = await page.evaluate(async () => {
    await window.__app.unmount();
    return { subs: window.__c.subscribers('business_commands'), children: document.getElementById('win').children.length };
  });
  assert.equal(res.subs, 0);
  assert.equal(res.children, 0);
  assert.deepEqual(page.__errors, []);
});
