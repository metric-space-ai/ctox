// shiftflow gegen den App-API-4.0-Kern-Stub (createStubKernel, read-only unter /__stub/) in echtem Chromium.
// Stub: CTOX_APP_API_STUB (Pflicht-Pin ~/.local/state/shell-v4/stub-pins/68b83a3), sonst ~/.local/state/ctox-sync-v4/stub.
// Lauf: CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/68b83a3 node --test src/apps/business-os/modules/shiftflow/tests/v4-api.test.mjs
// Screenshots (1280/420) nach tests/shots/ mit SHIFTFLOW_SHOTS=1.
// shiftflow hat keine Befehlstypen (V4-PORT.md): der Befehlsfall entfällt; Meldungen gehen in die Zone.
import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { dirname, join, normalize, extname } from 'node:path';
import { homedir } from 'node:os';

const STUB = process.env.CTOX_APP_API_STUB || join(homedir(), '.local/state/ctox-sync-v4/stub');
const HERE = dirname(fileURLToPath(import.meta.url));
const BOS = normalize(join(HERE, '../../..'));
const require = createRequire(import.meta.url);
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || join(homedir(), 'Documents/ctox/src/apps/business-os/node_modules/playwright'));
const TYPES = { '.js': 'text/javascript', '.mjs': 'text/javascript', '.css': 'text/css', '.html': 'text/html', '.json': 'application/json', '.svg': 'image/svg+xml' };
const APP = 'shiftflow';

