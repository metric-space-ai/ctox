// invoices gegen den eingefrorenen App-API-4.0-Kern-Stub (createStubKernel) in echtem Chromium.
// Stub: CTOX_APP_API_STUB (Pflicht-Pin ~/.local/state/shell-v4/stub-pins/f967baf), read-only unter /__stub/.
// Eigene Attrappe (v4/tools/port/test-api.mjs) nur für den Abo-Zähler. Muster: modules/knowledge/tests/v4-api.test.mjs.
// Lauf: CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/f967baf node --test src/apps/business-os/modules/invoices/tests/v4-api.test.mjs
// Screenshots (1280 / 420) landen in modules/invoices/tests/shots/.
import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { readFile, mkdir } from 'node:fs/promises';
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

// Die Shell lädt Basis- und App-CSS aus dem Manifest und stellt den Fenster-Container (business-app-window).
const PAGE = `<!doctype html><html><head><meta charset="utf-8">
<link rel="stylesheet" href="/ui-contract/v1/workjet-ui-contract.css">
<link rel="stylesheet" href="/app.css">
<link rel="stylesheet" href="/shared/base.css">
<link rel="stylesheet" href="/themes/ctox-desktop-shell.css">
<link rel="stylesheet" href="/modules/invoices/index.css">
<style>html,body{margin:0;background:var(--surface-1,#f4f4f2)}
#win{width:1280px;height:800px;container-name:business-app-window;container-type:inline-size;position:relative;overflow:hidden}</style>
</head><body><div id="win"></div></body></html>`;

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
      // Wie der Service Worker (§16.15): ctox:shared/x.js → Shell-Pfad (PORTING.md §4 Falle 2).
      if (/\.m?js$/.test(file)) body = Buffer.from(body.toString('utf8').replace(/(['"])ctox:shared\//g, '$1/shared/'));
      res.writeHead(200, { 'content-type': TYPES[extname(file)] || 'application/octet-stream' }); res.end(body);
    } catch { res.writeHead(404); res.end(); }
  });
  await new Promise((r) => server.listen(0, '127.0.0.1', r));
  base = `http://127.0.0.1:${server.address().port}/`;
  browser = await chromium.launch();
  page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
  const errors = [];
  page.on('pageerror', (e) => errors.push(e.message));
  page.on('console', (m) => { if (m.type() === 'error') console.error('[page]', m.text()); });
  page.on('response', (r) => { if (r.status() >= 400) console.error('[page] HTTP', r.status(), r.url()); });
  page.__errors = errors;
  await page.goto(base);
  // Befehlsrouten der echten invoices-Version (v4/manifests/invoices.manifest.json, wie customers/E35): ohne aktive
  // Version lehnt der Kern seit E26/E27 z. B. invoices.payment.allocate beim Schreiben mit unknown_command ab.
  await page.evaluate((routes) => { window.__invoicesRoutes = routes; }, realManifestRoutes('invoices'));
  await page.evaluate(async () => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    window.__mod = await import('/modules/invoices/index.js');
    const kernel = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false, userId: 'acc-1', actor: { id: 'acc-1', role: 'admin', permissions: ['apps.source.edit'] } });
    await kernel.ready;
    // FIXT-R: Eigentümer der Fremd-Collections (echte Manifeste, inkl. ctox-platform) vor der eigenen Version.
    for (const o of window.__invoicesRoutes?._fixture_owners ?? []) if (!(await kernel.sim.inspect('business_apps', o.app))?.data?.active_ver) await kernel.sim.publishAppVersion(o.app, o.ver, o.publish);
    await kernel.sim.publishAppVersion('invoices', '0.1.0', { title: 'Rechnungen', manifestExtra: window.__invoicesRoutes });
    const day = 86400000;
    const now = Date.now();
    await kernel.sim.push('customer_accounts', [{ id: 'cust_1', name: 'Acme GmbH', email: 'buchhaltung@acme.test', account_status: 'active', customer_stage: 'active', health_status: 'healthy', search_text: 'acme gmbh buchhaltung@acme.test', is_deleted: false, created_at_ms: 1, updated_at_ms: 1 }]);
    await kernel.sim.push('accounting_invoices', [
      { id: 'inv_posted', state: 'posted', invoice_type: 'sale_out', party_id: 'cust_1', invoice_number: 'RE-2026-0001', invoice_date_ms: now - 20 * day, due_date_ms: now + 10 * day, subtotal_cents: 10000, tax_cents: 1900, total_cents: 11900, paid_cents: 0, open_cents: 11900, currency: 'EUR', lines: [{ id: 'l1', position: 1, description: 'Beratung', quantity: 1000, unit: 'h', unit_price_cents: 10000, tax_rate: 0.19, account_code: '8400' }], search_text: 're-2026-0001 acme gmbh', is_deleted: false, created_at_ms: 3, updated_at_ms: 3 },
      { id: 'inv_overdue', state: 'overdue', invoice_type: 'sale_out', party_id: 'cust_1', invoice_number: 'RE-2026-0002', invoice_date_ms: now - 40 * day, due_date_ms: now - 5 * day, total_cents: 5950, paid_cents: 0, open_cents: 5950, currency: 'EUR', lines: [], search_text: 're-2026-0002 acme gmbh', is_deleted: false, created_at_ms: 2, updated_at_ms: 2 },
      { id: 'inv_draft', state: 'draft', invoice_type: 'sale_out', party_id: 'cust_1', invoice_number: '', invoice_date_ms: now, due_date_ms: now + 14 * day, total_cents: 0, currency: 'EUR', lines: [{ id: 'd1', position: 1, description: 'Entwurfsposition', quantity: 2000, unit: 'Stk', unit_price_cents: 2500, tax_rate: 0.19, account_code: '8400' }], search_text: 'acme gmbh', is_deleted: false, created_at_ms: 1, updated_at_ms: 1 },
    ]);
    await kernel.sim.settle();
    window.__k = kernel;
    window.__api = kernel.appApi('invoices', '0.1.0');
    window.__zone = () => kernel.statusZone.forApp('invoices').get().filter((e) => e.source === 'app');
    window.__commands = () => window.__api.query('business_commands', { sort: [{ created_at: 'asc' }] }).get();
  });
});

