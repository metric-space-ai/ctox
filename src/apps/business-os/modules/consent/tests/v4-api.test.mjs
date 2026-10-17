// consent gegen den eingefrorenen App-API-4.0-Kern-Stub (createStubKernel) in echtem Chromium.
// Muster: modules/knowledge/tests/v4-api.test.mjs. Stub-Pfad aus CTOX_APP_API_STUB (Pflicht-Pin
// ~/.local/state/shell-v4/stub-pins/64acc2c), read-only unter /__stub/ ausgeliefert.
// Lauf: CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/64acc2c node --test src/apps/business-os/modules/consent/tests/v4-api.test.mjs
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
import { realManifestRoutes } from '../../../v4/tools/port/real-manifest.mjs';

const APP = 'consent';
const STUB = process.env.CTOX_APP_API_STUB || join(homedir(), '.local/state/shell-v4/stub-pins/64acc2c');
const HERE = dirname(fileURLToPath(import.meta.url));
const BOS = normalize(join(HERE, '../../..'));
const require = createRequire(import.meta.url);
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || join(homedir(), 'Documents/ctox/src/apps/business-os/node_modules/playwright'));
const TYPES = { '.js': 'text/javascript', '.mjs': 'text/javascript', '.css': 'text/css', '.html': 'text/html', '.json': 'application/json', '.svg': 'image/svg+xml' };
const COLLECTIONS = ['business_consents'];

