// app-store gegen den verbindlichen App-API-4.0-Stub des Kerns (createStubKernel, gepinnt über CTOX_APP_API_STUB,
// read-only unter /__stub/ ausgeliefert) in echtem Chromium. Muster: modules/knowledge/tests/v4-api.test.mjs.
// Lauf: CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/68b83a3 node --test src/apps/business-os/modules/app-store/tests/v4-api.test.mjs
import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { dirname, join, normalize, extname } from 'node:path';
import { homedir } from 'node:os';

const STUB = process.env.CTOX_APP_API_STUB || join(homedir(), '.local/state/shell-v4/stub-pins/68b83a3');
const HERE = dirname(fileURLToPath(import.meta.url));
const BOS = normalize(join(HERE, '../../..'));
const SHOTS = join(HERE, 'shots');
const require = createRequire(import.meta.url);
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || join(homedir(), 'Documents/ctox/src/apps/business-os/node_modules/playwright'));
const TYPES = { '.js': 'text/javascript', '.mjs': 'text/javascript', '.css': 'text/css', '.html': 'text/html', '.json': 'application/json', '.svg': 'image/svg+xml', '.png': 'image/png' };

const PAGE = `<!doctype html><html><head><meta charset="utf-8">
<link rel="stylesheet" href="/ui-contract/v1/workjet-ui-contract.css">
<link rel="stylesheet" href="/app.css">
<link rel="stylesheet" href="/shared/base.css">
<link rel="stylesheet" href="/themes/ctox-desktop-shell.css">
<link rel="stylesheet" href="/modules/app-store/index.css">
<style>html,body{margin:0;height:100%;overflow:hidden} #win{position:relative;width:1280px;height:760px;overflow:hidden;display:flex;flex-direction:column}</style>
</head><body><div id="win"></div></body></html>`;

const manifest = (version, extra = {}) => ({ version, title: extra.title, layout: { category: extra.category || 'Business' }, collections: extra.collections || {}, files: { 'index.js': 'h' }, schema_rev: extra.schemaRev ?? 2, provenance: { base_release: 'r1', base_hash: `h-${version}`, locally_modified: extra.modified === true } });
const APPS = [
  { id: 'customers', title: 'Kunden', active_ver: '3', migrated_at_seq: 10 },
  { id: 'notes', title: 'Notizen', active_ver: '1', migrated_at_seq: 5 },
  { id: 'documents', title: 'Dokumente', active_ver: '2', migrated_at_seq: 7 },
];
const VERSIONS = [
  { id: 'customers@2', app_id: 'customers', app_ver: '2', manifest: manifest('1.0.0', { category: 'Sales', schemaRev: 1 }), migrated_at_seq: 8 },
  { id: 'customers@3', app_id: 'customers', app_ver: '3', manifest: manifest('1.0.1', { category: 'Sales', collections: { customer_accounts: {} } }), migrated_at_seq: 10 },
  { id: 'customers@4', app_id: 'customers', app_ver: '4', manifest: manifest('1.0.2', { category: 'Sales' }), migrated_at_seq: 20 },
  { id: 'notes@1', app_id: 'notes', app_ver: '1', manifest: manifest('2.0.0'), migrated_at_seq: 5 },
  { id: 'documents@2', app_id: 'documents', app_ver: '2', manifest: manifest('1.4.0', { modified: true }), migrated_at_seq: 7 },
  { id: 'documents@3', app_id: 'documents', app_ver: '3', manifest: manifest('1.5.0'), migrated_at_seq: 21 },
];
// §16.29 app_catalog (Kern-Simulation, tests/catalog-core-sim.mjs): customers hat main 4 > aktiv 3 (Update); notes nur
// instance 2 (kein Update); documents main 3 > aktiv 2, aber lokal angepasst (gesondert markiert, nicht im Band);
// kanban nicht installiert (Marketplace); starter-crm ist eine Vorlage.
const CATALOG = [
  { app_id: 'customers', app_ver: '4', title: 'Kunden', layout: { category: 'Sales' }, source: 'main', published_at: '2026-10-14T10:00:00Z' },
  { app_id: 'customers', app_ver: '3', title: 'Kunden', layout: { category: 'Sales' }, source: 'main', published_at: '2026-10-01T10:00:00Z' },
  { app_id: 'notes', app_ver: '1', title: 'Notizen', layout: { category: 'Business' }, source: 'main', published_at: '2026-09-01T10:00:00Z' },
  { app_id: 'notes', app_ver: '2', title: 'Notizen', layout: { category: 'Business' }, source: 'instance', published_at: '2026-10-10T10:00:00Z' },
  { app_id: 'documents', app_ver: '3', title: 'Dokumente', layout: { category: 'Business' }, source: 'main', published_at: '2026-10-12T10:00:00Z' },
  { app_id: 'kanban', app_ver: '1', title: 'Kanban', layout: { category: 'Produktivität', description: 'Aufgaben in Spalten' }, source: 'main', published_at: '2026-10-13T10:00:00Z' },
  { app_id: 'starter-crm', app_ver: '1', title: 'Starter CRM', layout: { category: 'Vorlagen', template: true }, source: 'main', published_at: '2026-10-13T10:00:00Z' },
];

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
      // Wie der Service Worker (§16.15): ctox:shared/x.js → Shell-Pfad (OFFEN-F).
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
  page.__errors = errors;
  await page.goto(base);
  await page.evaluate(async ({ apps, versions, catalog }) => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    const { createCatalogCore } = await import('/modules/app-store/tests/catalog-core-sim.mjs');
    window.__mod = await import('/modules/app-store/index.js');
    const kernel = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false, actor: { role: 'admin', permissions: ['apps.source.view', 'apps.source.edit'] } });
    await kernel.ready;
    await kernel.sim.push('business_apps', [{ id: 'app-store', title: 'App Store' }, { id: 'creator', title: 'Creator' }]);
    await kernel.sim.push('business_app_versions', versions);
    await kernel.sim.push('business_apps', apps);
    await kernel.sim.settle();
    const core = createCatalogCore(kernel, kernel.appApi('app-store', '1'));
    core.setCatalog(catalog);
    const api = core.api;
    window.__core = core;
    window.__k = kernel;
    window.__api = api;
    window.__zone = () => kernel.statusZone.forApp('app-store').get().filter((e) => e.source === 'app');
    window.__commands = () => api.query('business_commands', { sort: [{ created_at: 'asc' }] }).get();
  }, { apps: APPS, versions: VERSIONS, catalog: CATALOG });
});