after(async () => { await browser?.close(); await new Promise((r) => server?.close(r)); });

const settle = async () => { await page.evaluate(() => window.__k.sim.settle()); await page.waitForTimeout(250); };
const text = (sel) => page.$eval(sel, (el) => el.textContent);
const clickSel = (sel) => page.evaluate((s) => document.querySelector(s).click(), sel);
const remount = (opts = {}) => page.evaluate(async (o) => {
  const { withDialogAnswer } = await import('/v4/tools/port/test-api.mjs');
  await window.__app.unmount();
  const api = o.dialog === undefined ? window.__c.api : withDialogAnswer(window.__c.api, o.dialog);
  window.__app = await window.__mod.mount(document.getElementById('win'), api, {});
}, opts);
const tab = (id) => page.evaluate((t) => [...document.querySelectorAll('[data-tab]')].find((b) => b.dataset.tab === t).click(), id);

test('mount: rendert Live-Daten, nur api.*, gescoptes Root, Titel über api.shell', async () => {
  await page.evaluate(async () => {
    const { countingApi } = await import('/v4/tools/port/test-api.mjs');
    window.__c = countingApi(window.__api);
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, {});
  });
  await settle();
  const list = await text('[data-invoices-list]');
  assert.match(list, /RE-2026-0001/);
  assert.match(list, /RE-2026-0002/);
  assert.match(list, /Acme GmbH/);
  assert.equal(await page.$eval('#win', (el) => el.dataset.app), 'invoices');
  assert.equal(await page.evaluate(() => window.__k.shell.window('invoices').state.get().title), 'Rechnungen');
  assert.equal(await page.evaluate(() => window.__c.subscribers('accounting_invoices')), 1);
  assert.equal((await text('[data-pg-count="ueberfaellig"]')).trim(), '(1)');
  assert.equal((await text('[data-pg-count="alle"]')).trim(), '(3)');
  assert.equal(await page.$eval('[data-invoices-rail]', (el) => el.dataset.pgWired), 'true');
  // Wie Outlook/lexoffice: beim Öffnen ist der erste Eintrag gewählt (kein leerer Arbeitsbereich), kein Fehlerstreifen.
  assert.match(await text('[data-invoices-main-body]'), /RE-2026-0001/);
  assert.equal(await page.$eval('[data-invoice-id="inv_posted"]', (el) => el.getAttribute('aria-selected')), 'true');
  assert.equal(await page.$('[data-invoices-banner]'), null);
  assert.deepEqual(page.__errors, []);
});

