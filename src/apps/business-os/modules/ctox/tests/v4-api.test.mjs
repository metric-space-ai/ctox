// ctox (Crew) gegen den eingefrorenen App-API-4.0-Stub (createStubKernel, Pin 67229d2 über
// CTOX_APP_API_STUB, read-only unter /__stub/ ausgeliefert) in echtem Chromium.
// Muster: modules/knowledge/tests/v4-api.test.mjs. Eigene Hüllen nur, wo der Stub nicht abdeckt:
// countingApi (Abo-Zähler) und withDialogAnswer (Dialog-Antwort) aus v4/tools/port/test-api.mjs.
// Lauf: CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/4b0d0ac node --test src/apps/business-os/modules/ctox/tests/v4-api.test.mjs
// Screenshots (1280/420) nach modules/ctox/tests/shots/, wenn CTOX_V4_SHOTS=1.
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
const APP = 'ctox';

const PAGE = `<!doctype html><html><head><meta charset="utf-8">
<link rel="stylesheet" href="/app.css">
<link rel="stylesheet" href="/shared/base.css">
<link rel="stylesheet" href="/modules/ctox/index.css">
<style>html,body{margin:0}#win{position:relative;overflow:hidden}</style>
</head><body><div id="win" style="width:1280px;height:800px"></div></body></html>`;

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
      // Wie der Service Worker (§16.15): ctox:shared/x.js → Shell-Pfad (OFFEN-F).
      if (/\.m?js$/.test(file)) body = Buffer.from(body.toString('utf8').replace(/(['"])ctox:shared\//g, '$1/shared/'));
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
  page.on('response', (r) => { if (r.status() >= 400) console.error('[page] HTTP', r.status(), r.url()); });
  page.__errors = errors;
  await page.goto(base);
});

after(async () => { await browser?.close(); await new Promise((r) => server?.close(r)); });

const settle = async () => { await page.evaluate(() => window.__k.sim.settle()); await page.waitForTimeout(250); };
const text = (sel) => page.$eval(sel, (el) => el.textContent);
const COLLECTIONS = ['business_commands', 'ctox_queue_tasks', 'ctox_bug_reports', 'ctox_crew_members', 'communication_accounts'];

test('Kernel vorbereitet', async () => {
  // E26/E27: Befehlsrouten + Deklarationen der echten ctox-Version (v4/manifests/ctox.manifest.json);
  // ohne aktive Version lehnt der Kern App-Befehle (ctox.task.delete) beim Schreiben mit unknown_command ab.
  await page.evaluate(async (routes) => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    window.__mod = await import('/modules/ctox/index.js');
    const kernel = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false, actor: { role: 'admin', permissions: ['apps.source.edit'] } });
    await kernel.ready;
    await kernel.sim.push('business_apps', [{ id: 'ctox', title: 'Crew' }, { id: 'knowledge', title: 'Knowledge' }]);
    // FIXT-R: Eigentümer der Fremd-Collections (echte Manifeste, inkl. ctox-platform) vor der eigenen Version.
    for (const o of routes?._fixture_owners ?? []) if (!(await kernel.sim.inspect('business_apps', o.app))?.data?.active_ver) await kernel.sim.publishAppVersion(o.app, o.ver, o.publish);
    await kernel.sim.publishAppVersion('ctox', '1.1.2', { title: 'Crew', manifestExtra: routes });
    await kernel.sim.push('ctox_queue_tasks', [
      { id: 'task-alpha', title: 'Alpha Aufgabe', prompt: 'Alpha erledigen', status: 'queued', route_status: 'pending', priority: 'normal', updated_at_ms: Date.now() - 2000 },
      { id: 'task-beta', title: 'Beta Aufgabe', prompt: 'Beta erledigen', status: 'failed', route_status: 'failed', failure_class: 'terminal', updated_at_ms: Date.now() - 4000 },
    ]);
    await kernel.sim.push('ctox_crew_members', [
      { id: 'member-milo', name: 'Milo', shape: 'round', color: '#3f7ad8', archived: false, state: 'idle', soul: { voice: 'ruhig', sketch: '' }, updated_at_ms: 1 },
    ]);
    await kernel.sim.push('ctox_harness_status', [{ id: 'harness', service_running: true, paused: false, worker_capacity: 2, pending_count: 1 }]);
    await kernel.sim.settle();
    const api = kernel.appApi('ctox', '1.1.2');
    window.__k = kernel;
    window.__api = api;
    window.__zone = () => kernel.statusZone.forApp('ctox').get().filter((e) => e.source === 'app');
    window.__commands = () => api.query('business_commands', { sort: [{ created_at: 'asc' }] }).get();
  }, realManifestRoutes('ctox'));
  assert.ok(await page.evaluate(() => Boolean(window.__api && window.__mod.mount)));
});

