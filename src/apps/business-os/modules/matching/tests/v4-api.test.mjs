// matching gegen den App-API-4.0-Kern-Stub (createStubKernel, Pin 67229d2) in echtem Chromium.
// Muster: modules/knowledge/tests/v4-api.test.mjs. Abo-Zähler über countingApi (v4/tools/port/test-api.mjs).
// Lauf: CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/4b0d0ac node --test src/apps/business-os/modules/matching/tests/v4-api.test.mjs
// Screenshots: MATCHING_SHOTS=1 zusätzlich → tests/shots/matching-{1280,420}.png
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
const SUBS = ['matching_requirements', 'matching_objects', 'matching_results'];

// Shell lädt base.css/app.css und die App-CSS aus dem Manifest; der Test bildet das nach.
const PAGE = `<!doctype html><html><head><meta charset="utf-8">
<link rel="stylesheet" href="/shared/base.css"><link rel="stylesheet" href="/app.css">
<link rel="stylesheet" href="/modules/matching/index.css">
<style>html,body{margin:0;height:100%} #win{position:relative;overflow:hidden;container:business-app-window / inline-size}</style>
</head><body><div id="win" style="width:1280px;height:820px"></div></body></html>`;

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
  page = await browser.newPage({ viewport: { width: 1280, height: 820 } });
  const errors = [];
  page.on('pageerror', (e) => errors.push(e.message));
  page.on('console', (m) => { if (m.type() === 'error') console.error('[page]', m.text()); });
  page.on('response', (r) => { if (r.status() >= 400 && !/\.css$/.test(r.url())) console.error('[page] HTTP', r.status(), r.url()); });
  page.__errors = errors;
  await page.goto(base);
  // E48 (Stub 8265bd7): Fremdlesen (business_commands, Eigentümer ctox-platform) braucht die veröffentlichte Deklaration
  // aus dem echten Manifest (v4/manifests/matching.manifest.json).
  await page.evaluate((routes) => { window.__routes = routes; }, realManifestRoutes('matching'));
  await page.evaluate(async () => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    window.__mod = await import('/modules/matching/index.js');
    const kernel = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false, actor: { role: 'admin', permissions: ['apps.source.edit'] } });
    await kernel.ready;
    for (const o of window.__routes?._fixture_owners ?? []) if (!(await kernel.sim.inspect('business_apps', o.app))?.data?.active_ver) await kernel.sim.publishAppVersion(o.app, o.ver, o.publish);
    await kernel.sim.publishAppVersion('matching', '1.0.2', { title: 'Matching', manifestExtra: window.__routes });
    await kernel.sim.push('matching_requirements', [
      { id: 'req-alpha', kind: 'requirement', title: 'Alpha Backend Engineer', status: 'active', updated_at_ms: 3,
        data: { source: { id: 'src-acme', name: 'Acme GmbH' }, requirement: { id: 'req-alpha', title: 'Alpha Backend Engineer', sourceId: 'src-acme' } } },
      { id: 'req-beta', kind: 'requirement', title: 'Beta Data Analyst', status: 'active', updated_at_ms: 2,
        data: { id: 'req-beta', title: 'Beta Data Analyst', sourceId: 'src-acme', sourceName: 'Acme GmbH' } },
    ]);
    await kernel.sim.push('matching_objects', [
      { id: 'obj-ada', kind: 'object', title: 'Ada Lovelace', status: 'active', updated_at_ms: 2,
        data: { object: { id: 'obj-ada', name: 'Ada Lovelace', taxonomy: 'Mathematikerin', skills: ['Rust', 'SQL'] } } },
      { id: 'obj-bob', kind: 'object', title: 'Bob Builder', status: 'active', updated_at_ms: 1,
        data: { id: 'obj-bob', name: 'Bob Builder', taxonomy: 'Ingenieur', skills: ['Bau'] } },
    ]);
    await kernel.sim.push('matching_results', [
      { id: 'match-alpha-ada', kind: 'match', title: 'Alpha × Ada', status: 'active', updated_at_ms: 4,
        data: { match: { id: 'match-alpha-ada', requirementId: 'req-alpha', objectId: 'obj-ada', sourceId: 'src-acme', active: true, score: 82,
          items: [{ title: 'Rust', matchScore: 0.9, priority: 'base' }] } } },
    ]);
    await kernel.sim.settle();
    window.__k = kernel;
    window.__api = kernel.appApi('matching', '1.0.2');
    window.__zone = () => kernel.statusZone.forApp('matching').get().filter((e) => e.source === 'app');
    window.__commands = () => window.__api.query('business_commands', { sort: [{ created_at: 'asc' }] }).get();
  });
});

