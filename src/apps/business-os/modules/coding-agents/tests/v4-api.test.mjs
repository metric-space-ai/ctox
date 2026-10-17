// coding-agents (Strang AGENT: Übersicht aller Agent-Läufe aus coding_turns, Klick → Code-Ansicht der App) gegen den
// verbindlichen App-API-4.0-Stub des Kerns (createStubKernel, Stub-Pin, read-only unter /__stub/) in echtem Chromium.
// coding_turns simuliert kein Stub-Pin als Kern-Collection; die Zeilen kommen per sim.push (Form wie draft_turn.rs).
// Lauf: CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/4b0d0ac \
//       node --test src/apps/business-os/modules/coding-agents/tests/v4-api.test.mjs
import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { dirname, join, normalize, extname } from 'node:path';
import { homedir } from 'node:os';
import { realManifestRoutes } from '../../../v4/tools/port/real-manifest.mjs';

const STUB = process.env.CTOX_APP_API_STUB || join(homedir(), '.local/state/shell-v4/stub-pins/4b0d0ac');
const BOS = normalize(join(dirname(fileURLToPath(import.meta.url)), '../../..'));
const require = createRequire(import.meta.url);
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || join(homedir(), 'Documents/ctox/src/apps/business-os/node_modules/playwright'));
const TYPES = { '.js': 'text/javascript', '.mjs': 'text/javascript', '.css': 'text/css', '.html': 'text/html', '.json': 'application/json', '.svg': 'image/svg+xml' };

const PAGE = `<!doctype html><html><head><meta charset="utf-8">
<link rel="stylesheet" href="/modules/coding-agents/index.css">
</head><body><div id="win" style="width:1280px;height:820px"></div></body></html>`;

const TURN = 'ctox.coding.turn';
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
      // Wie der Service Worker (§16.15): ctox:shared/x.js → Shell-Pfad umschreiben.
      if (/\.m?js$/.test(file)) body = Buffer.from(body.toString('utf8').replace(/(['"])ctox:shared\//g, '$1/shared/'));
      res.writeHead(200, { 'content-type': TYPES[extname(file)] || 'application/octet-stream' }); res.end(body);
    } catch { res.writeHead(404); res.end(); }
  });
  await new Promise((r) => server.listen(0, '127.0.0.1', r));
  base = `http://127.0.0.1:${server.address().port}/`;
  browser = await chromium.launch();
  page = await browser.newPage();
  const errors = [];
  page.on('pageerror', (e) => errors.push(e.message));
  page.on('console', (m) => { if (m.type() === 'error') console.error('[page]', m.text()); });
  page.__errors = errors;
  await page.goto(base);
  // FIXT-8265 (E48): echte Deklarationen der coding-agents-Version (v4/manifests/coding-agents.manifest.json: business_apps,
  // business_commands, coding_turns von ctox-platform); ohne veröffentlichte Version sperrt der Kern Plattform-Collections.
  await page.evaluate((routes) => { window.__routes = routes; }, realManifestRoutes('coding-agents'));
  await page.evaluate(async () => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    window.__mod = await import('/modules/coding-agents/index.js');
    const kernel = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false, actor: { role: 'admin', permissions: ['apps.source.edit'] } });
    await kernel.ready;
    await kernel.sim.push('business_apps', [
      { id: 'notes', title: 'Notes', updated_at_ms: 2 },
      { id: 'invoices', title: 'Invoices', updated_at_ms: 1 },
    ]);
    for (const o of window.__routes?._fixture_owners ?? []) if (!(await kernel.sim.inspect('business_apps', o.app))?.data?.active_ver) await kernel.sim.publishAppVersion(o.app, o.ver, o.publish);
    await kernel.sim.publishAppVersion('coding-agents', '0.2.6', { title: 'Coding-Agenten', manifestExtra: window.__routes });
    const now = Date.now();
    await kernel.sim.push('coding_turns', [
      { id: 't-old', app_id: 'invoices', draft_id: 'u/invoices', user_id: 'u', command_id: 'cmd-t-old', state: 'done', created_at_ms: now - 60000, updated_at_ms: now - 50000, finished_at_ms: now - 50000, changed_files: ['index.js', 'a.js'], messages: [{ role: 'user', text: 'Rechnungsnummer fett', at_ms: now - 60000 }, { role: 'assistant', text: 'ok', at_ms: now - 50000 }] },
      { id: 't-run', app_id: 'notes', draft_id: 'u/notes', user_id: 'u', command_id: 'cmd-t-run', state: 'running', created_at_ms: now - 120000, updated_at_ms: now - 110000, changed_files: [], messages: [{ role: 'user', text: 'Suche nach Datum', at_ms: now - 120000 }] },
      { id: 't-new', app_id: 'notes', draft_id: 'u/notes', user_id: 'u', command_id: 'cmd-t-new', state: 'conflict', created_at_ms: now - 1000, updated_at_ms: now - 500, finished_at_ms: now - 500, changed_files: ['index.js'], messages: [{ role: 'user', text: 'Titel kürzen\nund mehr', at_ms: now - 1000 }], patch: { ref: 'a'.repeat(64), file_id: 'coding-patch:t-new' } },
    ]);
    await kernel.sim.settle();
    const api = kernel.appApi('coding-agents', '0.2.6');
    const opened = [];
    // api.shell.openSource mitschneiden (die Shell öffnet dort die Code-Ansicht mit Agent-Seitenleiste).
    const routes = new Set();
    window.__route = (params) => { for (const cb of routes) cb({ from: 'news', to: 'coding-agents', params }); };
    window.__api = { ...api, shell: { ...api.shell, openSource: (id) => { opened.push(id); return true; }, onRoute: (cb) => { routes.add(cb); return () => routes.delete(cb); } } };
    window.__opened = opened;
    window.__k = kernel;
    window.__zone = () => kernel.statusZone.forApp('coding-agents').get().filter((e) => e.source === 'app');
  });
});