test('mount: rendert Live-Daten, nur api.*, gescoptes Root, Titel über api.shell', async () => {
  await page.evaluate(async () => {
    const { countingApi } = await import('/v4/tools/port/test-api.mjs');
    window.__c = countingApi(window.__api);
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, {});
  });
  await settle();
  const list = await text('[data-ctox-left] [data-task-list]');
  assert.match(list, /Alpha Aufgabe/);
  assert.match(list, /Beta Aufgabe/);
  assert.equal(await page.$eval('#win', (el) => el.dataset.app), 'ctox');
  assert.equal(await page.evaluate(() => window.__k.shell.window('ctox').state.get().title), 'Crew');
  for (const c of COLLECTIONS) assert.equal(await page.evaluate((name) => window.__c.subscribers(name), c), 1, c);
  // Nichts außerhalb des App-Fensters: der body trägt nur das Fenster.
  assert.deepEqual(await page.evaluate(() => [...document.body.children].map((el) => el.id || el.tagName)), ['win']);
  assert.deepEqual(page.__errors, []);
});

test('Live-Änderung vom Core (sim.push) erscheint ohne Neuladen, Löschung verschwindet', async () => {
  await page.evaluate(() => window.__k.sim.push('ctox_queue_tasks', { id: 'task-gamma', title: 'Gamma Live', status: 'queued', route_status: 'pending', updated_at_ms: Date.now() }));
  await settle();
  assert.match(await text('[data-ctox-left] [data-task-list]'), /Gamma Live/);
  await page.evaluate(() => window.__k.sim.push('ctox_queue_tasks', { id: 'task-gamma', _deleted: true }));
  await settle();
  assert.doesNotMatch(await text('[data-ctox-left] [data-task-list]'), /Gamma Live/);
});

test('leere Collections rendern keinen Zustandstext und keine Zone', async () => {
  const empty = await page.evaluate(async (routes) => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    const k2 = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false, instanceId: 'empty' });
    await k2.ready;
    // FIXT-8265 (E48): auch der leere Kern braucht die veröffentlichte ctox-Version; ohne sie sind business_commands,
    // ctox_queue_tasks und ctox_runtime_settings (Plattform, auto-veröffentlicht) undeklariert gesperrt.
    for (const o of routes?._fixture_owners ?? []) if (!(await k2.sim.inspect('business_apps', o.app))?.data?.active_ver) await k2.sim.publishAppVersion(o.app, o.ver, o.publish);
    await k2.sim.publishAppVersion('ctox', '1.1.2', { title: 'Crew', manifestExtra: routes });
    const host = document.createElement('div');
    host.style.cssText = 'width:900px;height:600px;position:relative';
    document.body.append(host);
    const app = await window.__mod.mount(host, k2.appApi('ctox', '1.1.2'), {});
    await new Promise((r) => setTimeout(r, 300));
    const listText = host.querySelector('[data-task-list]').textContent.trim();
    const mainText = host.querySelector('[data-ctox-main] .ctox-flow-well')?.textContent.trim() || '';
    const hasStateBox = Boolean(host.querySelector('.ctox-syncing, [role="status"], [role="alert"], [data-ctox-data-state]'));
    const zone = k2.statusZone.forApp('ctox').get().map((e) => e.code);
    await app.unmount();
    host.remove();
    await k2.close();
    return { listText, mainText, hasStateBox, zone };
  }, realManifestRoutes('ctox'));
  assert.equal(empty.listText, '');
  assert.equal(empty.mainText, '');
  assert.equal(empty.hasStateBox, false);
  assert.deepEqual(empty.zone, []);
});

test('Schreiben: api.put erscheint live und wird vom Core angenommen', async () => {
  const result = await page.evaluate(async () => {
    const h = window.__api.put('ctox_queue_tasks', { id: 'task-delta', title: 'Delta Put', status: 'queued', route_status: 'pending', updated_at_ms: Date.now() });
    await h.local;
    return h.result;
  });
  assert.equal(result.status, 'accepted');
  await settle();
  assert.match(await text('[data-ctox-left] [data-task-list]'), /Delta Put/);
});

