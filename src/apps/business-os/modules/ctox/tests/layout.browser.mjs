// Component browser acceptance on the v4 contract: the real mount(root, api) against the frozen
// App-API stub (Pin 67229d2, CTOX_APP_API_STUB), real app CSS plus the shell theme, synthetic task data.
// Geometry across five widths, the routing-failure card, the job editor, the in-window task drawer,
// crew controls and the grouped creature click. Production authentication, replication and
// performance are separate gates.
//
// v4-Umstellung (V4-PORT.md): die früheren Szenarien „verzögerter Harness-Status" und „Status während
// der Aufgaben-Hydration" prüften Lese-Wettläufe des RxDB-Lesewegs (findOne/$.subscribe, Lese-Koaleszenz).
// Diesen Leseweg gibt es nicht mehr: api.query/api.get liefern synchron den lokalen Stand, jede Änderung
// kommt als Abo-Emission. Sie sind ersetzt durch „Pause → Befehl, bestätigter Status erst aus der
// Projektion". Das Settings-Szenario (react-settings, Shell) gehört nicht zur App.
//
// Lauf: node --test src/apps/business-os/modules/ctox/tests/layout.browser.mjs (anderer Stub: CTOX_APP_API_STUB=<dir>)
import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { readFile, mkdir, writeFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { dirname, join, normalize, extname } from 'node:path';
import { homedir } from 'node:os';
import { realManifestRoutes } from '../../../v4/tools/port/real-manifest.mjs';

// Stub: CTOX_APP_API_STUB, sonst die eingecheckte Pin-Kopie 68b83a3 (tests/fixtures/app-api-stub/PIN.md) – auch in CI.
const STUB = process.env.CTOX_APP_API_STUB || join(dirname(fileURLToPath(import.meta.url)), 'fixtures/app-api-stub');
const HERE = dirname(fileURLToPath(import.meta.url));
const BOS = normalize(join(HERE, '../../..'));
const OUT = process.env.CTOX_EVIDENCE_DIR || join(HERE, 'shots');
const require = createRequire(import.meta.url);
// Lader (OLDTEST-Muster): playwright aus dem Checkout (CI: npm ci), sonst PLAYWRIGHT_MODULE, sonst Haupt-Checkout.
function loadPlaywright() {
  try { return require('playwright'); } catch {
    return require(process.env.PLAYWRIGHT_MODULE || join(homedir(), 'Documents/ctox/src/apps/business-os/node_modules/playwright'));
  }
}
const { chromium } = loadPlaywright();
const TYPES = { '.js': 'text/javascript', '.mjs': 'text/javascript', '.css': 'text/css', '.html': 'text/html', '.json': 'application/json', '.svg': 'image/svg+xml' };

const PAGE = `<!doctype html><html data-theme="light"><head><meta charset="utf-8">
<link rel="stylesheet" href="/app.css"><link rel="stylesheet" href="/shared/base.css">
<link rel="stylesheet" href="/modules/ctox/index.css">
<style>html,body{margin:0}#win{position:fixed;inset:0;overflow:hidden}</style>
</head><body><div id="win"></div></body></html>`;

let server, browser, base;
const results = [];

before(async () => {
  server = createServer(async (req, res) => {
    const path = decodeURIComponent(new URL(req.url, 'http://x').pathname);
    if (path === '/') { res.writeHead(200, { 'content-type': 'text/html' }); res.end(PAGE); return; }
    const [rootDir, rel] = path.startsWith('/__stub/') ? [STUB, path.slice('/__stub'.length)] : [BOS, path];
    const file = normalize(join(rootDir, rel));
    if (!file.startsWith(rootDir)) { res.writeHead(403); res.end(); return; }
    try {
      let body = await readFile(file);
      if (/\.m?js$/.test(file)) body = Buffer.from(body.toString('utf8').replace(/(['"])ctox:shared\//g, '$1/shared/'));
      res.writeHead(200, { 'content-type': TYPES[extname(file)] || 'application/octet-stream' }); res.end(body);
    } catch { res.writeHead(404); res.end(); }
  });
  await new Promise((r) => server.listen(0, '127.0.0.1', r));
  base = `http://127.0.0.1:${server.address().port}/`;
  browser = await chromium.launch({ headless: true });
  await mkdir(OUT, { recursive: true });
});

after(async () => {
  await writeFile(join(OUT, 'layout-results.json'), JSON.stringify(results, null, 2)).catch(() => {});
  await browser?.close();
  await new Promise((r) => server?.close(r));
});

// Routen, Rechte und Collections der echten ctox-Version (v4/manifests/ctox.manifest.json) plus das Kern-Manifest
// ctox-platform (E48): ctox liest business_commands/ctox_queue_tasks als Fremd-Collections der Plattform (_fixture_owners).
const CTOX_MANIFEST_ROUTES = realManifestRoutes('ctox');

// One page per scenario: a fresh stub kernel with the routing-failure fixture, mounted into #win.
async function openFixture(width, { extra = null } = {}) {
  const page = await browser.newPage({ viewport: { width, height: 710 } });
  const errors = [];
  page.on('pageerror', (e) => errors.push(String(e)));
  page.on('console', (m) => { if (m.type() === 'error') errors.push(`console: ${m.text()}`); });
  page.__errors = errors;
  await page.goto(base);
  await page.evaluate(async ({ extraFixture, ctoxManifest }) => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    const mod = await import('/modules/ctox/index.js');
    const kernel = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false, actor: { role: 'admin', permissions: ['apps.source.edit'] } });
    await kernel.ready;
    await kernel.sim.push('business_apps', [{ id: 'ctox', title: 'Crew' }, { id: 'shell', title: 'Shell' }]);
    // E35 (Option B): die aktive ctox-Version mit ihren Befehlsrouten veröffentlichen (nach der business_apps-Zeile, sonst fehlt active_ver), wie die anderen v4-Gate-Dateien.
    // Ohne aktives Manifest lehnt der Kern App-Befehle (ctox.queue.pause) beim Schreiben mit unknown_command ab.
    // FIXT-R: Eigentümer der Fremd-Collections (echte Manifeste, inkl. ctox-platform) vor der eigenen Version.
    for (const o of ctoxManifest?._fixture_owners ?? []) if (!(await kernel.sim.inspect('business_apps', o.app))?.data?.active_ver) await kernel.sim.publishAppVersion(o.app, o.ver, o.publish);
    await kernel.sim.publishAppVersion('ctox', '1.1.2', { title: 'Crew', manifestExtra: ctoxManifest });
    await kernel.sim.push('business_commands', [{ id: 'layout-command', command_id: 'layout-command', execution_task_id: 'layout-task', execution_mode: 'queue', execution_phase: 'queued', status: 'accepted', payload: { title: 'Cereda' }, execution_progress: { phase: 'queued', steps: [] }, updated_at_ms: Date.now() }]);
    await kernel.sim.push('ctox_queue_tasks', [{ id: 'layout-task', command_id: 'layout-command', status: 'queued', route_status: 'failed', failure_class: 'terminal', failure_attempt_count: 4, status_note: 'thread/start MCP handshake timeout', updated_at_ms: Date.now() }]);
    await kernel.sim.push('ctox_harness_status', [{ id: 'harness', paused: false, service_running: true, worker_capacity: 1, updated_at_ms: 1 }]);
    if (extraFixture) for (const [collection, docs] of Object.entries(extraFixture)) await kernel.sim.push(collection, docs);
    await kernel.sim.settle();
    window.__k = kernel;
    window.__api = kernel.appApi('ctox', '1.1.2');
    window.__app = await mod.mount(document.getElementById('win'), window.__api, {});
    window.__commands = () => window.__api.query('business_commands', { sort: [{ created_at: 'asc' }] }).get().filter((c) => c.type);
    window.__zone = () => kernel.statusZone.forApp('ctox').get().filter((e) => e.source === 'app');
  }, { extraFixture: extra, ctoxManifest: CTOX_MANIFEST_ROUTES });
  await settle(page);
  return page;
}

async function settle(page) {
  await page.evaluate(() => window.__k.sim.settle());
  await page.waitForTimeout(220);
}

for (const width of [430, 630, 768, 1000, 1280]) {
  test(`Geometrie und Fehlerkarte bei ${width}px`, async () => {
    const page = await openFixture(width);
    try {
      await page.evaluate(() => document.querySelector('[data-select-task-id="layout-task"]')?.click());
      await settle(page);
      const measured = await page.locator('[data-flow-canvas]').evaluate((canvas) => ({ height: canvas.getBoundingClientRect().height, width: canvas.getBoundingClientRect().width, nodes: canvas.querySelectorAll('.ctox-flow-node-g').length }));
      results.push({ viewportWidth: width, ...measured, errors: page.__errors });
      await page.screenshot({ path: join(OUT, `layout-${width}.png`) });
      assert.deepEqual(page.__errors, []);
      assert.equal(measured.nodes, 16);
      const reason = await page.locator('.ctox-task-reason').first().textContent();
      assert.match(reason, /4 Versuche/);
      assert.match(reason, /Verbindung zu einem Werkzeug/);
      assert.doesNotMatch(reason, /MCP|thread\/start/);
      assert.match(await page.locator('[data-pg-band="waiting"]').textContent(), /\(0\)/);
      assert.equal(await page.locator('.ctox-task-pipeline').first().getAttribute('aria-label'), 'Fehler');
      assert.equal(await page.locator('[data-task-id="layout-task"][data-creature-node-id="model-failed"]').count(), 1);
      assert.ok(measured.height >= 200, `Harness collapsed at width ${width}: ${JSON.stringify(measured)}`);
      await page.locator('[data-node-id="queued"]').scrollIntoViewIfNeeded();
      const visible = await page.locator('[data-node-id="queued"]').evaluate((node) => { const r = node.getBoundingClientRect(); return document.elementsFromPoint(r.x + r.width / 2, r.y + r.height / 2).some((e) => e === node || node.contains(e)); });
      assert.ok(visible, `Harness node is clipped at width ${width}`);
      // Nichts außerhalb des App-Fensters, keine horizontale Seitenverschiebung.
      assert.deepEqual(await page.evaluate(() => [...document.body.children].map((el) => el.id || el.tagName)), ['win']);
      assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth + 1));
    } finally { await page.close(); }
  });
}

