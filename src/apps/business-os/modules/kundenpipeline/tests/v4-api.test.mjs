// kundenpipeline gegen den App-API-4.0-Kern-Stub (createStubKernel, read-only unter /__stub/) in echtem Chromium.
// Stub: CTOX_APP_API_STUB (Pflicht-Pin ~/.local/state/shell-v4/stub-pins/f967baf), sonst derselbe Pin.
// Lauf: CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/f967baf node --test src/apps/business-os/modules/kundenpipeline/tests/v4-api.test.mjs
// Screenshots (1280/420) nach tests/shots/ mit KUNDENPIPELINE_SHOTS=1.
import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { dirname, join, normalize, extname } from 'node:path';
import { homedir } from 'node:os';
import { realManifestRoutes } from '../../../v4/tools/port/real-manifest.mjs';

const STUB = process.env.CTOX_APP_API_STUB || join(homedir(), '.local/state/shell-v4/stub-pins/f967baf');
const HERE = dirname(fileURLToPath(import.meta.url));
const BOS = normalize(join(HERE, '../../..'));
const require = createRequire(import.meta.url);
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || join(homedir(), 'Documents/ctox/src/apps/business-os/node_modules/playwright'));
const TYPES = { '.js': 'text/javascript', '.mjs': 'text/javascript', '.css': 'text/css', '.html': 'text/html', '.json': 'application/json', '.svg': 'image/svg+xml' };
const APP = 'kundenpipeline';

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
  // Befehlsrouten der echten kundenpipeline-Version (v4/manifests/kundenpipeline.manifest.json, wie customers/E35): ohne
  // aktive Version lehnt der Kern seit E26/E27 kundenpipeline.decision.resolve beim Schreiben mit unknown_command ab.
  await page.evaluate((routes) => { window.__kundenpipelineRoutes = routes; }, realManifestRoutes('kundenpipeline'));
  await page.evaluate(async (APP) => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    window.__mod = await import(`/modules/${APP}/index.js`);
    const kernel = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false });
    await kernel.ready;
    // FIXT-R: Eigentümer der Fremd-Collections (echte Manifeste, inkl. ctox-platform) vor der eigenen Version.
    for (const o of window.__kundenpipelineRoutes?._fixture_owners ?? []) if (!(await kernel.sim.inspect('business_apps', o.app))?.data?.active_ver) await kernel.sim.publishAppVersion(o.app, o.ver, o.publish);
    await kernel.sim.publishAppVersion(APP, '1', { title: 'Decision Hub', manifestExtra: window.__kundenpipelineRoutes });
    await kernel.sim.push('kundenpipeline_vorgaenge', [
      { id: 'case-1', title: 'Anfrage Müller GmbH', status: 'offen', kunde_name: 'Müller GmbH', quelle_json: { kanal: 'mail', body_clean: 'Bitte Angebot für 40 Module.' }, created_at_ms: 1, updated_at_ms: 1 },
    ]);
    await kernel.sim.push('kundenpipeline_entscheidungen', [
      { id: 'dec-1', titel: 'Projekt zuordnen', typ: 'zuordnung', status: 'offen', vorgang_id: 'case-1', frage_json: { question: 'Gehört die Anfrage zu Projekt Solar?' }, created_at_ms: 10, updated_at_ms: 10 },
      { id: 'dec-2', titel: 'Agent fragt nach', typ: 'agent_escalation', status: 'offen', frage_json: { question: 'Welche Variante?' }, aktionen_json: [{ wert: 'a', label: 'Variante A' }, { wert: 'b', label: 'Variante B' }], created_at_ms: 20, updated_at_ms: 20 },
    ]);
    await kernel.sim.settle();
    const api = kernel.appApi(APP, '1');
    window.__k = kernel;
    window.__api = api;
    window.__zone = () => kernel.statusZone.forApp(APP).get().filter((e) => e.source === 'app');
    window.__rows = (c) => api.query(c, {}).get();
    window.__commands = () => api.query('business_commands', { sort: [{ created_at: 'asc' }] }).get();
  }, APP);
});

after(async () => { await browser?.close(); await new Promise((r) => server?.close(r)); });

const settle = async () => { await page.evaluate(() => window.__k.sim.settle()); await page.waitForTimeout(250); };
const listText = () => page.$eval('[data-decision-list]', (el) => el.textContent);
const SUBS = ['kundenpipeline_entscheidungen', 'kundenpipeline_vorgaenge', 'kundenpipeline_projekte'];