test('Befehl: Aufgabe speichern → business_commands-Dokument; failed → genau ein Zoneneintrag', async () => {
  await page.evaluate(() => window.__k.sim.failCommand('ctox.task.update', 'kaputt'));
  await page.evaluate(() => document.querySelector('[data-select-task-id="task-alpha"]').click());
  await settle();
  await page.evaluate(() => document.querySelector('[data-job-toggle]').click());
  await page.fill('[data-job-panel] [data-ctox-task-edit] input[name="title"]', 'Alpha geändert');
  await page.evaluate(() => document.querySelector('[data-job-panel] [data-ctox-task-edit] button[type="submit"]').click());
  await settle();
  const cmd = await page.evaluate(() => window.__commands().at(-1));
  assert.equal(cmd.type, 'ctox.task.update');
  assert.equal(cmd.payload.task_id, 'task-alpha');
  assert.equal(cmd.payload.title, 'Alpha geändert');
  assert.equal(cmd.payload.record_id, 'task-alpha');
  assert.equal(cmd.payload.module, 'ctox');
  assert.equal(cmd.status, 'failed');
  const zone = await page.evaluate(() => window.__zone().filter((e) => e.code.startsWith('ctox.command.')));
  assert.equal(zone.length, 1, JSON.stringify(zone));
  assert.equal(zone[0].code, 'ctox.command.ctox.task.update');
  assert.match(zone[0].text, /kaputt/);
  // Die UI hat nicht gewartet und zeigt keinen Statustext im Formular.
  assert.equal(await page.evaluate(() => document.querySelectorAll('[data-ctox-task-action-status]').length), 0);
});

test('Befehl mit Bestätigung: Löschen fragt über api.shell.dialog, ohne Ja kein Befehl', async () => {
  const before = await page.evaluate(() => window.__commands().length);
  await page.evaluate(() => document.querySelector('[data-job-panel] [data-ctox-task-delete]')?.click());
  await settle();
  const r1 = await page.evaluate(() => ({ n: window.__commands().length, dialogs: window.__k.sim.dialogs.length }));
  assert.equal(r1.n, before, 'Stub-Dialog liefert default:false → kein Befehl');
  assert.equal(r1.dialogs, 1);
  await page.evaluate(async () => {
    const { withDialogAnswer } = await import('/v4/tools/port/test-api.mjs');
    const snap = structuredClone(window.__app.snapshot());
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), withDialogAnswer(window.__c.api, true), { restored: snap });
  });
  await settle();
  await page.evaluate(() => document.querySelector('[data-job-panel] [data-ctox-task-delete]').click());
  await settle();
  const cmd = await page.evaluate(() => window.__commands().at(-1));
  assert.equal(cmd.type, 'ctox.task.delete');
  assert.equal(cmd.payload.task_id, 'task-alpha');
  assert.equal(cmd.status, 'done');
});

test('Crew-Steuerung: Pause als Befehl, Pausenzustand nur in der Zustandszone', async () => {
  await page.evaluate(() => window.__k.sim.push('ctox_harness_status', { id: 'harness', service_running: true, paused: true, pause_reason: 'Test', worker_capacity: 2 }));
  await settle();
  const zone = await page.evaluate(() => window.__zone().map((e) => e.code));
  assert.ok(zone.includes('ctox.harness.paused'), JSON.stringify(zone));
  assert.equal(await page.evaluate(() => document.querySelectorAll('.ctox-paused-note').length), 0);
  await page.evaluate(() => window.__k.sim.push('ctox_harness_status', { id: 'harness', service_running: true, paused: false, worker_capacity: 2 }));
  await settle();
  assert.ok(!(await page.evaluate(() => window.__zone().map((e) => e.code))).includes('ctox.harness.paused'));
});

// Rundgang RUND1: Klick auf ein Crew-Mitglied lief in eine Endlosrekursion (watchMemberRuns abonnierte im sofortigen
// subscribe-Rückruf erneut, RangeError „Maximum call stack size exceeded“).
test('Crew-Mitglied öffnen: Läufe-Abo genau einmal, keine Rekursion, Schublade zeigt das Mitglied', async () => {
  const consoleErrors = [];
  const onConsole = (m) => { if (m.type() === 'error') consoleErrors.push(m.text()); };
  page.on('console', onConsole);
  await page.evaluate(() => window.__k.sim.push('ctox_runs', [{ id: 'run-milo-1', crew_member_id: 'member-milo', task_id: 'task-alpha', status: 'completed', started_at_ms: 1, finished_at_ms: 2, updated_at_ms: 2 }]));
  await settle();
  const runsSubsBefore = await page.evaluate(() => window.__c.subscribers('ctox_runs'));
  const opened = await page.evaluate(() => {
    const b = document.querySelector('#win [data-crew-member-id="member-milo"], #win [data-open-crew-member="member-milo"]');
    b?.click();
    return Boolean(b);
  });
  await settle();
  page.off('console', onConsole);
  assert.ok(opened, 'Mitgliederknopf vorhanden');
  assert.deepEqual(consoleErrors.filter((t) => /Maximum call stack/.test(t)), []);
  const runsSubs = await page.evaluate(() => window.__c.subscribers('ctox_runs'));
  assert.ok(runsSubs <= runsSubsBefore + 1, `genau ein zusätzliches ctox_runs-Abo (vorher ${runsSubsBefore}, nachher ${runsSubs})`);
  assert.match(await page.evaluate(() => document.querySelector('#win .ctox-member-drawer')?.textContent || ''), /Milo/);
  assert.deepEqual(page.__errors, []);
});

