// Strang BUCH-3 (16.10.2026): Belegkontierung bearbeitbar, Datei über api.files, Buchen mit den eingegebenen Werten,
// Konflikt ohne stilles Überschreiben. Echte Maus/Tastatur (page.mouse/page.keyboard) im Stub-Kern (Chromium).
//
// Lauf:       CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/64acc2c node --test src/apps/business-os/modules/buchhaltung/tests/belegkontierung.test.mjs
// Gegenprobe: zusätzlich BUCH_OLD_DIR=<Ordner mit altem index.js/index.html/index.css> → muss rot sein.
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
const STUB = process.env.CTOX_APP_API_STUB || join(homedir(), '.local/state/shell-v4/stub-pins/64acc2c');
const OLD = process.env.BUCH_OLD_DIR || '';
const HERE = dirname(fileURLToPath(import.meta.url));
const BOS = normalize(join(HERE, '../../..'));
const SHOTS = join(HERE, 'shots');
const require = createRequire(import.meta.url);
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || join(homedir(), 'Documents/ctox/src/apps/business-os/node_modules/playwright'));
const TYPES = { '.js': 'text/javascript', '.mjs': 'text/javascript', '.css': 'text/css', '.html': 'text/html', '.json': 'application/json', '.svg': 'image/svg+xml' };

const PAGE = `<!doctype html><html lang="de"><head><meta charset="utf-8">
<link rel="stylesheet" href="/app.css">
<link rel="stylesheet" href="/shared/base.css">
<link rel="stylesheet" href="/modules/${APP}/index.css">
<style>html,body{margin:0} #win{position:relative;overflow:hidden;background:var(--bg);container:business-app-window / inline-size}</style>
</head><body><div id="win" style="width:1280px;height:800px"></div></body></html>`;

// 1×1-PNG (rot), echte Bilddatei für die Vorschau.
const PNG = Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8DwHwAFBQIAX8jx0gAAAABJRU5ErkJggg==', 'base64');

let server, browser, page;

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
  page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
  page.__errors = [];
  page.on('pageerror', (e) => page.__errors.push(e.message));
  await page.goto(`http://127.0.0.1:${server.address().port}/`);
  // PIN-ALL (E48): echte Routen/Rechte/Collections aus v4/manifests/buchhaltung.manifest.json; ohne aktive Version prüft der
  // Kern weder eigene Collections noch Schemas wie in Produktion.
  await page.evaluate((routes) => { window.__routes = routes; }, realManifestRoutes(APP));
  await page.evaluate(async (APP) => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    window.__mod = await import(`/modules/${APP}/index.js`);
    const kernel = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false });
    await kernel.ready;
    await kernel.sim.push('business_apps', [{ id: APP, title: 'Buchhaltung' }]);
    for (const o of window.__routes?._fixture_owners ?? []) if (!(await kernel.sim.inspect('business_apps', o.app))?.data?.active_ver) await kernel.sim.publishAppVersion(o.app, o.ver, o.publish);
    await kernel.sim.publishAppVersion(APP, '1', { title: 'Buchhaltung', manifestExtra: window.__routes });
    await kernel.sim.settle();
    window.__k = kernel;
    const raw = kernel.appApi(APP, '1');
    // Testhebel: files.put scheitern lassen (window.__failPut), sonst unverändert.
    const delegate = (target, overrides) => {
      const out = {};
      for (const key of Object.keys(target)) {
        if (key in overrides) continue;
        Object.defineProperty(out, key, { enumerable: true, get: () => { const v = target[key]; return typeof v === 'function' ? v.bind(target) : v; } });
      }
      return Object.freeze(Object.assign(out, overrides));
    };
    const failing = () => {
      const err = Object.assign(new Error('Speicher voll'), { code: 'quota' });
      const local = Promise.reject(err); local.catch(() => {});
      const ref = Promise.reject(err); ref.catch(() => {});
      return { local, ref, result: Promise.resolve({ status: 'rejected', reason: 'quota' }), cancel: () => true };
    };
    window.__navigations = [];
    const shell = delegate(raw.shell, { navigate: (app, args) => { window.__navigations.push({ app, args }); } });
    const files = delegate(raw.files, { put: (blob, meta) => (window.__failPut ? failing() : raw.files.put(blob, meta)) });
    window.__api = delegate(raw, { files, shell });
    window.__raw = raw;
    window.__zone = () => kernel.statusZone.forApp(APP).get().filter((e) => e.source === 'app').map((e) => e.code);
    window.__receipts = () => raw.query('accounting_receipts', {}).get();
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__api, {});
  }, APP);
  await settle();
  await page.selectOption('[data-workbench-view]', 'receipts');
  await page.waitForTimeout(200);
});

