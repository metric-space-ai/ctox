// nachweise gegen den eingefrorenen App-API-4.0-Kern-Stub (createStubKernel) in echtem Chromium.
// Muster: modules/knowledge/tests/v4-api.test.mjs. Stub-Pfad aus CTOX_APP_API_STUB (Pflicht-Pin
// ~/.local/state/shell-v4/stub-pins/64acc2c), read-only unter /__stub/ ausgeliefert.
// Lauf: CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/64acc2c node --test src/apps/business-os/modules/nachweise/tests/v4-api.test.mjs
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

const APP = 'nachweise';
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
  // FIXT-R: echte Routen/Rechte/Collections aus v4/manifests/nachweise.manifest.json (strikter Kern seit E26/E48).
  // Stub 8265bd7 liest in runCommand das Handler-Artefakt (manifest.files[path]) VOR dem sim.onCommand-Simulator; die
  // Fixture veröffentlicht keine Dateien („Artefakt undefined fehlt“). Die simulierten Typen daher ohne Handler-Pfad
  // veröffentlichen – Routen und command_rights bleiben die echten.
  const routes = realManifestRoutes(APP);
  for (const type of ['ats.deployment.check', 'ats.leistungsnachweis.signoff']) if (routes.handlers) delete routes.handlers[type];
  await page.evaluate((routes) => { window.__routes = routes; }, routes);
  await page.evaluate(async (APP) => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    window.__mod = await import(`/modules/${APP}/index.js`);
    const kernel = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false });
    await kernel.ready;
    await kernel.sim.push('business_apps', [{ id: APP, title: 'Nachweise' }]);
    // FIXT-R: Eigentümer der Fremd-Collections (echte Manifeste, inkl. ctox-platform) vor der eigenen Version; nach der business_apps-Zeile.
    for (const o of window.__routes?._fixture_owners ?? []) if (!(await kernel.sim.inspect('business_apps', o.app))?.data?.active_ver) await kernel.sim.publishAppVersion(o.app, o.ver, o.publish);
    await kernel.sim.publishAppVersion(APP, '1', { title: 'Nachweise', manifestExtra: window.__routes });
    const day = 86_400_000;
    const now = Date.now();
    await kernel.sim.push('business_credentials', [
      { id: 'cred_1', subject_id: 'cand-alice', subject_name: 'Alice Adler', subject_type: 'candidate', credential_type: 'staplerschein', issuer: 'DEKRA', verified: true, verified_by: 'ines', valid_until_ms: now + 400 * day, status: 'valid', created_at_ms: 1, updated_at_ms: 3 },
      { id: 'cred_2', subject_id: 'cand-bob', subject_name: 'Bob Berger', subject_type: 'candidate', credential_type: 'g25', issuer: 'Betriebsarzt', verified: true, valid_until_ms: now - 5 * day, status: 'valid', created_at_ms: 2, updated_at_ms: 2 },
      { id: 'cred_3', subject_id: 'cand-carla-diaz-montenegro-von-hohenberg', subject_name: 'Carla Diaz-diaz-montenegro-von-hohenberg', subject_type: 'candidate', credential_type: 'aufenthaltstitel', verified: false, valid_until_ms: now + 10 * day, status: 'unverified', created_at_ms: 3, updated_at_ms: 1 },
    ]);
    await kernel.sim.settle();
    kernel.sim.onCommand('ats.deployment.check', (p) => (p.subject_id === 'cand-alice' ? { ready: true, blockers: [] } : { ready: false, blockers: [{ credential_type: 'g25', reason: 'abgelaufen' }] }));
    kernel.sim.onCommand('ats.leistungsnachweis.signoff', (p) => ({ ok: true, record_id: p.record_id, billing_released: true, invoice_id: 'inv_9', net_total: 1234.5 }));
    window.__k = kernel;
    window.__api = kernel.appApi(APP, '1');
    window.__zone = () => kernel.statusZone.forApp(APP).get().filter((e) => e.source === 'app');
    window.__commands = () => window.__api.query('business_commands', { sort: [{ created_at: 'asc' }] }).get();
  }, APP);
});