test('Live-Änderung vom Core (sim.push) erscheint ohne Neuladen, Löschung verschwindet', async () => {
  await page.evaluate(() => window.__k.sim.push('accounting_invoices', { id: 'inv_live', state: 'paid', invoice_type: 'sale_out', party_id: 'party-404', invoice_number: 'RE-LIVE-9', total_cents: 100, currency: 'EUR', search_text: 're-live-9', is_deleted: false, created_at_ms: 9, updated_at_ms: 9 }));
  await settle();
  assert.match(await text('[data-invoices-list]'), /RE-LIVE-9/);
  // Kunde ohne Kontakt: „Unbekannt“, nie die technische ID.
  assert.match(await text('[data-invoices-list]'), /Unbekannt/);
  assert.doesNotMatch(await text('[data-invoices-list]'), /party-404/);
  assert.equal((await text('[data-pg-count="bezahlt"]')).trim(), '(1)');
  await page.evaluate(() => window.__k.sim.push('accounting_invoices', { id: 'inv_live', _deleted: true }));
  await settle();
  assert.doesNotMatch(await text('[data-invoices-list]'), /RE-LIVE-9/);
});

test('leere Collections rendern keinen Zustandstext und keine Zone', async () => {
  const empty = await page.evaluate(async () => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    const k2 = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false, instanceId: 'empty' });
    await k2.ready;
    const host = document.createElement('div');
    document.body.append(host);
    const app = await window.__mod.mount(host, k2.appApi('invoices', '0.1.0'), {});
    await new Promise((r) => setTimeout(r, 300));
    const out = {
      listText: host.querySelector('[data-invoices-list]').textContent.trim(),
      mainText: host.querySelector('[data-invoices-main-body]').textContent.trim(),
      hasStateBox: Boolean(host.querySelector('.ctox-syncing, .ctox-empty, [role="status"], .invoices-error-banner')),
      zone: k2.statusZone.forApp('invoices').get().map((e) => e.code),
    };
    await app.unmount();
    host.remove();
    await k2.close();
    return out;
  });
  // Leere Liste: genau ein Satz plus die Aktion „Neue Rechnung“, kein Lade-/Zustandstext, keine Zone.
  assert.deepEqual(empty, { listText: '', mainText: 'Noch keine Rechnungen.Neue Rechnung', hasStateBox: false, zone: [] });
  // Modulzustand ist modulweit (V4-PORT.md OFFEN-4): Hauptfenster neu montieren.
  await remount();
  await settle();
});

test('Zahlung zuordnen: ohne Zahlungs-ID kein leeres payment_id; failed → genau ein Zoneneintrag', async () => {
  await page.evaluate(() => window.__k.sim.failCommand('invoices.payment.allocate', 'allocation exceeds open invoice balance'));
  await clickSel('[data-invoice-id="inv_posted"]');
  await settle();
  await tab('payments');
  // Eingabe in Euro (deutsch), Befehl in Cent.
  await page.fill('.invoices-payment-form [name="amount_eur"]', '50,00');
  await page.fill('.invoices-payment-form [name="payment_id"]', '   ');
  // Formular-Absenden (required-Prüfung des Browsers umgangen: es geht um den Befehl, nicht um die Eingabe).
  await page.evaluate(() => document.querySelector('.invoices-payment-form').dispatchEvent(new Event('submit', { cancelable: true, bubbles: true })));
  await settle();
  const cmd = await page.evaluate(() => window.__commands().at(-1));
  assert.equal(cmd.type, 'invoices.payment.allocate');
  assert.equal(cmd.app_id, 'invoices');
  assert.equal(cmd.record_id, 'inv_posted');
  assert.equal(Object.hasOwn(cmd.payload, 'payment_id'), false, 'keine leere Zahlungs-ID');
  assert.equal(cmd.payload.invoice_id, 'inv_posted');
  assert.equal(cmd.payload.amount_cents, 5000);
  assert.equal(cmd.status, 'failed');
  const zone = await page.evaluate(() => window.__zone());
  assert.equal(zone.length, 1);
  assert.equal(zone[0].code, 'invoices.command.invoices.payment.allocate');
  assert.match(zone[0].text, /exceeds/);
  // Mit Kennung wird sie mitgeschickt.
  await page.fill('.invoices-payment-form [name="payment_id"]', 'pay_42');
  await page.evaluate(() => document.querySelector('.invoices-payment-form').dispatchEvent(new Event('submit', { cancelable: true, bubbles: true })));
  await settle();
  const cmd2 = await page.evaluate(() => window.__commands().at(-1));
  assert.equal(cmd2.payload.payment_id, 'pay_42');
  assert.equal(cmd2.status, 'done');
  assert.equal((await page.evaluate(() => window.__zone())).length, 0, 'done räumt den Zoneneintrag');
});

