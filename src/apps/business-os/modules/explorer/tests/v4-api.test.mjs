// explorer gegen den eingefrorenen App-API-4.0-Stub (createStubKernel) in echtem Chromium.
// Stub: CTOX_APP_API_STUB (Pflicht-Pin ~/.local/state/shell-v4/stub-pins/68b83a3), Muster modules/knowledge.
// Lauf: CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/68b83a3 node --test src/apps/business-os/modules/explorer/tests/v4-api.test.mjs
import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { readFile, mkdir, stat } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { dirname, join, normalize, extname } from 'node:path';
import { homedir } from 'node:os';

const APP = 'explorer';
const STUB = process.env.CTOX_APP_API_STUB || join(homedir(), '.local/state/shell-v4/stub-pins/68b83a3');
const HERE = dirname(fileURLToPath(import.meta.url));
const BOS = normalize(join(HERE, '../../..'));
const SHOTS = join(HERE, 'shots');
const require = createRequire(import.meta.url);
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || join(homedir(), 'Documents/ctox/src/apps/business-os/node_modules/playwright'));
const TYPES = { '.js': 'text/javascript', '.mjs': 'text/javascript', '.css': 'text/css', '.html': 'text/html', '.json': 'application/json', '.svg': 'image/svg+xml' };

const PAGE = `<!doctype html><html><head><meta charset="utf-8">
<link rel="stylesheet" href="/app.css"><link rel="stylesheet" href="/shared/base.css">
<link rel="stylesheet" href="/modules/${APP}/index.css">
<style>html,body{margin:0;height:100%;overflow:hidden;background:var(--bg)} #win{width:100vw;height:100vh;container:business-app-window/inline-size;--shell-v2-icon-size:28px}</style>
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
      // Wie der Service Worker (§16.15): ctox:shared/x.js → Shell-Pfad (PORTING.md Falle 2).
      if (/\.m?js$/.test(file)) body = Buffer.from(body.toString('utf8').replace(/(['"])ctox:shared\//g, '$1/shared/'));
      res.writeHead(200, { 'content-type': TYPES[extname(file)] || 'application/octet-stream' }); res.end(body);
    } catch { res.writeHead(404); res.end(); }
  });
  await new Promise((r) => server.listen(0, '127.0.0.1', r));
  base = `http://127.0.0.1:${server.address().port}/`;
  browser = await chromium.launch();
  page = await browser.newPage({ viewport: { width: 1280, height: 720 } });
  const errors = [];
  page.on('pageerror', (e) => errors.push(e.message));
  page.on('console', (m) => { if (m.type() === 'error') console.error('[page]', m.text()); });
  page.__errors = errors;
  await page.goto(base);
  await page.evaluate(async (APP) => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    window.__mod = await import(`/modules/${APP}/index.js`);
    const kernel = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false });
    await kernel.ready;
    await kernel.sim.push('business_apps', [{ id: 'explorer', title: 'Explorer' }, { id: 'file-viewer', title: 'Datei-Viewer' }, { id: 'documents', title: 'Documents' }, { id: 'spreadsheets', title: 'Spreadsheets' }]);
    const now = Date.now();
    await kernel.sim.push('desktop_files', [
      { id: 'fs_root', parent_id: '', path: '/', name: 'Files', kind: 'folder', mime_type: '', extension: '', size_bytes: 0, source: 'system', is_deleted: false, sort_index: 0, created_at_ms: 1, updated_at_ms: 1 },
      { id: 'fs_ctox', parent_id: 'fs_root', path: '/CTOX', name: 'CTOX', kind: 'folder', mime_type: '', extension: '', size_bytes: 0, source: 'system', is_deleted: false, sort_index: 40, created_at_ms: 1, updated_at_ms: 1 },
      { id: 'lazy-log', parent_id: 'fs_ctox', path: '/CTOX/run.log', local_path: '/srv/ctox/run.log', name: 'run.log', kind: 'file', mime_type: 'text/plain', size_bytes: 2000000, source: 'ctox', content_state: 'lazy', is_deleted: false, created_at_ms: 2, updated_at_ms: 2 },
    ]);
    await kernel.sim.push('documents', [{ id: 'doc-1', title: 'Angebot Alpha', filename: 'alpha.docx', mime_type: 'application/vnd.openxmlformats-officedocument.wordprocessingml.document', status: 'draft', current_version_id: 'v1', index_text: '', is_deleted: false, created_at_ms: 3, updated_at_ms: now }]);
    await kernel.sim.settle();
    window.__k = kernel;
    window.__api = kernel.appApi(APP, '1.0.0');
    window.__viewer = kernel.appApi('file-viewer', '1.0.0');
    window.__routes = [];
    window.__viewer.shell.onRoute((ev) => window.__routes.push(ev));
    window.__zone = () => kernel.statusZone.forApp(APP).get().filter((e) => e.source === 'app');
    window.__commands = () => window.__api.query('business_commands', { sort: [{ created_at: 'asc' }] }).get();
    window.__upload = async (text, meta) => {
      const h = window.__api.files.put(new Blob([text], { type: meta.mime }), meta);
      await h.local;
      await h.result;
      return h.ref;
    };
  }, APP);
});