after(async () => { await browser?.close(); await new Promise((r) => server?.close(r)); });

const settle = async () => { await page.evaluate(() => window.__k.sim.settle()); await page.waitForTimeout(150); };
const listText = () => page.$eval('[data-ats-list]', (el) => el.textContent);
const zoneCodes = () => page.evaluate(() => window.__zone().map((e) => e.code));

test('mount: zeigt Live-Daten, nur api.*, Root gescoped, Titel über api.shell', async () => {
  await page.evaluate(async () => {
    const { countingApi } = await import('/v4/tools/port/test-api.mjs');
    window.__c = countingApi(window.__api);
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, {});
  });
  await settle();
  const list = await listText();
  assert.match(list, /Alice Adler/);
  assert.match(list, /Bob Berger/);
  assert.equal(await page.$eval('#win', (el) => el.dataset.app), 'nachweise');
  assert.equal(await page.evaluate(() => window.__k.shell.window('nachweise').state.get().title), 'Nachweise');
  assert.equal(await page.evaluate(() => window.__c.subscribers('business_credentials')), 1);
  assert.equal(await page.$eval('[data-pg-count="all"]', (el) => el.textContent), ' (3)');
  assert.equal(await page.$eval('[data-pg-count="critical"]', (el) => el.textContent), ' (2)');
  assert.equal(await page.$eval('.nachweise-rail', (el) => el.dataset.pgWired), 'true', 'App verdrahtet die Spalten-Grammatik selbst');
  // Textdiät: kein Erklär-Untertitel und kein Kicker über dem Titel, keine technische ID im Detail.
  assert.equal(await page.$('[data-ats-sub]'), null);
  assert.doesNotMatch(await page.$eval('#win', (el) => el.innerText), /Tresor|\bATS\b/);
  assert.deepEqual(page.__errors, []);
});

test('Live-Änderung (sim.push) erscheint ohne Neuladen, Löschung verschwindet', async () => {
  await page.evaluate(() => window.__k.sim.push('business_credentials', { id: 'cred_live', subject_id: 'cand-dora-live', subject_name: 'Dora Live', credential_type: 'g37', verified: false, updated_at_ms: 9 }));
  await settle();
  assert.match(await listText(), /Dora Live/);
  await page.evaluate(() => window.__k.sim.push('business_credentials', { id: 'cred_live', _deleted: true }));
  await settle();
  assert.doesNotMatch(await listText(), /Dora Live/);
});

test('Filter: Band „Kritisch“ und Suche wirken über die selbst verdrahtete Grammatik', async () => {
  await page.click('[data-pg-band="critical"]');
  await settle();
  assert.match(await listText(), /Bob Berger/);
  assert.doesNotMatch(await listText(), /Alice Adler/);
  await page.click('[data-pg-band="all"]');
  await page.fill('[data-pg-search]', 'carla');
  await settle();
  assert.match(await listText(), /Carla Diaz/);
  assert.doesNotMatch(await listText(), /Bob Berger/);
  await page.fill('[data-pg-search]', '');
  await settle();
});

test('leere Collection rendert keinen Text und keine Zone', async () => {
  const empty = await page.evaluate(async () => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    const k2 = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false, instanceId: 'empty' });
    await k2.ready;
    const host = document.createElement('div');
    document.body.append(host);
    const app = await window.__mod.mount(host, k2.appApi('nachweise', '1'), {});
    await new Promise((r) => setTimeout(r, 250));
    const out = {
      listText: host.querySelector('[data-ats-list]').textContent.trim(),
      hasStateBox: Boolean(host.querySelector('.ctox-syncing, .ctox-empty, [role="status"], .ctox-callout')),
      zone: k2.statusZone.forApp('nachweise').get().map((e) => e.code),
    };
    await app.unmount();
    host.remove();
    await k2.close();
    return out;
  });
  assert.equal(empty.listText, '');
  assert.equal(empty.hasStateBox, false);
  assert.deepEqual(empty.zone, []);
});

