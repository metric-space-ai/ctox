// FIX9b (Shell v4): Tabelle sofort als Lesevorschau, Editor (ctox:lib/ctox-office) startet dahinter;
// Zeilenaktion „Weitere Aktionen“ statt Zahnrad; Suchplatzhalter passt.
// Lauf: CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/a8765b9 \
//       node --import ./v4/tools/node/ctox-shared.mjs --test --test-force-exit modules/spreadsheets/tests/fix9b.test.mjs
import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { dirname, join, normalize, extname } from 'node:path';
import { homedir } from 'node:os';

const STUB = process.env.CTOX_APP_API_STUB || join(homedir(), '.local/state/shell-v4/stub-pins/a8765b9');
const BOS = normalize(join(dirname(fileURLToPath(import.meta.url)), '../../..'));
const require = createRequire(import.meta.url);
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || join(homedir(), 'Documents/ctox/src/apps/business-os/node_modules/playwright'));
const TYPES = { '.js': 'text/javascript', '.mjs': 'text/javascript', '.css': 'text/css', '.html': 'text/html', '.json': 'application/json', '.svg': 'image/svg+xml', '.wasm': 'application/wasm' };
const PAGE = `<!doctype html><html><head><meta charset="utf-8">
<link rel="stylesheet" href="/app.css"><link rel="stylesheet" href="/shared/base.css">
<link rel="stylesheet" href="/modules/spreadsheets/index.css">
<style>html,body{margin:0;height:100%;background:var(--bg,#fff)} #win{--shell-v2-icon-size:28px;width:100vw;height:100vh;position:relative;overflow:hidden}</style>
</head><body><div id="win"></div></body></html>`;