after(async () => { await browser?.close(); await new Promise((r) => server?.close(r)); });

const settle = async () => { await page.evaluate(() => window.__k.sim.settle()); await page.waitForTimeout(350); };
const tableText = () => page.$eval('[data-explorer-table]', (el) => el.textContent);
const files = () => page.evaluate(() => window.__api.query('desktop_files', {}).get());
const clickRow = (label) => page.evaluate((label) => [...document.querySelectorAll('.app-explorer-row')].find((r) => r.querySelector('.app-explorer-file-name')?.textContent === label)?.click(), label);
const dblRow = (label) => page.evaluate((label) => [...document.querySelectorAll('.app-explorer-row')].find((r) => r.querySelector('.app-explorer-file-name')?.textContent === label)?.dispatchEvent(new MouseEvent('dblclick', { bubbles: true })), label);

test('mount: Files-Wurzel live aus desktop_files, Systemordner angelegt, kein Zustandstext', async () => {
  await page.evaluate(async () => {
    const { countingApi } = await import('/v4/tools/port/test-api.mjs');
    window.__c = countingApi(window.__api);
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, {});
  });
  await settle();
  const t = await tableText();
  assert.match(t, /CTOX/);
  assert.match(t, /Documents/, 'Systemordner fs_documents per api.put angelegt');
  const ids = (await files()).map((d) => d.id);
  for (const id of ['fs_desktop', 'fs_documents', 'fs_spreadsheets', 'fs_downloads']) assert.ok(ids.includes(id), id);
  assert.equal(await page.$eval('#win', (el) => el.dataset.app), 'explorer');
  assert.equal(await page.evaluate(() => window.__k.shell.window('explorer').state.get().title), 'Explorer');
  assert.equal(await page.evaluate(() => Boolean(document.querySelector('.ctox-syncing, [role="status"], .app-explorer-message, [data-explorer-status], [data-explorer-refresh]'))), false);
  assert.deepEqual(await page.evaluate(() => window.__zone()), []);
  assert.equal(await page.evaluate(() => window.__c.subscribers('desktop_files')), 1);
  assert.deepEqual(page.__errors, []);
});

test('leerer Ordner rendert nichts; Live-Änderung (sim.push) erscheint, Löschung verschwindet', async () => {
  await dblRow('Downloads');
  await settle();
  assert.equal((await tableText()).trim(), '');
  assert.equal((await page.$eval('[data-explorer-preview]', (el) => el.textContent)).trim(), '');
  await page.evaluate(() => window.__k.sim.push('desktop_files', { id: 'dl-1', parent_id: 'fs_downloads', path: '/Downloads/rechnung.pdf', name: 'rechnung.pdf', kind: 'file', mime_type: 'application/pdf', size_bytes: 10, source: 'ctox', is_deleted: false, created_at_ms: 5, updated_at_ms: 5 }));
  await settle();
  assert.match(await tableText(), /rechnung\.pdf/);
  await page.evaluate(() => window.__k.sim.push('desktop_files', { id: 'dl-1', _deleted: true }));
  await settle();
  assert.doesNotMatch(await tableText(), /rechnung\.pdf/);
});