test('Mahnlauf: invoices.dunning.run mit filter.invoice_id (Handler wertet ihn nicht aus, OFFEN-1)', async () => {
  await clickSel('[data-invoice-id="inv_overdue"]');
  await settle();
  await tab('dunning');
  await page.evaluate(() => document.querySelector('.invoices-tab button.is-primary').click());
  await settle();
  const cmd = await page.evaluate(() => window.__commands().at(-1));
  assert.equal(cmd.type, 'invoices.dunning.run');
  assert.match(cmd.payload.run_id, /^dunning_/);
  assert.equal(cmd.record_id, cmd.payload.run_id);
  assert.deepEqual(cmd.payload.filter, { invoice_id: 'inv_overdue' });
});

test('Rundgang RUND2: nicht überfällige Rechnung – kein Reiter „Mahnen“, kein Zustandstext', async () => {
  await clickSel('[data-invoice-id="inv_posted"]');
  await settle();
  const r = await page.evaluate(() => ({
    dunningTab: Boolean(document.querySelector('#win [data-tab="dunning"]')),
    text: document.querySelector('#win').innerText.includes('nur für überfällige'),
  }));
  assert.deepEqual(r, { dunningTab: false, text: false });
  await clickSel('[data-invoice-id="inv_overdue"]');
  await settle();
  assert.equal(await page.evaluate(() => Boolean(document.querySelector('#win [data-tab="dunning"]'))), true);
});

test('Entwurf: Speichern → update; Buchen → update, danach post erst nach done', async () => {
  await clickSel('[data-invoice-id="inv_draft"]');
  await settle();
  const desc = '.invoices-lines-table tbody tr td:nth-child(2) input';
  // Menge in Stück, Einzelpreis in Euro (deutsch); intern Tausendstel bzw. Cent.
  const qty = '.invoices-lines-table tbody tr td:nth-child(3) input';
  const price = '.invoices-lines-table tbody tr td:nth-child(5) input';
  assert.deepEqual(await page.evaluate(([q, p]) => [document.querySelector(q).value, document.querySelector(p).value], [qty, price]), ['2', '25,00']);
  assert.doesNotMatch(await page.$eval('.invoices-lines-table thead', (el) => el.textContent), /‰|Cent/);
  await page.fill(desc, 'Workshop');
  await page.dispatchEvent(desc, 'change');
  await page.evaluate(() => [...document.querySelectorAll('.invoices-actions .ctox-button')].find((b) => !b.classList.contains('is-primary')).click());
  await settle();
  const upd = await page.evaluate(() => window.__commands().at(-1));
  assert.equal(upd.type, 'invoices.invoice.update');
  assert.equal(upd.record_id, 'inv_draft');
  assert.equal(upd.payload.invoice_id, 'inv_draft');
  assert.equal(upd.payload.lines[0].description, 'Workshop');
  assert.equal(upd.payload.total_cents, 5950);
  assert.equal(upd.payload.lines[0].quantity, 2000);
  assert.equal(upd.payload.lines[0].unit_price_cents, 2500);
  assert.match(await page.$eval('.invoices-totals', (el) => el.textContent), /59,50\s€/);
  const before = await page.evaluate(() => window.__commands().length);
  await page.evaluate(() => document.querySelector('.invoices-actions .ctox-button.is-primary').click());
  await settle();
  const types = await page.evaluate((n) => window.__commands().slice(n).map((c) => c.type), before);
  assert.deepEqual(types, ['invoices.invoice.update', 'invoices.invoice.post']);
  const post = await page.evaluate(() => window.__commands().at(-1));
  assert.deepEqual({ invoice_id: post.payload.invoice_id, record_id: post.record_id }, { invoice_id: 'inv_draft', record_id: 'inv_draft' });
});

