// desktop gegen den eingefrorenen App-API-4.0-Stub (createStubKernel) in echtem Chromium.
// Stub: CTOX_APP_API_STUB (Pflicht-Pin ~/.local/state/shell-v4/stub-pins/64acc2c), Muster modules/knowledge.
// Lauf: CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/64acc2c node --test src/apps/business-os/modules/desktop/tests/v4-api.test.mjs
import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { readFile, mkdir, stat } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { dirname, join, normalize, extname } from 'node:path';
import { homedir } from 'node:os';
import { realManifestRoutes } from '../../../v4/tools/port/real-manifest.mjs';

const APP = 'desktop';
const STUB = process.env.CTOX_APP_API_STUB || join(homedir(), '.local/state/shell-v4/stub-pins/64acc2c');
const HERE = dirname(fileURLToPath(import.meta.url));
const BOS = normalize(join(HERE, '../../..'));
const SHOTS = join(HERE, 'shots');
const require = createRequire(import.meta.url);
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || join(homedir(), 'Documents/ctox/src/apps/business-os/node_modules/playwright'));
const TYPES = { '.js': 'text/javascript', '.mjs': 'text/javascript', '.css': 'text/css', '.html': 'text/html', '.json': 'application/json', '.svg': 'image/svg+xml' };

const PAGE = `<!doctype html><html><head><meta charset="utf-8">
<link rel="stylesheet" href="/app.css"><link rel="stylesheet" href="/shared/base.css">
<link rel="stylesheet" href="/modules/${APP}/index.css">
<style>html,body{margin:0;height:100%;overflow:hidden;background:var(--bg)} #win{width:100vw;height:100vh;container:business-app-window/inline-size;--shell-v2-icon-size:28px}</style>
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
      // Wie der Service Worker (§16.15): ctox:shared/x.js → Shell-Pfad (PORTING.md Falle 2).
      if (/\.m?js$/.test(file)) body = Buffer.from(body.toString('utf8').replace(/(['"])ctox:shared\//g, '$1/shared/'));
      res.writeHead(200, { 'content-type': TYPES[extname(file)] || 'application/octet-stream' }); res.end(body);
    } catch { res.writeHead(404); res.end(); }
  });
  await new Promise((r) => server.listen(0, '127.0.0.1', r));
  base = `http://127.0.0.1:${server.address().port}/`;
  browser = await chromium.launch();
  page = await browser.newPage({ viewport: { width: 1280, height: 720 } });
  const errors = [];
  page.on('pageerror', (e) => errors.push(e.message));
  page.on('console', (m) => { if (m.type() === 'error') console.error('[page]', m.text()); });
  page.__errors = errors;
  await page.goto(base);
  // FIXT-R: echte Routen/Rechte/Collections/Fremd-Collections von desktop (v4/manifests/desktop.manifest.json).
  // Ziel-App ctox (commands_in ctox.chat.task von '*') ebenfalls mit echtem Manifest: Querbefehle brauchen seit E39 eine
  // aktive Zielversion mit commands_in + handler.
  await page.evaluate((routes) => { window.__routes = routes; }, { ...realManifestRoutes(APP), _ctox: realManifestRoutes('ctox') });
  await page.evaluate(async (APP) => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    window.__mod = await import(`/modules/${APP}/index.js`);
    const kernel = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false });
    await kernel.ready;
    await kernel.sim.push('business_apps', [{ id: 'desktop', title: 'Desktop' }, { id: 'explorer', title: 'Explorer' }, { id: 'notes', title: 'Notizen' }, { id: 'ctox', title: 'CTOX' },
      { id: 'lib:ctox-office', title: 'lib:ctox-office@1.0' }]);
    // FIXT-R: Eigentümer der Fremd-Collections (ctox-platform, documents, notes) vor der eigenen Version, diese nach der
    // business_apps-Zeile.
    for (const o of window.__routes?._fixture_owners ?? []) if (!(await kernel.sim.inspect('business_apps', o.app))?.data?.active_ver) await kernel.sim.publishAppVersion(o.app, o.ver, o.publish);
    const { _ctox, ...routes } = window.__routes;
    await kernel.sim.publishAppVersion('ctox', '1.0.0', { title: 'CTOX', manifestExtra: _ctox });
    await kernel.sim.publishAppVersion(APP, '1.0.0', { title: 'Desktop', manifestExtra: routes });
    await kernel.sim.push('desktop_icons', [{ id: 'custom-notes', target_type: 'module', target_module: 'notes', label: 'Meine Notizen', x: 24, y: 24, hidden: false, sort_index: 0, updated_at_ms: 1 }]);
    await kernel.sim.settle();
    window.__k = kernel;
    window.__api = kernel.appApi(APP, '1.0.0');

    window.__zone = () => kernel.statusZone.forApp(APP).get().filter((e) => e.source === 'app');
    window.__commands = () => window.__api.query('business_commands', { sort: [{ created_at: 'asc' }] }).get();
    window.__upload = async (text, meta) => {
      return null;
    };
  }, APP);
});