test('Schreiben: Nachweis anlegen → api.put, lokal sofort, angenommen, Formular geleert', async () => {
  await page.click('[data-action="new"]');
  await page.fill('[data-ats-form] [name="subject_id"]', 'cand-emil-neu');
  await page.fill('[data-ats-form] [name="issuer"]', 'TÜV');
  await page.click('[data-ats-form] button[type="submit"]');
  await settle();
  // Über das Formular angelegt (nur Kandidaten-ID, kein Name): „Unbekannt“, nie die ID.
  assert.match(await listText(), /Unbekannt/);
  assert.doesNotMatch(await listText(), /cand-emil-neu/);
  const doc = await page.evaluate(() => window.__api.query('business_credentials', { selector: { subject_id: 'cand-emil-neu' } }).get()[0]);
  assert.equal(doc.status, 'unverified');
  assert.equal(doc.issuer, 'TÜV');
  assert.equal(await page.inputValue('[data-ats-form] [name="subject_id"]'), '');
  assert.deepEqual(await zoneCodes(), []);
});

test('Befehl Einsatz-Prüfung: business_commands-Dokument sofort; Ergebnis bereit/blockiert in der Zone', async () => {
  await page.click('[data-toggle-actions]');
  await page.fill('[data-ats-gate-form] [name="subject_id"]', 'cand-alice');
  await page.fill('[data-ats-gate-form] [name="required_types"]', 'staplerschein, g25');
  await page.click('[data-ats-gate-form] button[type="submit"]');
  const immediate = await page.evaluate(() => window.__commands().at(-1));
  assert.equal(immediate.type, 'ats.deployment.check');
  assert.deepEqual(immediate.payload, { subject_id: 'cand-alice', required_types: ['staplerschein', 'g25'] });
  await settle();
  let zone = await page.evaluate(() => window.__zone());
  assert.deepEqual(zone.map((e) => [e.code, e.level]), [['nachweise.deployment.check', 'info']]);
  assert.match(zone[0].text, /Einsatzbereit: cand-alice/);
  await page.fill('[data-ats-gate-form] [name="subject_id"]', 'cand-bob');
  await page.click('[data-ats-gate-form] button[type="submit"]');
  await settle();
  zone = await page.evaluate(() => window.__zone());
  assert.equal(zone.length, 1, 'gleicher Code ersetzt den Eintrag');
  assert.equal(zone[0].level, 'warn');
  assert.match(zone[0].text, /Nicht einsatzbereit: cand-bob.*abgelaufen/);
  await page.evaluate(() => window.__api.status.clear('nachweise.deployment.check'));
});

test('Befehl Sign-off: failed → genau ein Zoneneintrag; done → Zusammenfassung (info), Formular geleert', async () => {
  await page.evaluate(() => window.__k.sim.failCommand('ats.leistungsnachweis.signoff', 'kaputt'));
  await page.fill('[data-ats-signoff-form] [name="record_id"]', 'ptr_7');
  await page.fill('[data-ats-signoff-form] [name="charge_rate"]', '42.5');
  await page.click('[data-ats-signoff-form] button[type="submit"]');
  await settle();
  let zone = await page.evaluate(() => window.__zone());
  assert.deepEqual(zone.map((e) => e.code), ['nachweise.command.ats.leistungsnachweis.signoff']);
  assert.match(zone[0].text, /kaputt/);
  assert.equal(await page.inputValue('[data-ats-signoff-form] [name="record_id"]'), 'ptr_7', 'Eingabe bleibt bei failed');
  const cmd = await page.evaluate(() => window.__commands().at(-1));
  assert.deepEqual(cmd.payload, { record_id: 'ptr_7', charge_rate: 42.5 });
  await page.click('[data-ats-signoff-form] button[type="submit"]');
  await settle();
  zone = await page.evaluate(() => window.__zone());
  assert.deepEqual(zone.map((e) => [e.code, e.level]), [['nachweise.signoff.result', 'info']]);
  assert.match(zone[0].text, /inv_9/);
  assert.equal(await page.inputValue('[data-ats-signoff-form] [name="record_id"]'), '');
  await page.evaluate(() => window.__api.status.clear('nachweise.signoff.result'));
});