test('1280: Kopfzeile einreihig, Editor-Entwurf überlebt Live-Änderung, Mehr-Menü im Fenster ohne Kanäle-Knopf', async () => {
  const page = await openFixture(1280);
  try {
    await page.evaluate(() => document.querySelector('[data-select-task-id="layout-task"]')?.click());
    await settle(page);
    const headerHeight = await page.locator('[data-ctox-main] > header').evaluate((node) => node.getBoundingClientRect().height);
    assert.ok(headerHeight <= 40, `Header must be one row, got ${headerHeight}`);
    const gap = await page.locator('.ctox-harness-app').evaluate((node) => node.getBoundingClientRect().bottom - node.querySelector('[data-ctox-main]').getBoundingClientRect().bottom);
    assert.ok(gap <= 16, `Unused bottom reserve: ${gap}px`);
    await page.locator('[data-job-toggle]').click();
    assert.deepEqual(await page.locator('[data-job-panel] select[name="priority"] option').allTextContents(), ['Dringend', 'Hoch', 'Normal', 'Niedrig']);
    await page.locator('[data-job-panel] input[name="title"]').fill('Ungespeicherter Entwurf');
    await page.locator('[data-job-panel] input[name="title"]').blur();
    await page.evaluate(() => window.__k.sim.push('ctox_queue_tasks', { id: 'layout-task', command_id: 'layout-command', status: 'queued', route_status: 'failed', failure_class: 'terminal', failure_attempt_count: 4, status_note: 'Neuer persistierter Status', updated_at_ms: Date.now() }));
    await settle(page);
    assert.equal(await page.locator('[data-job-panel] input[name="title"]').inputValue(), 'Ungespeicherter Entwurf');
    await page.locator('[data-job-toggle]').click();
    assert.equal(await page.locator('[data-job-panel]').isVisible(), false);
    await page.locator('[data-ctox-main] .ctox-more-actions > summary').click();
    // „Kanäle verwalten“ entfällt (FR4-3): keine v4-App trägt die Kanal-Einstellungen, also kein Navigationsziel.
    assert.equal(await page.locator('[data-manage-channels]').count(), 0);
    const webstack = page.locator('[data-webstack-toggle]');
    assert.equal(await webstack.isVisible(), true);
    // Das Menü liegt im App-Fenster (kein Top-Layer-Popover).
    assert.equal(await page.evaluate(() => document.getElementById('win').contains(document.querySelector('.ctox-more-actions-body'))), true);
    await page.locator('[data-ctox-main] .ctox-more-actions > summary').click();
    assert.equal(await page.locator('.ctox-more-actions[open]').count(), 0);
    assert.deepEqual(page.__errors, []);
  } finally { await page.close(); }
});