test('mount: rendert Live-Daten, nur api.*, gescoptes Root, Titel über api.shell; keine Neuladeschleife', async () => {
  await page.evaluate(async () => {
    window.__intervals = 0;
    const orig = window.setInterval;
    window.setInterval = (...args) => { window.__intervals += 1; return orig(...args); };
    const { countingApi } = await import('/v4/tools/port/test-api.mjs');
    window.__c = countingApi(window.__api);
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, {});
  });
  await settle();
  const list = await listText();
  assert.match(list, /Projekt zuordnen/);
  assert.match(list, /Agent fragt nach/);
  assert.match(await page.$eval('[data-detail-body]', (el) => el.textContent), /Gehört die Anfrage zu Projekt Solar\?/);
  assert.equal(await page.$eval('#win', (el) => el.dataset.app), 'kundenpipeline');
  assert.equal(await page.evaluate(() => window.__k.shell.window('kundenpipeline').state.get().title), 'Decision Hub');
  assert.deepEqual(await page.evaluate((subs) => subs.map((c) => window.__c.subscribers(c)), SUBS), [1, 1, 1]);
  assert.equal(await page.evaluate(() => window.__intervals), 0, 'kein setInterval (vorher 10-s-Reload)');
  assert.equal(await page.evaluate(() => Boolean(document.querySelector('[data-refresh]'))), false);
  assert.deepEqual(page.__errors, []);
});

test('Live-Änderung vom Core (sim.push) erscheint ohne Neuladen, Löschung verschwindet', async () => {
  await page.evaluate(() => window.__k.sim.push('kundenpipeline_entscheidungen', { id: 'dec-live', titel: 'Live Triage', typ: 'triage', status: 'offen', created_at_ms: 30, updated_at_ms: 30 }));
  await settle();
  assert.match(await listText(), /Live Triage/);
  await page.evaluate(() => window.__k.sim.push('kundenpipeline_entscheidungen', { id: 'dec-live', _deleted: true }));
  await settle();
  assert.doesNotMatch(await listText(), /Live Triage/);
});

test('leere Collections rendern keinen Zustandstext und keine Zone', async () => {
  const res = await page.evaluate(async () => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    const k2 = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false, instanceId: 'empty' });
    await k2.ready;
    const host = document.createElement('div');
    host.style.cssText = 'width:900px;height:600px';
    document.body.append(host);
    const app = await window.__mod.mount(host, k2.appApi('kundenpipeline', '1'), {});
    await new Promise((r) => setTimeout(r, 300));
    const texts = ['[data-decision-list]', '[data-detail-body]', '[data-action-body]', '[data-detail-kicker]', '[data-detail-title]'].map((sel) => host.querySelector(sel)?.textContent.trim() ?? null);
    const hasStateBox = Boolean(host.querySelector('.ctox-syncing, .kundenpipeline-empty, [role="status"]'));
    const zone = k2.statusZone.forApp('kundenpipeline').get().map((e) => e.code);
    await app.unmount();
    host.remove();
    await k2.close();
    return { texts, hasStateBox, zone };
  });
  assert.deepEqual(res.texts, ['', '', '', '', '']);
  assert.equal(res.hasStateBox, false);
  assert.deepEqual(res.zone, []);
  await page.evaluate(async () => {
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, {});
  });
  await settle();
});

test('Befehl: Option wählen → api.command ohne Warten; failed → genau ein Zoneneintrag, done räumt', async () => {
  await page.evaluate(() => window.__k.sim.failCommand('kundenpipeline.decision.resolve', 'Policy verweigert'));
  await page.evaluate(() => document.querySelector('[data-decision-id="dec-2"]').click());
  await settle();
  await page.evaluate(() => document.querySelector('[data-decision-option="b"]').click());
  await settle();
  await settle();
  const cmd = await page.evaluate(() => window.__commands().at(-1));
  assert.equal(cmd.type, 'kundenpipeline.decision.resolve');
  assert.equal(cmd.payload.entscheidung_id, 'dec-2');
  assert.equal(cmd.payload.option_id, 'b');
  assert.equal(cmd.payload.record_id, 'dec-2');
  assert.equal(cmd.status, 'failed');
  let zone = await page.evaluate(() => window.__zone());
  assert.equal(zone.length, 1);
  assert.equal(zone[0].code, 'kundenpipeline.command.kundenpipeline.decision.resolve');
  assert.match(zone[0].text, /Policy verweigert/);
  assert.equal(await page.evaluate(() => Boolean(document.querySelector('#win [role="status"], #win .kundenpipeline-command-status'))), false);
  await page.evaluate(() => document.querySelector('[data-decision-option="a"]').click());
  await settle();
  await settle();
  assert.equal(await page.evaluate(() => window.__commands().at(-1).status), 'done');
  zone = await page.evaluate(() => window.__zone());
  assert.equal(zone.length, 0);
  // Zuordnung: answer-Befehl mit vorgang_id
  await page.evaluate(() => document.querySelector('[data-decision-id="dec-1"]').click());
  await settle();
  await page.evaluate(() => document.querySelector('[data-decision-option="annehmen"]').click());
  await settle();
  const answer = await page.evaluate(() => window.__commands().at(-1));
  assert.equal(answer.type, 'kundenpipeline.decision.answer');
  assert.equal(answer.payload.vorgang_id, 'case-1');
  assert.equal(answer.payload.wert, 'annehmen');
});