after(async () => { await browser?.close(); await new Promise((r) => server?.close(r)); });

const settle = async () => { await page.evaluate(() => window.__k.sim.settle()); await page.waitForTimeout(250); };
const labels = () => page.$$eval('.desktop-icon .desktop-icon-label', (els) => els.map((e) => e.textContent));
const icons = () => page.evaluate(() => window.__api.query('desktop_icons', {}).get());

test('mount: Icons live aus desktop_icons, Standard-Icons für business_apps angelegt, Layout-Dokument, keine Zustandstexte', async () => {
  await page.evaluate(async () => {
    const { countingApi } = await import('/v4/tools/port/test-api.mjs');
    window.__c = countingApi(window.__api);
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, {});
  });
  await settle();
  const l = await labels();
  assert.ok(l.includes('Meine Notizen'));
  assert.ok(l.includes('Explorer'));
  assert.ok(l.includes('CTOX Backend'));
  assert.ok(!l.includes('Desktop'), 'eigene App nicht als Icon');
  const ids = (await icons()).map((d) => d.id);
  for (const id of ['desk_icon_explorer', 'desk_icon_notes', 'desk_icon_ctox']) assert.ok(ids.includes(id), id);
  // Plattform (ctox-platform, vom Kern veröffentlicht) und Bibliotheken (lib:*) sind keine Apps → kein Icon.
  for (const id of ['desk_icon_ctox-platform', 'desk_icon_lib:ctox-office']) assert.ok(!ids.includes(id), id);
  assert.ok(!l.some((t) => /^lib:|ctox-platform/.test(t)), JSON.stringify(l));
  const layout = await page.evaluate(() => window.__api.get('desktop_layout', 'layout').get());
  assert.equal(layout.grid_cell_w, 120);
  assert.equal(await page.evaluate(() => Boolean(document.querySelector('.ctox-syncing, [role="status"], [data-widget-sync], [data-widget-status], .desktop-icon-empty'))), false);
  assert.match(await page.$eval('[data-widget-time]', (el) => el.textContent), /\d{1,2}:\d{2}/);
  assert.deepEqual(await page.evaluate(() => window.__zone()), []);
  assert.equal(await page.evaluate(() => window.__k.shell.window('desktop').state.get().title), 'Desktop');
  assert.deepEqual(page.__errors, []);
});

test('Live-Änderung (sim.push) erscheint, verstecktes Icon verschwindet', async () => {
  await page.evaluate(() => window.__k.sim.push('desktop_icons', { id: 'custom-notes', target_type: 'module', target_module: 'notes', label: 'Notizen live', x: 24, y: 24, hidden: false, sort_index: 0, updated_at_ms: 5 }));
  await settle();
  assert.ok((await labels()).includes('Notizen live'));
  await page.evaluate(() => window.__k.sim.push('desktop_icons', { id: 'custom-notes', target_type: 'module', target_module: 'notes', label: 'Notizen live', hidden: true, updated_at_ms: 6 }));
  await settle();
  assert.ok(!(await labels()).includes('Notizen live'));
});

