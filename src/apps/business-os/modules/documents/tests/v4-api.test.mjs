// documents gegen den eingefrorenen App-API-4.0-Kern-Stub (createStubKernel, Pin CTOX_APP_API_STUB, read-only unter
// /__stub/ ausgeliefert) in echtem Chromium. Muster: modules/knowledge/tests/v4-api.test.mjs.
// Lauf: CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/64acc2c node --test src/apps/business-os/modules/documents/tests/v4-api.test.mjs
// Screenshots (1280 und 420) mit CTOX_DOCUMENTS_SHOTS=1 nach tests/shots/.
import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { readFile, mkdir } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { dirname, join, normalize, extname } from 'node:path';
import { homedir } from 'node:os';
import { realManifestRoutes } from '../../../v4/tools/port/real-manifest.mjs';

const STUB = process.env.CTOX_APP_API_STUB || join(homedir(), '.local/state/shell-v4/stub-pins/64acc2c');
const HERE = dirname(fileURLToPath(import.meta.url));
const BOS = normalize(join(HERE, '../../..'));
const require = createRequire(import.meta.url);
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || join(homedir(), 'Documents/ctox/src/apps/business-os/node_modules/playwright'));
const TYPES = { '.js': 'text/javascript', '.mjs': 'text/javascript', '.css': 'text/css', '.html': 'text/html', '.json': 'application/json', '.svg': 'image/svg+xml' };

const PAGE = `<!doctype html><html><head><meta charset="utf-8">
<link rel="stylesheet" href="/app.css"><link rel="stylesheet" href="/shared/base.css">
<link rel="stylesheet" href="/modules/documents/index.css">
<style>html,body{margin:0;height:100%;background:var(--bg,#fff)} #win{--shell-v2-icon-size:28px;width:100vw;height:100vh;position:relative;overflow:hidden} /* Wert wie v4/frame/frame.css .shell-window */</style>
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
      // Wie der Service Worker (§16.15): ctox:shared/x → Shell-Pfad (JS-Importe und CSS-@import).
      // ctox:lib/<name>@<major>/ → Shell-Standardbibliothek (SW: /~a/shell@<ver>/lib/…; hier die Quelle vendor/<name>/).
      if (/\.(m?js|css)$/.test(file)) body = Buffer.from(body.toString('utf8').replace(/(['"])ctox:shared\//g, '$1/shared/').replace(/(['"`(])ctox:lib\/([a-z0-9][a-z0-9._-]*)@\d+\//g, '$1/vendor/$2/'));
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
  page.on('response', (r) => { if (r.status() >= 400 && !/\/app\.css$|base\.css$/.test(r.url())) console.error('[page] HTTP', r.status(), r.url()); });
  page.__errors = errors;
  await page.goto(base);
  // FIXT-R: echte Routen/Rechte/Collections/Fremd-Collections von documents (v4/manifests/documents.manifest.json).
  await page.evaluate((routes) => { window.__routes = routes; }, realManifestRoutes('documents'));
  await page.evaluate(async () => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    const format = await import('/modules/documents/vendor/document-format.mjs');
    window.__format = format;
    window.__mod = await import('/modules/documents/index.js');
    const kernel = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false, actor: { role: 'admin', permissions: ['apps.source.edit'] } });
    await kernel.ready;
    await kernel.sim.push('business_apps', [{ id: 'ctox', title: 'CTOX' }, { id: 'documents', title: 'Dokumente' }]);
    // FIXT-R: Eigentümer der Fremd-Collections (ctox-platform, knowledge, spreadsheets) vor der eigenen Version, diese
    // nach der business_apps-Zeile.
    for (const o of window.__routes?._fixture_owners ?? []) if (!(await kernel.sim.inspect('business_apps', o.app))?.data?.active_ver) await kernel.sim.publishAppVersion(o.app, o.ver, o.publish);
    await kernel.sim.publishAppVersion('documents', '1.0.0', { title: 'Dokumente', manifestExtra: window.__routes });
    const md = (text) => format.importMarkdown(text).document;
    const doc = (id, title, updated) => ({
      id, title, filename: `${id}.md`, description: '', mime_type: 'text/markdown', status: 'Imported',
      document_type: 'markdown_document', owner_id: '', current_version_id: `${id}_v1`, source_sha256: '',
      page_count: 0, diagnostics_count: 0, linked_records: [], tags: ['angebot'], display_cache: {}, index_text: title,
      is_deleted: false, created_at_ms: updated, updated_at_ms: updated,
    });
    const ver = (id, text, updated) => ({
      id: `${id}_v1`, document_id: id, version: 1, source_kind: 'imported_markdown', blob_id: `${id}_v1_blob`,
      model_json: md(text), diagnostics: [], created_at_ms: updated, updated_at_ms: updated,
    });
    await kernel.sim.push('documents', [doc('doc_alpha', 'Alpha Angebot', 20), doc('doc_beta', 'Beta Vertrag', 10)]);
    await kernel.sim.push('document_versions', [ver('doc_alpha', '# Alpha\n\nAlpha Inhalt', 20), ver('doc_beta', '# Beta\n\nBeta Inhalt', 10)]);
    await kernel.sim.push('knowledge_items', [{ id: 'skill:vertrieb', kind: 'skillbook', title: 'Vertrieb Skill', updated_at_ms: 5 }]);
    await kernel.sim.settle();
    window.__k = kernel;
    window.__api = kernel.appApi('documents', '1.0.0');
    window.__zone = () => kernel.statusZone.forApp('documents').get().filter((e) => e.source === 'app');
    window.__commands = () => window.__api.query('business_commands', { sort: [{ created_at: 'asc' }] }).get();
    window.__md = md;
    window.__doc = doc;
    window.__ver = ver;
  });
});