test('Entwurf löschen fragt über api.shell.dialog, ohne Ja kein Befehl', async () => {
  const before = await page.evaluate(() => window.__commands().length);
  await page.evaluate(() => document.querySelector('[data-invoices-main-actions] [data-action="delete"]').click());
  await settle();
  assert.equal(await page.evaluate(() => window.__commands().length), before, 'Stub-Dialog liefert default:false');
  assert.ok(await page.evaluate(() => window.__k.sim.dialogs.length) >= 1);
  await remount({ dialog: true });
  await settle();
  await clickSel('[data-invoice-id="inv_draft"]');
  await settle();
  await page.evaluate(() => document.querySelector('[data-invoices-main-actions] [data-action="delete"]').click());
  await settle();
  const cmd = await page.evaluate(() => window.__commands().at(-1));
  assert.equal(cmd.type, 'invoices.invoice.delete');
  assert.equal(cmd.payload.invoice_id, 'inv_draft');
  await remount();
  await settle();
});

test('Import: ungültiges JSON → Zone; gültig → invoices.invoice.create', async () => {
  const r = await page.evaluate(() => {
    const bad = window.__mod.__invoicesTestHooks.importInvoicesText('{kaputt');
    const zoneBad = window.__zone().map((e) => e.code);
    const good = window.__mod.__invoicesTestHooks.importInvoicesText(JSON.stringify([{ party_id: 'cust_1', lines: [{ description: 'Import', quantity: 1000, unit_price_cents: 100 }] }]));
    return { bad, zoneBad, good, zoneAfter: window.__zone().map((e) => e.code) };
  });
  assert.equal(r.bad, 0);
  assert.ok(r.zoneBad.includes('invoices.import.invalid'));
  assert.equal(r.good, 1);
  assert.ok(!r.zoneAfter.includes('invoices.import.invalid'));
  const cmd = await page.evaluate(() => window.__commands().at(-1));
  assert.equal(cmd.type, 'invoices.invoice.create');
  assert.equal(cmd.payload.party_id, 'cust_1');
});

test('Schreiben: api.put erscheint live und wird vom Core angenommen', async () => {
  const result = await page.evaluate(async () => {
    const h = window.__api.put('accounting_invoices', { id: 'inv_put', state: 'posted', invoice_type: 'sale_out', party_id: 'cust_1', invoice_number: 'RE-PUT-1', total_cents: 1, currency: 'EUR', search_text: 're-put-1 acme gmbh', is_deleted: false, created_at_ms: 20, updated_at_ms: 20 });
    await h.local;
    return h.result;
  });
  assert.equal(result.status, 'accepted');
  await settle();
  assert.match(await text('[data-invoices-list]'), /RE-PUT-1/);
});

