// credentials gegen den eingefrorenen App-API-4.0-Kern-Stub (createStubKernel) in echtem Chromium.
// Muster: modules/knowledge/tests/v4-api.test.mjs. Stub-Pfad aus CTOX_APP_API_STUB (Pflicht-Pin
// ~/.local/state/shell-v4/stub-pins/8265bd7), read-only unter /__stub/ ausgeliefert.
// Lauf: CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/8265bd7 node --test src/apps/business-os/modules/credentials/tests/v4-api.test.mjs
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

const APP = 'credentials';
const STUB = process.env.CTOX_APP_API_STUB || join(homedir(), '.local/state/shell-v4/stub-pins/8265bd7');
const HERE = dirname(fileURLToPath(import.meta.url));
const BOS = normalize(join(HERE, '../../..'));
const require = createRequire(import.meta.url);
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || join(homedir(), 'Documents/ctox/src/apps/business-os/node_modules/playwright'));
const TYPES = { '.js': 'text/javascript', '.mjs': 'text/javascript', '.css': 'text/css', '.html': 'text/html', '.json': 'application/json', '.svg': 'image/svg+xml' };
const COLLECTIONS = ['business_commands'];

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
  // FIXT-R: echte Routen/Rechte/Fremd-Collections der credentials-Version (v4/manifests/credentials.manifest.json) plus
  // Eigentümer (ctox-platform für business_commands). Ohne aktive Version lehnt der Kern ctox.secret.* ab.
  await page.evaluate((routes) => { window.__routes = routes; }, realManifestRoutes(APP));
  await page.evaluate(async (APP) => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    window.__mod = await import(`/modules/${APP}/index.js`);
    const kernel = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false });
    await kernel.ready;
    await kernel.sim.push('business_apps', [{ id: APP, title: 'Zugangsdaten' }]);
    for (const o of window.__routes?._fixture_owners ?? []) if (!(await kernel.sim.inspect('business_apps', o.app))?.data?.active_ver) await kernel.sim.publishAppVersion(o.app, o.ver, o.publish);
    await kernel.sim.publishAppVersion(APP, '1', { title: 'Zugangsdaten', manifestExtra: window.__routes });
    window.__secrets = new Map([
      ['OPENAI_API_KEY', { name: 'OPENAI_API_KEY', description: 'Alpha OpenAI Zugang', is_set: true, updated_at: '2026-10-01T10:00:00Z' }],
      ['SMTP_PASSWORD', { name: 'SMTP_PASSWORD', description: 'Beta Mailversand', is_set: false }],
    ]);
    kernel.sim.onCommand('ctox.secret.list', () => ({ catalog: [...window.__secrets.values()].filter((e) => !e.extra), extra: [...window.__secrets.values()].filter((e) => e.extra) }));
    kernel.sim.onCommand('ctox.secret.put', (p) => { window.__secrets.set(p.name, { name: p.name, description: '', is_set: true, extra: !window.__secrets.has(p.name), updated_at: '2026-10-15T10:00:00Z' }); return { ok: true }; });
    kernel.sim.onCommand('ctox.secret.delete', (p) => { const e = window.__secrets.get(p.name); if (e) window.__secrets.set(p.name, { ...e, is_set: false }); return { ok: true }; });
    await kernel.sim.settle();
    window.__k = kernel;
    window.__api = kernel.appApi(APP, '1');
    window.__zone = () => kernel.statusZone.forApp(APP).get().filter((e) => e.source === 'app');
    window.__commands = () => window.__api.query('business_commands', { sort: [{ created_at: 'asc' }] }).get();
  }, APP);
});

after(async () => { await browser?.close(); await new Promise((r) => server?.close(r)); });

const settle = async () => { await page.evaluate(() => window.__k.sim.settle()); await page.waitForTimeout(200); await page.evaluate(() => window.__k.sim.settle()); await page.waitForTimeout(100); };
const listText = () => page.$eval('[data-cred-list]', (el) => el.textContent);
const commands = (type) => page.evaluate((type) => window.__commands().filter((c) => c.type === type), type);

