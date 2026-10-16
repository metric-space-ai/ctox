// Strang BUCH-2 (16.10.2026): Beleg-Upload schreibt nur echte Datei-Angaben, keine erfundene OCR.
// Früher legte handleBelegeUpload jede Datei als „Hetzner Online GmbH“, HET-2026-98127, 100/19/119 € an (bzw.
// „Deutsche Telekom AG“, TEL-7816-12, wenn der Dateiname „telekom“ enthielt), und die Evidenzspalte zeichnete daraus
// eine nachgebildete Rechnung (StNr DE814712499, Fallback INV-98172).
//
// Lauf:      CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/8265bd7 node --test src/apps/business-os/modules/buchhaltung/tests/beleg-upload.test.mjs
// Gegenprobe: zusätzlich BUCH_OLD_DIR=<Ordner mit altem index.js/index.html/index.css> → muss rot sein.
import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { readFile, mkdir } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { dirname, join, normalize, extname } from 'node:path';
import { homedir } from 'node:os';

const APP = 'buchhaltung';
const STUB = process.env.CTOX_APP_API_STUB || join(homedir(), '.local/state/shell-v4/stub-pins/8265bd7');
const OLD = process.env.BUCH_OLD_DIR || '';
const HERE = dirname(fileURLToPath(import.meta.url));
const BOS = normalize(join(HERE, '../../..'));
const require = createRequire(import.meta.url);
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || join(homedir(), 'Documents/ctox/src/apps/business-os/node_modules/playwright'));
const TYPES = { '.js': 'text/javascript', '.mjs': 'text/javascript', '.css': 'text/css', '.html': 'text/html', '.json': 'application/json', '.svg': 'image/svg+xml' };

const PAGE = `<!doctype html><html lang="de"><head><meta charset="utf-8">
<link rel="stylesheet" href="/app.css">
<link rel="stylesheet" href="/shared/base.css">
<link rel="stylesheet" href="/modules/${APP}/index.css">
<style>html,body{margin:0} #win{position:relative;overflow:hidden;background:var(--bg)}</style>
</head><body><div id="win" style="width:1280px;height:760px"></div></body></html>`;

// Werte der alten Attrappe – kein Feld eines neuen Belegs darf sie tragen.
const FAKE_TEXT = /Hetzner|HET-2026-98127|Telekom|TEL-7816|DE814712499|INV-98172|EINGANGSRECHNUNG|Dienstleistungen laut Leistungsbeschreibung/;
const FAKE_NUMBERS = new Set([10000, 1900, 11900]);
const INVENTED_FIELDS = ['supplier_name', 'invoice_number', 'invoice_date', 'net_amount', 'tax_amount', 'gross_amount', 'suggested_account_id', 'vat_id', 'ocr_raw', 'ocr_confidence'];

let server, browser, page;
const SHOTS = join(HERE, 'shots');

before(async () => {
  server = createServer(async (req, res) => {
    const path = decodeURIComponent(new URL(req.url, 'http://x').pathname);
    if (path === '/') { res.writeHead(200, { 'content-type': 'text/html' }); res.end(PAGE); return; }
    const oldFile = OLD && /^\/modules\/buchhaltung\/index\.(js|html|css)$/.test(path) ? join(OLD, path.split('/').pop()) : null;
    const [rootDir, rel] = path.startsWith('/__stub/') ? [STUB, path.slice('/__stub'.length)] : [BOS, path];
    const file = oldFile || normalize(join(rootDir, rel));
    if (!oldFile && !file.startsWith(rootDir)) { res.writeHead(403); res.end(); return; }
    try {
      let body = await readFile(file);
      if (/\.m?js$/.test(file)) body = Buffer.from(body.toString('utf8').replace(/(['"])ctox:shared\//g, '$1/shared/'));
      res.writeHead(200, { 'content-type': TYPES[extname(file)] || 'application/octet-stream' }); res.end(body);
    } catch { res.writeHead(404); res.end(); }
  });
  await new Promise((r) => server.listen(0, '127.0.0.1', r));
  browser = await chromium.launch();
  page = await browser.newPage({ viewport: { width: 1280, height: 760 } });
  page.__errors = [];
  page.on('pageerror', (e) => page.__errors.push(e.message));
  await page.goto(`http://127.0.0.1:${server.address().port}/`);
  await page.evaluate(async (APP) => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    window.__mod = await import(`/modules/${APP}/index.js`);
    const kernel = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false });
    await kernel.ready;
    await kernel.sim.push('business_apps', [{ id: APP, title: 'Buchhaltung' }]);
    await kernel.sim.settle();
    window.__k = kernel;
    window.__api = kernel.appApi(APP, '1');
    window.__zone = () => kernel.statusZone.forApp(APP).get().filter((e) => e.source === 'app').map((e) => e.code);
    window.__receipts = () => window.__api.query('accounting_receipts', {}).get();
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__api, {});
  }, APP);
  await page.evaluate(() => window.__k.sim.settle());
  await page.waitForTimeout(300);
  await page.selectOption('[data-workbench-view]', 'receipts');
  await page.waitForTimeout(200);
});

after(async () => { await browser?.close(); await new Promise((r) => server?.close(r)); });

const settle = async () => { await page.evaluate(() => window.__k.sim.settle()); await page.waitForTimeout(300); };

// Echte Maus auf die Ablagefläche → Dateiauswahl des Browsers → Datei.
async function uploadViaMouse(name, mimeType, bytes) {
  const box = await page.locator('[data-file-dropzone]').boundingBox();
  assert.ok(box, 'Ablagefläche sichtbar');
  const chooser = page.waitForEvent('filechooser');
  await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
  await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
  await (await chooser).setFiles({ name, mimeType, buffer: Buffer.from(bytes) });
  await settle();
}