test('snapshot/restored: offener Entwurf, Auswahl und Suche überleben den Remount', async () => {
  await page.evaluate(() => window.__k.sim.push('accounting_invoices', { id: 'inv_draft2', state: 'draft', invoice_type: 'sale_out', party_id: 'cust_1', invoice_number: '', invoice_date_ms: Date.now(), total_cents: 0, currency: 'EUR', lines: [{ id: 'x1', position: 1, description: 'Alt', quantity: 1000, unit: 'Stk', unit_price_cents: 100, tax_rate: 0.19, account_code: '8400' }], search_text: 'acme gmbh', is_deleted: false, created_at_ms: 30, updated_at_ms: 30 }));
  await settle();
  await clickSel('[data-invoice-id="inv_draft2"]');
  await settle();
  const desc = '.invoices-lines-table tbody tr td:nth-child(2) input';
  await page.fill(desc, 'halb getippt');
  await page.fill('[data-pg-search]', 'acme');
  await settle();
  const snap = await page.evaluate(async () => {
    const cloned = structuredClone(window.__app.snapshot());
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, { restored: cloned });
    return cloned;
  });
  assert.equal(snap.selectedInvoiceId, 'inv_draft2');
  assert.equal(snap.edit.dirty, true);
  assert.equal(snap.edit.inv.lines[0].description, 'halb getippt');
  assert.equal(snap.search, 'acme');
  assert.ok(JSON.stringify(snap).length < 1024 * 1024);
  await settle();
  const after = await page.evaluate((sel) => ({
    desc: document.querySelector(sel)?.value,
    search: document.querySelector('[data-pg-search]').value,
    selected: document.querySelector('[data-invoice-id].is-selected')?.getAttribute('data-invoice-id'),
  }), desc);
  assert.deepEqual(after, { desc: 'halb getippt', search: 'acme', selected: 'inv_draft2' });
  // Eine Live-Änderung am Entwurf überschreibt die offene Eingabe nicht.
  await page.evaluate(() => window.__k.sim.push('accounting_invoices', { id: 'inv_draft2', state: 'draft', invoice_type: 'sale_out', party_id: 'cust_1', invoice_number: '', total_cents: 0, currency: 'EUR', lines: [{ id: 'x1', position: 1, description: 'Server', quantity: 1000, unit: 'Stk', unit_price_cents: 100, tax_rate: 0.19, account_code: '8400' }], search_text: 'acme gmbh', is_deleted: false, created_at_ms: 31, updated_at_ms: 31 }));
  await settle();
  assert.equal(await page.$eval(desc, (el) => el.value), 'halb getippt');
  assert.equal(await page.evaluate(() => window.__c.subscribers('accounting_invoices')), 1, 'alte Abos sind abgemeldet');
  assert.deepEqual(page.__errors, []);
});

test('FIX9a: „Neue Rechnung“ öffnet einen leeren Editor ohne Befehl; Abbrechen hinterlässt nichts; erst „Entwurf speichern“ legt an', async () => {
  const count = () => page.evaluate(() => window.__commands().length);
  const rows = () => page.evaluate(() => document.querySelectorAll('[data-invoices-list] [data-invoice-id]').length);
  const before = await count();
  const rowsBefore = await rows();
  await clickSel('[data-invoices-rail] [data-action="new"]');
  await settle();
  assert.equal(await count(), before, 'Öffnen legt nichts an');
  assert.ok(await page.$('[data-invoices-main-body] .invoices-editor'), 'Editor offen');
  assert.equal(await page.$eval('[data-invoices-main-body] .invoices-editor select', (el) => el.value), '', 'Kunde leer');
  assert.equal(await page.$eval('[data-invoices-main-body] .invoices-lines-table tbody', (el) => el.children.length), 0, 'keine Positionen');
  assert.equal(await page.$('[data-invoices-main-actions] [data-action="delete"]'), null, 'nichts zu löschen');
  await clickSel('[data-invoices-main-body] [data-action="cancel-new"]');
  await settle();
  assert.equal(await page.$('[data-invoices-main-body] .invoices-editor'), null, 'Abbrechen schließt');
  assert.equal(await count(), before, 'Abbrechen hinterlässt keinen Befehl');
  assert.equal(await rows(), rowsBefore, 'Abbrechen hinterlässt keine Zeile');

  await clickSel('[data-invoices-rail] [data-action="new"]');
  await settle();
  await page.selectOption('[data-invoices-main-body] .invoices-editor select', 'cust_1');
  await settle();
  await page.evaluate(() => [...document.querySelectorAll('.invoices-actions .ctox-button')].find((b) => !b.classList.contains('is-primary') && !b.dataset.action).click());
  await settle();
  const created = await page.evaluate((n) => window.__commands().slice(n), before);
  assert.deepEqual(created.map((c) => c.type), ['invoices.invoice.create']);
  assert.equal(created[0].payload.party_id, 'cust_1');
  assert.equal(created[0].payload.invoice_type, 'sale_out');
  assert.equal(created[0].payload.invoice_id, created[0].record_id);
  assert.deepEqual(page.__errors, []);
});