test('mount: Liste kommt als Ergebnis von ctox.secret.list (api.command, gepushter Status), Titel über api.shell', async () => {
  await page.evaluate(async () => {
    const { countingApi } = await import('/v4/tools/port/test-api.mjs');
    window.__c = countingApi(window.__api);
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, {});
  });
  await settle();
  const list = await listText();
  assert.match(list, /OPENAI_API_KEY/);
  assert.match(list, /SMTP_PASSWORD/);
  assert.equal(await page.$eval('#win', (el) => el.dataset.app), APP);
  assert.equal(await page.evaluate((APP) => window.__k.shell.window(APP).state.get().title, APP), 'Zugangsdaten');
  const lists = await commands('ctox.secret.list');
  assert.equal(lists.length, 1, 'genau ein Listenbefehl beim Öffnen');
  assert.equal(lists[0].status, 'done');
  assert.equal(await page.evaluate(() => window.__c.subscribers('business_commands')), 1);
  assert.deepEqual(page.__errors, []);
});

test('Live: put/delete eines anderen Geräts (sim.push in business_commands) löst genau eine Neuliste aus', async () => {
  const before = (await commands('ctox.secret.list')).length;
  await page.evaluate(() => {
    window.__secrets.set('GAMMA_TOKEN', { name: 'GAMMA_TOKEN', description: 'Gamma Live', is_set: true, extra: true });
    return window.__k.sim.push('business_commands', { id: 'peer-put-1', type: 'ctox.secret.put', payload: { name: 'GAMMA_TOKEN' }, status: 'done', app_id: 'credentials' });
  });
  await settle();
  assert.match(await listText(), /GAMMA_TOKEN/);
  assert.equal((await commands('ctox.secret.list')).length, before + 1);
  await page.evaluate(() => { window.__secrets.delete('GAMMA_TOKEN'); return window.__k.sim.push('business_commands', { id: 'peer-del-1', type: 'ctox.secret.delete', payload: { name: 'GAMMA_TOKEN' }, status: 'done', app_id: 'credentials' }); });
  await settle();
  assert.doesNotMatch(await listText(), /GAMMA_TOKEN/);
});

