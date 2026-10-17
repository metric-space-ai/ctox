// cv-print-builder gegen den verbindlichen App-API-4.0-Stub des Kerns (createStubKernel, gepinnt über CTOX_APP_API_STUB,
// read-only unter /__stub/ ausgeliefert) in echtem Chromium. Muster: modules/knowledge/tests/v4-api.test.mjs.
// Lauf: CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/4b0d0ac node --test src/apps/business-os/modules/cv-print-builder/tests/v4-api.test.mjs
// Playwright: aus PLAYWRIGHT_MODULE, sonst ~/Documents/ctox/src/apps/business-os/node_modules/playwright.
// SHOTS=1 legt zusätzlich Bildschirmfotos in 1280 und 420 nach tests/shots/.
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
const TYPES = { '.js': 'text/javascript', '.mjs': 'text/javascript', '.css': 'text/css', '.html': 'text/html', '.json': 'application/json', '.svg': 'image/svg+xml' };
const APP = 'cv-print-builder';
const PARSE = 'ctox.cv_print.apply_parse';
const LIVE = ['documents', 'document_versions', 'business_commands', 'ctox_queue_tasks'];

const PAGE = `<!doctype html><html><head><meta charset="utf-8">
<link rel="stylesheet" href="/ui-contract/v1/workjet-ui-contract.css">
<link rel="stylesheet" href="/app.css">
<link rel="stylesheet" href="/shared/base.css">
<link rel="stylesheet" href="/themes/ctox-desktop-shell.css">
<link rel="stylesheet" href="/modules/${APP}/index.css">
<style>html,body{margin:0;overflow:hidden;background:var(--bg,#f4f4f2)}#win{position:relative;width:1280px;height:760px;display:flex;flex-direction:column;overflow:hidden}#win>*{flex:1;min-height:0}</style>
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
      // Wie der Service Worker (§16.15): ctox:shared/x.js → Shell-Pfad (PORTING §4 Falle 2).
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
  // FIXT-R: echte Routen/Rechte/Fremd-Collections von cv-print-builder (v4/manifests/cv-print-builder.manifest.json).
  // window.__routes ist hier schon die Navigationsliste der ctox-App, daher window.__manifestRoutes.
  await page.evaluate((routes) => { window.__manifestRoutes = routes; }, realManifestRoutes(APP));
  await page.evaluate(async (APP) => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    window.__mod = await import(`/modules/${APP}/index.js`);
    const kernel = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false, userId: 'michael', actor: { id: 'michael', display_name: 'Michael', role: 'admin' } });
    await kernel.ready;
    await kernel.sim.push('business_apps', [{ id: APP, title: 'CV Print Builder' }, { id: 'ctox', title: 'CTOX' }]);
    // FIXT-R: Eigentümer der Fremd-Collections (ctox-platform, desktop, documents) vor der eigenen Version, diese nach
    // der business_apps-Zeile.
    for (const o of window.__manifestRoutes?._fixture_owners ?? []) if (!(await kernel.sim.inspect('business_apps', o.app))?.data?.active_ver) await kernel.sim.publishAppVersion(o.app, o.ver, o.publish);
    await kernel.sim.publishAppVersion(APP, '0.1.2', { title: 'CV Print Builder', manifestExtra: window.__manifestRoutes });
    const api = kernel.appApi(APP, '0.1.2');
    // Original-PDF von Bernd liegt als Datei in api.files (Ref = sha256).
    const pdf = new Blob(['%PDF-1.4\n1 0 obj<</Type/Catalog>>endobj\ntrailer<</Root 1 0 R>>\n%%EOF'], { type: 'application/pdf' });
    const up = api.files.put(pdf, { id: 'cv_file_cv_b', name: 'Bernd_Upload.pdf', mime: 'application/pdf' });
    await up.local;
    const ref = await up.ref;
    const now = Date.now();
    const exp = [{ job_title: 'Projektleiterin', employer: 'Muster AG', start_date: '2019', end_date: 'heute', job_description: ['Leitung von Digitalprojekten'] }];
    const edu = [{ degree: 'M.Sc. Informatik', institution: 'Uni Stuttgart', start_date: '2012', end_date: '2014', details: [] }];
    const modelA = {
      schema: 'ctox.cv_print_profile.v1',
      source: { filename: 'Anna_Beispiel.pdf', mime_type: 'application/pdf', size_bytes: 0, sha256: '' },
      candidate: { id: 'cv_a', name: 'Anna Beispiel', firstName: 'Anna', lastName: 'Beispiel', currentRole: 'Projektleiterin', location: 'Stuttgart', availability: 'sofort', languages: [{ label: 'Deutsch' }], skills: ['Scrum'],
        additional: [{ key: 'cv.experience', label: 'Berufserfahrung (CV)', value: exp }, { key: 'cv.education', label: 'Ausbildung (CV)', value: edu }, { key: 'cv.skills', label: 'Skills (CV)', value: { Fachkenntnisse: ['Scrum'], Sprachkenntnisse: ['Deutsch'] } }, { key: 'cv.meta', label: 'Stammdaten (CV)', value: {} }] },
      print: { template: 'modern', anonymize: false, showLogo: false, logoDataUrl: '', preset: 'standard', overrides: {} },
      workflow: { phase: 'review', view_mode: 'split', chat_id: '', command_id: '', task_id: '', task_status: 'completed', diagnostics: [], created_at_ms: now - 9000, updated_at_ms: now - 1000 },
    };
    const modelB = {
      schema: 'ctox.cv_print_profile.v1',
      source: { file_id: 'cv_file_cv_b', file_ref: ref, filename: 'Bernd_Upload.pdf', mime_type: 'application/pdf', size_bytes: pdf.size, sha256: ref },
      candidate: { id: 'cv_b', name: 'Bernd Upload', firstName: 'Bernd', lastName: 'Upload', additional: [] },
      print: { template: 'minimal', anonymize: false, showLogo: true, logoDataUrl: '', preset: 'standard', overrides: {} },
      workflow: { phase: 'uploaded', view_mode: 'original', chat_id: '', command_id: '', task_id: '', task_status: '', diagnostics: [], created_at_ms: now - 50000, updated_at_ms: now - 50000 },
    };
    const doc = (id, title, versionId, status, updated) => ({ id, title, filename: `${title.replace(' ', '_')}.pdf`, mime_type: 'application/vnd.ctox.cv-print-profile+json', status, document_type: 'cv_print_profile', owner_id: '', current_version_id: versionId, source_sha256: '', page_count: 0, diagnostics_count: 0, linked_records: [], display_cache: {}, index_text: title, is_deleted: false, created_at_ms: updated, updated_at_ms: updated });
    await kernel.sim.push('document_versions', [
      { id: 'cv_a_v1', document_id: 'cv_a', version: 1, source_kind: 'cv_pdf_upload', blob_id: '', model_json: modelA, diagnostics: [], created_at_ms: now, updated_at_ms: now },
      { id: 'cv_b_v1', document_id: 'cv_b', version: 1, source_kind: 'cv_pdf_upload', blob_id: ref, model_json: modelB, diagnostics: [], created_at_ms: now, updated_at_ms: now },
    ]);
    await kernel.sim.push('documents', [doc('cv_a', 'Anna Beispiel', 'cv_a_v1', 'review', now - 1000), doc('cv_b', 'Bernd Upload', 'cv_b_v1', 'uploaded', now - 50000)]);
    // Fremdes Dokument (anderer document_type) erscheint nicht.
    await kernel.sim.push('documents', [{ ...doc('memo_1', 'Memo', 'memo_v1', 'draft', now), document_type: 'memo' }]);
    await kernel.sim.settle();
    window.__k = kernel;
    window.__api = api;
    window.__ref = ref;
    window.__zone = () => kernel.status.forApp(APP).get().filter((e) => e.source === 'app');
    window.__commands = () => api.query('business_commands', { sort: [{ created_at: 'asc' }] }).get();
    window.__routes = [];
    kernel.appApi('ctox', '1').shell.onRoute((ev) => window.__routes.push(ev.params));
  }, APP);
});

after(async () => { await browser?.close(); await new Promise((r) => server?.close(r)); });

const settle = async () => { await page.evaluate(() => window.__k.sim.settle()); await page.waitForTimeout(220); };
const listText = () => page.$eval('[data-cv-list]', (el) => el.textContent.replace(/\s+/g, ' ').trim());
const select = async (id) => { await page.evaluate((id) => document.querySelector(`[data-cv-select="${id}"]`).click(), id); await settle(); };

async function shot(name, width) {
  if (!process.env.SHOTS) return;
  await mkdir(join(HERE, 'shots'), { recursive: true });
  await page.setViewportSize({ width, height: 760 });
  await page.$eval('#win', (el, w) => { el.style.width = `${w}px`; }, width);
  await page.waitForTimeout(300);
  await page.screenshot({ path: join(HERE, 'shots', `${name}-${width}.png`) });
  await page.setViewportSize({ width: 1280, height: 760 });
  await page.$eval('#win', (el) => { el.style.width = '1280px'; });
}

test('mount: Profile aus documents/document_versions live, Auswahl auf der Bühne, gescoptes Root, Titel über api.shell', async () => {
  await page.evaluate(async () => {
    const { countingApi } = await import('/v4/tools/port/test-api.mjs');
    window.__c = countingApi(window.__api);
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, {});
  });
  await settle();
  const list = await listText();
  assert.match(list, /Anna Beispiel/);
  assert.match(list, /Bernd Upload/);
  assert.doesNotMatch(list, /Memo/);
  assert.equal(await page.$eval('#win', (el) => el.dataset.app), 'cv-print-builder');
  assert.equal(await page.evaluate(() => window.__k.shell.window('cv-print-builder').state.get().title), 'CV Print Builder');
  assert.deepEqual(await page.evaluate((l) => l.map((c) => window.__c.subscribers(c)), LIVE), [1, 1, 1, 1]);
  const stage = await page.evaluate(() => ({
    selected: window.__app.snapshot().selectedId,
    title: document.querySelector('[data-cv-stage-title]').textContent,
    sheet: Boolean(document.querySelector('.cv-print-sheet')),
    editor: Boolean(document.querySelector('[data-cv-field-editor]')),
    counts: document.querySelector('[data-pg-count="all"]').textContent.trim(),
  }));
  assert.equal(stage.selected, 'cv_a', 'jüngstes Profil ist ausgewählt');
  assert.equal(stage.title, 'Anna Beispiel');
  assert.equal(stage.sheet, true);
  assert.equal(stage.editor, true, 'Korrekturphase zeigt den Feld-Editor');
  assert.equal(stage.counts, '(2)');
  assert.deepEqual(await page.evaluate(() => window.__zone().map((e) => e.code)), []);
  assert.deepEqual(page.__errors, []);
  await shot('cv-print-builder', 1280);
  await shot('cv-print-builder', 420);
});

test('Original-PDF kommt über api.files.get(ref) als Blob – kein Platzhaltertext, keine Zone', async () => {
  await select('cv_b');
  await page.waitForFunction(() => document.querySelector('[data-cv-original-body] iframe'));
  const res = await page.evaluate(() => ({
    src: document.querySelector('[data-cv-original-body] iframe').getAttribute('src'),
    text: document.querySelector('[data-cv-original-body]').textContent.trim(),
    kicker: document.querySelector('[data-cv-stage-kicker]').textContent,
  }));
  assert.match(res.src, /^blob:/);
  assert.equal(res.text, '');
  assert.equal(res.kicker, 'PDF geladen');
  assert.deepEqual(await page.evaluate(() => window.__zone().map((e) => e.code)), []);
});

test('Live-Änderung vom Core (sim.push) erscheint ohne Neuladen, Löschung verschwindet', async () => {
  await page.evaluate(async () => {
    const now = Date.now();
    await window.__k.sim.push('document_versions', { id: 'cv_c_v1', document_id: 'cv_c', version: 1, source_kind: 'cv_profile_import', blob_id: '', model_json: { schema: 'ctox.cv_print_profile.v1', source: {}, candidate: { name: 'Carla Live', additional: [] }, print: { template: 'classic' }, workflow: { phase: 'approved' } }, diagnostics: [], created_at_ms: now, updated_at_ms: now });
    await window.__k.sim.push('documents', { id: 'cv_c', title: 'Carla Live', document_type: 'cv_print_profile', current_version_id: 'cv_c_v1', status: 'approved', is_deleted: false, created_at_ms: now, updated_at_ms: 1 });
  });
  await settle();
  assert.match(await listText(), /Carla Live/);
  assert.equal(await page.$eval('[data-pg-count="approved"]', (el) => el.textContent.trim()), '(1)');
  await page.evaluate(() => window.__k.sim.push('documents', { id: 'cv_c', _deleted: true }));
  await settle();
  assert.doesNotMatch(await listText(), /Carla Live/);
  assert.equal(await page.$eval('[data-pg-count="approved"]', (el) => el.textContent.trim()), '(0)');
});

test('leere Collections rendern keinen Zustandstext und keine Zone', async () => {
  const empty = await page.evaluate(async () => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    const k2 = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false, instanceId: 'empty' });
    await k2.ready;
    // E48: auch der leere Kern braucht die veröffentlichte Version mit foreign_collections (business_commands, ctox_queue_tasks).
    for (const o of window.__manifestRoutes?._fixture_owners ?? []) if (!(await k2.sim.inspect('business_apps', o.app))?.data?.active_ver) await k2.sim.publishAppVersion(o.app, o.ver, o.publish);
    await k2.sim.publishAppVersion('cv-print-builder', '0.1.2', { title: 'CV Print Builder', manifestExtra: window.__manifestRoutes });
    const host = document.createElement('div');
    host.style.cssText = 'width:900px;height:600px';
    document.body.append(host);
    const app = await window.__mod.mount(host, k2.appApi('cv-print-builder', '0.1.2'), {});
    await new Promise((r) => setTimeout(r, 300));
    const texts = ['[data-cv-list]', '[data-cv-stage]', '[data-cv-stage-controls]'].map((sel) => host.querySelector(sel)?.textContent.trim() || '');
    const hasStateBox = Boolean(host.querySelector('.ctox-syncing, [role="status"], .ctox-empty'));
    const headHidden = host.querySelector('[data-cv-stage-head]').hidden;
    const zone = k2.status.forApp('cv-print-builder').get().map((e) => e.code);
    await app.unmount();
    host.remove();
    await k2.close();
    return { texts, hasStateBox, headHidden, zone };
  });
  for (const value of empty.texts) assert.equal(value, '');
  assert.equal(empty.hasStateBox, false);
  assert.equal(empty.headHidden, true);
  assert.deepEqual(empty.zone, []);
});

test('Befehl: Parsing starten → ctox.cv_print.apply_parse sofort lokal, nie warten; failed → genau ein Zoneneintrag', async () => {
  await page.evaluate((t) => window.__k.sim.failCommand(t, 'pdftotext fehlt'), PARSE);
  await select('cv_b');
  const before = await page.evaluate(() => window.__commands().length);
  const immediate = await page.evaluate(() => {
    document.querySelector('[data-cv-action]').click();
    return window.__commands().length;
  });
  assert.equal(immediate, before + 1, 'Befehlsdokument liegt ohne Warten lokal');
  await settle();
  const cmd = await page.evaluate(() => window.__commands().at(-1));
  assert.equal(cmd.type, 'ctox.cv_print.apply_parse');
  assert.equal(cmd.payload.record_id, 'cv_b');
  assert.equal(cmd.record_id, 'cv_b', 'record_id als Befehls-Option am Dokument');
  assert.equal(cmd.payload.document_id, 'cv_b');
  assert.equal(cmd.payload.version_id, 'cv_b_v1');
  assert.equal(cmd.payload.source_file_ref, await page.evaluate(() => window.__ref));
  assert.equal(cmd.payload.source_file.kind, 'file');
  assert.equal(cmd.payload.skill, undefined, 'Skill wählt der Handler (cv-print-builder/parser), nicht die UI');
  assert.equal(cmd.payload.source_file_id, undefined, 'keine desktop_files-ID für neue Profile');
  assert.match(cmd.payload.instruction, /Keine Tools, keine Shell/);
  assert.equal(cmd.status, 'failed');
  const version = await page.evaluate(() => window.__api.get('document_versions', 'cv_b_v1').get().model_json.workflow);
  assert.equal(version.command_id, cmd.id);
  assert.equal(version.phase, 'parsing');
  const zone = await page.evaluate(() => window.__zone());
  assert.equal(zone.length, 1);
  assert.equal(zone[0].code, 'cv-print-builder.command.ctox.cv_print.apply_parse');
  assert.match(zone[0].text, /pdftotext fehlt/);
  assert.equal(zone[0].ref, cmd.id);
  // Die Live-Projektion des fehlgeschlagenen Befehls setzt die Phase auf „Fehler“ (wie v3).
  assert.equal(await page.$eval('[data-cv-stage-kicker]', (el) => el.textContent), 'Fehler');
  // Kein Chat-Dokument, keine Datei-Dokumente in fremden Collections. business_chats deklariert das Manifest nicht:
  // der Kern verweigert Lesen (forbidden), und kein Schreibversuch liegt in Outbox oder Ablehnungsarchiv.
  assert.deepEqual(await page.evaluate(async () => {
    let chatsRead = 'readable';
    try { window.__api.query('business_chats', {}).get(); } catch (e) { chatsRead = e.code; }
    const writes = [...await window.__k.sim.outbox(), ...await window.__k.sim.archiveLocal()].filter((e) => e.collection === 'business_chats').length;
    return { chatsRead, writes, files: ['desktop_files', 'desktop_file_chunks'].map((c) => window.__api.query(c, {}).get().length) };
  }), { chatsRead: 'forbidden', writes: 0, files: [0, 0] });
});

test('Befehl done: Ergebnis {task_id} erscheint am Profil, Zone geräumt, „Flow öffnen“ navigiert in die CTOX-App', async () => {
  await page.evaluate(() => document.querySelector('[data-cv-action]').click());
  await settle();
  // ctox.cv_print.apply_parse ist ein Plattformbefehl: seit f967baf führt der simulierte Core ihn selbst aus (Ergebnis
  // {ok:true}, sim.onCommand greift für Plattformtypen nicht). Das Ergebnis {task_id} meldet der Core wie im Betrieb
  // über den gepushten Befehlsstatus.
  let cmd = await page.evaluate(() => window.__commands().at(-1));
  assert.equal(cmd.type, PARSE);
  await page.evaluate((cmd) => window.__k.sim.push('business_commands', { ...cmd, status: 'done', progress: 1, result: { task_id: 'queue::task-42' } }), cmd);
  await settle();
  cmd = await page.evaluate(() => window.__commands().at(-1));
  assert.equal(cmd.status, 'done');
  assert.deepEqual(await page.evaluate(() => window.__zone().map((e) => e.code)), []);
  const link = await page.$('[data-cv-open-task]');
  assert.ok(link, 'Flow-Link sichtbar');
  assert.equal(await link.getAttribute('data-cv-task-id'), 'queue::task-42');
  await link.click();
  await settle();
  assert.deepEqual(await page.evaluate(() => window.__routes.at(-1)), { task_id: 'queue::task-42', command_id: cmd.id, source_module: 'cv-print-builder' });
});

test('Schreiben: Feld im Korrektur-Editor → api.patch document_versions + documents, vom Core angenommen; Freigabe und Druck im App-Fenster', async () => {
  await select('cv_a');
  await page.fill('[data-cv-field-path="candidate.currentRole"]', 'Programmleiterin');
  await page.$eval('[data-cv-field-path="candidate.currentRole"]', (el) => el.blur());
  await settle();
  const saved = await page.evaluate(() => ({
    role: window.__api.get('document_versions', 'cv_a_v1').get().model_json.candidate.currentRole,
    index: window.__api.get('documents', 'cv_a').get().index_text,
  }));
  assert.equal(saved.role, 'Programmleiterin');
  assert.match(saved.index, /Programmleiterin/);
  const result = await page.evaluate(async () => {
    const h = window.__api.patch('documents', 'cv_a', { updated_at_ms: Date.now() });
    await h.local;
    return h.result;
  });
  assert.equal(result.status, 'accepted');
  // Freigeben → Phase approved, danach Drucken = verstecktes iframe in root mit nur dem Druckbogen.
  await page.evaluate(() => document.querySelector('[data-cv-action]').click());
  await settle();
  assert.equal(await page.evaluate(() => window.__api.get('document_versions', 'cv_a_v1').get().model_json.workflow.phase), 'approved');
  await page.evaluate(() => document.querySelector('[data-cv-action]').click());
  const frame = await page.evaluate(() => {
    const f = document.querySelector('#win iframe[aria-hidden="true"]');
    return f ? { inRoot: document.getElementById('win').contains(f), sheet: Boolean(f.contentDocument.querySelector('[data-app="cv-print-builder"] .cv-print-frame .cv-print-sheet')) } : null;
  });
  assert.deepEqual(frame, { inRoot: true, sheet: true });
  assert.deepEqual(page.__errors, []);
});

test('Dialog statt window.confirm: „Alle reparsen“ ohne Antwort → kein Befehl, Dialog im Fenster protokolliert', async () => {
  const before = await page.evaluate(() => ({ n: window.__commands().length, d: window.__k.sim.dialogs.length }));
  await page.evaluate(() => document.querySelector('[data-cv-reparse-all]').click());
  await settle();
  const after = await page.evaluate(() => ({ n: window.__commands().length, d: window.__k.sim.dialogs.length, last: window.__k.sim.dialogs.at(-1) }));
  assert.equal(after.n, before.n);
  assert.equal(after.d, before.d + 1);
  assert.equal(after.last.app_id, 'cv-print-builder');
  assert.equal(after.last.spec.kind, 'confirm');
  assert.equal(after.last.spec.default, false);
});

test('snapshot/restored: offene Editor-Eingabe, Auswahl, Suche und Band überleben den Remount', async () => {
  // Zurück in die Korrekturphase, damit der Editor steht.
  await page.evaluate(async () => {
    const v = window.__api.get('document_versions', 'cv_a_v1').get();
    await window.__api.patch('document_versions', 'cv_a_v1', { model_json: { ...v.model_json, workflow: { ...v.model_json.workflow, phase: 'review' } } }).local;
  });
  await settle();
  await select('cv_a');
  await page.fill('[data-pg-search]', 'anna');
  await page.evaluate(() => document.querySelector('[data-pg-band="review"]').click());
  await settle();
  // Offene Eingabe ohne blur (noch nicht gespeichert); ein Live-Neuzeichnen dazwischen darf sie nicht verlieren.
  await page.evaluate(() => {
    const input = document.querySelector('[data-cv-field-path="candidate.location"]');
    input.value = 'Halb getippt';
    input.dispatchEvent(new Event('input', { bubbles: true }));
  });
  await page.evaluate(() => window.__k.sim.push('ctox_queue_tasks', { id: 'task-noise', status: 'queued', updated_at_ms: Date.now() }));
  await settle();
  assert.equal(await page.$eval('[data-cv-field-path="candidate.location"]', (el) => el.value), 'Halb getippt');
  const snap = await page.evaluate(async () => {
    const cloned = structuredClone(window.__app.snapshot()); // Shell-Ablauf Entwurf §5
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, { restored: cloned });
    return cloned;
  });
  assert.equal(snap.selectedId, 'cv_a');
  assert.equal(snap.grammar.search, 'anna');
  assert.equal(snap.grammar.band, 'review');
  assert.equal(snap.editorDraft.fields['f:candidate.location'], 'Halb getippt');
  assert.ok(JSON.stringify(snap).length < 1024 * 1024);
  await settle();
  const after = await page.evaluate(() => ({
    location: document.querySelector('[data-cv-field-path="candidate.location"]')?.value,
    selected: window.__app.snapshot().selectedId,
    search: document.querySelector('[data-pg-search]').value,
    band: document.querySelector('[data-pg-band][aria-selected="true"]').dataset.pgBand,
    list: document.querySelector('[data-cv-list]').textContent,
    stored: window.__api.get('document_versions', 'cv_a_v1').get().model_json.candidate.location,
  }));
  assert.equal(after.location, 'Halb getippt');
  assert.equal(after.selected, 'cv_a');
  assert.equal(after.search, 'anna');
  assert.equal(after.band, 'review');
  assert.match(after.list, /Anna Beispiel/);
  assert.doesNotMatch(after.list, /Bernd Upload/);
  assert.equal(after.stored, 'Stuttgart', 'die offene Eingabe ist noch nicht gespeichert');
  assert.deepEqual(await page.evaluate((l) => l.map((c) => window.__c.subscribers(c)), LIVE), [1, 1, 1, 1]);
});

test('unmount meldet alle Abos ab und leert root', async () => {
  const res = await page.evaluate(async (l) => {
    await window.__app.unmount();
    await window.__k.sim.push('documents', { id: 'cv_late', title: 'Spät', document_type: 'cv_print_profile', current_version_id: 'x', updated_at_ms: 10 });
    await window.__k.sim.settle();
    return {
      subs: l.map((c) => window.__c.subscribers(c)),
      children: document.getElementById('win').children.length,
    };
  }, LIVE);
  assert.deepEqual(res.subs, [0, 0, 0, 0]);
  assert.equal(res.children, 0);
  assert.deepEqual(page.__errors, []);
});