after(async () => { await browser?.close(); await new Promise((r) => server?.close(r)); });

const settle = async () => { await page.evaluate(() => window.__k.sim.settle()); await page.waitForTimeout(200); };
const text = (sel) => page.$eval(sel, (el) => el.textContent);


const runs = () => page.$$eval('#win [data-ca-run]', (els) => els.map((b) => ({ id: b.dataset.caRun, app: b.dataset.app, state: b.dataset.state, text: b.textContent })));

test('mount: Läufe aus coding_turns, laufende zuerst, dann neueste; App-Titel statt IDs; Standard-Pane-Kopf ohne eigenes Icon', async () => {
  await page.evaluate(async () => {
    const { countingApi } = await import('/v4/tools/port/test-api.mjs');
    window.__c = countingApi(window.__api);
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, {});
  });
  await settle();
  const r = await runs();
  assert.deepEqual(r.map((x) => x.id), ['t-run', 't-new', 't-old']);
  assert.match(r[0].text, /Notes/);
  assert.match(r[0].text, /Suche nach Datum/);
  assert.match(r[2].text, /Invoices.*2 Dateien/);
  assert.doesNotMatch(r[1].text, /und mehr/); // nur erste Zeile des Auftrags
  assert.equal(await page.$eval('#win', (el) => el.dataset.app), 'coding-agents');
  assert.equal(await page.evaluate(() => window.__k.shell.window('coding-agents').state.get().title), 'Coding-Agenten');
  assert.equal(await page.evaluate(() => window.__c.subscribers('coding_turns')), 1);
  assert.equal(await page.evaluate(() => window.__c.subscribers('business_apps')), 1);
  const head = await page.$eval('#win', (w) => ({
    title: w.querySelector('.ctox-pane-header .ctox-pane-title')?.textContent,
    ownMark: !!w.querySelector('.coding-agents-rail-mark, .coding-agents-rail-spacer, .ctox-pane-header svg, .ctox-pane-header img'),
    stateText: [...w.querySelectorAll('[data-ca-run]')].some((b) => /läuft|fertig|Patch bereit/.test(b.textContent)),
  }));
  assert.deepEqual(head, { title: 'Agent-Läufe', ownMark: false, stateText: false });
  assert.deepEqual(page.__errors, []);
});

test('Klick auf einen Lauf öffnet die Code-Ansicht der betreffenden App (api.shell.openSource), kein navigate', async () => {
  await page.click('#win [data-ca-run="t-old"]');
  await page.click('#win [data-ca-run="t-run"]');
  assert.deepEqual(await page.evaluate(() => window.__opened.slice()), ['invoices', 'notes']);
});