after(async () => { await browser?.close(); await new Promise((r) => server?.close(r)); });

const settle = async () => { await page.evaluate(() => window.__k.sim.settle()); await page.waitForTimeout(250); };
const listText = () => page.$eval('[data-documents-list]', (el) => el.textContent.replace(/\s+/g, ' ').trim());
const STATE_TEXT = /lad|load|sync|warte|wait|vorbereitet|initiali|keine dokumente|no documents/i;

async function mountMain(apiExpr = 'window.__c.api', restored = undefined) {
  await page.evaluate(async ({ apiExpr, restored }) => {
    // eslint-disable-next-line no-eval
    const api = eval(apiExpr);
    window.__app = await window.__mod.mount(document.getElementById('win'), api, restored ? { restored } : {});
  }, { apiExpr, restored });
  await settle();
}

test('mount: Live-Daten aus api.query, Titel über api.shell, gescoptes root, Markdown-Version im Editor', async () => {
  await page.evaluate(async () => {
    const { countingApi } = await import('/v4/tools/port/test-api.mjs');
    window.__c = countingApi(window.__api);
  });
  await mountMain();
  const text = await listText();
  assert.match(text, /Alpha Angebot/);
  assert.match(text, /Beta Vertrag/);
  assert.equal(await page.$eval('#win', (el) => el.dataset.app), 'documents');
  assert.equal(await page.evaluate(() => window.__k.shell.window('documents').state.get().title), 'Dokumente');
  assert.match(await page.$eval('[data-documents-editor]', (el) => el.textContent), /Alpha Inhalt/);
  assert.equal(await page.evaluate(() => window.__c.subscribers('documents')), 1);
  assert.equal(await page.evaluate(() => window.__c.subscribers('document_versions')), 1, 'ein Versionsabo je Auswahl');
  assert.deepEqual(await page.evaluate(() => window.__zone()), []);
  assert.deepEqual(page.__errors, []);
});

test('Live-Änderung vom Core (sim.push) erscheint ohne Neuladen, Löschung verschwindet', async () => {
  await page.evaluate(() => window.__k.sim.push('documents', window.__doc('doc_gamma', 'Gamma Live', 30)));
  await settle();
  assert.match(await listText(), /Gamma Live/);
  await page.evaluate(() => window.__k.sim.push('documents', { id: 'doc_gamma', _deleted: true }));
  await settle();
  assert.doesNotMatch(await listText(), /Gamma Live/);
});

