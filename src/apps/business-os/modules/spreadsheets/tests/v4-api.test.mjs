// spreadsheets gegen den eingefrorenen App-API-4.0-Kern-Stub (createStubKernel, Pin CTOX_APP_API_STUB, read-only unter
// /__stub/ ausgeliefert) in echtem Chromium. Muster: modules/knowledge/tests/v4-api.test.mjs.
// Lauf: CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/4b0d0ac node --test src/apps/business-os/modules/spreadsheets/tests/v4-api.test.mjs
// Screenshots (1280 und 420) mit CTOX_SPREADSHEETS_SHOTS=1 nach tests/shots/.
import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { readFile, mkdir } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { dirname, join, normalize, extname } from 'node:path';
import { homedir } from 'node:os';
import { realManifestRoutes } from '../../../v4/tools/port/real-manifest.mjs';

const STUB = process.env.CTOX_APP_API_STUB || join(homedir(), '.local/state/shell-v4/stub-pins/4b0d0ac');
const HERE = dirname(fileURLToPath(import.meta.url));
const BOS = normalize(join(HERE, '../../..'));
const require = createRequire(import.meta.url);
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || join(homedir(), 'Documents/ctox/src/apps/business-os/node_modules/playwright'));
const TYPES = { '.js': 'text/javascript', '.mjs': 'text/javascript', '.css': 'text/css', '.html': 'text/html', '.json': 'application/json', '.svg': 'image/svg+xml', '.wasm': 'application/wasm' };