test('Live: neuer Lauf erscheint oben, Zustandswechsel und Löschung ohne Neuladen', async () => {
  await page.evaluate(() => window.__k.sim.push('coding_turns', { id: 't-live', app_id: 'invoices', draft_id: 'u/invoices', user_id: 'u', command_id: 'cmd-t-live', state: 'queued', created_at_ms: Date.now(), updated_at_ms: Date.now(), messages: [{ role: 'user', text: 'Neu', at_ms: Date.now() }] }));
  await settle();
  let r = await runs();
  assert.equal(r[0].id, 't-live');
  await page.evaluate(() => window.__k.sim.push('coding_turns', [{ id: 't-run', app_id: 'notes', draft_id: 'u/notes', user_id: 'u', command_id: 'cmd-t-run', state: 'done', created_at_ms: Date.now() - 120000, updated_at_ms: Date.now(), finished_at_ms: Date.now(), changed_files: ['x.js'], messages: [{ role: 'user', text: 'Suche nach Datum', at_ms: Date.now() - 120000 }] }, { id: 't-live', _deleted: true }]));
  await settle();
  r = await runs();
  assert.equal(r.some((x) => x.id === 't-live'), false);
  assert.equal(r.find((x) => x.id === 't-run').state, 'done');
  assert.deepEqual(r.map((x) => x.id), ['t-new', 't-old', 't-run']);
});

test('Suche und Deep-Link {app}: filtern; snapshot/restored behält beides', async () => {
  await page.fill('#win [data-ca-search]', 'rechnung');
  assert.deepEqual((await runs()).map((x) => x.id), ['t-old']);
  await page.fill('#win [data-ca-search]', '');
  await page.evaluate(() => window.__route({ app: 'notes' }));
  await settle();
  const filtered = (await runs()).map((x) => x.app);
  assert.ok(filtered.length >= 1 && filtered.every((a) => a === 'notes'), JSON.stringify(filtered));
  assert.match(await text('#win [data-ca-title]'), /Agent-Läufe · Notes/);
  const snap = await page.evaluate(() => window.__app.snapshot());
  assert.deepEqual(snap, { search: '', app: 'notes' });
  await page.evaluate(async (s) => { await window.__app.unmount(); window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, { restored: s }); }, snap);
  await settle();
  assert.ok((await runs()).every((x) => x.app === 'notes'));
  // Start-Route (z. B. aus „Bugs & Features“): {app} gewinnt gegen den Snapshot; Altform {moduleId}.
  await page.evaluate(async () => { await window.__app.unmount(); window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, { restored: { app: 'notes' }, route: { params: { moduleId: 'invoices' } } }); });
  await settle();
  assert.deepEqual((await runs()).map((x) => x.app), ['invoices']);
  await page.evaluate(() => window.__route({}));
  await settle();
  assert.ok((await runs()).length >= 3);
});

test('leer: ein Satz, was hier erscheint; keine Zone, kein Zustandskasten', async () => {
  const empty = await page.evaluate(async () => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    const k2 = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false, instanceId: 'empty' });
    await k2.ready;
    // FIXT-8265: auch der leere Kern braucht die veröffentlichte coding-agents-Version (Fremdlesen deklariert).
    for (const o of window.__routes?._fixture_owners ?? []) if (!(await k2.sim.inspect('business_apps', o.app))?.data?.active_ver) await k2.sim.publishAppVersion(o.app, o.ver, o.publish);
    await k2.sim.publishAppVersion('coding-agents', '0.2.6', { title: 'Coding-Agenten', manifestExtra: window.__routes });
    const host = document.createElement('div');
    document.body.append(host);
    const app = await window.__mod.mount(host, k2.appApi('coding-agents', '0.2.6'), {});
    await new Promise((r) => setTimeout(r, 300));
    const out = { text: host.querySelector('[data-ca-empty]').textContent, hidden: host.querySelector('[data-ca-empty]').hidden,
      box: Boolean(host.querySelector('.ctox-syncing, [role="status"]')), zone: k2.statusZone.forApp('coding-agents').get().map((e) => e.code) };
    await app.unmount();
    host.remove();
    await k2.close();
    return out;
  });
  assert.equal(empty.hidden, false);
  assert.match(empty.text, /^Noch keine Läufe – Start über „Code ansehen“/);
  assert.doesNotMatch(empty.text, /wird geladen|synchronisier|loading|syncing/i);
  assert.equal(empty.box, false);
  assert.deepEqual(empty.zone, []);
});

test('unmount: meldet alle Abos ab und leert root', async () => {
  await page.evaluate(() => window.__app.unmount());
  assert.equal(await page.evaluate(() => window.__c.subscribers('coding_turns')), 0);
  assert.equal(await page.evaluate(() => window.__c.subscribers('business_apps')), 0);
  assert.equal(await page.$eval('#win', (el) => el.children.length), 0);
  assert.deepEqual(page.__errors, []);
});