after(async () => { await browser?.close(); server?.closeAllConnections?.(); await new Promise((r) => server?.close(r)); });

const settle = async () => { await page.evaluate(() => window.__k.sim.settle()); await page.waitForTimeout(150); };
const click = (sel) => page.evaluate((s) => { const el = document.querySelector(s); if (!el) throw new Error(`fehlt: ${s}`); el.click(); }, sel);
const gridText = () => page.$eval('[data-apps-grid]', (el) => el.textContent);
const selectApp = async (id) => {
  await click(`[data-apps-grid] [data-app-id="${id}"]`);
  await settle();
};

test('mount: Katalog aus business_apps/business_app_versions, nur api.*, Titel über api.shell', async () => {
  await page.evaluate(async () => {
    const { countingApi } = await import('/v4/tools/port/test-api.mjs');
    window.__c = countingApi(window.__api);
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, {});
  });
  await settle();
  assert.equal(await page.$eval('#win', (el) => el.dataset.app), 'app-store');
  assert.equal(await page.evaluate(() => window.__k.shell.window('app-store').state.get().title), 'App Store');
  assert.equal(await page.evaluate(() => window.__c.subscribers('business_apps')), 1);
  assert.equal(await page.evaluate(() => window.__c.subscribers('business_app_versions')), 1);
  assert.equal(await page.evaluate(() => window.__c.subscribers('app_catalog')), 1);
  // Installierte Apps, dichte Liste (die WebGL-Regalansicht ist nicht Gegenstand dieses Tests).
  await click('[data-scope="installed"]');
  await click('[data-store-view-toggle]');
  await settle();
  const text = await gridText();
  assert.match(text, /Kunden/);
  assert.match(text, /Notizen/);
  assert.match(text, /Dokumente/);
  assert.equal(await page.$('[data-store-message], [data-loading-spinner], .store-empty-state'), null);
  assert.deepEqual(page.__errors, []);
});

