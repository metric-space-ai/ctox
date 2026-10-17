// buchhaltung gegen den eingefrorenen App-API-4.0-Kern-Stub (createStubKernel) in echtem Chromium.
// Muster: modules/knowledge/tests/v4-api.test.mjs. Stub-Pfad aus CTOX_APP_API_STUB (Pflicht-Pin
// ~/.local/state/shell-v4/stub-pins/4b0d0ac), read-only unter /__stub/ ausgeliefert.
// Lauf: CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/4b0d0ac node --test src/apps/business-os/modules/buchhaltung/tests/v4-api.test.mjs
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

const APP = 'buchhaltung';
const STUB = process.env.CTOX_APP_API_STUB || join(homedir(), '.local/state/shell-v4/stub-pins/4b0d0ac');
const HERE = dirname(fileURLToPath(import.meta.url));
const BOS = normalize(join(HERE, '../../..'));
const require = createRequire(import.meta.url);
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || join(homedir(), 'Documents/ctox/src/apps/business-os/node_modules/playwright'));
const TYPES = { '.js': 'text/javascript', '.mjs': 'text/javascript', '.css': 'text/css', '.html': 'text/html', '.json': 'application/json', '.svg': 'image/svg+xml' };
const COLLECTIONS = ['accounting_accounts', 'accounting_journal_entries', 'accounting_journal_entry_lines', 'accounting_receipts', 'accounting_bank_statements', 'accounting_bank_statement_lines'];

