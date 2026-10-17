// intake gegen den eingefrorenen App-API-4.0-Kern-Stub (createStubKernel) in echtem Chromium.
// Muster: modules/knowledge/tests/v4-api.test.mjs. Stub-Pfad aus CTOX_APP_API_STUB (Pflicht-Pin
// ~/.local/state/shell-v4/stub-pins/4b0d0ac), read-only unter /__stub/ ausgeliefert.
// Lauf: CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/4b0d0ac node --test src/apps/business-os/modules/intake/tests/v4-api.test.mjs
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

const APP = 'intake';
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
  // FIXT-R: echte Routen/Rechte/Collections aus v4/manifests/intake.manifest.json (strikter Kern seit E26/E48).
  await page.evaluate((routes) => { window.__routes = routes; }, realManifestRoutes(APP));
  await page.evaluate(async (APP) => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    window.__mod = await import(`/modules/${APP}/index.js`);
    const kernel = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false });
    await kernel.ready;
    await kernel.sim.push('business_apps', [{ id: APP, title: 'Bewerbungseingang' }]);
    // FIXT-R: Eigentümer der Fremd-Collections (echte Manifeste, inkl. ctox-platform) vor der eigenen Version; nach der business_apps-Zeile.
    for (const o of window.__routes?._fixture_owners ?? []) if (!(await kernel.sim.inspect('business_apps', o.app))?.data?.active_ver) await kernel.sim.publishAppVersion(o.app, o.ver, o.publish);
    await kernel.sim.publishAppVersion(APP, '1', { title: 'Bewerbungseingang', manifestExtra: window.__routes });
    await kernel.sim.push('applications', [
      { id: 'appl_1', channel: 'job_board', status: 'new', vacancy_id: 'VAC-7', candidate: { name: 'Alice Ng', email: 'alice@example.org' }, documents: [{ name: 'cv.pdf' }], received_at_ms: Date.UTC(2026, 9, 14), updated_at_ms: 2 },
      { id: 'appl_2', channel: 'referral', status: 'hired', candidate: { name: 'Bob Lee', phone: '+49 30 1234' }, received_at_ms: Date.UTC(2026, 9, 12), updated_at_ms: 1 },
      { id: 'appl_3', channel: 'career_site', status: 'screening', candidate: { name: 'Carla Diaz-Montenegro von Hohenberg', email: 'carla.diaz@example.org' }, received_at_ms: Date.UTC(2026, 9, 13), updated_at_ms: 3 },
    ]);
    await kernel.sim.settle();
    kernel.sim.onCommand('ats.intake.capture', (payload) => ({ ok: true, application_id: 'appl_new', dedupe_key: 'email:' + (payload.email || '') }));
    window.__k = kernel;
    window.__api = kernel.appApi(APP, '1');
    window.__zone = () => kernel.statusZone.forApp(APP).get().filter((e) => e.source === 'app');
    window.__commands = () => window.__api.query('business_commands', { sort: [{ created_at: 'asc' }] }).get();
  }, APP);
});

after(async () => { await browser?.close(); await new Promise((r) => server?.close(r)); });

const settle = async () => { await page.evaluate(() => window.__k.sim.settle()); await page.waitForTimeout(150); };
const listText = () => page.$eval('[data-ats-list]', (el) => el.textContent);

test('mount: zeigt Live-Daten, nur api.*, Root gescoped, Titel über api.shell', async () => {
  await page.evaluate(async () => {
    const { countingApi } = await import('/v4/tools/port/test-api.mjs');
    window.__c = countingApi(window.__api);
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, {});
  });
  await settle();
  const list = await listText();
  assert.match(list, /Alice Ng/);
  assert.match(list, /Bob Lee/);
  assert.equal(await page.$eval('#win', (el) => el.dataset.app), 'intake');
  assert.equal(await page.evaluate(() => window.__k.shell.window('intake').state.get().title), 'Bewerbungseingang');
  assert.equal(await page.evaluate(() => window.__c.subscribers('applications')), 1);
  assert.equal(await page.$eval('[data-pg-count="all"]', (el) => el.textContent), ' (3)');
  assert.equal(await page.$eval('.intake-rail', (el) => el.dataset.pgWired), 'true', 'App verdrahtet die Spalten-Grammatik selbst');
  assert.deepEqual(page.__errors, []);
});

