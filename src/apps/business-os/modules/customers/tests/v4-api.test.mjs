// customers gegen den verbindlichen App-API-4.0-Stub des Kerns (createStubKernel, gepinnt über CTOX_APP_API_STUB,
// read-only unter /__stub/ ausgeliefert) in echtem Chromium. Muster: modules/knowledge/tests/v4-api.test.mjs.
// Lauf: CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/f967baf node --test src/apps/business-os/modules/customers/tests/v4-api.test.mjs
// Playwright: aus PLAYWRIGHT_MODULE, sonst ~/Documents/ctox/src/apps/business-os/node_modules/playwright.
import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { dirname, join, normalize, extname } from 'node:path';
import { homedir } from 'node:os';
import { realManifestRoutes } from '../../../v4/tools/port/real-manifest.mjs';

const STUB = process.env.CTOX_APP_API_STUB || join(homedir(), '.local/state/shell-v4/stub-pins/f967baf');
const HERE = dirname(fileURLToPath(import.meta.url));
const BOS = normalize(join(HERE, '../../..'));
const SHOTS = join(HERE, 'shots');
const require = createRequire(import.meta.url);
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || join(homedir(), 'Documents/ctox/src/apps/business-os/node_modules/playwright'));
const TYPES = { '.js': 'text/javascript', '.mjs': 'text/javascript', '.css': 'text/css', '.html': 'text/html', '.json': 'application/json', '.svg': 'image/svg+xml' };

const PAGE = `<!doctype html><html><head><meta charset="utf-8">
<link rel="stylesheet" href="/ui-contract/v1/workjet-ui-contract.css">
<link rel="stylesheet" href="/app.css">
<link rel="stylesheet" href="/shared/base.css">
<link rel="stylesheet" href="/themes/ctox-desktop-shell.css">
<link rel="stylesheet" href="/modules/customers/index.css">
<style>html,body{margin:0;height:100%;overflow:hidden} #win{position:relative;width:1280px;height:760px;overflow:hidden;display:flex;flex-direction:column}</style>
</head><body><div id="win"></div></body></html>`;

let server, browser, page, base;

const ACCOUNTS = [
  { id: 'acct-a', name: 'Acme GmbH', domain: 'acme.test', industry: 'SaaS', customer_stage: 'active', health_status: 'healthy', account_status: 'active', search_text: 'acme gmbh acme.test saas', is_deleted: false, created_at_ms: 1, updated_at_ms: 3, last_activity_at_ms: 3 },
  { id: 'acct-b', name: 'Beta AG', domain: 'beta.test', industry: 'Industrie', customer_stage: 'renewal', health_status: 'at_risk', account_status: 'active', search_text: 'beta ag beta.test industrie', is_deleted: false, created_at_ms: 1, updated_at_ms: 2, last_activity_at_ms: 2 },
];