test('FIX9a: Suche ohne Treffer → „Keine Treffer“ + „Filter zurücksetzen“; Status in der Listenansicht nicht abgeschnitten', async () => {
  await page.fill('[data-pg-search]', 'zzzz-kein-treffer');
  await settle();
  const listText = await text('[data-invoices-list]');
  assert.match(listText, /Keine Treffer/);
  assert.doesNotMatch(listText, /Noch keine/);
  assert.match(await text('[data-pg-count="alle"]'), /\(\d+\)/);
  await clickSel('[data-invoices-list] [data-action="reset-filters"]');
  await settle();
  assert.equal(await page.$eval('[data-pg-search]', (el) => el.value), '');
  assert.ok(await page.evaluate(() => document.querySelectorAll('[data-invoices-list] [data-invoice-id]').length) > 0);
  assert.doesNotMatch(await text('[data-invoices-list]'), /Keine Treffer/);

  // Listenansicht in schmaler Spalte mit langem Status „Teilweise bezahlt“.
  await page.evaluate(() => window.__k.sim.push('accounting_invoices', { id: 'inv_part', state: 'partially_paid', invoice_type: 'sale_out', party_id: 'cust_1', invoice_number: 'RE-2026-0003-LANGE-NUMMER-MIT-ZUSATZ', total_cents: 100, paid_cents: 50, open_cents: 50, currency: 'EUR', lines: [], search_text: 're-2026-0003-lange-nummer-mit-zusatz acme gmbh', is_deleted: false, created_at_ms: 99, updated_at_ms: 99 }));
  await settle();
  await page.evaluate(() => { const b = document.querySelector('[data-invoices-rail] [data-pg-view="list"]'); if (b) b.click(); });
  await page.$eval('[data-invoices-rail]', (el) => { el.style.width = '230px'; el.style.maxWidth = '230px'; });
  await settle();
  const cut = await page.evaluate(() => [...document.querySelectorAll('[data-invoices-list] [data-invoice-id]')].map((row) => {
    const b = row.querySelector('.ctox-badge');
    const r = row.getBoundingClientRect();
    const br = b.getBoundingClientRect();
    return { id: row.dataset.invoiceId, text: b.textContent, clipped: b.scrollWidth > Math.ceil(br.width) + 1 || br.right > r.right + 0.5 };
  }).filter((x) => x.clipped));
  assert.deepEqual(cut, [], 'Status abgeschnitten');
  await page.$eval('[data-invoices-rail]', (el) => { el.style.width = ''; el.style.maxWidth = ''; });
  await page.evaluate(() => window.__k.sim.push('accounting_invoices', { id: 'inv_part', _deleted: true }));
  await settle();
});