function assertHonest(receipt) {
  for (const [key, value] of Object.entries(receipt)) {
    assert.doesNotMatch(String(value), FAKE_TEXT, `${key} trägt einen Fantasiewert`);
    if (typeof value === 'number' && key !== 'file_size') assert.ok(!FAKE_NUMBERS.has(value), `${key} = ${value} (alter Fantasiebetrag)`);
  }
  for (const key of INVENTED_FIELDS) assert.equal(receipt[key] ?? null, null, `${key} muss leer bleiben`);
}

test('Upload per Maus: genau ein Beleg mit echten Metadaten, ohne erfundene Felder', async () => {
  const before = await page.evaluate(() => window.__receipts().length);
  const t0 = Date.now();
  const bytes = '%PDF-1.4\n% BUCH-2 Testbeleg\n'.repeat(20);
  await uploadViaMouse('Rechnung_Maerz.pdf', 'application/pdf', bytes);
  const rows = await page.evaluate(() => window.__receipts());
  assert.equal(rows.length, before + 1, 'genau ein neuer Beleg');
  const r = rows.find((x) => x.filename === 'Rechnung_Maerz.pdf');
  assert.ok(r);
  assert.equal(r.file_size, Buffer.byteLength(bytes));
  assert.equal(r.mime_type, 'application/pdf');
  assert.ok(r.uploaded_at_ms >= t0 && r.uploaded_at_ms <= Date.now(), `uploaded_at_ms ${r.uploaded_at_ms}`);
  assert.equal(r.status, 'draft');
  // BUCH-3: die Datei liegt über api.files im Speicher, file_storage_url ist ihre sha256-Referenz.
  assert.match(r.file_storage_url, /^[0-9a-f]{64}$/, 'Datei-Referenz statt vorgetäuschtem Archivpfad');
  assert.ok(r.file_id, 'files-Dokument verknüpft');
  assertHonest(r);
});

test('Dateiname mit „telekom“: trotzdem kein erfundener Lieferant', async () => {
  await uploadViaMouse('telekom_rechnung.pdf', 'application/pdf', '%PDF-1.4\n');
  const r = await page.evaluate(() => window.__receipts().find((x) => x.filename === 'telekom_rechnung.pdf'));
  assert.ok(r);
  assertHonest({ ...r, filename: '' });
});

test('Anzeige: Liste, Evidenz und Belegkontierung zeigen nur echte Angaben (leere Kontierung)', async () => {
  const row = page.locator('[data-receipt-click-id]').filter({ hasText: 'Rechnung_Maerz.pdf' }).first();
  const b = await row.boundingBox();
  await page.mouse.click(b.x + 40, b.y + b.height / 2);
  await page.waitForTimeout(400);
  const ui = await page.evaluate(() => {
    const win = document.getElementById('win');
    const drawer = win.querySelector('[data-drawer-content-body]');
    return {
      text: win.innerText,
      rowText: win.querySelector('[data-receipt-click-id] ~ tr, [data-receipt-click-id]') ? [...win.querySelectorAll('[data-receipt-click-id]')].find((tr) => tr.innerText.includes('Rechnung_Maerz.pdf'))?.innerText : '',
      evidence: win.querySelector('[data-ocr-preview-container]')?.innerText || '',
      drawerInputs: ['supplier_name', 'invoice_number', 'invoice_date'].map((n) => drawer?.querySelector(`[data-receipt-form] [name="${n}"]`)?.value ?? 'fehlt'),
    };
  });
  assert.doesNotMatch(ui.text, FAKE_TEXT);
  assert.doesNotMatch(ui.text, /\bOCR\b/, 'keine OCR-Anzeige ohne Belegerkennung');
  assert.doesNotMatch(ui.rowText, /100,00|19,00|119,00|Unbekannt/);
  assert.match(ui.evidence, /application\/pdf/);
  assert.deepEqual(ui.drawerInputs, ['', '', ''], 'Lieferant, Rechnungsnummer, Belegdatum leer');
  // Buchungssatz-Vorschau: kein vorgeschlagenes Aufwandskonto (früher Standard 4930), keine Beträge, kein NaN.
  const preview = () => page.$eval('#receipt-preview-table-container', (el) => el.innerText);
  assert.doesNotMatch(await preview(), /4930|6815|NaN|\d+,\d\d\s*€/);
  const cb = await page.locator('#rcpt-bewirtung').boundingBox();
  await page.mouse.click(cb.x + cb.width / 2, cb.y + cb.height / 2);
  await page.waitForTimeout(150);
  assert.doesNotMatch(await preview(), /NaN|\d+,\d\d\s*€/);
  await page.mouse.click(cb.x + cb.width / 2, cb.y + cb.height / 2);
  await page.waitForTimeout(150);
  await mkdir(SHOTS, { recursive: true });
  await page.screenshot({ path: join(SHOTS, OLD ? 'buch2-upload-alt-1280.png' : 'buch2-upload-1280.png') });
});

test('Buchen eines leeren Belegs legt keine Buchung an, Grund in der Zone', async () => {
  const before = await page.evaluate(() => window.__api.query('accounting_journal_entries', {}).get().length);
  const id = await page.evaluate(() => window.__receipts().find((x) => x.filename === 'Rechnung_Maerz.pdf').id);
  await page.evaluate((id) => window.postReceiptDirectly(id), id);
  await settle();
  assert.equal(await page.evaluate(() => window.__api.query('accounting_journal_entries', {}).get().length), before);
  assert.ok((await page.evaluate(() => window.__zone())).includes('buchhaltung.receipt.incomplete'));
  assert.deepEqual(page.__errors, []);
});
