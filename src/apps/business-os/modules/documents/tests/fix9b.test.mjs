// FIX9b (Shell v4): Word-Dokument sofort als Lesevorschau, Editor (ctox:lib/ctox-office) startet dahinter;
// Zeilenaktion „Weitere Aktionen“ statt Sonnensymbol; Suchplatzhalter passt.
// Lauf: CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/8265bd7 \
//       node --import ./v4/tools/node/ctox-shared.mjs --test --test-force-exit modules/documents/tests/fix9b.test.mjs
import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { dirname, join, normalize, extname } from 'node:path';
import { homedir } from 'node:os';

const STUB = process.env.CTOX_APP_API_STUB || join(homedir(), '.local/state/shell-v4/stub-pins/8265bd7');
const BOS = normalize(join(dirname(fileURLToPath(import.meta.url)), '../../..'));
const require = createRequire(import.meta.url);
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || join(homedir(), 'Documents/ctox/src/apps/business-os/node_modules/playwright'));
const TYPES = { '.js': 'text/javascript', '.mjs': 'text/javascript', '.css': 'text/css', '.html': 'text/html', '.json': 'application/json', '.svg': 'image/svg+xml', '.wasm': 'application/wasm' };
const PAGE = `<!doctype html><html><head><meta charset="utf-8">
<link rel="stylesheet" href="/app.css"><link rel="stylesheet" href="/shared/base.css">
<link rel="stylesheet" href="/modules/documents/index.css">
<style>html,body{margin:0;height:100%;background:var(--bg,#fff)} #win{--shell-v2-icon-size:28px;width:100vw;height:100vh;position:relative;overflow:hidden}</style>
</head><body><div id="win"></div></body></html>`;
const DOCX = 'application/vnd.openxmlformats-officedocument.wordprocessingml.document';

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
  await page.evaluate(async (DOCX) => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    const format = await import('/modules/documents/vendor/document-format.mjs');
    window.__mod = await import('/modules/documents/index.js');
    const kernel = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false, actor: { role: 'admin', permissions: ['apps.source.edit'] } });
    await kernel.ready;
    const doc = (id, title, extra = {}) => ({
      id, title, filename: `${id}.docx`, description: '', mime_type: DOCX, status: 'Imported', document_type: 'word_document',
      owner_id: '', current_version_id: `${id}_v1`, source_sha256: '', page_count: 1, diagnostics_count: 0, linked_records: [],
      tags: [], display_cache: {}, index_text: '', is_deleted: false, created_at_ms: 20, updated_at_ms: 20, ...extra,
    });
    const ver = (id, model) => ({ id: `${id}_v1`, document_id: id, version: 1, source_kind: 'imported_docx', blob_id: `${id}_v1_blob`, model_json: model, diagnostics: [], created_at_ms: 20, updated_at_ms: 20 });
    await kernel.sim.push('documents', [
      doc('doc_word', 'Angebot Wartungsvertrag'),
      doc('doc_text', 'Protokoll Kickoff', { index_text: 'Teilnehmer: Vertrieb und Technik\n\nNächster Termin am Montag', updated_at_ms: 10 }),
    ]);
    await kernel.sim.push('document_versions', [
      ver('doc_word', format.importMarkdown('# Angebot\n\nWartungsvertrag für Halle 4 mit monatlicher Inspektion.').document),
      ver('doc_text', {}),
    ]);
    await kernel.sim.settle();
    window.__k = kernel;
    window.__zone = () => kernel.statusZone.forApp('documents').get().filter((e) => e.source === 'app');
    window.__t0 = performance.now();
    window.__app = await window.__mod.mount(document.getElementById('win'), kernel.appApi('documents', '1.0.0'), {});
  }, DOCX);
});
after(async () => { await browser?.close(); await new Promise((r) => server?.close(r)); });

const previewText = () => page.evaluate(() => {
  const p = document.querySelector('[data-documents-read-preview]');
  return p && !p.hidden && p.checkVisibility() ? p.innerText : '';
});

test('FIX9b: Word-Dokument erscheint sofort als Lesevorschau aus model_json; Editor startet unsichtbar dahinter', async () => {
  await page.waitForFunction(() => /Wartungsvertrag für Halle 4/.test(document.querySelector('[data-documents-read-preview]')?.innerText || ''), null, { timeout: 3000 });
  const tPreview = await page.evaluate(() => performance.now() - window.__t0);
  assert.ok(tPreview < 3000, `Vorschau nach ${Math.round(tPreview)} ms`);
  assert.match(await previewText(), /Angebot[\s\S]*Wartungsvertrag für Halle 4/);
  const visibleOutsidePreview = await page.evaluate(() => {
    const host = document.querySelector('[data-documents-editor]');
    return [...host.children].filter((c) => !c.matches('[data-documents-read-preview]') && c.checkVisibility({ visibilityProperty: true })).map((c) => c.className);
  });
  assert.deepEqual(visibleOutsidePreview, [], 'nur die Vorschau ist sichtbar, der Editor wärmt dahinter');
  // Der Editor wird angelegt (Bibliothek geladen) – das Vorschaubild bleibt, bis er bereit ist oder scheitert.
  await page.waitForFunction(() => document.querySelector('.documents-ctox-documents-frame') || window.__zone().some((e) => e.code === 'documents.editor'), null, { timeout: 15000 });
  const frame = await page.evaluate(() => document.querySelector('.documents-ctox-documents-frame')?.classList.contains('is-warming') ?? null);
  if (frame !== null && await page.$('[data-documents-read-preview]')) assert.equal(frame, true, 'Editor liegt unsichtbar hinter der Vorschau');
  console.log(`# FIX9b documents: Lesevorschau nach ${Math.round(tPreview)} ms`);
  assert.deepEqual(page.__errors, []);
});

test('FIX9b: ohne model_json Vorschau aus index_text; ohne beides keine leere Kachel', async () => {
  const html = await page.evaluate(() => {
    const h = window.__mod.__documentsTestHooks;
    return { text: h.docxReadPreviewHtml({ index_text: 'Zeile A\n\nZeile <B>' }, { model_json: {} }), none: h.docxReadPreviewHtml({ index_text: '' }, {}) };
  });
  assert.equal(html.text, '<p>Zeile A</p><p>Zeile &lt;B&gt;</p>');
  assert.equal(html.none, '');
});

test('FIX9b: Zeilenaktion „Weitere Aktionen“ (⋯) statt Sonnensymbol; Suchplatzhalter nicht abgeschnitten', async () => {
  const rows = await page.$$eval('[data-document-manage]', (els) => els.map((b) => ({ title: b.title, label: b.getAttribute('aria-label'), path: b.querySelector('path')?.getAttribute('d') || '' })));
  assert.ok(rows.length >= 2);
  for (const r of rows) {
    assert.equal(r.title, 'Weitere Aktionen');
    assert.match(r.label, / – Weitere Aktionen$/);
    assert.doesNotMatch(r.path, /M12 8\.5a3\.5/, 'kein Zahnrad-/Sonnenpfad');
  }
  const fit = await page.$eval('[data-documents-search]', (el) => {
    const cs = getComputedStyle(el);
    const ctx = document.createElement('canvas').getContext('2d');
    ctx.font = `${cs.fontWeight} ${cs.fontSize} ${cs.fontFamily}`;
    const need = ctx.measureText(el.placeholder).width + parseFloat(cs.paddingLeft) + parseFloat(cs.paddingRight);
    return { placeholder: el.placeholder, need, have: el.clientWidth };
  });
  assert.ok(fit.need <= fit.have, JSON.stringify(fit));
});