const PAGE = `<!doctype html><html><head><meta charset="utf-8">
<link rel="stylesheet" href="/app.css"><link rel="stylesheet" href="/shared/base.css">
<link rel="stylesheet" href="/modules/spreadsheets/index.css">
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
  page.__errors = errors;
  await page.goto(base);
  // FIXT-R: echte Routen/Rechte/Collections/Fremd-Collections von spreadsheets (v4/manifests/spreadsheets.manifest.json).
  await page.evaluate((routes) => { window.__routes = routes; }, realManifestRoutes('spreadsheets'));
  await page.evaluate(async () => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    window.__mod = await import('/modules/spreadsheets/index.js');
    const kernel = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false, actor: { role: 'admin', permissions: ['apps.source.edit'] } });
    await kernel.ready;
    await kernel.sim.push('business_apps', [{ id: 'ctox', title: 'CTOX' }, { id: 'spreadsheets', title: 'Tabellen' }]);
    // FIXT-R: Eigentümer der Fremd-Collections (ctox-platform, desktop, documents) vor der eigenen Version, diese nach
    // der business_apps-Zeile.
    for (const o of window.__routes?._fixture_owners ?? []) if (!(await kernel.sim.inspect('business_apps', o.app))?.data?.active_ver) await kernel.sim.publishAppVersion(o.app, o.ver, o.publish);
    await kernel.sim.publishAppVersion('spreadsheets', '1.0.0', { title: 'Tabellen', manifestExtra: window.__routes });
    const sheet = (id, title, updated) => ({
      id, title, filename: `${id}.xlsx`, mime_type: 'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
      status: 'Imported', spreadsheet_type: 'xlsx', owner_id: '', current_version_id: `${id}_v1`, source_sha256: '',
      row_count: 0, col_count: 0, diagnostics_count: 0, linked_records: [], tags: ['budget'], display_cache: {},
      index_text: title, is_deleted: false, created_at_ms: updated, updated_at_ms: updated,
    });
    const ver = (id, updated) => ({
      id: `${id}_v1`, spreadsheet_id: id, version: 1, source_kind: 'imported_xlsx', blob_id: `${id}_v1_blob`,
      model_json: { format: 'xlsx', data: [], columns: [] }, diagnostics: [], created_at_ms: updated, updated_at_ms: updated,
    });
    await kernel.sim.push('spreadsheets', [sheet('sheet_alpha', 'Alpha Budget', 20), sheet('sheet_beta', 'Beta Forecast', 10)]);
    await kernel.sim.push('spreadsheet_versions', [ver('sheet_alpha', 20), ver('sheet_beta', 10)]);
    await kernel.sim.settle();
    window.__k = kernel;
    window.__api = kernel.appApi('spreadsheets', '1.0.0');
    window.__zone = () => kernel.statusZone.forApp('spreadsheets').get().filter((e) => e.source === 'app');
    window.__commands = () => window.__api.query('business_commands', { sort: [{ created_at: 'asc' }] }).get();
    window.__sheet = sheet;
  });
});

after(async () => { await browser?.close(); await new Promise((r) => server?.close(r)); });

const settle = async () => { await page.evaluate(() => window.__k.sim.settle()); await page.waitForTimeout(250); };
const listText = () => page.$eval('[data-spreadsheets-list]', (el) => el.textContent.replace(/\s+/g, ' ').trim());
// Der Office-Editor (vendor/ctox-office, iframe) läuft gegen den Stub nicht; sein Start-/Öffnungsfehler darf nur
// als Zone `spreadsheets.editor` erscheinen, nie als Kasten im Inhalt.
const appZone = () => page.evaluate(() => window.__zone().map((e) => e.code).filter((c) => c !== 'spreadsheets.editor'));

test('mount: Live-Daten aus api.query, Titel über api.shell, gescoptes root, kein Zustandskasten', async () => {
  await page.evaluate(async () => {
    const { countingApi } = await import('/v4/tools/port/test-api.mjs');
    window.__c = countingApi(window.__api);
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, {});
  });
  await settle();
  const text = await listText();
  assert.match(text, /Alpha Budget/);
  assert.match(text, /Beta Forecast/);
  assert.equal(await page.$eval('#win', (el) => el.dataset.app), 'spreadsheets');
  assert.equal(await page.evaluate(() => window.__k.shell.window('spreadsheets').state.get().title), 'Tabellen');
  assert.equal(await page.evaluate(() => window.__c.subscribers('spreadsheets')), 1);
  assert.equal(await page.evaluate(() => window.__c.subscribers('spreadsheet_versions')), 1, 'ein Versionsabo je Auswahl');
  assert.equal(await page.evaluate(() => Boolean(document.querySelector('.ctox-syncing, .spreadsheets-error, #win .ctox-empty'))), false);
  assert.deepEqual(await appZone(), []);
  assert.deepEqual(page.__errors, []);
});

test('Live-Änderung vom Core (sim.push) erscheint ohne Neuladen, Löschung verschwindet', async () => {
  await page.evaluate(() => window.__k.sim.push('spreadsheets', window.__sheet('sheet_gamma', 'Gamma Live', 30)));
  await settle();
  assert.match(await listText(), /Gamma Live/);
  await page.evaluate(() => window.__k.sim.push('spreadsheets', { id: 'sheet_gamma', _deleted: true }));
  await settle();
  assert.doesNotMatch(await listText(), /Gamma Live/);
});

test('leere Collections: kein Text in Liste und Editor, keine Zone', async () => {
  const empty = await page.evaluate(async () => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    const k2 = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false, instanceId: 'empty' });
    await k2.ready;
    const host = document.createElement('div');
    host.style.cssText = 'width:900px;height:500px;position:relative';
    document.body.append(host);
    const app = await window.__mod.mount(host, k2.appApi('spreadsheets', '1.0.0'), {});
    await new Promise((r) => setTimeout(r, 300));
    const result = {
      list: host.querySelector('[data-spreadsheets-list]').textContent.trim(),
      editorBody: [...host.querySelector('[data-spreadsheets-editor]').children].filter((el) => !el.matches('header')).map((el) => el.textContent.trim()).join(''),
      stateBox: Boolean(host.querySelector('.ctox-syncing, [role="status"]:not([data-spreadsheets-form-status]), .ctox-empty')),
      zone: k2.statusZone.forApp('spreadsheets').get().map((e) => e.code),
    };
    await app.unmount();
    host.remove();
    await k2.close();
    return result;
  });
  assert.equal(empty.list, '');
  assert.equal(empty.editorBody, '');
  assert.equal(empty.stateBox, false);
  assert.deepEqual(empty.zone, []);
});

test('Import über api.files: CSV → files-Dokument, Version und Datensatz per api.put, Bytes über files.get', async () => {
  await page.click('[data-spreadsheets-import-open]');
  await page.setInputFiles('[data-import-file]', { name: 'umsatz.csv', mimeType: 'text/csv', buffer: Buffer.from('Monat,Umsatz\nJan,100\nFeb,120\n') });
  await page.fill('[data-import-tags]', 'Sales');
  await page.click('[data-spreadsheets-import-form] button[type="submit"]');
  await settle();
  const r = await page.evaluate(async () => {
    const record = window.__api.query('spreadsheets', { selector: { filename: 'umsatz.csv' } }).get()[0];
    const version = window.__api.get('spreadsheet_versions', record.current_version_id).get();
    const file = window.__api.get('files', version.blob_id).get();
    const bytes = await (await window.__api.files.get(file.ref)).text();
    return { record, version, file, bytes, drawer: Boolean(document.querySelector('[data-spreadsheets-app-drawer]')) };
  });
  assert.equal(r.record.title, 'umsatz');
  assert.deepEqual(r.record.tags, ['Sales']);
  assert.equal(r.record.row_count, 3);
  assert.equal(r.version.spreadsheet_id, r.record.id);
  assert.equal(r.file.ref, r.record.source_sha256);
  assert.equal(r.file.owner_app, 'spreadsheets');
  assert.match(r.bytes, /Feb,120/);
  assert.equal(r.drawer, false);
  assert.match(await listText(), /umsatz/);
});

test('Rundgang RUND2: Import-Schublade ohne Fehlerzeile; falsche Datei → Zone, Schließen räumt', async () => {
  await page.click('[data-spreadsheets-import-open]');
  await settle();
  const pristine = await page.evaluate(() => {
    const st = document.querySelector('[data-spreadsheets-import-form] [data-spreadsheets-form-status]');
    return { text: st?.textContent || '', hidden: st ? st.hidden : true, disabled: document.querySelector('[data-spreadsheets-import-form] button[type="submit"]').disabled };
  });
  assert.deepEqual(pristine, { text: '', hidden: true, disabled: true });
  await page.setInputFiles('[data-spreadsheets-import-form] [data-import-file]', { name: 'bild.png', mimeType: 'image/png', buffer: Buffer.from('x') });
  await settle();
  assert.ok((await appZone()).includes('spreadsheets.import.file'), 'falsche Datei meldet die Zone');
  assert.equal(await page.evaluate(() => document.querySelector('[data-spreadsheets-import-form] [data-spreadsheets-form-status]').textContent), '');
  await page.evaluate(() => document.querySelector('[data-spreadsheets-import-form]').closest('[data-spreadsheets-app-drawer]').querySelector('button[type="button"]')?.click());
  await settle();
  assert.equal((await appZone()).includes('spreadsheets.import.file'), false);
});

test('Befehl über die Office-Brücke: business_commands-Dokument, prepare → done, failed lehnt ab; Bytes über api.files', async () => {
  const r = await page.evaluate(async () => {
    const { createOfficeBridgeV4, putFile } = await import('/modules/spreadsheets/office-bridge-v4.js');
    const k = window.__k;
    const api = window.__api;
    k.sim.onCommand('office.spreadsheet.prepare', (payload) => ({ ok: true, operation: 'prepare', version_id: payload.version_id, editor_blob_id: 'sed_1' }));
    const bridge = createOfficeBridgeV4(api, 'spreadsheet');
    const prepared = await bridge.prepare({ recordId: 'sheet_alpha', versionId: 'sheet_alpha_v1' });
    const cmd = window.__commands().find((c) => c.type === 'office.spreadsheet.prepare' && c.record_id === 'sheet_alpha');
    k.sim.failCommand('office.spreadsheet.commit', 'conflict');
    let commitError = '';
    try { await bridge.commit({ recordId: 'sheet_alpha', baseVersionId: 'sheet_alpha_v1', bytes: new Uint8Array([1, 2, 3]) }); } catch (e) { commitError = e.message; }
    const commitCmd = window.__commands().filter((c) => c.type === 'office.spreadsheet.commit' && c.record_id === 'sheet_alpha').at(-1);
    await putFile(api, 'sheet_alpha_v1_blob', new TextEncoder().encode('XLSX-BYTES'), 'application/octet-stream');
    await putFile(api, 'sed_1', new TextEncoder().encode('ED-BYTES'), 'application/octet-stream');
    const loaded = await bridge.loadVersion({ recordId: 'sheet_alpha' });
    bridge.dispose();
    return {
      prepared, cmd, commitError, commitCmd,
      stagedFile: api.get('files', commitCmd.payload.editor_blob_id).get(),
      canonical: new TextDecoder().decode(loaded.canonicalBytes),
      editor: new TextDecoder().decode(loaded.editorBytes),
    };
  });
  assert.equal(r.prepared.editor_blob_id, 'sed_1');
  assert.equal(r.cmd.status, 'done');
  assert.equal(r.cmd.record_id, 'sheet_alpha');
  assert.equal(r.cmd.payload.module, 'spreadsheets');
  assert.match(r.commitError, /conflict/);
  assert.equal(r.commitCmd.status, 'failed');
  assert.equal(r.stagedFile.ref, r.commitCmd.payload.editor_sha256, 'Editor-Bytes liegen als files-Dokument vor dem Befehl');
  assert.equal(r.canonical, 'XLSX-BYTES');
  assert.equal(r.editor, 'ED-BYTES');
});

test('Löschen fragt über api.shell.dialog: ohne Ja bleibt die Tabelle, mit Ja is_deleted', async () => {
  const openManageAndDelete = async () => {
    await page.evaluate(() => {
      const card = [...document.querySelectorAll('.spreadsheets-card')].find((el) => /Beta Forecast/.test(el.textContent));
      card.querySelector('.spreadsheets-card-manage').click();
    });
    await settle();
    await page.click('[data-spreadsheets-app-drawer] [data-action="delete"]');
    await settle();
  };
  await openManageAndDelete();
  assert.equal(await page.evaluate(() => window.__api.get('spreadsheets', 'sheet_beta').get().is_deleted), false);
  assert.ok(await page.evaluate(() => window.__k.sim.dialogs.length) >= 1);
  await page.evaluate(async () => {
    const { withDialogAnswer } = await import('/v4/tools/port/test-api.mjs');
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), withDialogAnswer(window.__c.api, true), {});
  });
  await settle();
  await openManageAndDelete();
  assert.equal(await page.evaluate(() => window.__api.get('spreadsheets', 'sheet_beta').get().is_deleted), true);
  assert.doesNotMatch(await listText(), /Beta Forecast/);
});

test('snapshot/restored: Auswahl und offene Sucheingabe überleben den Remount', async () => {
  await page.fill('[data-spreadsheets-search]', 'Alpha');
  await page.waitForTimeout(300);
  const snap = await page.evaluate(async () => {
    const cloned = structuredClone(window.__app.snapshot());
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, { restored: cloned });
    return cloned;
  });
  assert.equal(snap.searchQuery.toLowerCase(), 'alpha', 'pane-grammar normalisiert die Suche');
  assert.ok(snap.selectedId);
  assert.ok(JSON.stringify(snap).length < 1024 * 1024);
  await settle();
  const after = await page.evaluate(() => ({
    search: document.querySelector('[data-spreadsheets-search]').value,
    selected: window.__app.snapshot().selectedId,
    list: document.querySelector('[data-spreadsheets-list]').textContent,
  }));
  assert.equal(after.search, snap.searchQuery);
  assert.equal(after.selected, snap.selectedId);
  assert.match(after.list, /Alpha Budget/);
  assert.doesNotMatch(after.list, /umsatz/);
  assert.equal(await page.evaluate(() => window.__c.subscribers('spreadsheets')), 1, 'alte Abos sind abgemeldet');
  await page.fill('[data-spreadsheets-search]', '');
  await page.waitForTimeout(300);
  assert.deepEqual(page.__errors, []);
});

test('Screenshots 1280 und 420 (nur mit CTOX_SPREADSHEETS_SHOTS=1)', { skip: !process.env.CTOX_SPREADSHEETS_SHOTS }, async () => {
  await mkdir(join(HERE, 'shots'), { recursive: true });
  await page.setViewportSize({ width: 1280, height: 800 });
  await settle();
  await page.screenshot({ path: join(HERE, 'shots/spreadsheets-1280.png') });
  await page.setViewportSize({ width: 420, height: 800 });
  await settle();
  await page.screenshot({ path: join(HERE, 'shots/spreadsheets-420.png') });
  await page.setViewportSize({ width: 1280, height: 800 });
});

test('unmount meldet alle Abos ab und leert root', async () => {
  const res = await page.evaluate(async () => {
    await window.__app.unmount();
    await window.__k.sim.push('spreadsheets', window.__sheet('sheet_late', 'Late', 99));
    await window.__k.sim.settle();
    return {
      subs: ['spreadsheets', 'spreadsheet_versions', 'spreadsheet_runbooks'].map((c) => window.__c.subscribers(c)),
      children: document.getElementById('win').children.length,
    };
  });
  assert.deepEqual(res.subs, [0, 0, 0]);
  assert.equal(res.children, 0);
  assert.deepEqual(page.__errors, []);
});