test('§9.6: Hinweis „lokal angepasst“ steht in der Zone, nicht in der Liste', async () => {
  const zone = await page.evaluate(() => window.__zone());
  assert.equal(zone.length, 1);
  assert.equal(zone[0].code, 'app-store.update.locally_modified.documents');
  assert.equal(zone[0].level, 'info');
  assert.match(zone[0].text, /Dokumente: Update verfügbar, App lokal angepasst/);
  assert.doesNotMatch(await page.$eval('#win', (el) => el.textContent), /lokal angepasst/);
  // Lokal angepasste App: Update nur nach ausdrücklicher Bestätigung (v3: confirm, mode 'discard'); ohne Ja kein Befehl.
  await selectApp('documents');
  const beforeCmds = await page.evaluate(() => window.__commands().length);
  const beforeDialogs = await page.evaluate(() => window.__k.sim.dialogs.length);
  await click('[data-detail-actions] [data-card-action="update"]');
  await settle();
  assert.equal(await page.evaluate(() => window.__commands().length), beforeCmds);
  const dialog = await page.evaluate(() => window.__k.sim.dialogs.at(-1));
  assert.equal(await page.evaluate(() => window.__k.sim.dialogs.length), beforeDialogs + 1);
  assert.match(JSON.stringify(dialog), /lokale Änderungen/);
  // … die unveränderte direkt.
  await selectApp('customers');
  assert.ok(await page.$('[data-detail-actions] [data-card-action="update"]'));
});

test('§16.29 Updates: nur höhere main-Version und nicht angepasst; angepasste App gesondert markiert', async () => {
  await click('[data-scope="installed"]');
  await click('[data-pg-band="updates"]');
  await settle();
  const updates = await gridText();
  assert.match(updates, /Kunden/, 'customers: main 4 > aktiv 3');
  assert.doesNotMatch(updates, /Notizen/, 'notes: nur instance 2 – kein Update');
  assert.doesNotMatch(updates, /Dokumente/, 'documents: lokal angepasst – nicht im Band');
  assert.doesNotMatch(updates, /App Store|Creator/, 'ohne Katalogeintrag kein Update (v3-Fehler: jede App stand im Band)');
  assert.equal(await page.$eval('[data-pg-count="updates"]', (el) => el.textContent.trim()), '(1)');
  await click('[data-pg-band="catalog"]');
  // Karten (DOM, reduzierte Bewegung) zeigen den Fachbegriff am Eintrag – kein Banner, kein Zustandstext.
  await page.emulateMedia({ reducedMotion: 'reduce' });
  await click('[data-store-view-toggle]');
  await settle();
  const marks = await page.$$eval('[data-apps-grid] [data-update-customized]', (els) => els.map((el) => [el.closest('[data-app-id]').dataset.appId, el.dataset.updateCustomized, el.textContent.trim()]));
  assert.deepEqual(marks, [['documents', '3', 'Update 3 · angepasst']]);
  assert.equal(await page.$('.store-message, [data-store-message], [role="alert"]'), null);
  await click('[data-store-view-toggle]');
  await page.emulateMedia({ reducedMotion: 'no-preference' });
  await settle();
});

test('Live-Änderung: neue App erscheint, gelöschte verschwindet, Zone räumt sich', async () => {
  // migrated_at_seq ≤ Checkpoint, sonst wartet die Stub-Aktivierung auf data-ready (README Nr. 17) und settle läuft ab.
  await page.evaluate(async () => {
    const cp = window.__k.sim.checkpoint;
    await window.__k.sim.push('business_app_versions', [{ id: 'tickets@1', app_id: 'tickets', app_ver: '1', manifest: { version: '0.9.0', layout: { category: 'Support' }, provenance: {} }, migrated_at_seq: cp }]);
    await window.__k.sim.push('business_apps', [{ id: 'tickets', title: 'Tickets Live', active_ver: '1', migrated_at_seq: cp }]);
  });
  await settle();
  assert.match(await gridText(), /Tickets Live/);
  await page.evaluate(() => window.__k.sim.push('business_apps', { id: 'tickets', _deleted: true }));
  await settle();
  assert.doesNotMatch(await gridText(), /Tickets Live/);
  // documents wird auf die neue Version gehoben → kein Update mehr → Zoneneintrag weg.
  await page.evaluate(async () => {
    const cp = window.__k.sim.checkpoint;
    const v3 = window.__api.get('business_app_versions', 'documents@3').get();
    await window.__k.sim.push('business_app_versions', [{ ...v3, migrated_at_seq: cp }]);
    await window.__k.sim.push('business_apps', [{ id: 'documents', title: 'Dokumente', active_ver: '3', migrated_at_seq: cp }]);
  });
  await settle();
  assert.deepEqual(await page.evaluate(() => window.__zone().map((e) => e.code)), []);
});