let server, browser, page;
before(async () => {
  server = createServer(async (req, res) => {
    const path = decodeURIComponent(new URL(req.url, 'http://x').pathname);
    if (path === '/') { res.writeHead(200, { 'content-type': 'text/html' }); res.end(PAGE); return; }
    const [rootDir, rel] = path.startsWith('/__stub/') ? [STUB, path.slice('/__stub'.length)] : [BOS, path];
    const file = normalize(join(rootDir, rel));
    if (!file.startsWith(rootDir)) { res.writeHead(403); res.end(); return; }
    try {
      let body = await readFile(file);
      if (/\.(m?js|css)$/.test(file)) body = Buffer.from(body.toString('utf8').replace(/(['"])ctox:shared\//g, '$1/shared/').replace(/(['"`(])ctox:lib\/([a-z0-9][a-z0-9._-]*)@\d+\//g, '$1/vendor/$2/'));
      res.writeHead(200, { 'content-type': TYPES[extname(file)] || 'application/octet-stream' }); res.end(body);
    } catch { res.writeHead(404); res.end(); }
  });
  await new Promise((r) => server.listen(0, '127.0.0.1', r));
  browser = await chromium.launch();
  page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
  page.__errors = [];
  page.on('pageerror', (e) => page.__errors.push(e.message));
  await page.goto(`http://127.0.0.1:${server.address().port}/`);
  await page.evaluate(async () => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    window.__mod = await import('/modules/spreadsheets/index.js');
    const kernel = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false, actor: { role: 'admin', permissions: ['apps.source.edit'] } });
    await kernel.ready;
    const sheet = (id, title, updated) => ({
      id, title, filename: `${id}.csv`, mime_type: 'text/csv', status: 'Imported', spreadsheet_type: 'csv', owner_id: '',
      current_version_id: `${id}_v1`, source_sha256: '', row_count: 3, col_count: 3, diagnostics_count: 0, linked_records: [],
      tags: [], display_cache: {}, index_text: title, is_deleted: false, created_at_ms: updated, updated_at_ms: updated,
    });
    await kernel.sim.push('spreadsheets', [sheet('sheet_price', 'Preisliste Q4', 20), sheet('sheet_plan', 'Plan', 10)]);
    await kernel.sim.push('spreadsheet_versions', [
      { id: 'sheet_price_v1', spreadsheet_id: 'sheet_price', version: 1, source_kind: 'imported_csv', blob_id: 'sheet_price_v1_blob', model_json: { data: [['Artikel', 'Preis'], ['Wartung Halle 4', '1250'], ['Inspektion', '310']], columns: [] }, diagnostics: [], created_at_ms: 20, updated_at_ms: 20 },
      { id: 'sheet_plan_v1', spreadsheet_id: 'sheet_plan', version: 1, source_kind: 'imported_csv', blob_id: 'sheet_plan_v1_blob', model_json: { data: [['A']], columns: [] }, diagnostics: [], created_at_ms: 10, updated_at_ms: 10 },
    ]);
    await kernel.sim.settle();
    window.__k = kernel;
    window.__zone = () => kernel.statusZone.forApp('spreadsheets').get().filter((e) => e.source === 'app');
    window.__t0 = performance.now();
    window.__app = await window.__mod.mount(document.getElementById('win'), kernel.appApi('spreadsheets', '1.0.0'), {});
  });
});
after(async () => { await browser?.close(); await new Promise((r) => server?.close(r)); });

test('FIX9b: Tabelle erscheint sofort als Lesevorschau aus model_json; Editor startet unsichtbar dahinter', async () => {
  await page.evaluate(() => [...document.querySelectorAll('[data-spreadsheets-list] button')].find((b) => /Preisliste Q4/.test(b.innerText))?.click());
  await page.waitForFunction(() => /Wartung Halle 4/.test(document.querySelector('[data-spreadsheets-read-preview]')?.innerText || ''), null, { timeout: 3000 });
  const tPreview = await page.evaluate(() => performance.now() - window.__t0);
  const cells = await page.$$eval('[data-spreadsheets-read-preview] tbody tr', (trs) => trs.map((tr) => [...tr.children].map((c) => c.textContent)));
  assert.deepEqual(cells.slice(0, 2), [['1', 'Artikel', 'Preis'], ['2', 'Wartung Halle 4', '1250']]);
  assert.equal(await page.$eval('[data-spreadsheets-read-preview] tbody tr:nth-child(2) td:nth-of-type(2)', (td) => td.className), 'is-num');
  const visibleOutsidePreview = await page.evaluate(() => {
    const host = document.querySelector('[data-spreadsheets-canvas]');
    return [...host.children].filter((c) => !c.matches('[data-spreadsheets-read-preview]') && c.checkVisibility({ visibilityProperty: true })).map((c) => c.className);
  });
  assert.deepEqual(visibleOutsidePreview, [], 'nur die Vorschau ist sichtbar, der Editor wärmt dahinter');
  console.log(`# FIX9b spreadsheets: Lesevorschau nach ${Math.round(tPreview)} ms`);
  assert.deepEqual(page.__errors, []);
});

test('FIX9b: ohne Zellinhalt keine Vorschau (kein leeres Raster, kein Wartetext)', async () => {
  const html = await page.evaluate(() => window.__mod.__spreadsheetsTestHooks.spreadsheetReadPreviewHtml({ model_json: { format: 'xlsx', data: [], columns: [] } }));
  assert.equal(html, '');
});

test('FIX9b: Zeilenaktion „Weitere Aktionen“ (⋯) statt Zahnrad; Suchplatzhalter nicht abgeschnitten', async () => {
  const rows = await page.$$eval('.spreadsheets-card-manage', (els) => els.map((b) => ({ title: b.title, label: b.getAttribute('aria-label'), path: b.querySelector('path')?.getAttribute('d') || '' })));
  assert.ok(rows.length >= 2);
  for (const r of rows) {
    assert.equal(r.title, 'Weitere Aktionen');
    assert.match(r.label, / – Weitere Aktionen$/);
    assert.doesNotMatch(r.path, /M12 8\.5a3\.5/, 'kein Zahnradpfad');
  }
  const fit = await page.$eval('[data-spreadsheets-search]', (el) => {
    const cs = getComputedStyle(el);
    const ctx = document.createElement('canvas').getContext('2d');
    ctx.font = `${cs.fontWeight} ${cs.fontSize} ${cs.fontFamily}`;
    const need = ctx.measureText(el.placeholder).width + parseFloat(cs.paddingLeft) + parseFloat(cs.paddingRight);
    return { placeholder: el.placeholder, need, have: el.clientWidth };
  });
  assert.ok(fit.need <= fit.have, JSON.stringify(fit));
});