test('leer: keine Geheimnisse → kein Lade-/Leertext, keine Zone', async () => {
  const res = await page.evaluate(async (APP) => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    const k2 = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false, instanceId: 'empty' });
    await k2.ready;
    // FIXT-8265: auch der leere Kern braucht die veröffentlichte credentials-Version (business_commands ist dort deklariert).
    for (const o of window.__routes?._fixture_owners ?? []) if (!(await k2.sim.inspect('business_apps', o.app))?.data?.active_ver) await k2.sim.publishAppVersion(o.app, o.ver, o.publish);
    await k2.sim.publishAppVersion(APP, '1', { title: 'Zugangsdaten', manifestExtra: window.__routes });
    k2.sim.onCommand('ctox.secret.list', () => ({ catalog: [], extra: [] }));
    const host = document.createElement('div');
    document.body.append(host);
    const app = await window.__mod.mount(host, k2.appApi(APP, '1'), {});
    await k2.sim.settle();
    await new Promise((r) => setTimeout(r, 150));
    const listText = host.querySelector('[data-cred-list]').textContent.trim();
    const stateBox = Boolean(host.querySelector('.ctox-syncing, [role="status"], .ctox-empty'));
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

test('Schreiben: Wert setzen → ctox.secret.put, Wert nie im Befehlsdokument, Liste aktualisiert; failed → genau ein Zoneneintrag', async () => {
  await page.evaluate(() => document.querySelector('[data-cred-root] [data-action="new"], [data-credentials-root] [data-action="new"]').click());
  await page.fill('[data-cred-key]', 'DELTA_KEY');
  await page.fill('[data-cred-value]', 'geheim-123');
  await page.evaluate(() => document.querySelector('[data-cred-form]').requestSubmit());
  await settle();
  const put = (await commands('ctox.secret.put')).at(-1);
  assert.equal(put.status, 'done');
  assert.equal(put.payload.name, 'DELTA_KEY');
  assert.equal(put.payload.record_id, 'DELTA_KEY');
  assert.ok(!('value' in put.payload), 'lokaler Befehls-Datensatz ist redigiert');
  assert.match(await listText(), /DELTA_KEY/);
  assert.equal(await page.$eval('[data-cred-value]', (el) => el.value), '', 'Wertfeld bleibt leer');
  // Textdiät: der Write-only-Hinweis ist keine Infobox mehr, sondern Tooltip am Wertfeld.
  assert.match(await page.$eval('[data-cred-value]', (el) => el.title), /nie wieder angezeigt/);
  assert.equal(await page.$$eval('[data-credentials-root] .ctox-callout', (n) => n.length), 0);
  let zone = await page.evaluate(() => window.__zone().map((e) => e.code));
  assert.ok(zone.includes('credentials.secret.saved'), 'Erfolg in der Zustandszone (früher ctx.notifications)');
  await page.evaluate(() => window.__api.status.clear('credentials.secret.saved'));
  await page.evaluate(() => window.__k.sim.failCommand('ctox.secret.put', 'kaputt'));
  await page.fill('[data-cred-value]', 'noch-geheimer');
  await page.evaluate(() => document.querySelector('[data-cred-form]').requestSubmit());
  await settle();
  const failed = (await commands('ctox.secret.put')).at(-1);
  assert.equal(failed.status, 'failed');
  zone = await page.evaluate(() => window.__zone());
  assert.equal(zone.length, 1);
  assert.equal(zone[0].code, 'credentials.command.ctox.secret.put');
  assert.match(zone[0].text, /kaputt/);
  await page.evaluate(() => window.__api.status.clear('credentials.command.ctox.secret.put'));
});

test('Löschen fragt über api.shell.dialog: ohne Ja kein Befehl', async () => {
  const before = (await commands('ctox.secret.delete')).length;
  await page.evaluate(() => document.querySelector('[data-context-record-id="OPENAI_API_KEY"]').click());
  await page.evaluate(() => document.querySelector('[data-cred-detail] [data-action="delete"]').click());
  await settle();
  assert.equal((await commands('ctox.secret.delete')).length, before);
  assert.ok(await page.evaluate(() => window.__k.sim.dialogs.length) >= 1);
});

test('snapshot/restored: Auswahl, Ansicht und eingegebener Schlüssel überleben den Remount (Wert nie)', async () => {
  await page.evaluate(() => document.querySelector('[data-action="new"]').click());
  await page.fill('[data-cred-key]', 'HALB_FERTIG');
  await page.fill('[data-cred-value]', 'nicht-mitnehmen');
  const snap = await page.evaluate(async () => {
    const cloned = structuredClone(window.__app.snapshot());
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, { restored: cloned });
    return cloned;
  });
  assert.equal(snap.keyDraft, 'HALB_FERTIG');
  assert.ok(!JSON.stringify(snap).includes('nicht-mitnehmen'), 'Geheimnis nie im Snapshot');
  await settle();
  assert.equal(await page.$eval('[data-cred-key]', (el) => el.value), 'HALB_FERTIG');
  assert.equal(await page.evaluate(() => window.__c.subscribers('business_commands')), 1, 'alte Abos sind abgemeldet');
  assert.deepEqual(page.__errors, []);
});

test('Screenshots 1280/420', async () => {
  await mkdir(join(HERE, 'shots'), { recursive: true });
  await page.evaluate(() => document.querySelector('[data-context-record-id="OPENAI_API_KEY"]').click());
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
    await window.__k.sim.push('business_commands', { id: 'late', type: 'ctox.secret.put', payload: { name: 'LATE' }, status: 'done' });
    await window.__k.sim.settle();
    return { subs: COLLECTIONS.map((c) => window.__c.subscribers(c)), children: document.getElementById('win').children.length };
  }, COLLECTIONS);
  assert.deepEqual(res.subs, COLLECTIONS.map(() => 0));
  assert.equal(res.children, 0);
  assert.deepEqual(page.__errors, []);
});