const PAGE = `<!doctype html><html lang="de"><head><meta charset="utf-8">
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
  // FIXT-8265 (E48): echte Routen/Rechte/Collections aus v4/manifests/buchhaltung.manifest.json (business_commands ist dort
  // als Fremdlesen von ctox-platform deklariert); ohne veröffentlichte Version sperrt der Kern Plattform-Collections.
  await page.evaluate((routes) => { window.__routes = routes; }, realManifestRoutes(APP));
  await page.evaluate(async (APP) => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    window.__mod = await import(`/modules/${APP}/index.js`);
    const kernel = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false });
    await kernel.ready;
    await kernel.sim.push('business_apps', [{ id: APP, title: 'Buchhaltung' }]);
    for (const o of window.__routes?._fixture_owners ?? []) if (!(await kernel.sim.inspect('business_apps', o.app))?.data?.active_ver) await kernel.sim.publishAppVersion(o.app, o.ver, o.publish);
    await kernel.sim.publishAppVersion(APP, '1', { title: 'Buchhaltung', manifestExtra: window.__routes });
    await kernel.sim.push('accounting_journal_entries', [
      { id: 'je-1', posting_date: '2026-10-01', type: 'journal', number: 'J-2026-0001', narration: 'Alpha Miete Oktober', posted_at: 1, updated_at_ms: 2 },
      { id: 'je-2', posting_date: '2026-10-02', type: 'journal', number: 'J-2026-0002', narration: 'Beta Büromaterial', updated_at_ms: 1 },
    ]);
    await kernel.sim.push('accounting_journal_entry_lines', [
      { id: 'je-1-l0', journal_entry_id: 'je-1', account_id: 'SKR03_4210', debit: 120000, credit: 0, line_no: 0, updated_at_ms: 1 },
      { id: 'je-1-l1', journal_entry_id: 'je-1', account_id: 'SKR03_1200', debit: 0, credit: 120000, line_no: 1, updated_at_ms: 1 },
    ]);
    await kernel.sim.settle();
    window.__k = kernel;
    window.__api = kernel.appApi(APP, '1');
    window.__zone = () => kernel.statusZone.forApp(APP).get().filter((e) => e.source === 'app');
    window.__commands = () => window.__api.query('business_commands', { sort: [{ created_at: 'asc' }] }).get();
  }, APP);
});

after(async () => { await browser?.close(); await new Promise((r) => server?.close(r)); });

const settle = async () => { await page.evaluate(() => window.__k.sim.settle()); await page.waitForTimeout(250); };
const listText = () => page.$eval('[data-booking-list]', (el) => el.textContent);

test('mount: rendert Live-Daten, nur api.*, gescoptes Root, Titel über api.shell', async () => {
  await page.evaluate(async () => {
    const { countingApi } = await import('/v4/tools/port/test-api.mjs');
    window.__c = countingApi(window.__api);
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, {});
  });
  await settle();
  const list = await listText();
  assert.match(list, /Alpha Miete Oktober/);
  assert.match(list, /Beta Büromaterial/);
  assert.equal(await page.$eval('#win', (el) => el.dataset.app), 'buchhaltung');
  assert.equal(await page.evaluate(() => window.__k.shell.window('buchhaltung').state.get().title), 'Buchhaltung');
  assert.equal(await page.evaluate(() => window.__c.subscribers('accounting_journal_entries')), 1);
  // Kontenrahmen-Initialisierung schreibt lokal über api.put (Fachverhalten unverändert).
  assert.ok(await page.evaluate(() => window.__api.query('accounting_accounts', {}).get().length) >= 40);
  assert.deepEqual(page.__errors, []);
});

test('Live-Änderung vom Core (sim.push) erscheint ohne Neuladen, Löschung verschwindet', async () => {
  await page.evaluate(() => window.__k.sim.push('accounting_journal_entries', { id: 'je-live', posting_date: '2026-10-03', type: 'journal', number: 'J-LIVE', narration: 'Gamma Live', updated_at_ms: 3 }));
  await settle();
  assert.match(await listText(), /Gamma Live/);
  await page.evaluate(() => window.__k.sim.push('accounting_journal_entries', { id: 'je-live', _deleted: true }));
  await settle();
  assert.doesNotMatch(await listText(), /Gamma Live/);
});

test('leere Collections: kein Lade-/Leertext, keine Zone', async () => {
  // Modulzustand und window.*-Handler sind modulweit (V4-PORT.md OFFEN-4): Hauptfenster abmelden, leer montieren,
  // danach wieder montieren.
  const res = await page.evaluate(async (APP) => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    await window.__app.unmount();
    const k2 = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false, instanceId: 'empty' });
    await k2.ready;
    const host = document.createElement('div');
    document.body.append(host);
    const app = await window.__mod.mount(host, k2.appApi(APP, '1'), {});
    await new Promise((r) => setTimeout(r, 120));
    await k2.sim.settle();
    await new Promise((r) => setTimeout(r, 200));
    // Keine Musterbuchungen/-anlagen in echten Büchern: Ansehen schreibt kein Journal.
    const api2 = k2.appApi(APP, '1');
    const journal = api2.query('accounting_journal_entries', {}).get().length + api2.query('accounting_journal_entry_lines', {}).get().length;
    const mockText = /MacBook|Aeron|Server Cluster|99\.4|Stammkapital/.test(host.textContent);
    const stateBox = Boolean(host.querySelector('.ctox-syncing, [role="status"]'));
    const text = host.textContent;
    const zone = k2.statusZone.forApp(APP).get().filter((e) => e.source === 'app').map((e) => e.code);
    await app.unmount();
    host.remove();
    await k2.close();
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, {});
    return { stateBox, synced: /synchronisiert|wird geladen|Syncing/i.test(text), zone, journal, mockText };
  }, APP);
  await settle();
  assert.equal(res.stateBox, false);
  assert.equal(res.synced, false);
  assert.deepEqual(res.zone, []);
  assert.equal(res.journal, 0, 'kein Muster-Journal beim ersten Öffnen');
  assert.equal(res.mockText, false, 'keine Musteranlagen/feste Konfidenz');
});

test('Befehl: AfA-Lauf → business_commands-Dokument; failed → genau ein Zoneneintrag', async () => {
  await page.evaluate(() => window.__k.sim.failCommand('business_os.data.modify', 'kaputt'));
  await page.evaluate(() => document.querySelector('[data-panel="assets"] [data-action="trigger-depreciations"]').click());
  await settle();
  const cmd = await page.evaluate(() => window.__commands().at(-1));
  assert.equal(cmd.type, 'business_os.data.modify');
  assert.match(cmd.payload.instruction, /AfA-Abschreibungslauf/);
  assert.equal(cmd.payload.record_id, 'buchhaltung');
  assert.equal(cmd.status, 'failed');
  const zone = await page.evaluate(() => window.__zone());
  assert.equal(zone.length, 1);
  assert.equal(zone[0].code, 'buchhaltung.command.business_os.data.modify');
  assert.match(zone[0].text, /kaputt/);
});

test('Schreiben mit Bestätigung: Storno fragt über api.shell.dialog; mit Ja api.put → accepted', async () => {
  const before = await page.evaluate(() => window.__api.query('accounting_journal_entries', {}).get().length);
  await page.evaluate(() => window.triggerStorno('je-1'));
  await settle();
  assert.equal(await page.evaluate(() => window.__api.query('accounting_journal_entries', {}).get().length), before, 'Stub-Dialog liefert default:false → nichts');
  assert.equal(await page.evaluate(() => window.__k.sim.dialogs.length), 1);
  await page.evaluate(async () => {
    const { withDialogAnswer } = await import('/v4/tools/port/test-api.mjs');
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), withDialogAnswer(window.__c.api, true), {});
  });
  await settle();
  await page.evaluate(() => window.triggerStorno('je-1'));
  await settle();
  const res = await page.evaluate(() => {
    const rows = window.__api.query('accounting_journal_entries', {}).get();
    return { storno: rows.find((r) => r.type === 'storno' && r.ref_id === 'je-1'), original: rows.find((r) => r.id === 'je-1') };
  });
  assert.ok(res.storno, 'Stornobuchung angelegt');
  assert.equal(res.original.reversed_by_id, res.storno.id);
  const outbox = await page.evaluate(async () => (await window.__k.sim.outbox()).length);
  assert.equal(outbox, 0, 'alles vom Core angenommen');
  const zone = await page.evaluate(() => window.__zone().map((e) => e.code));
  assert.ok(zone.includes('buchhaltung.booking.storno'), 'Erfolgsmeldung in der Zustandszone statt alert()');
  assert.ok(!zone.some((c) => c.startsWith('write.rejected')));
});

test('snapshot/restored: Ansicht, Auswahl und offene Eingabe überleben den Remount', async () => {
  await page.evaluate(() => { const sel = document.querySelector('[data-workbench-view]'); sel.value = 'journal'; sel.dispatchEvent(new Event('change', { bubbles: true })); });
  await page.evaluate(() => { document.querySelector('[data-pg-search]').value = 'halb fertige Suche'; });
  const snap = await page.evaluate(async () => {
    const cloned = structuredClone(window.__app.snapshot());
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, { restored: cloned });
    return cloned;
  });
  assert.equal(snap.activeNav, 'journal');
  assert.equal(snap.inputs['[data-pg-search]'], 'halb fertige Suche');
  assert.ok(JSON.stringify(snap).length < 1024 * 1024);
  await settle();
  const after = await page.evaluate(() => ({
    search: document.querySelector('[data-pg-search]').value,
    nav: window.__app.snapshot().activeNav,
    view: document.querySelector('[data-workbench-view]').value,
  }));
  assert.equal(after.search, 'halb fertige Suche');
  assert.equal(after.nav, 'journal');
  assert.equal(after.view, 'journal');
  assert.equal(await page.evaluate(() => window.__c.subscribers('accounting_journal_entries')), 1, 'alte Abos sind abgemeldet');
  assert.deepEqual(page.__errors, []);
});

test('Screenshots 1280/420', async () => {
  await mkdir(join(HERE, 'shots'), { recursive: true });
  await page.evaluate(() => { const sel = document.querySelector('[data-workbench-view]'); sel.value = 'journal'; sel.dispatchEvent(new Event('change', { bubbles: true })); });
  await settle();
  await page.screenshot({ path: join(HERE, 'shots', `${APP}-1280.png`) });
  await page.setViewportSize({ width: 420, height: 860 });
  await page.$eval('#win', (el) => { el.style.width = '420px'; el.style.height = '860px'; });
  await page.waitForTimeout(250);
  await page.screenshot({ path: join(HERE, 'shots', `${APP}-420.png`) });
  const overflow = await page.$eval('#win', (el) => el.scrollWidth - el.clientWidth);
  assert.ok(overflow <= 1, `kein horizontaler Überlauf (${overflow}px)`);
  await page.setViewportSize({ width: 1280, height: 760 });
  await page.$eval('#win', (el) => { el.style.width = '1280px'; el.style.height = '760px'; });
});

// Rundgang RUND1: „Archivpfad kopieren“ meldete Erfolg, obwohl die Zwischenablage ablehnte (unbehandelte Ablehnung).
test('Archivpfad kopieren: verweigerte Zwischenablage → Fehler in der Zone, keine Erfolgsmeldung, kein Seitenfehler', async () => {
  const before = page.__errors.length;
  await page.evaluate(async () => {
    const orig = navigator.clipboard?.writeText;
    Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText: () => Promise.reject(new DOMException('Write permission denied.', 'NotAllowedError')) } });
    try { await window.fibuCopyArchivePath('archive/beleg.pdf'); } finally { void orig; }
  });
  await settle();
  const codes = await page.evaluate(() => window.__zone().map((e) => e.code));
  assert.ok(codes.some((c) => /archive\.copy_failed/.test(c)), JSON.stringify(codes));
  assert.ok(!codes.some((c) => /archive\.copied/.test(c)), JSON.stringify(codes));
  assert.equal(page.__errors.length, before);
  await page.evaluate(async () => {
    Object.defineProperty(navigator, 'clipboard', { configurable: true, value: { writeText: () => Promise.resolve() } });
    await window.fibuCopyArchivePath('archive/beleg.pdf');
  });
  await settle();
  const codes2 = await page.evaluate(() => window.__zone().map((e) => e.code));
  assert.ok(codes2.some((c) => /archive\.copied/.test(c)) && !codes2.some((c) => /archive\.copy_failed/.test(c)), JSON.stringify(codes2));
});

// Rundgang RUND1 @420: der Buchungseditor scrollte 4 px seitlich (Aktionsleiste −14 px bei 10 px Innenpolster).
test('Buchungseditor im schmalen Fenster (Container ≤ 768 px) scrollt nicht seitlich', async () => {
  await page.$eval('#win', (el) => { el.style.width = '420px'; el.style.height = '860px'; el.style.container = 'business-app-window / inline-size'; });
  await page.evaluate(() => [...document.querySelectorAll('#win button')].find((b) => /Neue Buchung|Neue manuelle Buchung/.test(b.getAttribute('aria-label') || b.title || b.textContent))?.click());
  await settle();
  const r = await page.evaluate(() => { const c = document.querySelector('#win .fibu-drawer-content'); return c && c.checkVisibility() ? c.scrollWidth - c.clientWidth : null; });
  await page.$eval('#win', (el) => { el.style.width = '1280px'; el.style.height = '760px'; el.style.container = ''; });
  assert.notEqual(r, null, 'Buchungseditor offen');
  assert.ok(r <= 0, `kein Seitenscrollen im Editor (${r}px)`);
});

test('unmount meldet alle Abos ab und leert root', async () => {
  const res = await page.evaluate(async (COLLECTIONS) => {
    await window.__app.unmount();
    await window.__k.sim.push('accounting_journal_entries', { id: 'je-late', posting_date: '2026-10-09', type: 'journal', number: 'J-LATE', updated_at_ms: 10 });
    await window.__k.sim.settle();
    return { subs: COLLECTIONS.map((c) => window.__c.subscribers(c)), children: document.getElementById('win').children.length };
  }, COLLECTIONS);
  assert.deepEqual(res.subs, COLLECTIONS.map(() => 0));
  assert.equal(res.children, 0);
  assert.deepEqual(page.__errors, []);
});