after(async () => { await browser?.close(); await new Promise((r) => server?.close(r)); });

async function settle() { await page.evaluate(() => window.__k.sim.settle()); await page.waitForTimeout(300); }
async function mouseClick(selector) {
  await page.locator(selector).first().scrollIntoViewIfNeeded();
  const b = await page.locator(selector).first().boundingBox();
  assert.ok(b, `nicht sichtbar: ${selector}`);
  await page.mouse.move(b.x + Math.min(b.width / 2, 40), b.y + b.height / 2);
  await page.mouse.click(b.x + Math.min(b.width / 2, 40), b.y + b.height / 2);
}
async function upload(name, mimeType, buffer) {
  const chooser = page.waitForEvent('filechooser');
  await mouseClick('[data-file-dropzone]');
  await (await chooser).setFiles({ name, mimeType, buffer });
  await settle();
}
// Feld per Maus anklicken, Inhalt per Tastatur ersetzen.
async function typeInto(name, text) {
  await mouseClick(`[data-receipt-form] [name="${name}"]`);
  await page.keyboard.press('ControlOrMeta+A');
  await page.keyboard.press('Backspace');
  if (text) await page.keyboard.type(text);
}
const field = (name) => page.$eval(`[data-receipt-form] [name="${name}"]`, (el) => el.value);
const receipt = (filename) => page.evaluate((f) => window.__receipts().find((r) => r.filename === f) || null, filename);
async function openReceipt(filename) {
  const row = page.locator('[data-receipt-click-id]').filter({ hasText: filename }).first();
  const b = await row.boundingBox();
  await page.mouse.click(b.x + 40, b.y + b.height / 2);
  await page.waitForSelector('[data-receipt-form]');
  await page.waitForTimeout(200);
}

test('Upload: Datei liegt über api.files im Speicher, Referenz am Beleg, Vorschau in der Evidenz', async () => {
  await upload('Quittung.png', 'image/png', PNG);
  const r = await receipt('Quittung.png');
  assert.ok(r, 'Beleg angelegt');
  assert.match(r.file_storage_url, /^[0-9a-f]{64}$/);
  const stored = await page.evaluate(async (r) => {
    // E48: files ist für buchhaltung nicht deklariert (App nutzt nur api.files) → Testlesen über den simulierten Core.
    const doc = (await window.__k.sim.inspect('files', r.file_id))?.base?.data ?? null;
    const blob = await window.__raw.files.get(r.file_storage_url);
    return { doc, bytes: blob ? [...new Uint8Array(await blob.arrayBuffer())] : null };
  }, r);
  assert.equal(stored.doc?.ref, r.file_storage_url, 'files-Dokument mit derselben Referenz');
  assert.equal(stored.doc?.name, 'Quittung.png');
  assert.deepEqual(stored.bytes, [...PNG], 'gespeicherte Bytes = hochgeladene Bytes');
  await openReceipt('Quittung.png');
  await page.waitForSelector('[data-receipt-file] img', { timeout: 3000 });
  const img = await page.$eval('[data-receipt-file] img', (el) => ({ ok: el.complete && el.naturalWidth === 1, src: el.src.slice(0, 5) }));
  assert.deepEqual(img, { ok: true, src: 'blob:' });
  await mouseClick('[data-receipt-file-open]');
  const nav = await page.evaluate(() => window.__navigations.at(-1));
  assert.equal(nav?.app, 'file-viewer');
  assert.equal(nav?.args?.ref, r.file_storage_url);
});

test('Upload: files.put scheitert → kein Beleg, Grund in der Zone', async () => {
  const before = (await page.evaluate(() => window.__receipts().length));
  await page.evaluate(() => { window.__failPut = true; });
  await upload('kaputt.pdf', 'application/pdf', Buffer.from('%PDF-1.4\n'));
  await page.evaluate(() => { window.__failPut = false; });
  assert.equal(await page.evaluate(() => window.__receipts().length), before, 'kein Beleg ohne gespeicherte Datei');
  assert.ok((await page.evaluate(() => window.__zone())).includes('buchhaltung.receipt.upload_failed'));
});