test('Plattform-Befehl: Aktualisieren → ctox.app.update {app_id, base_app_ver, release_ver} ohne Warten; failed → genau ein Zoneneintrag', async () => {
  await page.evaluate(() => window.__k.sim.failCommand('ctox.app.update', 'kaputt'));
  await selectApp('customers');
  await click('[data-detail-actions] [data-card-action="update"]');
  const running = await page.evaluate(() => Boolean(document.querySelector('[data-detail-actions] .is-progress')));
  assert.equal(running, true, 'Fortschrittsknopf sofort, ohne auf den Core zu warten');
  await settle();
  const cmd = await page.evaluate(() => window.__commands().at(-1));
  assert.equal(cmd.type, 'ctox.app.update');
  assert.equal(cmd.record_id, 'customers');
  // §16.29: genau der Vertrag, keine v3-Felder (module_id, mode, expected_baseline_sha256).
  assert.deepEqual(cmd.payload, { app_id: 'customers', base_app_ver: '3', release_ver: '4' });
  assert.equal(cmd.status, 'failed');
  const zone = await page.evaluate(() => window.__zone());
  assert.equal(zone.length, 1);
  assert.equal(zone[0].code, 'app-store.command.ctox.app.update');
  assert.match(zone[0].text, /kaputt/);
  assert.equal(zone[0].ref, cmd.id);
  assert.equal(await page.$('[data-detail-actions] .is-progress'), null, 'Fortschritt endet mit failed');
});

test('Entfernen fragt über api.shell.dialog; ohne Ja kein Befehl, mit Ja ctox.app.remove → App archiviert', async () => {
  const before = await page.evaluate(() => window.__commands().length);
  await selectApp('notes');
  await click('[data-detail-actions] [data-card-action="uninstall"]');
  await settle();
  assert.equal(await page.evaluate(() => window.__commands().length), before);
  assert.ok(await page.evaluate(() => window.__k.sim.dialogs.length) >= 1);
  await page.evaluate(async () => {
    const { withDialogAnswer } = await import('/v4/tools/port/test-api.mjs');
    const snap = structuredClone(window.__app.snapshot());
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), withDialogAnswer(window.__c.api, true), { restored: snap });
  });
  await settle();
  await click('[data-detail-actions] [data-card-action="uninstall"]');
  await settle();
  const cmd = await page.evaluate(() => window.__commands().at(-1));
  assert.equal(cmd.type, 'ctox.app.remove');
  assert.deepEqual(cmd.payload, { app_id: 'notes', base_app_ver: '1' });
  assert.equal(cmd.status, 'done');
  // Ergebnis = Änderung an business_apps (Kern archiviert): die App verschwindet live.
  assert.doesNotMatch(await gridText(), /Notizen/);
});

test('Versionen kommen aus business_app_versions; Dialog liegt im App-root', async () => {
  await page.evaluate(async () => {
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, { restored: { v: 1, scope: 'installed', viewMode: 'list', selectedId: 'customers', drawerOpen: true } });
  });
  await settle();
  await click('[data-detail-actions] [data-card-action="versions"]');
  await settle();
  const r = await page.evaluate(() => ({
    rows: [...document.querySelectorAll('#win .app-version-row [data-rollback-version]')].map((b) => b.dataset.rollbackVersion),
    outside: document.querySelectorAll('body > .ctox-modal').length,
  }));
  assert.deepEqual(r.rows, ['customers@4', 'customers@3', 'customers@2']);
  assert.equal(r.outside, 0);
  await click('#win [data-version-close]');
});