test('leere Collections: kein Text in Liste und Editor, keine Zone', async () => {
  const empty = await page.evaluate(async () => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    const k2 = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false, instanceId: 'empty' });
    await k2.ready;
    // E48: auch der leere Kern braucht die veröffentlichte documents-Version (foreign_collections business_commands u. a.).
    for (const o of window.__routes?._fixture_owners ?? []) if (!(await k2.sim.inspect('business_apps', o.app))?.data?.active_ver) await k2.sim.publishAppVersion(o.app, o.ver, o.publish);
    await k2.sim.publishAppVersion('documents', '1.0.0', { title: 'Dokumente', manifestExtra: window.__routes });
    const host = document.createElement('div');
    host.style.cssText = 'width:900px;height:500px;position:relative';
    document.body.append(host);
    const app = await window.__mod.mount(host, k2.appApi('documents', '1.0.0'), {});
    await new Promise((r) => setTimeout(r, 300));
    const result = {
      list: host.querySelector('[data-documents-list]').textContent.trim(),
      editor: host.querySelector('[data-documents-editor]').textContent.trim(),
      stateBox: Boolean(host.querySelector('.ctox-syncing, [role="status"], .documents-loading')),
      zone: k2.statusZone.forApp('documents').get().map((e) => e.code),
    };
    await app.unmount();
    host.remove();
    await k2.close();
    return result;
  });
  assert.equal(empty.list, '');
  assert.equal(empty.editor, '');
  assert.equal(empty.stateBox, false);
  assert.deepEqual(empty.zone, []);
});

test('Schreiben: Markdown-Bearbeitung patcht die Version (api.patch → accepted) und den Datensatz', async () => {
  await page.click('.documents-markdown-toggle');
  await page.fill('.documents-markdown-textarea', '# Alpha\n\nAlpha geändert');
  await settle();
  const state = await page.evaluate(() => ({
    version: window.__api.get('document_versions', 'doc_alpha_v1').get(),
    record: window.__api.get('documents', 'doc_alpha').get(),
    outbox: window.__k.sim.outbox ? null : null,
  }));
  assert.match(JSON.stringify(state.version.model_json), /Alpha geändert/);
  assert.equal(state.record.status, 'Draft');
  assert.match(state.record.index_text, /Alpha geändert/);
});

test('Import über api.files: Datei-Metadaten in files, Version und Datensatz per api.put, Bytes über files.get', async () => {
  await page.click('[data-documents-import-open]');
  await page.setInputFiles('[data-documents-import-file]', { name: 'import-test.md', mimeType: 'text/markdown', buffer: Buffer.from('# Import\n\nImportierter Text') });
  await page.click('[data-documents-import-form] button[type="submit"]');
  await settle();
  const r = await page.evaluate(async () => {
    const record = window.__api.query('documents', { selector: { filename: 'import-test.md' } }).get()[0];
    const version = window.__api.get('document_versions', record.current_version_id).get();
    const file = window.__api.get('files', version.blob_id).get();
    const bytes = await (await window.__api.files.get(file.ref)).text();
    return { record, version, file, bytes, drawer: Boolean(document.querySelector('[data-documents-app-drawer]')) };
  });
  assert.equal(r.record.title, 'import-test');
  assert.equal(r.version.document_id, r.record.id);
  assert.equal(r.file.ref, r.record.source_sha256);
  assert.equal(r.file.owner_app, 'documents');
  assert.match(r.bytes, /Importierter Text/);
  assert.equal(r.drawer, false, 'Drawer schließt nach dem Import');
  assert.match(await listText(), /import-test/);
});

test('Befehl: Import mit Runbook legt business_commands-Dokument an; failed → genau ein Zoneneintrag', async () => {
  await page.evaluate(() => window.__k.sim.failCommand('research.systematic.report.create', 'kaputt'));
  await page.click('[data-documents-import-open]');
  await page.setInputFiles('[data-documents-import-file]', { name: 'runbook-test.md', mimeType: 'text/markdown', buffer: Buffer.from('# Runbook\n\nText') });
  await page.selectOption('[data-documents-import-mode]', 'runbook');
  await page.fill('[data-documents-runbook-prompt]', 'Bitte prüfen');
  await page.click('[data-documents-import-form] button[type="submit"]');
  await settle();
  const cmd = await page.evaluate(() => window.__commands().at(-1));
  assert.equal(cmd.type, 'research.systematic.report.create');
  assert.equal(cmd.payload.module, 'documents');
  assert.equal(cmd.payload.prompt, 'Bitte prüfen');
  assert.match(cmd.payload.record_id, /^doc_/);
  assert.equal(cmd.record_id, cmd.payload.record_id);
  assert.equal(cmd.status, 'failed');
  const zone = await page.evaluate(() => window.__zone());
  assert.equal(zone.length, 1);
  assert.equal(zone[0].code, 'documents.command.research.systematic.report.create');
  assert.match(zone[0].text, /kaputt/);
  await page.evaluate(() => window.__api.status.clear('documents.command.research.systematic.report.create'));
});