test('Ausfüllen per Maus/Tastatur: Vorschlag Steuer/Brutto, gespeichert per api.patch', async () => {
  await openReceipt('Quittung.png');
  await typeInto('supplier_name', 'Müller Bürobedarf GmbH');
  await typeInto('invoice_number', 'RE-4711');
  await mouseClick('[data-receipt-form] [name="invoice_date"]');
  await page.locator('[data-receipt-form] [name="invoice_date"]').fill('2026-10-14');
  const acct = await page.evaluate(() => {
    const opt = [...document.querySelectorAll('[data-receipt-form] [name="suggested_account_id"] option')].find((o) => /^4930 /.test(o.textContent));
    return opt ? { id: opt.value, label: opt.textContent } : null;
  });
  assert.ok(acct, 'Konto 4930 aus dem Kontenrahmen wählbar');
  await page.selectOption('[data-receipt-form] [name="suggested_account_id"]', acct.id);
  await typeInto('net_amount', '250,00');
  await page.selectOption('[data-receipt-form] [name="tax_rate"]', '19');
  assert.equal(await field('tax_amount'), '47,50', 'Steuer als Vorschlag');
  assert.equal(await field('gross_amount'), '297,50', 'Brutto = Netto + Steuer als Vorschlag');
  await page.keyboard.press('Tab');
  await settle();
  const r = await receipt('Quittung.png');
  assert.equal(r.supplier_name, 'Müller Bürobedarf GmbH');
  assert.equal(r.invoice_number, 'RE-4711');
  assert.equal(r.invoice_date, '2026-10-14');
  assert.equal(r.suggested_account_id, acct.id);
  assert.equal(r.net_amount, 25000);
  assert.equal(r.tax_rate, 19);
  assert.equal(r.tax_amount, 4750);
  assert.equal(r.gross_amount, 29750);
  global.__acct = acct;
});

test('Abweichendes Brutto bleibt stehen; Buchsperre bis Brutto = Netto + Steuer', async () => {
  await typeInto('gross_amount', '300,00');
  await page.keyboard.press('Tab');
  await typeInto('net_amount', '260,00');
  await page.keyboard.press('Tab');
  await settle();
  assert.equal(await field('tax_amount'), '49,40');
  assert.equal(await field('gross_amount'), '300,00', 'Nutzereintrag wird nicht überschrieben');
  const r = await receipt('Quittung.png');
  assert.equal(r.gross_amount, 30000);
  const journalBefore = await page.evaluate(() => window.__raw.query('accounting_journal_entries', {}).get().length);
  await mouseClick('[data-drawer-content-body] button.is-primary');
  await settle();
  assert.equal(await page.evaluate(() => window.__raw.query('accounting_journal_entries', {}).get().length), journalBefore, 'unausgeglichen → keine Buchung');
  assert.ok((await page.evaluate(() => window.__zone())).includes('buchhaltung.receipt.incomplete'));
  await typeInto('gross_amount', '309,40');
  await page.keyboard.press('Tab');
  await settle();
});

test('Reload: gespeicherte Werte bleiben', async () => {
  await page.evaluate(async () => {
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__api, {});
  });
  await settle();
  await page.selectOption('[data-workbench-view]', 'receipts');
  await page.waitForTimeout(200);
  await openReceipt('Quittung.png');
  assert.deepEqual(
    await Promise.all(['supplier_name', 'invoice_number', 'invoice_date', 'suggested_account_id', 'net_amount', 'tax_rate', 'tax_amount', 'gross_amount'].map(field)),
    ['Müller Bürobedarf GmbH', 'RE-4711', '2026-10-14', global.__acct.id, '260,00', '19', '49,40', '309,40'],
  );
  await mkdir(SHOTS, { recursive: true });
  await page.screenshot({ path: join(SHOTS, 'buch3-kontierung-1280.png') });
  await page.setViewportSize({ width: 420, height: 900 });
  await page.$eval('#win', (el) => { el.style.width = '420px'; el.style.height = '900px'; });
  await page.waitForTimeout(300);
  await page.screenshot({ path: join(SHOTS, 'buch3-kontierung-420.png') });
  const overflow = await page.$eval('#win .fibu-drawer-content', (el) => el.scrollWidth - el.clientWidth);
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.$eval('#win', (el) => { el.style.width = '1280px'; el.style.height = '800px'; });
  await page.waitForTimeout(300);
  assert.ok(overflow <= 0, `Kontierung scrollt bei 420 nicht seitlich (${overflow}px)`);
});