const PAGE = `<!doctype html><html lang="de"><head><meta charset="utf-8">
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
  // E26/E27: Befehlsrouten + Deklarationen der echten consent-Version (v4/manifests/consent.manifest.json);
  // ohne aktive Version lehnt der Kern App-Befehle (ats.subject.erase) beim Schreiben mit unknown_command ab.
  await page.evaluate((routes) => { window.__routes = routes; }, realManifestRoutes('consent'));
  await page.evaluate(async (APP) => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    window.__mod = await import(`/modules/${APP}/index.js`);
    const kernel = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false });
    await kernel.ready;
    await kernel.sim.push('business_apps', [{ id: APP, title: 'Einwilligungen' }]);
    // FIXT-R: Eigentümer der Fremd-Collections (echte Manifeste, inkl. ctox-platform) vor der eigenen Version.
    for (const o of window.__routes?._fixture_owners ?? []) if (!(await kernel.sim.inspect('business_apps', o.app))?.data?.active_ver) await kernel.sim.publishAppVersion(o.app, o.ver, o.publish);
    await kernel.sim.publishAppVersion(APP, '1', { title: 'Einwilligungen', manifestExtra: window.__routes });
    await kernel.sim.push('business_consents', [
      { id: 'c-1', subject_id: 'cand-alpha', subject_name: 'Alpha Person', purpose: 'Alpha Talentpool', legal_basis: 'consent', granted_at_ms: Date.now() - 864e5, updated_at_ms: 2 },
      { id: 'c-2', subject_id: 'cand-beta', subject_name: 'Beta Person', purpose: 'Beta Newsletter', legal_basis: 'consent', granted_at_ms: Date.now() - 864e5, withdrawn_at_ms: Date.now() - 3600e3, updated_at_ms: 1 },
    ]);
    kernel.sim.onCommand('ats.consent.check', (payload) => ({ ok: true, allowed: payload.subject_id === 'cand-alpha', purpose: payload.purpose }));
    await kernel.sim.settle();
    window.__k = kernel;
    window.__api = kernel.appApi(APP, '1');
    window.__zone = () => kernel.statusZone.forApp(APP).get().filter((e) => e.source === 'app');
    window.__commands = () => window.__api.query('business_commands', { sort: [{ created_at: 'asc' }] }).get();
  }, APP);
});

after(async () => { await browser?.close(); await new Promise((r) => server?.close(r)); });

const settle = async () => { await page.evaluate(() => window.__k.sim.settle()); await page.waitForTimeout(200); };
const listText = () => page.$eval('[data-ats-list]', (el) => el.textContent);

test('mount: rendert Live-Daten, nur api.*, gescoptes Root, Titel über api.shell', async () => {
  await page.evaluate(async () => {
    const { countingApi } = await import('/v4/tools/port/test-api.mjs');
    window.__c = countingApi(window.__api);
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, {});
  });
  await settle();
  const list = await listText();
  assert.match(list, /Alpha Talentpool/);
  assert.match(list, /Beta Newsletter/);
  assert.equal(await page.$eval('#win', (el) => el.dataset.app), APP);
  assert.equal(await page.evaluate((APP) => window.__k.shell.window(APP).state.get().title, APP), 'Einwilligungen');
  assert.equal(await page.evaluate(() => window.__c.subscribers('business_consents')), 1);
  assert.deepEqual(page.__errors, []);
});

test('Live-Änderung vom Core (sim.push) erscheint ohne Neuladen, Löschung verschwindet', async () => {
  await page.evaluate(() => window.__k.sim.push('business_consents', { id: 'c-live', subject_id: 'cand-gamma', subject_name: 'Gamma Person', purpose: 'Gamma Live', legal_basis: 'consent', granted_at_ms: Date.now(), updated_at_ms: 3 }));
  await settle();
  assert.match(await listText(), /Gamma Live/);
  await page.evaluate(() => window.__k.sim.push('business_consents', { id: 'c-live', _deleted: true }));
  await settle();
  assert.doesNotMatch(await listText(), /Gamma Live/);
});

test('leere Collections: kein Lade-/Leertext, keine Zone', async () => {
  const res = await page.evaluate(async (APP) => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    const k2 = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false, instanceId: 'empty' });
    await k2.ready;
    const host = document.createElement('div');
    document.body.append(host);
    const app = await window.__mod.mount(host, k2.appApi(APP, '1'), {});
    await new Promise((r) => setTimeout(r, 150));
    const listText = host.querySelector('[data-ats-list]').textContent.trim();
    const stateBox = Boolean(host.querySelector('.ctox-syncing, [role="status"]'));
    const zone = k2.statusZone.forApp(APP).get().filter((e) => e.source === 'app').map((e) => e.code);
    await app.unmount();
    host.remove();
    await k2.close();
    return { listText, stateBox, zone };
  }, APP);
  assert.equal(res.listText, '');
  assert.equal(res.stateBox, false);
  assert.deepEqual(res.zone, []);
});

test('Befehl: Einwilligung prüfen → business_commands-Dokument, Ergebnis im Prüfbereich; failed → genau ein Zoneneintrag', async () => {
  await page.fill('[data-ats-form] [name="subject_id"]', 'cand-alpha');
  await page.fill('[data-ats-form] [name="purpose"]', 'Alpha Talentpool');
  await page.evaluate(() => document.querySelector('[data-ats-form]').requestSubmit());
  await settle();
  let cmd = await page.evaluate(() => window.__commands().at(-1));
  assert.equal(cmd.type, 'ats.consent.check');
  assert.equal(cmd.payload.subject_id, 'cand-alpha');
  assert.equal(cmd.status, 'done');
  assert.match(await page.$eval('[data-ats-gate]', (el) => el.textContent), /cand-alpha/);
  await page.evaluate(() => window.__k.sim.failCommand('ats.consent.check', 'kaputt'));
  await page.evaluate(() => document.querySelector('[data-ats-form]').requestSubmit());
  await settle();
  cmd = await page.evaluate(() => window.__commands().at(-1));
  assert.equal(cmd.status, 'failed');
  const zone = await page.evaluate(() => window.__zone());
  assert.equal(zone.length, 1);
  assert.equal(zone[0].code, 'consent.command.ats.consent.check');
  assert.match(zone[0].text, /kaputt/);
});

test('Schreiben: Import über api.put → angenommen, live sichtbar, Meldung in der Zone', async () => {
  const [chooser] = await Promise.all([
    page.waitForEvent('filechooser'),
    page.evaluate(() => document.querySelector('[data-ats-root] [data-action="import"]').click()),
  ]);
  await chooser.setFiles({ name: 'c.json', mimeType: 'application/json', buffer: Buffer.from(JSON.stringify([{ subject_id: 'cand-delta', purpose: 'Delta Import', legal_basis: 'consent' }])) });
  await settle();
  assert.match(await listText(), /Delta Import/);
  const res = await page.evaluate(async () => ({ outbox: (await window.__k.sim.outbox()).length, zone: window.__zone().map((e) => e.code) }));
  assert.equal(res.outbox, 0);
  assert.ok(res.zone.includes('consent.import.done'));
});

test('Art.-17-Löschung nur nach Bestätigung im App-Fenster (api.shell.dialog, destruktiv)', async () => {
  const eraseCount = () => page.evaluate(() => window.__commands().filter((c) => c.type === 'ats.subject.erase').length);
  // Stub-Antwort = spec.default (false): Abbruch, kein Löschbefehl.
  await page.evaluate(() => document.querySelector('[data-context-record-id="c-1"]').click());
  await settle();
  const before = await eraseCount();
  const nDialogs = await page.evaluate(() => window.__k.sim.dialogs.length);
  await page.evaluate(() => document.querySelector('[data-action="rights-erase"]').click());
  await settle();
  const spec = await page.evaluate((n) => window.__k.sim.dialogs.slice(n).map((d) => d.spec)[0], nDialogs);
  assert.equal(spec.kind, 'confirm');
  assert.equal(spec.danger, true);
  assert.equal(spec.default, false);
  assert.match(spec.text, /cand-alpha/);
  assert.equal(await eraseCount(), before, 'ohne Bestätigung kein Löschbefehl');
  // Mit Bestätigung: genau ein Löschbefehl.
  await page.evaluate(async () => {
    const { withDialogAnswer } = await import('/v4/tools/port/test-api.mjs');
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), withDialogAnswer(window.__c.api, true), {});
  });
  await settle();
  await page.evaluate(() => document.querySelector('[data-context-record-id="c-1"]').click());
  await settle();
  await page.evaluate(() => document.querySelector('[data-action="rights-erase"]').click());
  await settle();
  assert.equal(await eraseCount(), before + 1);
  await page.evaluate(async () => {
    await window.__app.unmount();
    await window.__k.sim.push('business_consents', [{ id: 'c-1', subject_id: 'cand-alpha', subject_name: 'Alpha Person', purpose: 'Alpha Talentpool', legal_basis: 'consent', granted_at_ms: Date.now() - 864e5, updated_at_ms: 2 }]);
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, {});
  });
  await settle();
});

test('snapshot/restored: Auswahl und offene Eingabe überleben den Remount', async () => {
  await page.evaluate(() => document.querySelector('[data-context-record-id="c-2"]').click());
  await page.fill('[data-ats-form] [name="purpose"]', 'halb fertiger Zweck');
  const snap = await page.evaluate(async () => {
    const cloned = structuredClone(window.__app.snapshot());
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, { restored: cloned });
    return cloned;
  });
  assert.equal(snap.selectedId, 'c-2');
  assert.equal(snap.form.purpose, 'halb fertiger Zweck');
  assert.ok(JSON.stringify(snap).length < 1024 * 1024);
  await settle();
  const after = await page.evaluate(() => ({
    purpose: document.querySelector('[data-ats-form] [name="purpose"]').value,
    selected: window.__app.snapshot().selectedId,
    detailHidden: document.querySelector('[data-ats-detail]').hidden,
  }));
  assert.equal(after.purpose, 'halb fertiger Zweck');
  assert.equal(after.selected, 'c-2');
  assert.equal(after.detailHidden, false);
  assert.equal(await page.evaluate(() => window.__c.subscribers('business_consents')), 1, 'alte Abos sind abgemeldet');
  assert.deepEqual(page.__errors, []);
});

test('Screenshots 1280/420', async () => {
  await mkdir(join(HERE, 'shots'), { recursive: true });
  await page.screenshot({ path: join(HERE, 'shots', `${APP}-1280.png`) });
  await page.setViewportSize({ width: 420, height: 860 });
  await page.$eval('#win', (el) => { el.style.width = '420px'; el.style.height = '860px'; });
  await page.waitForTimeout(250);
  await page.screenshot({ path: join(HERE, 'shots', `${APP}-420.png`) });
  const overflow = await page.$eval('#win', (el) => el.scrollWidth - el.clientWidth);
  assert.ok(overflow <= 1, `kein horizontaler Überlauf (${overflow}px)`);
  await page.setViewportSize({ width: 1280, height: 760 });
  await page.$eval('#win', (el) => { el.style.width = '1280px'; el.style.height = '760px'; });
});

test('unmount meldet alle Abos ab und leert root', async () => {
  const res = await page.evaluate(async (COLLECTIONS) => {
    await window.__app.unmount();
    await window.__k.sim.push('business_consents', { id: 'c-late', subject_id: 'x', purpose: 'Late', updated_at_ms: 10 });
    await window.__k.sim.settle();
    return { subs: COLLECTIONS.map((c) => window.__c.subscribers(c)), children: document.getElementById('win').children.length };
  }, COLLECTIONS);
  assert.deepEqual(res.subs, COLLECTIONS.map(() => 0));
  assert.equal(res.children, 0);
  assert.deepEqual(page.__errors, []);
});