test('Schreiben: api.put einer Entscheidung wird angenommen und erscheint live', async () => {
  const result = await page.evaluate(async () => {
    const h = window.__api.put('kundenpipeline_entscheidungen', { id: 'dec-put', titel: 'Per Put', typ: 'triage', status: 'offen', created_at_ms: 40, updated_at_ms: 40 });
    await h.local;
    return h.result;
  });
  assert.equal(result.status, 'accepted');
  await settle();
  assert.match(await listText(), /Per Put/);
});

test('snapshot/restored: Suche, Filter, Auswahl und Aktionsspalte überleben den Remount', async () => {
  await page.evaluate(() => document.querySelector('[data-filter="all"]').click());
  await page.evaluate(() => document.querySelector('[data-decision-id="dec-1"]').click());
  await page.evaluate(() => document.querySelector('[data-toggle-actions]').click());
  await page.fill('[data-search]', 'Projekt');
  await settle();
  const snap = await page.evaluate(async () => {
    const cloned = structuredClone(window.__app.snapshot());
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, { restored: cloned });
    return cloned;
  });
  assert.deepEqual({ selectedId: snap.selectedId, filter: snap.filter, search: snap.search, actionsVisible: snap.actionsVisible }, { selectedId: 'dec-1', filter: 'all', search: 'Projekt', actionsVisible: true });
  await settle();
  const after = await page.evaluate(() => ({
    search: document.querySelector('[data-search]').value,
    snap: window.__app.snapshot(),
    list: document.querySelector('[data-decision-list]').textContent,
  }));
  assert.equal(after.search, 'Projekt');
  assert.equal(after.snap.selectedId, 'dec-1');
  assert.equal(after.snap.filter, 'all');
  assert.equal(after.snap.actionsVisible, true);
  assert.match(after.list, /Projekt zuordnen/);
  assert.doesNotMatch(after.list, /Agent fragt nach/);
  assert.deepEqual(await page.evaluate((subs) => subs.map((c) => window.__c.subscribers(c)), SUBS), [1, 1, 1], 'alte Abos sind abgemeldet');
  await page.fill('[data-search]', '');
  await settle();
  assert.deepEqual(page.__errors, []);
});

test('schmale Fenster (900/420): Liste mit Suche bleibt sichtbar und bedienbar (Rundgang RUND2)', async () => {
  for (const width of [900, 420]) {
    await page.evaluate((w) => { document.getElementById('win').style.width = `${w}px`; }, width);
    await page.waitForTimeout(150);
    const r = await page.evaluate(() => {
      const vis = (sel) => { const el = document.querySelector(`#win ${sel}`); if (!el) return false; const b = el.getBoundingClientRect(); return el.checkVisibility() && b.width > 40 && b.height > 20; };
      return { rail: vis('.kundenpipeline-rail'), search: vis('[data-search]'), list: vis('[data-decision-list]'), detail: vis('.kundenpipeline-detail') };
    });
    assert.deepEqual(r, { rail: true, search: true, list: true, detail: true }, `Breite ${width}`);
  }
  await page.evaluate(() => { document.getElementById('win').style.width = '1280px'; });
});

test('Screenshots 1280 und 420', { skip: !process.env.KUNDENPIPELINE_SHOTS && !process.env.V4_SHOTS }, async () => {
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
  const res = await page.evaluate(async (subs) => {
    await window.__app.unmount();
    await window.__k.sim.push('kundenpipeline_entscheidungen', { id: 'late', titel: 'Late', status: 'offen', created_at_ms: 50, updated_at_ms: 50 });
    await window.__k.sim.settle();
    return { subs: subs.map((c) => window.__c.subscribers(c)), children: document.getElementById('win').children.length };
  }, SUBS);
  assert.deepEqual(res.subs, [0, 0, 0]);
  assert.equal(res.children, 0);
  assert.deepEqual(page.__errors, []);
});