after(async () => { await browser?.close(); await new Promise((r) => server?.close(r)); });

const settle = async () => { await page.evaluate(() => window.__k.sim.settle()); await page.waitForTimeout(350); };
const winText = () => page.$eval('#win', (el) => el.innerText);

test('mount: rendert Live-Daten, nur api.*, gescoptes Root, Titel über api.shell', async () => {
  await page.evaluate(async () => {
    const { countingApi } = await import('/v4/tools/port/test-api.mjs');
    window.__c = countingApi(window.__api);
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, {});
  });
  await settle();
  const text = await winText();
  assert.match(text, /Alpha Backend Engineer/);
  assert.match(text, /Beta Data Analyst/);
  // Auswahl einer Anforderung zeigt die gerankten Objekte aus matching_results (Handler-Hülle data.match).
  await page.click('[data-requirement-id="req-alpha"]');
  await settle();
  assert.match(await winText(), /Ada Lovelace/);
  assert.equal(await page.$eval('#win', (el) => el.dataset.app), 'matching');
  assert.equal(await page.evaluate(() => window.__k.shell.window('matching').state.get().title), 'Matching');
  assert.deepEqual(await page.evaluate((subs) => subs.map((c) => window.__c.subscribers(c)), SUBS), [1, 1, 1]);
  assert.deepEqual(page.__errors, []);
});

test('Live-Änderung vom Core (sim.push) erscheint ohne Neuladen, Löschung verschwindet', async () => {
  await page.evaluate(() => window.__k.sim.push('matching_requirements', { id: 'req-gamma', kind: 'requirement', title: 'Gamma Live Rolle', status: 'active', updated_at_ms: 9, data: { id: 'req-gamma', title: 'Gamma Live Rolle', sourceId: 'src-acme', sourceName: 'Acme GmbH' } }));
  await settle();
  assert.match(await winText(), /Gamma Live Rolle/);
  await page.evaluate(() => window.__k.sim.push('matching_requirements', { id: 'req-gamma', _deleted: true }));
  await settle();
  assert.doesNotMatch(await winText(), /Gamma Live Rolle/);
});

test('leere Collections rendern keinen Zustandstext und keine Zone', async () => {
  const empty = await page.evaluate(async () => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    const k2 = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false, instanceId: 'empty' });
    await k2.ready;
    await window.__app.unmount();
    const host = document.getElementById('win');
    const app = await window.__mod.mount(host, k2.appApi('matching', '1.0.2'), {});
    await new Promise((r) => setTimeout(r, 300));
    const lists = ['#requirementList', '#objectList', '#sourceGrid'].map((sel) => host.querySelector(sel)?.innerText.trim() || '');
    const hasStateBox = Boolean(host.querySelector('.ctox-syncing, .ctox-empty, .import-status-card, [role="status"]'));
    const zone = k2.statusZone.forApp('matching').get().map((e) => e.code);
    const stateWords = /synchronis|geladen|wird geladen|Loading|Syncing|Noch keine/i.test(host.innerText);
    await app.unmount();
    await k2.close();
    return { lists, hasStateBox, zone, stateWords };
  });
  assert.deepEqual(empty.lists, ['', '', '']);
  assert.equal(empty.hasStateBox, false);
  assert.equal(empty.stateWords, false);
  assert.deepEqual(empty.zone, []);
  await page.evaluate(async () => {
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, {});
  });
  await settle();
});

test('Schreiben: „Neue Anforderung“ geht über api.put in die Schema-Hülle und erscheint live', async () => {
  // Anlegen läuft über api.shell.dialog (Blatt im App-Fenster), keine eigene Dialogschicht im App-Inhalt.
  await page.evaluate(async () => {
    const { withDialogAnswer } = await import('/v4/tools/port/test-api.mjs');
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), withDialogAnswer(window.__c.api, 'Delta Put Rolle'), {});
  });
  await settle();
  const before = await page.evaluate(() => window.__k.sim.dialogs.length);
  await page.click('#createRequirementBtn');
  await settle();
  assert.equal(await page.$('[data-matching-create-requirement], .business-dialog-layer'), null, 'keine V2-Dialogschicht');
  const specs = await page.evaluate((n) => window.__k.sim.dialogs.slice(n).map((d) => d.spec), before);
  assert.deepEqual(specs.map((s) => [s.kind, s.title, s.text]), [['prompt', 'Neue Anforderung', 'Titel'], ['prompt', 'Neue Anforderung', 'Quelle'], ['prompt', 'Neue Anforderung', 'Beschreibung']]);
  assert.equal(specs[0].required, true);
  assert.equal(specs[2].multiline, true);
  const rows = await page.evaluate(() => window.__api.query('matching_requirements', { selector: { title: 'Delta Put Rolle', kind: 'requirement' } }).get());
  assert.equal(rows.length, 1);
  assert.equal(rows[0].kind, 'requirement');
  assert.equal(rows[0].data.title, 'Delta Put Rolle');
  assert.equal(typeof rows[0].updated_at_ms, 'number');
  assert.match(await winText(), /Delta Put Rolle/);
  const writes = await page.evaluate(() => window.__k.sim.settle().then(() => window.__api.get('matching_requirements', window.__api.query('matching_requirements', { selector: { title: 'Delta Put Rolle', kind: 'requirement' } }).get()[0].id).get()));
  assert.ok(writes, 'vom Core angenommen und lokal sichtbar');
  await page.evaluate(async () => {
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, {});
  });
  await settle();
});

