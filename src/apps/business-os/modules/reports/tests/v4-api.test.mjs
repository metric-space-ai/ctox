// reports gegen den eingefrorenen App-API-4.0-Kern-Stub (createStubKernel) in echtem Chromium.
// Muster: modules/knowledge/tests/v4-api.test.mjs. Stub-Pfad aus CTOX_APP_API_STUB (Pflicht-Pin
// ~/.local/state/shell-v4/stub-pins/68b83a3), read-only unter /__stub/ ausgeliefert.
// Lauf: CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/68b83a3 node --test src/apps/business-os/modules/reports/tests/v4-api.test.mjs
// Playwright: aus PLAYWRIGHT_MODULE, sonst ~/Documents/ctox/src/apps/business-os/node_modules/playwright.
// Screenshots (1280 und 420) landen in tests/shots/.
import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { readFile, mkdir } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { dirname, join, normalize, extname } from 'node:path';
import { homedir } from 'node:os';

const APP = 'reports';
const STUB = process.env.CTOX_APP_API_STUB || join(homedir(), '.local/state/shell-v4/stub-pins/68b83a3');
const HERE = dirname(fileURLToPath(import.meta.url));
const BOS = normalize(join(HERE, '../../..'));
const require = createRequire(import.meta.url);
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || join(homedir(), 'Documents/ctox/src/apps/business-os/node_modules/playwright'));
const TYPES = { '.js': 'text/javascript', '.mjs': 'text/javascript', '.css': 'text/css', '.html': 'text/html', '.json': 'application/json', '.svg': 'image/svg+xml' };

const PAGE = `<!doctype html><html lang="de"><head><meta charset="utf-8">
<link rel="stylesheet" href="/ui-contract/v1/workjet-ui-contract.css">
<link rel="stylesheet" href="/app.css">
<link rel="stylesheet" href="/shared/base.css">
<link rel="stylesheet" href="/modules/${APP}/index.css">
<style>html,body{margin:0} #win{position:relative;overflow:hidden;background:var(--bg)}</style>
</head><body><div id="win" style="width:1280px;height:760px"></div></body></html>`;

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
      // Wie der Service Worker (§16.15): ctox:shared/x.js → Shell-Pfad (Import-Map greift für ctox: nicht).
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
  await page.evaluate(async (APP) => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    window.__mod = await import(`/modules/${APP}/index.js`);
    const kernel = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false, actor: { id: 'michael', role: 'admin' } });
    await kernel.ready;
    await kernel.sim.push('business_apps', [
      { id: 'reports', title: 'Bugs & Features' }, { id: 'ctox', title: 'CTOX' }, { id: 'threads', title: 'Threads' },
      { id: 'coding-agents', title: 'Coding Agents' }, { id: 'notes', title: 'Notizen' },
    ]);
    await kernel.sim.push('business_module_reports', [
      { id: 'rep_1', kind: 'bug', title: 'Notiz-Liste springt beim Speichern', summary: 'Nach dem Speichern scrollt die Liste nach oben.', expected: 'Scrollposition bleibt.', status: 'open', severity: 'high', module_id: 'notes', created_at_ms: Date.UTC(2026, 9, 14, 9), updated_at_ms: Date.UTC(2026, 9, 14, 10) },
      { id: 'rep_2', kind: 'feature_request', title: 'Export als CSV in der Rechnungsliste mit sehr langem Titel, der umbrechen muss', summary: 'CSV-Export gewünscht.', status: 'completed', severity: 'medium', module_id: 'notes', created_at_ms: Date.UTC(2026, 9, 12, 9), updated_at_ms: Date.UTC(2026, 9, 12, 9) },
    ]);
    await kernel.sim.push('ctox_bug_reports', [
      { id: 'bug_9', title: 'Kalender zeigt falsche Woche', description: 'KW 42 statt 43.', status: 'running', module: 'calendar', severity: 'low', updated_at_ms: Date.UTC(2026, 9, 13, 8) },
    ]);
    await kernel.sim.push('business_module_releases', [
      { id: 'rel_3', version_id: 'notes@3', module_id: 'notes', version: 3, status: 'active', created_at_ms: Date.UTC(2026, 9, 10) },
      { id: 'rel_2', version_id: 'notes@2', module_id: 'notes', version: 2, status: 'archived', created_at_ms: Date.UTC(2026, 9, 1) },
    ]);
    await kernel.sim.push('business_users', [
      { id: 'michael', display_name: 'Michael', role: 'admin' }, { id: 'alice', display_name: 'Alice Admin', role: 'admin' },
    ]);
    await kernel.sim.settle();
    window.__k = kernel;
    window.__api = kernel.appApi(APP, '1');
    window.__zone = () => (kernel.status || kernel.statusZone).forApp(APP).get().filter((e) => e.source === 'app');
    window.__commands = (type) => window.__api.query('business_commands', { selector: { app_id: 'reports', ...(type ? { type } : {}) }, sort: [{ created_at: 'asc' }] }).get();
  }, APP);
});