test('snapshot/restored: offener Editor-Entwurf und Auswahl überleben den Remount', async () => {
  await page.evaluate(() => document.querySelector('[data-select-task-id="task-beta"]').click());
  await settle();
  const selectedBefore = await page.evaluate(() => window.__app.snapshot().selectedTaskId);
  assert.equal(selectedBefore, 'task-beta');
  await page.evaluate(() => { const b = document.querySelector('[data-job-toggle]'); if (b.getAttribute('aria-expanded') !== 'true') b.click(); });
  await page.fill('[data-job-panel] [data-ctox-task-edit] textarea[name="prompt"]', 'halb fertiger Entwurf');
  const snap = await page.evaluate(async () => {
    const cloned = structuredClone(window.__app.snapshot()); // Shell-Ablauf Entwurf §5
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, { restored: cloned });
    return cloned;
  });
  assert.equal(snap.jobEditorOpen, true);
  assert.equal(snap.taskEditDrafts['task-beta'].prompt, 'halb fertiger Entwurf');
  assert.ok(JSON.stringify(snap).length < 1024 * 1024);
  await settle();
  const after = await page.evaluate(() => ({
    prompt: document.querySelector('[data-job-panel] [data-ctox-task-edit] textarea[name="prompt"]')?.value,
    panelHidden: document.querySelector('[data-job-panel]')?.hidden,
    selected: window.__app.snapshot().selectedTaskId,
  }));
  assert.equal(after.prompt, 'halb fertiger Entwurf');
  assert.equal(after.panelHidden, false);
  assert.equal(after.selected, 'task-beta');
  assert.equal(await page.evaluate(() => window.__c.subscribers('ctox_queue_tasks')), 1, 'alte Abos sind abgemeldet');
  assert.deepEqual(page.__errors, []);
});

test('Deep-Link über api.shell.navigate/onRoute wählt die Aufgabe', async () => {
  const ok = await page.evaluate(() => window.__api.shell.navigate('ctox', { task_id: 'task-delta' }));
  assert.equal(ok, true);
  await settle();
  assert.equal(await page.evaluate(() => window.__app.snapshot().selectedTaskId), 'task-delta');
});

test('Screenshots 1280 und 420', { skip: process.env.CTOX_V4_SHOTS !== '1' }, async () => {
  const dir = join(HERE, 'shots');
  await mkdir(dir, { recursive: true });
  await page.evaluate(() => { document.getElementById('win').style.cssText = 'width:1280px;height:800px;position:relative;overflow:hidden'; });
  await settle();
  await page.locator('#win').screenshot({ path: join(dir, 'ctox-1280.png') });
  await page.setViewportSize({ width: 420, height: 800 });
  await page.evaluate(() => { document.getElementById('win').style.cssText = 'width:420px;height:800px;position:relative;overflow:hidden'; });
  await page.waitForTimeout(400);
  await page.locator('#win').screenshot({ path: join(dir, 'ctox-420.png') });
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.evaluate(() => { document.getElementById('win').style.cssText = 'width:1280px;height:800px;position:relative;overflow:hidden'; });
  await page.evaluate(() => document.querySelector('[data-select-task-id="task-beta"]').click());
  await settle();
  await page.evaluate(() => document.querySelector('[data-open-selected-task]')?.click());
  await settle();
  await page.locator('#win').screenshot({ path: join(dir, 'ctox-1280-drawer.png') });
});

test('unmount meldet alle Abos ab und leert root', async () => {
  const res = await page.evaluate(async (collections) => {
    await window.__app.unmount();
    await window.__k.sim.push('ctox_queue_tasks', { id: 'task-late', title: 'Late', status: 'queued', updated_at_ms: Date.now() });
    await window.__k.sim.settle();
    return {
      subs: [...collections, 'ctox_harness_events', 'ctox_runs'].map((c) => window.__c.subscribers(c)),
      children: document.getElementById('win').children.length,
    };
  }, COLLECTIONS);
  assert.deepEqual(res.subs, COLLECTIONS.map(() => 0).concat([0, 0]));
  assert.equal(res.children, 0);
  assert.deepEqual(page.__errors, []);
});