test('Upload: api.files.put + desktop_files-Eintrag mit ref; Vorschau über api.files.get; Öffnen .txt → documents mit ref', async () => {
  await page.evaluate(async () => {
    const dt = new DataTransfer();
    dt.items.add(new File(['Hallo aus dem Upload'], 'gruss.txt', { type: 'text/plain' }));
    const ev = new DragEvent('drop', { bubbles: true, cancelable: true, dataTransfer: dt });
    document.querySelector('[data-explorer-root]').dispatchEvent(ev);
  });
  await settle();
  assert.match(await tableText(), /gruss\.txt/);
  const entry = (await files()).find((d) => d.name === 'gruss.txt');
  assert.equal(entry.parent_id, 'fs_downloads');
  assert.match(entry.content_ref, /^[0-9a-f]{64}$/);
  assert.equal(entry.content_hash_scheme, 'sha256-bytes-v1');
  const meta = await page.evaluate((id) => window.__api.get('files', id).get(), entry.id);
  assert.equal(meta.ref, entry.content_ref);
  assert.equal(meta.owner_app, 'explorer');
  assert.equal(await page.evaluate(async () => (await window.__k.sim.outbox()).length), 0, 'Upload bestätigt');
  await clickRow('gruss.txt');
  await settle();
  assert.equal(await page.$eval('[data-text-preview]', (el) => el.textContent), 'Hallo aus dem Upload');
  await page.evaluate(() => { window.__routes.length = 0; });
  await dblRow('gruss.txt');
  await settle();
  // .txt gehört laut associatedAppFor zu documents → navigate('documents', {open_file})
  const nav = await page.evaluate(() => window.__k.sim.navigations.at(-1));
  assert.equal(nav.to, 'documents');
  assert.equal(nav.params.open_file.ref, entry.content_ref);
  assert.equal(nav.params.open_file.source_file_id, entry.id);
});

test('lazy CTOX-Datei: Öffnen navigiert zum file-viewer mit file_id/path/content_state (ohne ref)', async () => {
  await page.evaluate(() => document.querySelector('[data-explorer-up]').click());
  await settle();
  await dblRow('CTOX');
  await settle();
  await page.evaluate(() => { window.__routes.length = 0; });
  await dblRow('run.log');
  await settle();
  const nav = await page.evaluate(() => window.__k.sim.navigations.at(-1));
  assert.equal(nav.to, 'file-viewer');
  assert.equal(nav.params.file_id, 'lazy-log');
  assert.equal(nav.params.content_state, 'lazy');
  assert.equal(nav.params.path, '/srv/ctox/run.log');
  assert.equal(nav.params.ref, undefined);
  assert.equal((await page.evaluate(() => window.__routes.length)), 1, 'file-viewer bekam die Route');
});