before(async () => {
  server = createServer(async (req, res) => {
    const path = decodeURIComponent(new URL(req.url, 'http://x').pathname);
    if (path === '/') { res.writeHead(200, { 'content-type': 'text/html' }); res.end(PAGE); return; }
    const [rootDir, rel] = path.startsWith('/__stub/') ? [STUB, path.slice('/__stub'.length)] : [BOS, path];
    const file = normalize(join(rootDir, rel));
    if (!file.startsWith(rootDir)) { res.writeHead(403); res.end(); return; }
    try {
      let body = await readFile(file);
      // Wie der Service Worker (§16.15): ctox:shared/x.js → Shell-Pfad (OFFEN-F, native Import-Map greift nicht).
      if (/\.m?js$/.test(file)) body = Buffer.from(body.toString('utf8').replace(/(['"])ctox:shared\//g, '$1/shared/'));
      res.writeHead(200, { 'content-type': TYPES[extname(file)] || 'application/octet-stream' }); res.end(body);
    } catch { res.writeHead(404); res.end(); }
  });
  await new Promise((r) => server.listen(0, '127.0.0.1', r));
  base = `http://127.0.0.1:${server.address().port}/`;
  browser = await chromium.launch({ executablePath: process.env.CHROMIUM_PATH || undefined });
  page = await browser.newPage({ viewport: { width: 1280, height: 760 } });
  const errors = [];
  page.on('pageerror', (e) => errors.push(e.message));
  page.on('console', (m) => { if (m.type() === 'error') console.error('[page]', m.text()); });
  page.on('response', (r) => { if (r.status() >= 400) console.error('[page] HTTP', r.status(), r.url()); });
  page.__errors = errors;
  await page.goto(base);
  // Befehlsrouten der echten customers-Version (v4/manifests/customers.manifest.json): handlers und command_rights.
  // FIXT-R: echter Routen-/Rechte-/Collections-Teil plus Eigentümer der Fremd-Collections (_fixture_owners).
  await page.evaluate((routes) => { window.__customersRoutes = routes; }, realManifestRoutes('customers'));
  await page.evaluate(async (accounts) => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    window.__mod = await import('/modules/customers/index.js');
    const kernel = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false, actor: { role: 'admin', permissions: [] } });
    await kernel.ready;
    await kernel.sim.push('business_apps', [{ id: 'customers', title: 'Kunden' }, { id: 'documents', title: 'Dokumente' }]);
    // Wie die anderen v4-Gate-Dateien (E35): aktive customers-Version mit ihren Befehlsrouten veröffentlichen, nach der
    // business_apps-Zeile (sonst fehlt active_ver). Ohne Route lehnt der Kern z. B. customers.import.from_outbound ab.
    // FIXT-R: Eigentümer der Fremd-Collections (echte Manifeste, inkl. ctox-platform) vor der eigenen Version.
    for (const o of window.__customersRoutes?._fixture_owners ?? []) if (!(await kernel.sim.inspect('business_apps', o.app))?.data?.active_ver) await kernel.sim.publishAppVersion(o.app, o.ver, o.publish);
    await kernel.sim.publishAppVersion('customers', '1', { title: 'Kunden', manifestExtra: window.__customersRoutes });
    await kernel.sim.push('customer_accounts', accounts);
    await kernel.sim.push('customer_contacts', [
      { id: 'contact-a', account_id: 'acct-a', first_name: 'Ada', last_name: 'Lovelace', email: 'ada@acme.test', job_title: 'CTO', is_primary_contact: true, search_text: 'ada lovelace ada@acme.test cto', is_deleted: false, created_at_ms: 1, updated_at_ms: 3 },
    ]);
    await kernel.sim.push('customer_opportunities', [
      { id: 'opp-a', account_id: 'acct-a', name: 'Renewal 2026', opportunity_type: 'renewal', stage: 'proposal', amount_cents: 1000000, currency: 'EUR', probability: 40, position: 0, search_text: 'renewal 2026', is_deleted: false, created_at_ms: 1, updated_at_ms: 3 },
    ]);
    await kernel.sim.push('outbound_companies', [
      { id: 'co-ready', campaign_id: 'camp-a', name: 'Gamma GmbH', domain: 'gamma.test', qualification_status: 'qualified', research_status: 'done', pipeline_status: 'pipeline', fit_score: 91, payload: {}, created_at_ms: 1, updated_at_ms: 9 },
    ]);
    await kernel.sim.push('outbound_pipeline_items', [
      { id: 'pipe-ready', campaign_id: 'camp-a', company_id: 'co-ready', company_name: 'Gamma GmbH', stage: 'qualified', contact_research_status: 'done', outreach_status: 'ready', contacts: [], payload: {}, created_at_ms: 1, updated_at_ms: 9 },
    ]);
    await kernel.sim.push('documents', [
      { id: 'doc-a', title: 'Acme MSA', filename: 'MSA.pdf', mime_type: 'application/pdf', status: 'active', current_version_id: 'docver-a1', index_text: 'acme msa', linked_records: [{ type: 'customer_account', id: 'acct-a' }], is_deleted: false, created_at_ms: 1, updated_at_ms: 8 },
    ]);
    kernel.sim.onCommand('customers.account.create', () => ({ ok: true }));
    await kernel.sim.settle();
    const api = kernel.appApi('customers', '1');
    window.__k = kernel;
    window.__api = api;
    window.__zone = () => kernel.statusZone.forApp('customers').get().filter((e) => e.source === 'app');
    window.__commands = () => api.query('business_commands', { sort: [{ created_at: 'asc' }] }).get();
  }, ACCOUNTS);
});

after(async () => { await browser?.close(); server?.closeAllConnections?.(); await new Promise((r) => server?.close(r)); });

const settle = async () => { await page.evaluate(() => window.__k.sim.settle()); await page.waitForTimeout(150); };
const text = (sel) => page.$eval(sel, (el) => el.textContent);
const click = (sel) => page.evaluate((s) => document.querySelector(s).click(), sel);

test('mount: Live-Daten, nur api.*, gescoptes Root, Titel über api.shell', async () => {
  await page.evaluate(async () => {
    const { countingApi } = await import('/v4/tools/port/test-api.mjs');
    window.__c = countingApi(window.__api);
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, {});
  });
  await settle();
  const list = await text('[data-customers-account-list]');
  assert.match(list, /Acme GmbH/);
  assert.match(list, /Beta AG/);
  assert.equal(await page.$eval('#win', (el) => el.dataset.app), 'customers');
  assert.equal(await page.evaluate(() => window.__k.shell.window('customers').state.get().title), 'Kunden');
  assert.equal(await page.evaluate(() => window.__c.subscribers('customer_accounts')), 1);
  assert.equal(await page.evaluate(() => window.__c.subscribers('business_commands')), 0, 'kein Befehlsprotokoll-Abo');
  // Erstes Konto ist gewählt, Arbeitsbereich zeigt es.
  assert.match(await text('[data-customers-main] .ctox-pane-title'), /Acme GmbH/);
  assert.deepEqual(await page.evaluate(() => window.__zone()), []);
  assert.deepEqual(page.__errors, []);
});