const PAGE = `<!doctype html><html><head><meta charset="utf-8">
<link rel="stylesheet" href="/ui-contract/v1/workjet-ui-contract.css"><link rel="stylesheet" href="/app.css"><link rel="stylesheet" href="/shared/base.css">
<link rel="stylesheet" href="/modules/${APP}/index.css">
<style>html,body{margin:0;height:100%;background:var(--bg,#f6f6f4)}#win{width:1280px;height:760px;position:relative;overflow:clip;contain:layout paint;container-type:inline-size;container-name:business-app-window}</style>
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
      // Wie der Service Worker (§16.15): ctox:shared/x.js → Shell-Pfad.
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
  page.__errors = errors;
  await page.goto(base);
  await page.evaluate(async (APP) => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    window.__mod = await import(`/modules/${APP}/index.js`);
    const kernel = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false });
    await kernel.ready;
    const now = new Date();
    const day = now.getDay();
    const monday = new Date(now.getFullYear(), now.getMonth(), now.getDate() - day + (day === 0 ? -6 : 1));
    const at = (d, h) => monday.getTime() + d * 86400000 + h * 3600000;
    const ts = Date.now();
    await kernel.sim.push('planning_projects', [
      { id: 'proj_a', kind: 'project', name: 'Messe Aufbau', client: 'Messe GmbH', location: 'Halle 1', hourly_rate: 80, color: '#06b6d4', status: 'active', created_at_ms: ts, updated_at_ms: ts },
    ]);
    await kernel.sim.push('planning_employees', [
      { id: 'emp_a', kind: 'employee', name: 'Anna Alpha', email: 'a@x', role: 'Service', weekly_target_hours: 40, status: 'active', avatar_color: 'hsl(200,80%,45%)', internal_hourly_rate: 20, departments: ['Service'], skills: [], created_at_ms: ts, updated_at_ms: ts },
    ]);
    await kernel.sim.push('planning_shifts', [
      { id: 'shift_a', kind: 'shift', employee_id: 'emp_a', project_id: 'proj_a', title: 'Frühdienst Alpha', start_time: at(1, 8), end_time: at(1, 16), location: 'Halle 1', department: 'Service', status: 'draft', notes: '', created_at_ms: ts, updated_at_ms: ts },
    ]);
    await kernel.sim.settle();
    const api = kernel.appApi(APP, '1');
    window.__k = kernel;
    window.__api = api;
    window.__at = at;
    window.__zone = () => kernel.statusZone.forApp(APP).get().filter((e) => e.source === 'app');
    window.__rows = (c) => api.query(c, {}).get();
  }, APP);
});

after(async () => { await browser?.close(); await new Promise((r) => server?.close(r)); });

const settle = async () => { await page.evaluate(() => window.__k.sim.settle()); await page.waitForTimeout(250); };
const listText = () => page.$eval('[data-shift-list]', (el) => el.textContent);

test('mount: rendert Live-Daten, nur api.*, gescoptes Root, Titel über api.shell', async () => {
  await page.evaluate(async () => {
    const { countingApi } = await import('/v4/tools/port/test-api.mjs');
    window.__c = countingApi(window.__api);
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, {});
  });
  await settle();
  assert.match(await listText(), /Frühdienst Alpha/);
  assert.match(await page.$eval('#schedulerGridBody', (el) => el.textContent), /Anna Alpha/);
  assert.equal(await page.$eval('#win', (el) => el.dataset.app), 'shiftflow');
  assert.equal(await page.evaluate(() => window.__k.shell.window('shiftflow').state.get().title), 'Einsatzplanung');
  assert.deepEqual(await page.evaluate(() => ['planning_employees', 'planning_projects', 'planning_shifts', 'planning_time_records'].map((c) => window.__c.subscribers(c))), [1, 1, 1, 1]);
  // Daten vorhanden → kein Seeding
  assert.equal(await page.evaluate(() => window.__rows('planning_employees').length), 1);
  assert.deepEqual(page.__errors, []);
});

test('Live-Änderung vom Core (sim.push) erscheint ohne Neuladen, Löschung verschwindet', async () => {
  await page.evaluate(() => window.__k.sim.push('planning_shifts', { id: 'shift_live', kind: 'shift', employee_id: 'emp_a', project_id: 'proj_a', title: 'Spätdienst Live', start_time: window.__at(2, 14), end_time: window.__at(2, 20), status: 'draft', created_at_ms: 1, updated_at_ms: 1 }));
  await settle();
  assert.match(await listText(), /Spätdienst Live/);
  await page.evaluate(() => window.__k.sim.push('planning_shifts', { id: 'shift_live', _deleted: true }));
  await settle();
  assert.doesNotMatch(await listText(), /Spätdienst Live/);
});

test('leere Collections (Erstabgleich läuft): kein Text, keine Zone, kein Seeding', async () => {
  const res = await page.evaluate(async () => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    const k2 = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false, instanceId: 'empty' });
    await k2.ready;
    k2.sim.setBacklog(3);
    const host = document.createElement('div');
    host.style.cssText = 'width:900px;height:600px';
    document.body.append(host);
    const app = await window.__mod.mount(host, k2.appApi('shiftflow', '1'), {});
    await new Promise((r) => setTimeout(r, 300));
    const texts = ['[data-shift-list]', '#conflictsList', '#timesheetsList', '#billingAggregationBody'].map((sel) => host.querySelector(sel)?.textContent.trim() ?? null);
    const hasStateBox = Boolean(host.querySelector('.ctox-syncing, .ctox-empty, [role="status"]'));
    const zone = k2.statusZone.forApp('shiftflow').get().map((e) => e.code);
    const seeded = k2.appApi('shiftflow', '1').query('planning_employees', {}).get().length;
    await app.unmount();
    host.remove();
    await k2.close();
    return { texts, hasStateBox, zone, seeded };
  });
  assert.deepEqual(res.texts, ['', '', '', '']);
  assert.equal(res.hasStateBox, false);
  assert.deepEqual(res.zone, []);
  assert.equal(res.seeded, 0);
  // Modulzustand ist modulweit (V4-PORT.md OFFEN): Hauptfenster neu montieren.
  await page.evaluate(async () => {
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, {});
  });
  await settle();
});

test('Schreiben: neue Schicht über den Drawer im App-root → api.put, live in der Liste', async () => {
  await page.click('#addShiftBtn');
  await settle();
  const inRoot = await page.evaluate(() => Boolean(document.querySelector('#win [data-shiftflow-drawer] form[data-shiftflow-form="shift"]')));
  assert.equal(inRoot, true, 'Drawer hängt im App-root');
  // Rundgang RUND2: der Absendeknopf heißt nach der Handlung, nicht nach dem Reiter („Dienstplan“).
  const submitLabel = await page.evaluate(() => document.querySelector('#win [data-shiftflow-drawer] form[data-shiftflow-form="shift"] button[type="submit"]')?.textContent.trim());
  assert.equal(submitLabel, 'Einplanen');
  await page.fill('#win [data-shiftflow-drawer] textarea[name="notes"]', 'Neue Testschicht');
  await page.evaluate(() => document.querySelector('#win [data-shiftflow-drawer] form').requestSubmit());
  await settle();
  const shifts = await page.evaluate(() => window.__rows('planning_shifts'));
  const created = shifts.find((s) => s.notes === 'Neue Testschicht');
  assert.ok(created, 'Schicht liegt in planning_shifts');
  assert.equal(await page.evaluate(() => Boolean(document.querySelector('#win [data-shiftflow-drawer]'))), false);
  const result = await page.evaluate(async () => {
    const h = window.__api.patch('planning_shifts', 'shift_a', { title: 'Frühdienst Patch', updated_at_ms: Date.now() });
    await h.local;
    return h.result;
  });
  assert.equal(result.status, 'accepted');
  await settle();
  assert.match(await listText(), /Frühdienst Patch/);
});

test('Rundgang RUND2: Abrechnung bei 420 px – „Filtern“ bleibt im Pane', async () => {
  await page.evaluate(() => { document.getElementById('win').style.width = '420px'; });
  await page.click('#win [data-main-view="billing"]');
  await settle();
  const r = await page.evaluate(() => {
    const b = document.querySelector('#win #billingFilterApplyBtn').getBoundingClientRect();
    const p = document.querySelector('#win #billingFilterApplyBtn').closest('.ctox-pane').getBoundingClientRect();
    return { btnRight: Math.round(b.right), paneRight: Math.round(p.right), w: Math.round(b.width) };
  });
  await page.click('#win [data-main-view="scheduler"]');
  await page.evaluate(() => { document.getElementById('win').style.width = '1280px'; });
  await settle();
  assert.ok(r.w > 40 && r.btnRight <= r.paneRight, JSON.stringify(r));
});

test('Veröffentlichen: Bestätigung über api.shell.dialog, Status nur in der Zone', async () => {
  await page.click('#btnPublishSchedule');
  await settle();
  const r1 = await page.evaluate(() => ({ drafts: window.__rows('planning_shifts').filter((s) => s.status !== 'published').length, dialogs: window.__k.sim.dialogs.length }));
  assert.ok(r1.drafts >= 1, 'ohne Ja bleibt alles Entwurf');
  assert.ok(r1.dialogs >= 1);
  await page.evaluate(async () => {
    const { withDialogAnswer } = await import('/v4/tools/port/test-api.mjs');
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), withDialogAnswer(window.__c.api, true), {});
  });
  await settle();
  await page.click('#btnPublishSchedule');
  await settle();
  const r2 = await page.evaluate(() => ({ drafts: window.__rows('planning_shifts').filter((s) => s.status !== 'published').length, zone: window.__zone() }));
  assert.equal(r2.drafts, 0);
  const pub = r2.zone.filter((e) => e.code === 'shiftflow.publish');
  assert.equal(pub.length, 1);
  assert.match(pub[0].text, /veröffentlicht/);
  assert.equal(await page.evaluate(() => Boolean(document.querySelector('#publishScheduleStatus'))), false);
});

test('snapshot/restored: offenes Drawer-Formular, Ansicht und Auswahl überleben den Remount', async () => {
  await page.evaluate(() => document.querySelector('[data-select-shift-id="shift_a"]')?.click());
  await settle();
  await page.evaluate(() => document.querySelector('#viewConflictsTabBtn')?.click());
  await settle();
  await page.evaluate(() => document.querySelector('[data-edit-shift-id="shift_a"]')?.click());
  await settle();
  await page.fill('#win [data-shiftflow-drawer] textarea[name="notes"]', 'halb fertige Notiz');
  const snap = await page.evaluate(async () => {
    const cloned = structuredClone(window.__app.snapshot());
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, { restored: cloned });
    return cloned;
  });
  assert.equal(snap.drawer.kind, 'shift');
  assert.equal(snap.drawer.recordId, 'shift_a');
  assert.equal(snap.drawer.values.notes, 'halb fertige Notiz');
  assert.ok(JSON.stringify(snap).length < 1024 * 1024);
  await settle();
  const after = await page.evaluate(() => ({
    notes: document.querySelector('#win [data-shiftflow-drawer] textarea[name="notes"]')?.value,
    snap: window.__app.snapshot(),
  }));
  assert.equal(after.notes, 'halb fertige Notiz');
  assert.equal(after.snap.selectedShiftId, 'shift_a');
  assert.equal(after.snap.currentView, snap.currentView);
  assert.deepEqual(await page.evaluate(() => ['planning_employees', 'planning_projects', 'planning_shifts', 'planning_time_records'].map((c) => window.__c.subscribers(c))), [1, 1, 1, 1], 'alte Abos sind abgemeldet');
  await page.evaluate(() => document.querySelector('#win [data-shiftflow-drawer] [data-drawer-cancel]')?.click());
  await page.evaluate(() => document.querySelector('#viewSchedulerTabBtn')?.click());
  await settle();
  assert.deepEqual(page.__errors, []);
});

test('Screenshots 1280 und 420', { skip: !process.env.SHIFTFLOW_SHOTS && !process.env.V4_SHOTS }, async () => {
  const shots = join(HERE, 'shots');
  await page.screenshot({ path: join(shots, `${APP}-1280.png`) });
  await page.evaluate(() => { const w = document.getElementById('win'); w.style.width = '420px'; });
  await page.setViewportSize({ width: 420, height: 760 });
  await page.waitForTimeout(300);
  await page.screenshot({ path: join(shots, `${APP}-420.png`) });
  await page.setViewportSize({ width: 1280, height: 760 });
  await page.evaluate(() => { const w = document.getElementById('win'); w.style.width = '1280px'; });
});

test('unmount meldet alle Abos ab und leert root', async () => {
  const res = await page.evaluate(async () => {
    await window.__app.unmount();
    await window.__k.sim.push('planning_shifts', { id: 'late', kind: 'shift', start_time: 1, end_time: 2, status: 'draft', created_at_ms: 1, updated_at_ms: 1 });
    await window.__k.sim.settle();
    return {
      subs: ['planning_employees', 'planning_projects', 'planning_shifts', 'planning_time_records'].map((c) => window.__c.subscribers(c)),
      children: document.getElementById('win').children.length,
    };
  });
  assert.deepEqual(res.subs, [0, 0, 0, 0]);
  assert.equal(res.children, 0);
  assert.deepEqual(page.__errors, []);
});