test('Ordner anlegen (api.put), Umbenennen (api.patch), Papierkorb nur mit Bestätigung über api.shell.dialog', async () => {
  await page.evaluate(() => document.querySelector('[data-explorer-new-folder]').click());
  await page.fill('.app-explorer-name-dialog input', 'Projekte');
  await page.evaluate(() => document.querySelector('.app-explorer-name-dialog form').requestSubmit());
  await settle();
  const folder = (await files()).find((d) => d.name === 'Projekte');
  assert.equal(folder.kind, 'folder');
  assert.equal(folder.parent_id, 'fs_ctox');
  assert.equal(folder.path, '/CTOX/Projekte');
  assert.match(await tableText(), /Projekte/);
  // Papierkorb: Stub-Dialog liefert default:false → nichts geändert
  await clickRow('Projekte');
  const dialogsBefore = await page.evaluate(() => window.__k.sim.dialogs.length);
  await page.evaluate(async () => {
    // Menü-Antwort 'trash' über eine Hülle (Stub-Menü liefert spec.default ?? null)
    const snap = structuredClone(window.__app.snapshot());
    await window.__app.unmount();
    const shell = Object.freeze({ ...Object.fromEntries(Object.keys(window.__c.api.shell).map((k) => [k, typeof window.__c.api.shell[k] === 'function' ? window.__c.api.shell[k].bind(window.__c.api.shell) : window.__c.api.shell[k]])), menu: (spec) => { window.__c.api.shell.menu(spec); return Promise.resolve('trash'); } });
    const base = Object.freeze({ ...Object.fromEntries(Object.keys(window.__c.api).map((k) => [k, typeof window.__c.api[k] === 'function' ? window.__c.api[k].bind(window.__c.api) : window.__c.api[k]])), shell });
    window.__menuApi = base;
    window.__app = await window.__mod.mount(document.getElementById('win'), base, { restored: snap });
  });
  await settle();
  await page.evaluate(() => document.querySelector('[data-explorer-selection-actions]').click());
  await settle();
  assert.equal((await files()).find((d) => d.name === 'Projekte').is_deleted, false, 'ohne Ja kein Papierkorb');
  assert.ok((await page.evaluate(() => window.__k.sim.dialogs.length)) >= dialogsBefore + 2, 'Menü und Bestätigung protokolliert');
  await page.evaluate(async () => {
    const { withDialogAnswer } = await import('/v4/tools/port/test-api.mjs');
    const snap = structuredClone(window.__app.snapshot());
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), withDialogAnswer(window.__menuApi, true), { restored: snap });
  });
  await settle();
  await page.evaluate(() => document.querySelector('[data-explorer-selection-actions]').click());
  await settle();
  assert.equal((await files()).find((d) => d.name === 'Projekte').is_deleted, true);
  assert.doesNotMatch(await tableText(), /Projekte/);
});

test('Business-OS-Quelle (foreign documents) live; Öffnen → navigate(moduleId, {record})', async () => {
  await page.evaluate(() => document.querySelector('[data-source-id="documents"]').click());
  await settle();
  assert.match(await tableText(), /Angebot Alpha/);
  await dblRow('Angebot Alpha');
  await settle();
  const nav = await page.evaluate(() => window.__k.sim.navigations.at(-1));
  assert.equal(nav.to, 'documents');
  assert.deepEqual(nav.params, { record: 'doc-1' });
});

// FIX8 Mangel 1: die Art-Spalte zeigte Rohwerte („Knowledge“, „Document“, „Word document“, „draft“) in deutscher Oberfläche.
test('FIX8: Art und Status deutsch (Wissen, Word-Dokument, Entwurf) statt englischer Rohwerte', async () => {
  await page.evaluate(() => window.__k.sim.push('knowledge_items', [{ id: 'kn-1', title: 'Leitfaden Onboarding', status: 'active', is_deleted: false, created_at_ms: 4, updated_at_ms: 4 }]));
  await settle();
  await page.evaluate(() => document.querySelector('[data-source-id="knowledge_items"]').click());
  await settle();
  const kn = await page.evaluate(() => [...document.querySelectorAll('.app-explorer-row')].map((r) => r.textContent.replace(/\s+/g, ' ').trim()));
  assert.ok(kn.some((t) => /Leitfaden Onboarding/.test(t) && /Wissen/.test(t) && /Aktiv/.test(t)), JSON.stringify(kn));
  assert.ok(!kn.some((t) => /Knowledge|\bactive\b/.test(t)), JSON.stringify(kn));
  await page.evaluate(() => document.querySelector('[data-source-id="documents"]').click());
  await settle();
  const docs = await tableText();
  assert.match(docs, /Word-Dokument/);
  assert.match(docs, /Entwurf/);
  assert.doesNotMatch(docs, /Word document|Document|\bdraft\b/);
  await clickRow('Angebot Alpha');
  await settle();
  const preview = await page.$eval('#win', (el) => el.textContent);
  assert.match(preview, /Word-Dokument/);
});