after(async () => { await browser?.close(); await new Promise((r) => server?.close(r)); });

const settle = async () => { await page.evaluate(() => window.__k.sim.settle()); await page.waitForTimeout(220); };
const text = (sel) => page.$eval(sel, (el) => el.textContent);

test('mount: Live-Daten aus beiden Report-Collections, gescoptes Root, Titel über api.shell', async () => {
  await page.evaluate(async () => {
    const { countingApi } = await import('/v4/tools/port/test-api.mjs');
    window.__c = countingApi(window.__api);
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, {});
  });
  await settle();
  const list = await text('[data-reports-list]');
  assert.match(list, /Notiz-Liste springt/);
  assert.match(list, /Kalender zeigt falsche Woche/);
  assert.match(await text('[data-report-title]'), /Notiz-Liste springt/, 'neuester Eintrag ausgewählt');
  assert.equal(await page.$eval('#win', (el) => el.dataset.app), APP);
  assert.equal(await page.evaluate(() => window.__k.shell.window('reports').state.get().title), 'Bugs & Features');
  assert.equal(await page.evaluate(() => window.__c.subscribers('business_module_reports')), 1);
  assert.deepEqual(await page.evaluate(() => window.__zone()), []);
  assert.deepEqual(page.__errors, []);
});

test('Live-Änderung vom Core erscheint ohne Neuladen, Löschung verschwindet', async () => {
  await page.evaluate(() => window.__k.sim.push('ctox_bug_reports', { id: 'bug_live', title: 'Live gemeldeter Fehler', status: 'open', module: 'notes', updated_at_ms: Date.UTC(2026, 9, 15) }));
  await settle();
  assert.match(await text('[data-reports-list]'), /Live gemeldeter Fehler/);
  await page.evaluate(() => window.__k.sim.push('ctox_bug_reports', { id: 'bug_live', _deleted: true }));
  await settle();
  assert.doesNotMatch(await text('[data-reports-list]'), /Live gemeldeter Fehler/);
});

test('leere Collections rendern keinen Zustandstext und keine Zone', async () => {
  const empty = await page.evaluate(async () => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    const k2 = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false, instanceId: 'empty' });
    await k2.ready;
    const host = document.createElement('div');
    host.style.cssText = 'width:900px;height:500px';
    document.body.append(host);
    const app = await window.__mod.mount(host, k2.appApi('reports', '1'), {});
    await new Promise((r) => setTimeout(r, 300));
    const out = {
      listText: host.querySelector('[data-reports-list]').textContent.trim(),
      detailText: host.querySelector('[data-reports-detail-scroll]').textContent.trim(),
      title: host.querySelector('[data-report-title]').textContent.trim(),
      stateBox: Boolean(host.querySelector('.ctox-syncing, .ctox-empty, [role="status"]')),
      zone: (k2.status || k2.statusZone).forApp('reports').get().map((e) => e.code),
    };
    await app.unmount();
    host.remove();
    await k2.close();
    return out;
  });
  assert.equal(empty.listText, '');
  assert.equal(empty.detailText, '');
  assert.equal(empty.title, '');
  assert.equal(empty.stateBox, false);
  assert.deepEqual(empty.zone, []);
  // Modulzustand ist modulweit (V4-PORT.md OFFEN): Hauptfenster neu montieren.
  await page.evaluate(async () => {
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, {});
  });
  await settle();
});