test('§16.29 Rollback: ctox.app.rollback {app_id, base_app_ver, target_ver}; älteres Schema ohne Rück-Migration → Ablehnung in die Zone', async () => {
  await page.evaluate(async () => {
    const { withDialogAnswer } = await import('/v4/tools/port/test-api.mjs');
    const snap = structuredClone(window.__app.snapshot());
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), withDialogAnswer(window.__c.api, true), { restored: snap });
  });
  await settle();
  const before = await page.evaluate(() => window.__commands().length);
  await click('[data-detail-actions] [data-card-action="versions"]');
  await settle();
  await click('#win [data-rollback-version="customers@2"]');
  await settle();
  const rej = await page.evaluate(() => window.__core.rejections.at(-1));
  assert.equal(rej.type, 'ctox.app.rollback');
  assert.deepEqual(rej.payload, { app_id: 'customers', base_app_ver: '3', target_ver: '2' });
  assert.equal(rej.reason, 'migration_required');
  assert.equal(await page.evaluate(() => window.__commands().length), before, 'abgelehnter Befehl erreicht business_commands nicht');
  const zone = await page.evaluate(() => window.__zone().filter((e) => e.code === 'app-store.command.ctox.app.rollback'));
  assert.equal(zone.length, 1);
  assert.equal(zone[0].level, 'error');
  assert.match(zone[0].text, /Zurücksetzen Kunden abgelehnt: .*Rück-Migration fehlt/);
  assert.equal(await page.evaluate(() => window.__core.actions['app-store.command.ctox.app.rollback']), undefined, 'keine Aktion – der Kern verlangt eine Rück-Migration');
  assert.equal(await page.$('[data-detail-actions] .is-progress'), null);
  assert.equal(await page.$eval('#win', (el) => el.textContent.includes('Rück-Migration')), false, 'Ablehnung nur in der Zone');
  await page.evaluate(() => { window.__api.status.clear('app-store.command.ctox.app.rollback'); });
});

test('Öffnen springt über api.shell.navigate', async () => {
  await click('[data-detail-actions] [data-card-action="open"]');
  const nav = await page.evaluate(() => window.__k.sim.navigations.at(-1));
  assert.equal(nav.to, 'customers');
  await click('[data-action="create-scratch"]');
  const nav2 = await page.evaluate(() => window.__k.sim.navigations.at(-1));
  assert.equal(nav2.to, 'creator');
  assert.equal(nav2.params.source, 'app-store');
  assert.equal(nav2.params.mode, 'scratch');
});

test('§16.29 Konflikt: Basis nicht mehr aktiv → Zone mit Text und Aktion „Auf aktuelle Version setzen“', async () => {
  // Kern ist der Projektion voraus (customers aktiv 5), der Client sieht noch 3.
  await page.evaluate(() => window.__core.setAhead('customers', '5'));
  await selectApp('customers');
  const before = await page.evaluate(() => window.__commands().length);
  await click('[data-detail-actions] [data-card-action="update"]');
  await settle();
  assert.equal(await page.evaluate(() => window.__commands().length), before);
  assert.deepEqual(await page.evaluate(() => window.__core.rejections.at(-1).payload), { app_id: 'customers', base_app_ver: '3', release_ver: '4' });
  let zone = await page.evaluate(() => window.__zone().filter((e) => e.code === 'app-store.conflict.customers'));
  assert.equal(zone.length, 1);
  assert.equal(zone[0].level, 'warn');
  assert.match(zone[0].text, /Update Kunden abgelehnt: Basis 3 ist nicht mehr aktiv/);
  assert.equal(await page.evaluate(() => window.__core.actions['app-store.conflict.customers']?.label), 'Auf aktuelle Version setzen');
  assert.equal(await page.$('[data-detail-actions] .is-progress'), null, 'Fortschritt endet mit der Ablehnung');
  assert.deepEqual(await page.evaluate(() => window.__zone().filter((e) => e.code.startsWith('app-store.command.')).map((e) => e.code)), [], 'kein zweiter Eintrag für dieselbe Ablehnung');
  // Projektion kommt nach; die Aktion setzt die Ansicht auf die aktive Version, der nächste Befehl nimmt sie als Basis.
  await page.evaluate(async () => {
    const cp = window.__k.sim.checkpoint;
    const v4 = window.__api.get('business_app_versions', 'customers@4').get();
    await window.__k.sim.push('business_app_versions', [{ ...v4, id: 'customers@5', app_ver: '5', manifest: { ...v4.manifest, version: '1.0.3' }, migrated_at_seq: cp }]);
    await window.__k.sim.push('business_apps', [{ id: 'customers', title: 'Kunden', active_ver: '5', migrated_at_seq: cp }]);
    window.__core.setAhead('customers', null);
    window.__k.sim.onCommand('ctox.app.release', async () => ({ ok: true }));
  });
  await settle();
  await page.evaluate(() => window.__core.actions['app-store.conflict.customers'].run());
  await settle();
  zone = await page.evaluate(() => window.__zone().map((e) => e.code));
  assert.equal(zone.includes('app-store.conflict.customers'), false);
  const snap = await page.evaluate(() => window.__app.snapshot());
  assert.equal(snap.selectedId, 'customers');
  assert.equal(snap.drawerOpen, true);
  assert.match(await page.$eval('[data-detail-version]', (el) => el.textContent.trim()), /^v?1\.0\.3$/, 'Detail zeigt die jetzt aktive Version');
  // Kein Update mehr (main 4 < aktiv 5); Freigabe nimmt die neue Basis.
  assert.equal(await page.$('[data-detail-actions] [data-card-action="update"]'), null);
  await click('[data-detail-actions] [data-card-action="release"]');
  await settle();
  await page.evaluate(() => { document.querySelector('#win .app-release-dialog').requestSubmit(); });
  await settle();
  const cmd = await page.evaluate(() => window.__commands().at(-1));
  assert.equal(cmd.type, 'ctox.app.release');
  assert.deepEqual(cmd.payload, { app_id: 'customers', base_app_ver: '5', channel: 'team' });
  assert.equal(cmd.status, 'done');
});