test('Live-Änderung vom Core (sim.push) erscheint ohne Neuladen, Löschung verschwindet', async () => {
  await page.evaluate(() => window.__k.sim.push('customer_accounts', { id: 'acct-g', name: 'Gamma Live KG', account_status: 'active', customer_stage: 'prospect', health_status: 'healthy', search_text: 'gamma live kg', is_deleted: false, created_at_ms: 9, updated_at_ms: 9, last_activity_at_ms: 9 }));
  await settle();
  assert.match(await text('[data-customers-account-list]'), /Gamma Live KG/);
  await page.evaluate(() => window.__k.sim.push('customer_accounts', { id: 'acct-g', _deleted: true }));
  await settle();
  assert.doesNotMatch(await text('[data-customers-account-list]'), /Gamma Live KG/);
  // Weiches Löschen (is_deleted) bleibt wie in v3 unsichtbar.
  await page.evaluate(() => window.__k.sim.push('customer_accounts', { id: 'acct-s', name: 'Soft Deleted', account_status: 'active', customer_stage: 'prospect', health_status: 'healthy', search_text: 'soft deleted', is_deleted: true, created_at_ms: 10, updated_at_ms: 10 }));
  await settle();
  assert.doesNotMatch(await text('[data-customers-account-list]'), /Soft Deleted/);
});

test('leere Collections rendern keinen Zustandstext und keine Zone', async () => {
  const empty = await page.evaluate(async () => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    const k2 = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false, instanceId: 'empty' });
    await k2.ready;
    const host = document.createElement('div');
    document.body.append(host);
    const app = await window.__mod.mount(host, k2.appApi('customers', '1'), {});
    await new Promise((r) => setTimeout(r, 250));
    const listText = host.querySelector('[data-customers-account-list]').textContent.trim();
    const mainText = host.querySelector('.customers-workbench-scroll')?.textContent.trim() ?? '';
    const hasStateBox = Boolean(host.querySelector('.ctox-syncing, .ctox-empty, [role="status"], .customers-command-state, .customers-permission-notice'));
    const zone = k2.statusZone.forApp('customers').get().map((e) => e.code);
    await app.unmount();
    host.remove();
    await k2.close();
    return { listText, mainText, hasStateBox, zone };
  });
  assert.equal(empty.listText, '');
  assert.equal(empty.mainText, '');
  assert.equal(empty.hasStateBox, false);
  assert.deepEqual(empty.zone, []);
  // Modulzustand ist modulweit (V4-PORT.md OFFEN): Hauptfenster neu montieren.
  await page.evaluate(async () => {
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, {});
  });
  await settle();
});

test('Schreiben: api.put erscheint live und wird vom Core angenommen', async () => {
  const result = await page.evaluate(async () => {
    const h = window.__api.put('customer_accounts', { id: 'acct-d', name: 'Delta Put GmbH', account_status: 'active', customer_stage: 'prospect', health_status: 'healthy', search_text: 'delta put gmbh', is_deleted: false, created_at_ms: 8, updated_at_ms: 8, last_activity_at_ms: 8 });
    await h.local;
    return h.result;
  });
  assert.equal(result.status, 'accepted');
  await settle();
  assert.match(await text('[data-customers-account-list]'), /Delta Put GmbH/);
});