test('FIX10: Positionen bei 420 als Karten „Menge × Preis = Betrag“ ohne Seitwärts-Scroll; breit unverändert als Tabelle', async () => {
  const measure = () => page.evaluate(() => {
    const wrap = [...document.querySelectorAll('#win .invoices-main .ctox-table-wrap')].find((el) => el.querySelector('.invoices-lines-view'));
    const row = wrap?.querySelector('tbody tr');
    const amount = row?.querySelector('[data-col="amount"]');
    const head = wrap?.querySelector('thead');
    return {
      overflow: wrap ? wrap.scrollWidth - wrap.clientWidth : null,
      width: wrap?.clientWidth ?? 0,
      // Sichtbarer Text samt ::before (× / = / Beschriftung), Zellen in DOM-Reihenfolge.
      text: [...(row?.children || [])].filter((td) => getComputedStyle(td).display !== 'none').map((td) => {
        const before = getComputedStyle(td, '::before').content;
        const pre = before && before !== 'none' && before !== 'normal' ? before.replace(/^"|"$/g, '') : '';
        return `${pre} ${td.querySelector('input')?.value ?? td.textContent}`;
      }).join(' ').replace(/\s+/g, ' ').trim(),
      // Menge, Preis und Betrag stehen in einer Zeile.
      sameLine: (() => {
        const tops = ['qty', 'price', 'amount'].map((c) => {
          const r = row?.querySelector(`[data-col="${c}"]`)?.getBoundingClientRect();
          return r ? (r.top + r.bottom) / 2 : -100;
        });
        return Math.max(...tops) - Math.min(...tops) <= 4;
      })(),
      amountShown: amount ? getComputedStyle(amount).display !== 'none' : false,
      amount: amount?.textContent || '',
      headShown: head ? getComputedStyle(head).display !== 'none' : false,
    };
  });
  try {
    await page.$eval('#win', (el) => { el.style.width = '420px'; });
    await clickSel('[data-invoice-id="inv_posted"]');
    await settle();
    const view = await measure();
    assert.ok(view.width > 0, 'Positionen sichtbar');
    assert.ok(view.overflow <= 1, `Detail-Positionen scrollen seitlich: ${JSON.stringify(view)}`);
    assert.equal(view.headShown, false);
    assert.equal(view.amountShown, true);
    assert.match(view.text, /^Beratung 1 h × 100,00\s€ .*= 100,00\s€$/, view.text);
    assert.equal(view.sameLine, true, 'Menge × Preis = Betrag in einer Zeile');
    // Editor (Entwurf): ebenfalls Karten, der Betrag folgt der Eingabe sofort.
    await clickSel('[data-invoice-id="inv_draft"]');
    await settle();
    const edit = await measure();
    assert.ok(edit.width > 0 && edit.overflow <= 1, `Editor-Positionen scrollen seitlich: ${JSON.stringify(edit)}`);
    assert.equal(edit.amountShown, true);
    assert.match(edit.amount, /50,00\s€/);
    assert.equal(edit.sameLine, true);
    await page.fill('.invoices-lines-table [data-col="qty"] input', '3');
    assert.match((await measure()).amount, /75,00\s€/);
    const labels = await page.$$eval('.invoices-lines-table tbody tr input', (els) => els.map((el) => el.getAttribute('aria-label')));
    assert.ok(labels.every(Boolean), `jedes Feld trägt eine Beschriftung: ${labels}`);
    // Breit: Tabelle mit Kopf, ohne Zusatzspalte.
    await page.$eval('#win', (el) => { el.style.width = '1280px'; });
    await page.waitForTimeout(50);
    const wide = await measure();
    assert.equal(wide.headShown, true);
    assert.equal(wide.amountShown, false);
  } finally {
    await page.$eval('#win', (el) => { el.style.width = '1280px'; });
    await remount();
    await settle();
  }
  assert.deepEqual(page.__errors ?? [], []);
});

test('Screenshots 1280 und 420 (gegen den Stub)', async () => {
  await mkdir(SHOTS, { recursive: true });
  await page.fill('[data-pg-search]', '');
  await page.dispatchEvent('[data-pg-search]', 'input');
  await clickSel('[data-invoice-id="inv_posted"]');
  await settle();
  for (const [w, h] of [[1280, 800], [420, 860]]) {
    await page.setViewportSize({ width: w, height: h });
    await page.$eval('#win', (el, size) => { el.style.width = `${size[0]}px`; el.style.height = `${size[1]}px`; }, [w, h]);
    await page.waitForTimeout(300);
    const overflow = await page.$eval('#win', (el) => el.scrollWidth - el.clientWidth);
    assert.ok(overflow <= 1, `kein horizontaler Überlauf bei ${w}px (gemessen ${overflow})`);
    await page.locator('#win').screenshot({ path: join(SHOTS, `invoices-${w}.png`) });
  }
  await clickSel('[data-invoice-id="inv_draft2"]');
  await page.waitForTimeout(200);
  await page.locator('#win').screenshot({ path: join(SHOTS, 'invoices-420-draft.png') });
});

test('unmount meldet alle Abos ab und leert root', async () => {
  const res = await page.evaluate(async () => {
    await window.__app.unmount();
    await window.__k.sim.push('accounting_invoices', { id: 'inv_late', state: 'posted', invoice_number: 'LATE', currency: 'EUR', search_text: 'late', is_deleted: false, invoice_type: 'sale_out', created_at_ms: 99, updated_at_ms: 99 });
    await window.__k.sim.settle();
    return {
      subs: ['accounting_invoices', 'customer_accounts', 'accounting_journal_entries'].map((c) => window.__c.subscribers(c)),
      children: document.getElementById('win').children.length,
    };
  });
  assert.deepEqual(res.subs, [0, 0, 0]);
  assert.equal(res.children, 0);
  assert.deepEqual(page.__errors, []);
});