// Strang SKL-A2: kein Rückweg-Schreiben mehr aus dem Browser (kein Abgleich über business_commands). Das Ergebnis
// setzt die Core-Seite am Report (die frühere Rückruf-Kette reports.delegation.* ist seit FIX10 entfernt); die Ansicht
// folgt live dem Report.
test('Rückweg: Coding-Turn in business_commands schreibt nichts aus dem Browser; Handler-Ergebnis erscheint live', async () => {
  const before = await page.evaluate(() => window.__api.get('business_module_reports', 'rep_1').get());
  await page.evaluate(() => window.__k.sim.push('business_commands', { id: 'cmd_turn_1', type: 'ctox.coding.turn', status: 'running', task_id: 'task_77', payload: { context: { record_id: 'rep_1' } } }));
  await settle();
  const unchanged = await page.evaluate(() => window.__api.get('business_module_reports', 'rep_1').get());
  assert.equal(unchanged.ctox_command_id, before.ctox_command_id);
  assert.equal(unchanged.task_id, before.task_id);
  assert.equal(unchanged.status, 'open');
  assert.equal(await page.evaluate(async () => (await window.__k.sim.outbox()).length), 0, 'kein Schreibversuch');
  // So schreibt die Core-Seite die Verknüpfung – erscheint live ohne Abgleich.
  await page.evaluate((doc) => window.__k.sim.push('business_module_reports', { ...doc, ctox_command_id: 'cmd_turn_1', task_id: 'task_77', status: 'running', updated_at_ms: doc.updated_at_ms + 1 }), unchanged);
  await settle();
  // Textdiät (TEXT-B): Roh-IDs (Befehl/Aufgabe) stehen nicht mehr als Feld im Detail; das Ergebnis erscheint live als
  // freigeschalteter Sprung „CTOX-Aufgabe zeigen“, der genau diese Aufgabe trägt (siehe Navigationstest).
  assert.doesNotMatch(await text('[data-reports-detail-scroll]'), /task_77|cmd_turn_1/);
  assert.equal(await page.evaluate(() => document.querySelector('[data-focus-task]')?.disabled), false, 'Aufgabe live verknüpft');
});

test('Befehl: Rollback legt business_commands-Dokument an, failed → genau ein Zoneneintrag, ohne Warten', async () => {
  await page.click('[data-toggle-actions]');
  await settle();
  assert.match(await text('[data-reports-actions]'), /v3/);
  await page.evaluate(() => window.__k.sim.failCommand('ctox.module.rollback', 'Version gesperrt'));
  await page.click('[data-rollback-module]');
  await settle();
  const cmd = await page.evaluate(() => window.__commands('ctox.module.rollback').at(-1));
  assert.equal(cmd.type, 'ctox.module.rollback');
  assert.equal(cmd.payload.version_id, 'notes@3');
  assert.equal(cmd.payload.module_id, 'notes');
  assert.equal(cmd.record_id, 'notes@3');
  assert.equal(cmd.status, 'failed');
  const zone = await page.evaluate(() => window.__zone());
  assert.equal(zone.length, 1);
  assert.equal(zone[0].code, 'reports.command.ctox.module.rollback');
  assert.match(zone[0].text, /Version gesperrt/);
  assert.doesNotMatch(await text('[data-reports-actions]'), /läuft|ausgeführt/, 'kein Zustandstext im Inhalt');
  // Erneuter Versuch läuft durch und räumt die Zone.
  await page.click('[data-rollback-module]');
  await settle();
  assert.equal(await page.evaluate(() => window.__commands('ctox.module.rollback').at(-1).status), 'done');
  assert.deepEqual(await page.evaluate(() => window.__zone()), []);
});

test('Navigation: CTOX Task zeigen springt über api.shell.navigate mit Task-Parametern', async () => {
  await page.click('[data-focus-task]');
  const nav = await page.evaluate(() => window.__k.sim.navigations.at(-1));
  assert.equal(nav.to || nav.target || nav.app_id, 'ctox');
  assert.equal(nav.params.task_id, 'task_77');
  assert.equal(nav.params.command_id, 'cmd_turn_1');
});