test('§16.29 Marketplace und Vorlagen aus app_catalog; Installieren = ctox.app.update mit base_app_ver null', async () => {
  await click('[data-scope="marketplace"]');
  await settle();
  const ids = await page.$$eval('[data-apps-grid] [data-app-id]', (els) => els.map((el) => el.dataset.appId).sort());
  // Neuester Eintrag je App; notes ist seit „Entfernen“ archiviert und steht wieder zur Installation; Vorlagen nicht hier.
  assert.deepEqual(ids, ['customers', 'documents', 'kanban', 'notes']);
  const status = await page.$$eval('[data-apps-grid] [data-app-id]', (els) => Object.fromEntries(els.map((el) => [el.dataset.appId, el.getAttribute('aria-label')])));
  assert.match(status.customers, /Installed/);
  assert.match(status.kanban, /Available/);
  await selectApp('kanban');
  assert.equal(await page.$eval('[data-detail-title]', (el) => el.textContent), 'Kanban');
  assert.equal(await page.$eval('[data-detail-source]', (el) => el.textContent), 'CTOX main');
  await click('[data-detail-actions] [data-card-action="install"]');
  await settle();
  const cmd = await page.evaluate(() => window.__commands().at(-1));
  assert.equal(cmd.type, 'ctox.app.update');
  assert.deepEqual(cmd.payload, { app_id: 'kanban', base_app_ver: null, release_ver: '1' });
  assert.equal(cmd.status, 'done');
  // Ergebnis an business_apps: Kanban ist installiert (Liste „Installiert“ live).
  await click('[data-scope="installed"]');
  await settle();
  assert.match(await gridText(), /Kanban/);
  // Vorlagen: Katalogeintrag mit layout.template → Creator mit der Vorlage.
  await click('[data-scope="template"]');
  await settle();
  const templates = await page.$$eval('[data-apps-grid] [data-app-id]', (els) => els.map((el) => el.dataset.appId).sort());
  assert.deepEqual(templates, ['create-scratch', 'starter-crm']);
  await selectApp('starter-crm');
  await click('[data-detail-actions] [data-card-action="open"]');
  const nav = await page.evaluate(() => window.__k.sim.navigations.at(-1));
  assert.equal(nav.to, 'creator');
  assert.equal(nav.params.mode, 'template');
  assert.equal(nav.params.template, 'starter-crm');
  await click('[data-scope="installed"]');
  await selectApp('customers');
});