test('Buchen nach dem Ausfüllen: genau eine Buchung mit den eingegebenen Werten', async () => {
  const before = await page.evaluate(() => window.__raw.query('accounting_journal_entries', {}).get().length);
  await mouseClick('[data-drawer-content-body] button.is-primary');
  await settle();
  const res = await page.evaluate(() => {
    const entries = window.__raw.query('accounting_journal_entries', {}).get();
    const r = window.__receipts().find((x) => x.filename === 'Quittung.png');
    const mine = entries.filter((e) => e.ref_id === r.id);
    const lines = window.__raw.query('accounting_journal_entry_lines', {}).get().filter((l) => mine[0] && l.journal_entry_id === mine[0].id);
    return { total: entries.length, mine, lines, status: r.status };
  });
  assert.equal(res.total, before + 1, 'genau eine neue Buchung');
  assert.equal(res.mine.length, 1);
  assert.equal(res.mine[0].posting_date, '2026-10-14');
  const debit = res.lines.reduce((s, l) => s + Number(l.debit || 0), 0);
  const credit = res.lines.reduce((s, l) => s + Number(l.credit || 0), 0);
  assert.equal(debit, 30940);
  assert.equal(credit, 30940);
  assert.ok(res.lines.some((l) => l.account_id === global.__acct.id && l.debit === 26000), JSON.stringify(res.lines));
  assert.ok(res.lines.some((l) => l.debit === 4940), 'Vorsteuer 49,40');
  assert.equal(res.status, 'posted');
});

test('Konflikt: Live-Änderung überschreibt die Eingabe nicht, Speichern mit veralteter Revision schreibt nichts', async () => {
  await page.selectOption('[data-workbench-view]', 'receipts'); // Buchen wechselt ins Journal
  await page.waitForTimeout(200);
  await upload('Konflikt.pdf', 'application/pdf', Buffer.from('%PDF-1.4\n% Konflikt\n'));
  await openReceipt('Konflikt.pdf');
  await typeInto('supplier_name', 'Meine Eingabe');
  // Fremde Änderung kommt über den Core, während das Feld noch offen ist.
  const r0 = await receipt('Konflikt.pdf');
  // E47: gelesene Dokumente tragen _rev/_pending (reserviert) → der Core-Push trägt nur die Datenfelder.
  await page.evaluate((r) => {
    const data = Object.fromEntries(Object.entries(r).filter(([k]) => !k.startsWith('_')));
    return window.__k.sim.push('accounting_receipts', { ...data, supplier_name: 'Fremd GmbH', updated_at_ms: r.updated_at_ms + 5000 });
  }, r0);
  await settle();
  assert.equal(await field('supplier_name'), 'Meine Eingabe', 'Eingabe nicht durch Live-Daten überschrieben');
  await page.keyboard.press('Tab');
  await settle();
  const r1 = await receipt('Konflikt.pdf');
  assert.equal(r1.supplier_name, 'Fremd GmbH', 'kein stilles Überschreiben');
  assert.ok((await page.evaluate(() => window.__zone())).includes('buchhaltung.receipt.conflict'));
  assert.deepEqual(page.__errors, []);
});

test('Upload: Core lehnt die Datei nach der lokalen Quittung ab → Beleg bleibt, markiert, Grund in der Zone', async (t) => {
  const supported = await page.evaluate(() => typeof window.__k.sim.rejectNextUpload === 'function');
  if (!supported) { t.skip('Stub vor E35/STUB-E30 (kein sim.rejectNextUpload)'); return; }
  await page.evaluate(() => window.__k.sim.rejectNextUpload({ code: 'quota', reason: 'Speicher voll' }));
  await upload('abgelehnt.png', 'image/png', PNG);
  await page.waitForTimeout(400);
  await settle();
  const r = await receipt('abgelehnt.png');
  assert.ok(r, 'Beleg bleibt erhalten (nicht gelöscht)');
  assert.equal(r.file_state, 'rejected', 'Beleg als „Datei abgelehnt“ markiert');
  assert.match(String(r.file_error), /Speicher voll|quota/);
  const zone = await page.evaluate(() => window.__zone());
  assert.ok(zone.some((code) => code.startsWith('buchhaltung.receipt.upload_rejected.')), JSON.stringify(zone));
});