test('Löschen fragt über api.shell.dialog: ohne Ja bleibt das Dokument, mit Ja is_deleted', async () => {
  const openManage = async () => {
    await page.evaluate(() => {
      const card = [...document.querySelectorAll('.documents-card')].find((el) => /Beta Vertrag/.test(el.textContent));
      card.querySelector('[data-document-manage]').click();
    });
    await page.click('[data-documents-delete]');
    await settle();
  };
  await openManage();
  assert.equal(await page.evaluate(() => window.__api.get('documents', 'doc_beta').get().is_deleted), false);
  assert.equal(await page.evaluate(() => window.__k.sim.dialogs.length), 1);
  await page.evaluate(async () => {
    const { withDialogAnswer } = await import('/v4/tools/port/test-api.mjs');
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), withDialogAnswer(window.__c.api, true), {});
  });
  await settle();
  await openManage();
  assert.equal(await page.evaluate(() => window.__api.get('documents', 'doc_beta').get().is_deleted), true);
  assert.doesNotMatch(await listText(), /Beta Vertrag/);
});

test('snapshot/restored: Auswahl, Suche und offener Markdown-Entwurf überleben den Remount', async () => {
  await page.evaluate(() => {
    const card = [...document.querySelectorAll('.documents-card')].find((el) => /Alpha Angebot/.test(el.textContent));
    card.querySelector('[data-document-id]').click();
  });
  await settle();
  await page.click('.documents-markdown-toggle');
  await page.evaluate(() => { document.querySelector('.documents-markdown-textarea').value = 'halb fertiger Entwurf'; });
  const snap = await page.evaluate(async () => {
    const cloned = structuredClone(window.__app.snapshot());
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, { restored: cloned });
    return cloned;
  });
  assert.equal(snap.selectedId, 'doc_alpha');
  assert.equal(snap.markdownDraft.text, 'halb fertiger Entwurf');
  assert.ok(JSON.stringify(snap).length < 1024 * 1024);
  await settle();
  const after = await page.evaluate(() => ({
    value: document.querySelector('.documents-markdown-textarea')?.value,
    hidden: document.querySelector('.documents-markdown-textarea')?.hidden,
    selected: window.__app.snapshot().selectedId,
  }));
  assert.equal(after.value, 'halb fertiger Entwurf');
  assert.equal(after.hidden, false);
  assert.equal(after.selected, 'doc_alpha');
  assert.equal(await page.evaluate(() => window.__c.subscribers('documents')), 1, 'alte Abos sind abgemeldet');
  assert.deepEqual(page.__errors, []);
});

test('Deep-Link über api.shell.onRoute: unbekannter Datensatz → Zustandszone, kein Kasten im Inhalt', async () => {
  await page.evaluate(() => window.__k.shell.navigate?.('documents', { record: 'doc_fehlt' }) ?? window.__api.shell.navigate('documents', { record: 'doc_fehlt' }));
  await settle();
  const zone = await page.evaluate(() => window.__zone().map((e) => e.code));
  const box = await page.evaluate(() => Boolean(document.querySelector('.ctox-callout, [data-documents-source-focus-status]')));
  assert.equal(box, false);
  // Ob der Stub onRoute für die eigene App auslöst, ist Stub-Verhalten; wenn ja, muss genau dieser Code erscheinen.
  if (zone.length) assert.deepEqual(zone, ['documents.route.unknown_record']);
});