// LOOP Runde 2 (§16.44): „An Coding Agent übergeben“ öffnet die Code-Ansicht der betroffenen App über
// api.shell.openSource(app, {tab:'reports', reportId}); kein Dialog, kein Befehl, nichts im Snapshot. „In Arbeit“ folgt
// aus coding_turn_id (gesetzt von der Code-Ansicht).
test('Übergabe: api.shell.openSource(App, {tab:"reports", reportId}) ohne Dialog und Befehl; „In Arbeit“ aus coding_turn_id', async () => {
  // Der Stub-Kern hat keine Shell-Fenster: api.shell.openSource wird wie vom Rahmen (v4/shell/app-host.js) gereicht und
  // protokolliert; die App wird mit dieser API neu montiert (der Snapshot behält die Auswahl).
  await page.evaluate(async () => {
    window.__opened = [];
    // wie delegate() in v4/tools/port/test-api.mjs: Schlüssel als Getter auf das Original, extra überschreibt.
    const over = (target, extra) => {
      const out = {};
      for (const k of Object.keys(target)) if (!(k in extra)) Object.defineProperty(out, k, { enumerable: true, get: () => (typeof target[k] === 'function' ? target[k].bind(target) : target[k]) });
      return Object.freeze(Object.assign(out, extra));
    };
    const base = window.__c.api;
    window.__shellApi = over(base, { shell: over(base.shell, { openSource: (...a) => { window.__opened.push(a); return true; } }) });
    const snap = structuredClone(window.__app.snapshot());
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__shellApi, { restored: snap });
  });
  await settle();
  const before = await page.evaluate(() => window.__commands().length);
  await page.click('[data-report-id="rep_1"]');
  await settle();
  await page.click('[data-head-delegate]');
  await settle();
  assert.equal(await page.$('[data-delegate-modal], .ctox-modal'), null, 'kein Dialog');
  assert.deepEqual(await page.evaluate(() => window.__opened), [['notes', { tab: 'reports', reportId: 'rep_1' }]]);
  assert.equal(await page.evaluate(() => window.__commands().length), before, 'kein Befehl');
  const snap = await page.evaluate(() => window.__app.snapshot());
  assert.equal(snap.selectedId, 'rep_1');
  assert.equal(snap.delegate, undefined);
  // Agent-Lauf aus der Code-Ansicht verknüpft die Meldung → Liste und Detail zeigen „In Arbeit“, Knopf bleibt nutzbar
  const doc = await page.evaluate(() => window.__api.get('business_module_reports', 'rep_1').get());
  await page.evaluate((d) => window.__k.sim.push('business_module_reports', { ...d, coding_turn_id: 'turn-42', updated_at_ms: d.updated_at_ms + 1 }), doc);
  await settle();
  assert.match(await text('[data-reports-list] [data-report-id="rep_1"]'), /In Arbeit/);
  assert.match(await text('[data-reports-actions]'), /Coding Agent · In Arbeit/);
  assert.equal(await page.evaluate(() => document.querySelector('[data-head-delegate]').disabled), false);
  await page.click('[data-reports-actions] [data-open-code-view]').catch(() => {});
  await settle();
  assert.equal(await page.evaluate(() => window.__opened.length), 2);
  assert.deepEqual(page.__errors, []);
});

test('Screenshots 1280 und 420 gegen den Stub', async () => {
  await mkdir(join(HERE, 'shots'), { recursive: true });
  await page.click('[data-report-id="rep_1"]');
  await settle();
  await page.screenshot({ path: join(HERE, 'shots', `${APP}-1280.png`) });
  await page.click('[data-toggle-actions]'); // gestapelt: Detail statt Aktionsspalte zeigen
  await page.setViewportSize({ width: 420, height: 860 });
  await page.$eval('#win', (el) => { el.style.width = '420px'; el.style.height = '860px'; });
  await page.waitForTimeout(250);
  await page.screenshot({ path: join(HERE, 'shots', `${APP}-420.png`) });
  const overflow = await page.$eval('#win', (el) => el.scrollWidth - el.clientWidth);
  assert.ok(overflow <= 0, `kein horizontaler Überlauf bei 420 (scrollWidth-clientWidth=${overflow})`);
  await page.setViewportSize({ width: 1280, height: 760 });
  await page.$eval('#win', (el) => { el.style.width = '1280px'; el.style.height = '760px'; });
});

test('unmount meldet alle Abos ab und leert root', async () => {
  const res = await page.evaluate(async () => {
    await window.__app.unmount();
    await window.__k.sim.push('business_module_reports', { id: 'rep_late', title: 'Late', updated_at_ms: 1 });
    await window.__k.sim.settle();
    return {
      subs: ['business_module_reports', 'ctox_bug_reports', 'business_module_releases', 'business_commands', 'ctox_queue_tasks', 'business_apps'].map((c) => window.__c.subscribers(c)),
      children: document.getElementById('win').children.length,
    };
  });
  assert.deepEqual(res.subs, [0, 0, 0, 0, 0, 0]);
  assert.equal(res.children, 0);
  assert.deepEqual(page.__errors, []);
});