test('snapshot/restored: offene Eingabe, Auswahl und Filter überleben den Remount', async () => {
  await page.click('[data-context-record-id="cred_3"]');
  await page.fill('[data-ats-signoff-form] [name="entleiher_account_id"]', 'acc_halb');
  await page.fill('[data-ats-form] [name="issuer"]', 'Ausländerbehörde halb');
  await page.click('[data-pg-tray-toggle]');
  await page.selectOption('[data-pg-filter][data-pg-name="status"]', 'unverified');
  await settle();
  const snap = await page.evaluate(async () => {
    const cloned = structuredClone(window.__app.snapshot());
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, { restored: cloned });
    return cloned;
  });
  assert.equal(snap.selectedId, 'cred_3');
  assert.equal(snap.forms.signoff.entleiher_account_id, 'acc_halb');
  assert.ok(JSON.stringify(snap).length < 1024 * 1024);
  await settle();
  const after = await page.evaluate(() => ({
    acc: document.querySelector('[data-ats-signoff-form] [name="entleiher_account_id"]').value,
    issuer: document.querySelector('[data-ats-form] [name="issuer"]').value,
    status: document.querySelector('[data-pg-filter][data-pg-name="status"]').value,
    trayHidden: document.querySelector('[data-pg-tray]').hidden,
    actionsHidden: document.querySelector('[data-ats-root]').classList.contains('is-actions-hidden'),
    detail: document.querySelector('[data-ats-detail]').hidden ? '' : document.querySelector('[data-ats-detail]').textContent,
    list: document.querySelector('[data-ats-list]').textContent,
    selected: window.__app.snapshot().selectedId,
  }));
  assert.equal(after.acc, 'acc_halb');
  assert.equal(after.issuer, 'Ausländerbehörde halb');
  assert.equal(after.status, 'unverified');
  assert.equal(after.trayHidden, false);
  assert.equal(after.actionsHidden, false);
  assert.match(after.detail, /Carla Diaz/);
  assert.doesNotMatch(after.list, /Alice Adler/, 'Statusfilter wiederhergestellt');
  assert.equal(after.selected, 'cred_3');
  assert.equal(await page.evaluate(() => window.__c.subscribers('business_credentials')), 1, 'alte Abos abgemeldet');
  await page.click('[data-pg-reset]');
  await page.click('[data-pg-tray-toggle]');
  await settle();
  assert.deepEqual(page.__errors, []);
});

test('Screenshots 1280 und 420 gegen den Stub', async () => {
  await mkdir(join(HERE, 'shots'), { recursive: true });
  await page.click('[data-context-record-id="cred_1"]');
  await settle();
  await page.screenshot({ path: join(HERE, 'shots', `${APP}-1280.png`) });
  await page.setViewportSize({ width: 420, height: 860 });
  await page.$eval('#win', (el) => { el.style.width = '420px'; el.style.height = '860px'; });
  await page.waitForTimeout(200);
  await page.screenshot({ path: join(HERE, 'shots', `${APP}-420.png`) });
  const overflow = await page.$eval('#win', (el) => el.scrollWidth - el.clientWidth);
  assert.ok(overflow <= 0, `kein horizontaler Überlauf bei 420 (scrollWidth-clientWidth=${overflow})`);
  await page.setViewportSize({ width: 1280, height: 760 });
  await page.$eval('#win', (el) => { el.style.width = '1280px'; el.style.height = '760px'; });
});

test('unmount meldet alle Abos ab und leert root', async () => {
  const res = await page.evaluate(async () => {
    await window.__app.unmount();
    await window.__k.sim.push('business_credentials', { id: 'cred_late', subject_id: 'late', updated_at_ms: 11 });
    await window.__k.sim.settle();
    return { subs: window.__c.subscribers('business_credentials'), children: document.getElementById('win').children.length };
  });
  assert.equal(res.subs, 0);
  assert.equal(res.children, 0);
  assert.deepEqual(page.__errors, []);
});