test('Befehl: PDF-Import → business_commands-Dokument ohne Warten; failed → genau ein Zoneneintrag', async () => {
  await page.evaluate(() => window.__k.sim.failCommand('matching.source.parse_object', 'Parser kaputt'));
  await page.setInputFiles('#objectImportPdfInput', { name: 'cv.pdf', mimeType: 'application/pdf', buffer: Buffer.from('%PDF-1.4\n%%EOF') });
  await settle();
  const cmd = await page.evaluate(() => window.__commands().at(-1));
  assert.equal(cmd.type, 'matching.source.parse_object');
  assert.equal(cmd.payload.input.kind, 'files');
  assert.equal(cmd.payload.input.files[0].name, 'cv.pdf');
  assert.match(cmd.payload.record_id, /^object_/);
  assert.equal(cmd.status, 'failed');
  const zone = await page.evaluate(() => window.__zone());
  assert.equal(zone.length, 1);
  assert.equal(zone[0].code, 'matching.command.matching.source.parse_object');
  assert.match(zone[0].text, /Parser kaputt/);
  assert.deepEqual(page.__errors, []);
});

test('snapshot/restored: offene Eingaben und Formular-Entwurf überleben den Remount', async () => {
  await page.fill('#sourceSearch', 'Alpha');
  const snap = await page.evaluate(async () => {
    const cloned = structuredClone(window.__app.snapshot());
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, { restored: cloned });
    return cloned;
  });
  assert.equal(snap.v, 1);
  assert.equal(snap.drafts.fields.sourceSearch, 'Alpha');
  assert.equal(snap.drafts.createRequirement, null, 'kein offenes Anlegeblatt');
  assert.ok(JSON.stringify(snap).length < 1024 * 1024);
  await settle();
  const after = await page.evaluate(() => ({
    search: document.getElementById('sourceSearch').value,
  }));
  assert.equal(after.search, 'Alpha');
  assert.deepEqual(await page.evaluate((subs) => subs.map((c) => window.__c.subscribers(c)), SUBS), [1, 1, 1], 'alte Abos sind abgemeldet');
  await page.fill('#sourceSearch', '');
  await page.dispatchEvent('#sourceSearch', 'input');
  assert.deepEqual(page.__errors, []);
});

test('Screenshots 1280 und 420 (MATCHING_SHOTS=1)', { skip: !process.env.MATCHING_SHOTS }, async () => {
  const dir = join(HERE, 'shots');
  await mkdir(dir, { recursive: true });
  await settle();
  await page.screenshot({ path: join(dir, 'matching-1280.png') });
  await page.setViewportSize({ width: 420, height: 820 });
  await page.$eval('#win', (el) => { el.style.width = '420px'; });
  await settle();
  await page.screenshot({ path: join(dir, 'matching-420.png') });
  const overflow = await page.$eval('#win', (el) => el.scrollWidth - el.clientWidth);
  await page.setViewportSize({ width: 1280, height: 820 });
  await page.$eval('#win', (el) => { el.style.width = '1280px'; });
  assert.ok(overflow <= 1, `horizontaler Überlauf ${overflow}px bei 420`);
});

test('unmount meldet alle Abos ab, löst Dokument-Listener und leert root', async () => {
  const res = await page.evaluate(async (subs) => {
    await window.__app.unmount();
    await window.__k.sim.push('matching_objects', { id: 'obj-late', kind: 'object', title: 'Late', status: 'active', updated_at_ms: 99, data: { name: 'Late' } });
    await window.__k.sim.settle();
    return { subs: subs.map((c) => window.__c.subscribers(c)), children: document.getElementById('win').children.length };
  }, SUBS);
  assert.deepEqual(res.subs, [0, 0, 0]);
  assert.equal(res.children, 0);
  await page.waitForTimeout(400);
  assert.deepEqual(page.__errors, []);
});