test('unbekannte Datei-ref beim Download → genau ein Zoneneintrag', async () => {
  await page.evaluate(() => document.querySelector('[data-source-id="desktop_files"]').click());
  await settle();
  await page.evaluate(() => window.__k.sim.push('desktop_files', { id: 'ghost', parent_id: 'fs_ctox', path: '/CTOX/ghost.bin', name: 'ghost.bin', kind: 'file', mime_type: 'application/octet-stream', size_bytes: 3, source: 'ctox', content_ref: 'f'.repeat(64), content_state: 'available', is_deleted: false, created_at_ms: 9, updated_at_ms: 9 }));
  await settle();
  await clickRow('ghost.bin');
  await page.evaluate(() => document.querySelector('[data-preview-download]').click());
  await settle();
  const zone = await page.evaluate(() => window.__zone().filter((e) => e.code === 'explorer.download'));
  assert.equal(zone.length, 1);
  assert.match(zone[0].text, /ghost\.bin/);
});

// Rundgang RUND1: der Upload-Dialog ließ sich nicht per Escape schließen.
test('Upload-Dialog: Escape schließt ihn', async () => {
  await page.evaluate(() => document.querySelector('#win [data-explorer-upload]')?.click());
  await page.waitForTimeout(150);
  const opened = await page.evaluate(() => Boolean(document.querySelector('#win .app-explorer-upload-dialog')));
  assert.ok(opened, 'Upload-Dialog offen');
  await page.keyboard.press('Escape');
  await page.waitForTimeout(150);
  assert.equal(await page.evaluate(() => Boolean(document.querySelector('#win .app-explorer-upload-dialog'))), false);
});

test('snapshot/restored: Quelle, Ordner, Suchtext und Auswahl überleben den Remount; Screenshots', async () => {
  await page.fill('[data-explorer-search]', 'run');
  await page.dispatchEvent('[data-explorer-search]', 'input');
  await settle();
  await clickRow('run.log');
  const snap = await page.evaluate(async () => {
    const cloned = structuredClone(window.__app.snapshot());
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, { restored: cloned });
    return cloned;
  });
  assert.equal(snap.query, 'run');
  assert.equal(snap.selectedId, 'lazy-log');
  assert.equal(snap.currentFolderId, 'fs_ctox');
  await settle();
  assert.equal(await page.$eval('[data-explorer-search]', (el) => el.value), 'run');
  assert.equal(await page.evaluate(() => window.__app.snapshot().selectedId), 'lazy-log');
  assert.match(await tableText(), /run\.log/);
  assert.equal(await page.evaluate(() => window.__c.subscribers('desktop_files')), 1, 'alte Abos abgemeldet');
  await page.fill('[data-explorer-search]', '');
  await page.dispatchEvent('[data-explorer-search]', 'input');
  await settle();
  await clickRow('run.log');
  await mkdir(SHOTS, { recursive: true });
  await page.screenshot({ path: join(SHOTS, 'explorer-1280.png') });
  await page.setViewportSize({ width: 420, height: 720 });
  await page.waitForTimeout(150);
  await page.screenshot({ path: join(SHOTS, 'explorer-420.png') });
  const overflow = await page.evaluate(() => document.getElementById('win').scrollWidth > window.innerWidth + 1);
  assert.equal(overflow, false, 'kein horizontaler Überlauf bei 420');
  await page.setViewportSize({ width: 1280, height: 720 });
  assert.deepEqual(page.__errors, []);
});

test('unmount meldet alle Abos ab und leert root', async () => {
  const res = await page.evaluate(async () => {
    await window.__app.unmount();
    return { subs: ['desktop_files', 'files', 'documents'].map((c) => window.__c.subscribers(c)), children: document.getElementById('win').children.length };
  });
  assert.deepEqual(res, { subs: [0, 0, 0], children: 0 });
  assert.deepEqual(page.__errors, []);
});