test('1280: Aufgaben-Drawer im root, Verlauf-Klappe und Entwurf überleben Live-Updates', async () => {
  const page = await openFixture(1280);
  try {
    await page.evaluate(() => document.querySelector('[data-select-task-id="layout-task"]')?.click());
    await settle(page);
    await page.locator('[data-ctox-main] .ctox-more-actions > summary').click();
    await page.locator('[data-open-selected-task]').click();
    await settle(page);
    assert.equal(await page.evaluate(() => document.getElementById('win').contains(document.querySelector('[data-ctox-drawer]'))), true);
    const taskHistory = page.locator('[data-ctox-drawer] .ctox-drawer-timeline');
    assert.equal(await taskHistory.getAttribute('open'), null);
    assert.ok(await taskHistory.evaluate((e) => e.getBoundingClientRect().height) <= 40);
    await taskHistory.locator('summary').click();
    assert.equal(await taskHistory.locator('.ctox-drawer-steps').isVisible(), true);
    await page.evaluate(() => window.__k.sim.push('ctox_queue_tasks', { id: 'layout-task', command_id: 'layout-command', status: 'queued', route_status: 'failed', failure_class: 'terminal', failure_attempt_count: 4, status_note: 'A new persisted status arrived', updated_at_ms: Date.now() }));
    await settle(page);
    assert.equal(await page.locator('[data-ctox-drawer] .ctox-drawer-timeline').getAttribute('open'), '');
    await page.locator('[data-ctox-drawer] .ctox-drawer-edit-fold > summary').click();
    const drawerTitle = page.locator('[data-ctox-drawer] input[name=title]');
    await drawerTitle.fill('Entwurf bleibt bei Live-Updates');
    await page.evaluate(() => window.__k.sim.push('ctox_queue_tasks', { id: 'layout-task', command_id: 'layout-command', status: 'queued', route_status: 'failed', failure_class: 'terminal', failure_attempt_count: 4, status_note: 'Another update while typing', updated_at_ms: Date.now() }));
    await settle(page);
    assert.equal(await drawerTitle.inputValue(), 'Entwurf bleibt bei Live-Updates');
    assert.equal(await drawerTitle.evaluate((e) => e === document.activeElement), true);
    await page.screenshot({ path: join(OUT, 'layout-1280-drawer.png') });
    assert.deepEqual(page.__errors, []);
  } finally { await page.close(); }
});