test('Befehl: Archivieren → business_commands mit record_id; failed → genau ein Zoneneintrag', async () => {
  await page.evaluate(() => document.querySelector('[data-customers-account-id="acct-a"]').click());
  await settle();
  await page.evaluate(() => window.__k.sim.failCommand('customers.account.archive', 'kaputt'));
  await click('[data-customers-action="archive-account"]');
  await settle();
  const cmd = await page.evaluate(() => window.__commands().at(-1));
  assert.equal(cmd.type, 'customers.account.archive');
  assert.equal(cmd.record_id, 'acct-a');
  assert.deepEqual(cmd.payload, { account_id: 'acct-a' });
  assert.equal(cmd.module, undefined);
  assert.equal(cmd.status, 'failed');
  const zone = await page.evaluate(() => window.__zone());
  assert.equal(zone.length, 1);
  assert.equal(zone[0].code, 'customers.command.customers.account.archive');
  assert.equal(zone[0].level, 'error');
  assert.match(zone[0].text, /kaputt/);
  assert.equal(zone[0].ref, cmd.id);
});

test('Befehl: Formular „Neuer Kunde“ → customers.account.create done, ohne Blockieren', async () => {
  await click('[data-customers-action="create-account"]');
  await page.waitForSelector('[data-customers-form] input[name="name"]');
  await page.fill('[data-customers-form] input[name="name"]', 'Epsilon SE');
  const formOpenAfterSubmit = await page.evaluate(() => {
    document.querySelector('[data-customers-form]').requestSubmit();
    return Boolean(document.querySelector('[data-customers-form]')); // synchron nach dem Absenden
  });
  assert.equal(formOpenAfterSubmit, false, 'Formular schließt sofort, vor der Core-Antwort');
  await settle();
  const cmd = await page.evaluate(() => window.__commands().at(-1));
  assert.equal(cmd.type, 'customers.account.create');
  assert.equal(cmd.payload.name, 'Epsilon SE');
  assert.equal(cmd.payload.source, 'business-os-customers-ui');
  assert.equal(cmd.status, 'done');
  const codes = await page.evaluate(() => window.__zone().map((e) => e.code));
  assert.ok(!codes.includes('customers.command.customers.account.create'));
});

test('Befehl: Übernahme aus Outbound trägt Ziel-IDs im payload (V4-HANDLERS U1)', async () => {
  await click('[data-customers-detail-tab="handoff"]');
  await settle();
  await click('[data-customers-outbound-company-id="co-ready"]');
  await settle();
  await click('[data-customers-action="import-outbound"]');
  await settle();
  const cmd = await page.evaluate(() => window.__commands().at(-1));
  assert.equal(cmd.type, 'customers.import.from_outbound');
  assert.equal(cmd.record_id, 'co-ready');
  assert.equal(cmd.payload.account_id, 'co-ready');
  assert.equal(cmd.payload.import_batch_id, 'co-ready');
  assert.equal(cmd.payload.outbound_company_id, 'co-ready');
  assert.equal(cmd.payload.pipeline_id, 'pipe-ready');
  await click('[data-customers-detail-tab="overview"]');
  await settle();
});

test('Formular-Validierung meldet in die Zone statt in den Inhalt', async () => {
  await click('[data-customers-action="create-account"]');
  await page.waitForSelector('[data-customers-form] input[name="name"]');
  await page.fill('[data-customers-form] input[name="name"]', '   '); // erfüllt required, scheitert an validateAccountDraft
  await page.evaluate(() => document.querySelector('[data-customers-form]').requestSubmit());
  await settle();
  const zone = await page.evaluate(() => window.__zone().filter((e) => e.code === 'customers.form.invalid'));
  assert.equal(zone.length, 1);
  assert.equal(await page.$('.customers-command-state'), null);
  await click('[data-customers-action="cancel-form"]');
  await settle();
});