test('Öffnen per Enter → api.shell.navigate(target)', async () => {
  await page.evaluate(() => [...document.querySelectorAll('.desktop-icon')].find((e) => e.dataset.target === 'explorer').dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter', bubbles: true })));
  await settle();
  const nav = await page.evaluate(() => window.__k.sim.navigations.at(-1));
  assert.equal(nav.to, 'explorer');
});

test('Kontextmenü über api.shell.menu: Icon entfernen schreibt hidden über api.patch; Kontext-Chat → ctox.chat.task + navigate(ctox) (SKL-D)', async () => {
  const menuApi = (answer, dialogAnswer = null) => page.evaluate(async ([answer, dialogAnswer]) => {
    const base = window.__c.api;
    const bind = (o) => Object.fromEntries(Object.keys(o).map((k) => [k, typeof o[k] === 'function' ? o[k].bind(o) : o[k]]));
    const shell = Object.freeze({ ...bind(base.shell), menu: (spec) => { base.shell.menu(spec); return Promise.resolve(answer); }, ...(dialogAnswer ? { dialog: () => Promise.resolve(dialogAnswer) } : {}) });
    const snap = structuredClone(window.__app.snapshot());
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), Object.freeze({ ...bind(base), shell }), { restored: snap });
  }, [answer, dialogAnswer]);
  await menuApi('delete-icon');
  await settle();
  await page.evaluate(() => [...document.querySelectorAll('.desktop-icon')].find((e) => e.dataset.target === 'ctox').dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: 50, clientY: 50 })));
  await settle();
  assert.equal((await icons()).find((d) => d.id === 'desk_icon_ctox').hidden, true);
  assert.ok(!(await labels()).includes('CTOX Backend'));
  const menus = await page.evaluate(() => window.__k.sim.dialogs.filter((d) => d.kind === 'menu').length);
  assert.ok(menus >= 1);
  await menuApi('ask', 'Wofür ist der Explorer?');
  await settle();
  await page.evaluate(() => [...document.querySelectorAll('.desktop-icon')].find((e) => e.dataset.target === 'explorer').dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true, clientX: 50, clientY: 50 })));
  await settle();
  // Baustein ctox:shared/context-chat.js: Standardbefehl der Chat-App ctox, dann Sprung in den Chat.
  const cmd = await page.evaluate(() => window.__api.query('business_commands', { sort: [{ created_at: 'asc' }] }).get().at(-1));
  assert.equal(cmd.type, 'ctox.chat.task');
  assert.equal(cmd.payload.source_app, 'desktop');
  assert.equal(cmd.payload.mode, 'ask');
  assert.equal(cmd.payload.text, 'Wofür ist der Explorer?');
  assert.equal(cmd.payload.context.desktop_icon.target_module, 'explorer');
  const nav = await page.evaluate(() => window.__k.sim.navigations.at(-1));
  assert.equal(nav.to, 'ctox');
  assert.equal(nav.params.command_id, cmd.id);
  assert.equal(nav.params.mode, 'ask');
});

test('Befehl ctox.module.save (App umbenennen) ohne Warten; failed → genau ein Zoneneintrag', async () => {
  await page.evaluate(() => window.__k.sim.push('business_apps', { id: 'notes', title: 'Notizen', entry: 'installed-modules/notes/index.html', source: 'installed', collections: [] }));
  await page.evaluate(() => window.__k.sim.failCommand('ctox.module.save', 'verweigert'));
  await page.evaluate(async () => {
    const base = window.__c.api;
    const bind = (o) => Object.fromEntries(Object.keys(o).map((k) => [k, typeof o[k] === 'function' ? o[k].bind(o) : o[k]]));
    const shell = Object.freeze({ ...bind(base.shell), menu: () => Promise.resolve('rename-app'), dialog: () => Promise.resolve('Notizbuch') });
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), Object.freeze({ ...bind(base), shell }), {});
  });
  await settle();
  await page.evaluate(() => [...document.querySelectorAll('.desktop-icon')].find((e) => e.dataset.target === 'notes').dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true })));
  await settle();
  const cmd = await page.evaluate(() => window.__api.query('business_commands', { sort: [{ created_at: 'asc' }] }).get().at(-1));
  assert.equal(cmd.type, 'ctox.module.save');
  assert.equal(cmd.payload.title, 'Notizbuch');
  assert.equal(cmd.payload.record_id, 'notes');
  assert.equal(cmd.status, 'failed');
  const zone = await page.evaluate(() => window.__zone().filter((e) => e.code === 'desktop.command.ctox.module.save'));
  assert.equal(zone.length, 1);
  assert.match(zone[0].text, /verweigert/);
  assert.equal((await icons()).find((d) => d.id === 'desk_icon_notes').label, 'Notizbuch', 'Icon-Beschriftung lokal sofort');
});