test('Office-Brücke: prepare löst über den gepushten Befehlsstatus auf, failed lehnt ab, loadVersion liest api.files', async () => {
  const r = await page.evaluate(async () => {
    const { createOfficeBridgeV4, putFile } = await import('/modules/documents/office-bridge-v4.js');
    const k = window.__k;
    const api = window.__api;
    k.sim.onCommand('office.document.prepare', (payload) => ({ ok: true, operation: 'prepare', version_id: payload.version_id, editor_blob_id: 'ed_1' }));
    const bridge = createOfficeBridgeV4(api, 'document');
    const prepared = await bridge.prepare({ recordId: 'doc_alpha', versionId: 'doc_alpha_v1' });
    k.sim.failCommand('office.document.export', 'engine down');
    let exportError = '';
    try { await bridge.export({ recordId: 'doc_alpha', versionId: 'doc_alpha_v1', format: 'docx' }); } catch (e) { exportError = e.message; }
    await putFile(api, 'doc_alpha_v1_blob', new TextEncoder().encode('ALPHA-BYTES'), 'text/markdown');
    await putFile(api, 'ed_1', new TextEncoder().encode('EDITOR-BYTES'), 'application/octet-stream');
    const loaded = await bridge.loadVersion({ recordId: 'doc_alpha' });
    bridge.dispose();
    return {
      prepared,
      exportError,
      canonical: new TextDecoder().decode(loaded.canonicalBytes),
      editor: new TextDecoder().decode(loaded.editorBytes),
      conversion: loaded.version.conversion_state,
    };
  });
  assert.equal(r.prepared.editor_blob_id, 'ed_1');
  assert.match(r.exportError, /engine down/);
  assert.equal(r.canonical, 'ALPHA-BYTES');
  assert.equal(r.editor, 'EDITOR-BYTES');
  assert.equal(r.conversion, 'prepared');
});

test('Agenten-Rückruf on_failed (Fehler und Abbruch) → je Aufgabe genau ein Zoneneintrag, complete → keiner (SKL-B3)', async () => {
  const zone = await page.evaluate(async () => {
    const cb = (id, type, payload) => ({ id, type, app_id: 'documents', status: 'done', payload });
    await window.__k.sim.push('business_commands', [
      cb('cb_ok', 'document.runbook.complete', { task_id: 'task_ok', status: 'completed', result: { text: 'fertig' } }),
      cb('cb_fail', 'research.systematic.report.failed', { task_id: 'task_fail', status: 'failed', error: { code: 'failed', message: 'harness timeout' } }),
      cb('cb_cancel', 'document.runbook.failed', { task_id: 'task_cancel', status: 'cancelled', result: null }),
    ]);
    await window.__k.sim.settle();
    return window.__zone().filter((e) => e.code.startsWith('documents.agent.')).map((e) => [e.code, e.text]);
  });
  assert.deepEqual(zone.sort(), [
    ['documents.agent.task_cancel', 'document.runbook.failed: cancelled'],
    ['documents.agent.task_fail', 'research.systematic.report.failed: harness timeout'],
  ]);
  await page.evaluate(() => { window.__api.status.clear('documents.agent.task_fail'); window.__api.status.clear('documents.agent.task_cancel'); });
});

test('Screenshots 1280 und 420 (nur mit CTOX_DOCUMENTS_SHOTS=1)', { skip: !process.env.CTOX_DOCUMENTS_SHOTS }, async () => {
  await mkdir(join(HERE, 'shots'), { recursive: true });
  await page.setViewportSize({ width: 1280, height: 800 });
  await settle();
  await page.screenshot({ path: join(HERE, 'shots/documents-1280.png') });
  await page.setViewportSize({ width: 420, height: 800 });
  await settle();
  await page.screenshot({ path: join(HERE, 'shots/documents-420.png') });
  await page.setViewportSize({ width: 1280, height: 800 });
});

test('unmount meldet alle Abos ab und leert root', async () => {
  const res = await page.evaluate(async () => {
    await window.__app.unmount();
    await window.__k.sim.push('documents', window.__doc('doc_late', 'Late', 99));
    await window.__k.sim.settle();
    return {
      subs: ['documents', 'document_versions', 'document_runbooks', 'knowledge_items', 'knowledge_runbooks', 'knowledge_tables', 'ctox_runtime_settings', 'business_commands'].map((c) => window.__c.subscribers(c)),
      children: document.getElementById('win').children.length,
    };
  });
  assert.deepEqual(res.subs, [0, 0, 0, 0, 0, 0, 0, 0]);
  assert.equal(res.children, 0);
  assert.deepEqual(page.__errors, []);
});