test('snapshot/restored: offene Eingabe und Auswahl überleben den Remount', async () => {
  await page.evaluate(() => document.querySelector('[data-customers-account-id="acct-b"]').click());
  await settle();
  await click('[data-customers-action="edit-account"]');
  await page.waitForSelector('[data-customers-form] input[name="name"]');
  await page.fill('[data-customers-form] input[name="name"]', 'Beta halb fertig');
  // Live-Änderung während des Tippens darf die Eingabe nicht verwerfen.
  await page.evaluate(() => window.__k.sim.push('customer_accounts', { id: 'acct-z', name: 'Zeta Live', account_status: 'active', customer_stage: 'prospect', health_status: 'healthy', search_text: 'zeta live', is_deleted: false, created_at_ms: 1, updated_at_ms: 1, last_activity_at_ms: 1 }));
  await settle();
  assert.equal(await page.$eval('[data-customers-form] input[name="name"]', (el) => el.value), 'Beta halb fertig');
  const snap = await page.evaluate(async () => {
    const cloned = structuredClone(window.__app.snapshot()); // Shell-Ablauf Entwurf §5
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, { restored: cloned });
    return cloned;
  });
  assert.equal(snap.formMode, 'account-edit');
  assert.equal(snap.selectedAccountId, 'acct-b');
  assert.equal(snap.formDraft.values.name, 'Beta halb fertig');
  assert.ok(JSON.stringify(snap).length < 1024 * 1024);
  await settle();
  assert.equal(await page.$eval('[data-customers-form] input[name="name"]', (el) => el.value), 'Beta halb fertig');
  assert.equal(await page.evaluate(() => window.__app.snapshot().selectedAccountId), 'acct-b');
  assert.equal(await page.evaluate(() => window.__c.subscribers('customer_accounts')), 1, 'alte Abos sind abgemeldet');
  await click('[data-customers-action="cancel-form"]');
  await settle();
});

test('Sprung in eine andere App über api.shell.navigate statt location.hash', async () => {
  const before = await page.evaluate(() => location.hash);
  await click('[data-customers-account-id="acct-a"]');
  await click('[data-customers-detail-tab="apps"]');
  await settle();
  const nav = await page.evaluate(() => {
    const link = document.querySelector('[data-customers-action="open-linked-app"][data-link-module="documents"]');
    if (!link) return null;
    link.click();
    return window.__k.sim.navigations.at(-1);
  });
  assert.ok(nav, 'Verknüpfte-App-Zeile vorhanden und Sprung protokolliert');
  assert.equal(nav.to, 'documents');
  assert.equal(nav.params.account_id, 'acct-a');
  assert.equal(nav.params.source_module, 'customers');
  assert.equal(await page.evaluate(() => location.hash), before);
  await click('[data-customers-detail-tab="overview"]');
  await settle();
});

test('Screenshots 1280 und 420 gegen den Stub', async () => {
  await page.evaluate(() => document.querySelector('[data-customers-account-id="acct-a"]').click());
  await settle();
  await page.screenshot({ path: join(SHOTS, 'customers-1280.png'), clip: { x: 0, y: 0, width: 1280, height: 760 } });
  await page.setViewportSize({ width: 420, height: 820 });
  await page.evaluate(() => { const w = document.getElementById('win'); w.style.width = '420px'; w.style.height = '820px'; });
  await page.waitForTimeout(250);
  await page.screenshot({ path: join(SHOTS, 'customers-420.png'), clip: { x: 0, y: 0, width: 420, height: 820 } });
  const overflow = await page.$eval('#win', (el) => el.scrollWidth - el.clientWidth);
  assert.ok(overflow <= 1, `kein waagrechter Überlauf (${overflow}px)`);
  await page.setViewportSize({ width: 1280, height: 760 });
  await page.evaluate(() => { const w = document.getElementById('win'); w.style.width = '1280px'; w.style.height = '760px'; });
});

test('unmount: alle Abos abgemeldet, root leer, keine Zone, keine pageerror', async () => {
  const r = await page.evaluate(async () => {
    await window.__app.unmount();
    const names = ['customer_accounts', 'customer_contacts', 'customer_opportunities', 'outbound_companies', 'notes'];
    return {
      subs: names.map((n) => window.__c.subscribers(n)),
      children: document.getElementById('win').childElementCount,
      zone: window.__zone().map((e) => e.code),
    };
  });
  assert.deepEqual(r.subs, [0, 0, 0, 0, 0]);
  assert.equal(r.children, 0);
  assert.deepEqual(r.zone, []);
  assert.deepEqual(page.__errors, []);
});