// Rundgang RUND1: über den Launcher geöffnet (Fenster wächst aus der Kachel) lagen alle Icons dauerhaft auf 12/12 –
// das Raster wurde an der winzigen Anfangsfläche berechnet, die Höhe geklemmt und gespeichert.
test('Fläche wächst nach dem Mount (Öffnen-Animation): Icons liegen nicht übereinander', async () => {
  const res = await page.evaluate(async () => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    const k = createStubKernel({ storage: 'memory', latencyMs: 2, multiTab: false, instanceId: 'grow-' + Date.now() });
    await k.ready;
    const apps = Array.from({ length: 24 }, (_, i) => ({ id: `app-${String(i).padStart(2, '0')}`, title: `App ${i}` }));
    await k.sim.push('business_apps', [{ id: 'desktop', title: 'Desktop' }, ...apps]);
    // E48: auch dieser Kern braucht die veröffentlichte desktop-Version (foreign_collections business_apps u. a.).
    const { _ctox, ...routes } = window.__routes;
    for (const o of routes._fixture_owners ?? []) if (!(await k.sim.inspect('business_apps', o.app))?.data?.active_ver) await k.sim.publishAppVersion(o.app, o.ver, o.publish);
    await k.sim.publishAppVersion('desktop', '1.0.0', { title: 'Desktop', manifestExtra: routes });
    await k.sim.settle();
    const host = document.createElement('div');
    host.style.cssText = 'position:fixed;left:0;top:0;width:40px;height:40px;container:business-app-window/inline-size;overflow:hidden';
    document.body.append(host);
    const api = k.appApi('desktop', '1.0.0');
    const app = await window.__mod.mount(host, api, {});
    await k.sim.settle();
    await new Promise((r) => setTimeout(r, 150));
    host.style.width = '1200px'; host.style.height = '640px';
    await new Promise((r) => setTimeout(r, 300));
    await k.sim.settle();
    await new Promise((r) => setTimeout(r, 200));
    const pos = [...host.querySelectorAll('.desktop-icon')].map((e) => `${e.style.left}/${e.style.top}`);
    await app.unmount();
    host.remove();
    return { n: pos.length, distinct: new Set(pos).size };
  });
  assert.ok(res.n >= 20, JSON.stringify(res));
  assert.equal(res.distinct, res.n, `alle Icons an eigener Stelle (${JSON.stringify(res)})`);
});

test('snapshot/restored: Auswahl und Sicherung überleben den Remount; Screenshots; unmount räumt', async () => {
  await page.evaluate(async () => {
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, { restored: { v: 1, selectedIconId: 'desk_icon_explorer', resetBackup: [{ id: 'x', target_type: 'module', updated_at_ms: 1 }] } });
  });
  await settle();
  const snap = await page.evaluate(() => structuredClone(window.__app.snapshot()));
  assert.equal(snap.selectedIconId, 'desk_icon_explorer');
  assert.equal(snap.resetBackup.length, 1);
  assert.equal(await page.evaluate(() => document.querySelector('.desktop-icon.selected')?.dataset.iconId), 'desk_icon_explorer');
  await mkdir(SHOTS, { recursive: true });
  await page.screenshot({ path: join(SHOTS, 'desktop-1280.png') });
  await page.setViewportSize({ width: 420, height: 720 });
  await page.waitForTimeout(150);
  await page.screenshot({ path: join(SHOTS, 'desktop-420.png') });
  const overflow = await page.evaluate(() => document.getElementById('win').scrollWidth > window.innerWidth + 1);
  assert.equal(overflow, false);
  await page.setViewportSize({ width: 1280, height: 720 });
  const res = await page.evaluate(async () => {
    await window.__app.unmount();
    return { subs: ['desktop_icons', 'desktop_layout', 'business_apps'].map((c) => window.__c.subscribers(c)), children: document.getElementById('win').children.length };
  });
  assert.deepEqual(res, { subs: [0, 0, 0], children: 0 });
  assert.deepEqual(page.__errors, []);
});