test('Crew-Steuerung: Pause ist ein Befehl, der bestätigte Zustand kommt nur aus der Projektion', async () => {
  const page = await openFixture(1280);
  try {
    await page.locator('[data-ctox-main] .ctox-more-actions > summary').click();
    const pause = page.locator('[data-harness-pause]');
    assert.equal(await pause.getAttribute('aria-pressed'), 'false');
    await pause.click();
    await settle(page);
    const cmd = await page.evaluate(() => window.__commands().at(-1));
    assert.equal(cmd.type, 'ctox.queue.pause');
    assert.equal(cmd.payload.paused, true);
    // Kein Rückschluss aus dem Befehl: ohne neue Projektion bleibt der Knopf unverändert.
    await page.locator('[data-ctox-main] .ctox-more-actions > summary').click();
    assert.equal(await page.locator('[data-harness-pause]').getAttribute('aria-pressed'), 'false');
    await page.evaluate(() => window.__k.sim.push('ctox_harness_status', { id: 'harness', paused: true, service_running: true, worker_capacity: 1, updated_at_ms: 2 }));
    await settle(page);
    assert.equal(await page.locator('[data-harness-pause]').getAttribute('aria-pressed'), 'true');
    assert.ok((await page.evaluate(() => window.__zone().map((e) => e.code))).includes('ctox.harness.paused'));
    assert.equal(await page.locator('[data-ctox-harness]').getAttribute('aria-label'), null, 'kein Zustandstext am Fenster');
    results.push({ scenario: 'pause-command-then-projection', passed: true });
    assert.deepEqual(page.__errors, []);
  } finally { await page.close(); }
});

// A member with several tasks opens their list on a real mouse click. The canvas' pan pointer capture
// used to retarget pointerup/click to the canvas, so the ×N list only opened by keyboard (thesen, 28.09.2026).
test('Wesen mit mehreren Aufgaben: echter Mausklick öffnet die Liste, Auswahl wählt die Aufgabe', async () => {
  const task = (id) => ({ id, title: `Task ${id}`, status: 'running', route_status: 'running', crew_member_id: 'crew:tavi', updated_at_ms: Date.now(), execution_progress: { phase: 'working', steps: [] } });
  const page = await openFixture(1280, { extra: {
    ctox_crew_members: [{ id: 'crew:tavi', name: 'Tavi', shape: 'blob', color: '#00aa9a', archived: false, state: 'on_duty', updated_at_ms: 1 }],
    ctox_queue_tasks: [task('g1'), task('g2'), task('g3')],
  } });
  try {
    await page.evaluate(() => document.querySelector('[data-select-task-id="g1"]')?.click());
    await settle(page);
    const slot = page.locator('.ctox-flow-creature-slot[data-crew-tasks]');
    assert.equal(await slot.count(), 1, 'one being carries the three tasks');
    const box = await slot.boundingBox();
    await page.mouse.click(box.x + box.width / 2, box.y + box.height / 2);
    const popover = page.locator('[data-crew-tasks-pop]');
    await popover.waitFor({ timeout: 3000 });
    assert.equal(await popover.locator('[data-crew-pop-task]').count(), 3, 'the list names every task of the member');
    await popover.locator('[data-crew-pop-task="g2"]').click();
    assert.equal(await page.locator('[data-crew-tasks-pop]').count(), 0, 'picking a task closes the list');
    assert.equal(await page.evaluate(() => window.__app.snapshot().selectedTaskId), 'g2', 'the picked task is selected');
    assert.deepEqual(page.__errors, []);
  } finally { await page.close(); }
});