test('snapshot/restored: Auswahl, Bereich, Suche und Ansicht überleben den Remount', async () => {
  await page.fill('[data-pg-search]', 'kun');
  await page.dispatchEvent('[data-pg-search]', 'input');
  await page.waitForTimeout(250);
  const snap = await page.evaluate(async () => {
    const cloned = structuredClone(window.__app.snapshot());
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, { restored: cloned });
    return cloned;
  });
  assert.equal(snap.scope, 'installed');
  assert.equal(snap.viewMode, 'list');
  assert.equal(snap.selectedId, 'customers');
  assert.equal(snap.drawerOpen, true);
  await settle();
  const after = await page.evaluate(() => window.__app.snapshot());
  assert.equal(after.scope, 'installed');
  assert.equal(after.selectedId, 'customers');
  assert.equal(await page.$eval('[data-detail-title]', (el) => el.textContent), 'Kunden');
  assert.equal(snap.query, 'kun');
  assert.equal(await page.$eval('[data-pg-search]', (el) => el.value), 'kun', 'Suchbegriff steht nach dem Remount sichtbar im Feld');
  assert.equal(await page.evaluate(() => window.__c.subscribers('business_apps')), 1, 'alte Abos sind abgemeldet');
});

test('leere Plattform: kein Zustandstext, keine Zone', async () => {
  const empty = await page.evaluate(async () => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    const k2 = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false, instanceId: 'empty' });
    await k2.ready;
    const host = document.createElement('div');
    document.body.append(host);
    const app = await window.__mod.mount(host, k2.appApi('app-store', '1'), { restored: { v: 1, scope: 'installed', viewMode: 'list' } });
    await new Promise((r) => setTimeout(r, 250));
    const gridText = host.querySelector('[data-apps-grid]').textContent.trim();
    const stateBox = Boolean(host.querySelector('.store-empty-state, [role="status"], [data-store-message], [data-loading-spinner]'));
    const zone = k2.statusZone.forApp('app-store').get().map((e) => e.code);
    await app.unmount();
    host.remove();
    await k2.close();
    return { gridText, stateBox, zone };
  });
  // Modulzustand ist modulweit (V4-PORT.md OFFEN-5): die zweite Montage hat die erste abgelöst → Hauptfenster neu
  // montieren, wie es die Shell beim Remount täte.
  await page.evaluate(async () => {
    const snap = structuredClone(window.__app.snapshot());
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, { restored: { ...snap, v: 1, scope: 'installed', viewMode: 'list', selectedId: 'customers', drawerOpen: true } });
  });
  await settle();
  assert.equal(empty.gridText, '');
  assert.equal(empty.stateBox, false);
  assert.deepEqual(empty.zone, []);
});

test('Screenshots 1280 und 420 gegen den Stub', async () => {
  await page.fill('[data-pg-search]', '');
  await page.dispatchEvent('[data-pg-search]', 'input');
  await page.waitForTimeout(250);
  assert.match(await gridText(), /Dokumente/, 'Liste nach geleerter Suche vollständig');
  await page.screenshot({ path: join(SHOTS, 'app-store-1280.png'), clip: { x: 0, y: 0, width: 1280, height: 760 } });
  await page.setViewportSize({ width: 420, height: 820 });
  await page.evaluate(() => { const w = document.getElementById('win'); w.style.width = '420px'; w.style.height = '820px'; });
  await page.waitForTimeout(300);
  await page.screenshot({ path: join(SHOTS, 'app-store-420.png'), clip: { x: 0, y: 0, width: 420, height: 820 } });
  const overflow = await page.$eval('#win', (el) => el.scrollWidth - el.clientWidth);
  assert.ok(overflow <= 1, `kein waagrechter Überlauf (${overflow}px)`);
  await page.setViewportSize({ width: 1280, height: 760 });
  await page.evaluate(() => { const w = document.getElementById('win'); w.style.width = '1280px'; w.style.height = '760px'; });
});

test('unmount: alle Abos abgemeldet, root leer, keine Zone, keine pageerror', async () => {
  const r = await page.evaluate(async () => {
    await window.__app.unmount();
    return {
      subs: ['business_apps', 'business_app_versions', 'app_catalog', 'app_drafts'].map((n) => window.__c.subscribers(n)),
      children: document.getElementById('win').childElementCount,
      zone: window.__zone().map((e) => e.code),
    };
  });
  assert.deepEqual(r.subs, [0, 0, 0, 0]);
  assert.equal(r.children, 0);
  assert.deepEqual(r.zone, []);
  assert.deepEqual(page.__errors, []);
});