test('Live-Änderung (sim.push) erscheint ohne Neuladen, Löschung verschwindet', async () => {
  await page.evaluate(() => window.__k.sim.push('applications', { id: 'appl_live', channel: 'email', status: 'new', candidate: { name: 'Dora Live' }, received_at_ms: Date.UTC(2026, 9, 15), updated_at_ms: 9 }));
  await settle();
  assert.match(await listText(), /Dora Live/);
  assert.equal(await page.$eval('[data-pg-count="all"]', (el) => el.textContent), ' (4)');
  await page.evaluate(() => window.__k.sim.push('applications', { id: 'appl_live', _deleted: true }));
  await settle();
  assert.doesNotMatch(await listText(), /Dora Live/);
});

test('Filter: Band „Abgeschlossen“ und Suche wirken über die selbst verdrahtete Grammatik', async () => {
  await page.click('[data-pg-band="closed"]');
  await settle();
  assert.match(await listText(), /Bob Lee/);
  assert.doesNotMatch(await listText(), /Alice Ng/);
  await page.click('[data-pg-band="all"]');
  await page.fill('[data-pg-search]', 'carla');
  await settle();
  assert.match(await listText(), /Carla/);
  assert.doesNotMatch(await listText(), /Bob Lee/);
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
    const app = await window.__mod.mount(host, k2.appApi('intake', '1'), {});
    await new Promise((r) => setTimeout(r, 250));
    const out = {
      listText: host.querySelector('[data-ats-list]').textContent.trim(),
      hasStateBox: Boolean(host.querySelector('.ctox-syncing, .ctox-empty, [role="status"], .ctox-callout')),
      zone: k2.statusZone.forApp('intake').get().map((e) => e.code),
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

test('Schreiben: api.put erscheint live und wird angenommen', async () => {
  const result = await page.evaluate(async () => {
    const h = window.__api.put('applications', { id: 'appl_put', channel: 'qr', status: 'new', candidate: { name: 'Emil Put' }, received_at_ms: Date.UTC(2026, 9, 10), updated_at_ms: 10 });
    await h.local;
    return h.result;
  });
  assert.equal(result.status, 'accepted');
  await settle();
  assert.match(await listText(), /Emil Put/);
});

test('Befehl: Erfassen legt business_commands-Dokument an (ohne Warten), Erfolg bleibt still', async () => {
  await page.click('[data-action="new"]');
  await page.fill('[name="name"]', 'Frieda Neu');
  await page.fill('[name="email"]', 'frieda@example.org');
  await page.click('button[type="submit"]');
  // UI wartet nicht: das Befehlsdokument existiert sofort lokal.
  const immediate = await page.evaluate(() => window.__commands().at(-1));
  assert.equal(immediate.type, 'ats.intake.capture');
  assert.deepEqual(immediate.payload, { name: 'Frieda Neu', email: 'frieda@example.org', phone: null, vacancy_id: null, channel: 'career_site' }, 'Reset setzt den Kanal auf die erste Option (wie vor dem Port)');
  await settle();
  const cmd = await page.evaluate(() => window.__commands().at(-1));
  assert.equal(cmd.status, 'done');
  assert.deepEqual(await page.evaluate(() => window.__zone()), []);
  assert.equal(await page.inputValue('[name="name"]'), '', 'Formular nach done geleert');
});

test('Befehl: failed → genau ein Zoneneintrag; blockiert → Zone mit Gründen, Eingabe bleibt', async () => {
  await page.evaluate(() => window.__k.sim.failCommand('ats.intake.capture', 'kaputt'));
  await page.fill('[name="name"]', 'Gustav Fehl');
  await page.click('button[type="submit"]');
  await settle();
  let zone = await page.evaluate(() => window.__zone());
  assert.equal(zone.length, 1);
  assert.equal(zone[0].code, 'intake.command.ats.intake.capture');
  assert.match(zone[0].text, /kaputt/);
  assert.equal(await page.inputValue('[name="name"]'), 'Gustav Fehl', 'Eingabe bleibt bei failed');
  await page.evaluate(() => window.__k.sim.onCommand('ats.intake.capture', () => ({ ok: false, blockers: ['Duplikat erkannt'] })));
  await page.click('button[type="submit"]');
  await settle();
  zone = await page.evaluate(() => window.__zone());
  assert.deepEqual(zone.map((e) => e.code), ['intake.capture.blocked']);
  assert.match(zone[0].text, /Duplikat erkannt/);
  assert.equal(await page.inputValue('[name="name"]'), 'Gustav Fehl');
  await page.evaluate(() => {
    window.__k.sim.onCommand('ats.intake.capture', () => ({ ok: true }));
    window.__api.status.clear('intake.capture.blocked');
  });
});

test('snapshot/restored: offene Eingabe, Auswahl und Filter überleben den Remount', async () => {
  await page.click('[data-context-record-id="appl_3"]');
  await page.click('details.ats-more > summary');
  await page.fill('[name="phone"]', '+49 halb fertig');
  await page.click('[data-pg-tray-toggle]');
  await page.selectOption('[data-pg-filter][data-pg-name="status"]', 'screening');
  await settle();
  const snap = await page.evaluate(async () => {
    const cloned = structuredClone(window.__app.snapshot());
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, { restored: cloned });
    return cloned;
  });
  assert.equal(snap.selectedId, 'appl_3');
  assert.equal(snap.form.phone, '+49 halb fertig');
  assert.ok(JSON.stringify(snap).length < 1024 * 1024);
  await settle();
  const after = await page.evaluate(() => ({
    phone: document.querySelector('[name="phone"]').value,
    name: document.querySelector('[name="name"]').value,
    status: document.querySelector('[data-pg-filter][data-pg-name="status"]').value,
    trayHidden: document.querySelector('[data-pg-tray]').hidden,
    detail: document.querySelector('[data-ats-detail]').hidden ? '' : document.querySelector('[data-ats-detail]').textContent,
    list: document.querySelector('[data-ats-list]').textContent,
    selected: window.__app.snapshot().selectedId,
  }));
  assert.equal(after.phone, '+49 halb fertig');
  assert.match(after.name, /Carla/);
  assert.equal(after.status, 'screening');
  assert.equal(after.trayHidden, false);
  assert.match(after.detail, /Carla/);
  assert.doesNotMatch(after.list, /Alice Ng/, 'Statusfilter wiederhergestellt');
  assert.equal(after.selected, 'appl_3');
  assert.equal(await page.evaluate(() => window.__c.subscribers('applications')), 1, 'alte Abos abgemeldet');
  // Filter für die Screenshots zurücksetzen.
  await page.click('[data-pg-reset]');
  await page.click('[data-pg-tray-toggle]');
  await settle();
  assert.deepEqual(page.__errors, []);
});

test('Screenshots 1280 und 420 gegen den Stub', async () => {
  await mkdir(join(HERE, 'shots'), { recursive: true });
  await page.click('[data-context-record-id="appl_1"]');
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
    await window.__k.sim.push('applications', { id: 'appl_late', channel: 'email', candidate: { name: 'Late' }, updated_at_ms: 11 });
    await window.__k.sim.settle();
    return { subs: window.__c.subscribers('applications'), children: document.getElementById('win').children.length };
  });
  assert.equal(res.subs, 0);
  assert.equal(res.children, 0);
  assert.deepEqual(page.__errors, []);
});
