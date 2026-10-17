// file-viewer gegen den eingefrorenen App-API-4.0-Stub (createStubKernel) in echtem Chromium.
// Stub: CTOX_APP_API_STUB (Pflicht-Pin ~/.local/state/shell-v4/stub-pins/4b0d0ac), Muster modules/knowledge.
// Lauf: CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/4b0d0ac node --test src/apps/business-os/modules/file-viewer/tests/v4-api.test.mjs
import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { readFile, mkdir, stat } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { dirname, join, normalize, extname } from 'node:path';
import { homedir } from 'node:os';
import { realManifestRoutes } from '../../../v4/tools/port/real-manifest.mjs';

const APP = 'file-viewer';
const STUB = process.env.CTOX_APP_API_STUB || join(homedir(), '.local/state/shell-v4/stub-pins/4b0d0ac');
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
  // FIXT-R: echte Routen/Rechte/Fremd-Collections von file-viewer und explorer (v4/manifests/*.manifest.json).
  await page.evaluate((routes) => { window.__routes = routes; }, { fv: realManifestRoutes(APP), explorer: realManifestRoutes('explorer') });
  await page.evaluate(async (APP) => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    window.__mod = await import(`/modules/${APP}/index.js`);
    const kernel = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false });
    await kernel.ready;
    await kernel.sim.push('business_apps', [{ id: 'explorer', title: 'Explorer' }, { id: APP, title: 'Datei-Viewer' }]);
    await kernel.sim.settle();
    // FIXT-R: Eigentümer der Fremd-Collections (ctox-platform, desktop, …) vor den eigenen Versionen, danach
    // file-viewer und explorer mit ihren echten Manifest-Teilen (nach den business_apps-Zeilen).
    for (const o of [...window.__routes.fv._fixture_owners, ...window.__routes.explorer._fixture_owners]) if (!(await kernel.sim.inspect('business_apps', o.app))?.data?.active_ver) await kernel.sim.publishAppVersion(o.app, o.ver, o.publish);
    await kernel.sim.publishAppVersion('explorer', '1.0.0', { title: 'Explorer', manifestExtra: window.__routes.explorer });
    await kernel.sim.publishAppVersion(APP, '1.0.0', { title: 'Datei-Viewer', manifestExtra: window.__routes.fv });
    window.__k = kernel;
    window.__api = kernel.appApi(APP, '1.0.0');
    window.__explorer = kernel.appApi('explorer', '1.0.0');
    window.__zone = () => kernel.statusZone.forApp(APP).get().filter((e) => e.source === 'app');
    window.__commands = () => window.__api.query('business_commands', { sort: [{ created_at: 'asc' }] }).get();
    window.__upload = async (text, meta) => {
      const h = window.__explorer.files.put(new Blob([text], { type: meta.mime }), meta);
      await h.local;
      await h.result;
      return h.ref;
    };
  }, APP);
});

after(async () => { await browser?.close(); await new Promise((r) => server?.close(r)); });

const settle = async () => { await page.evaluate(() => window.__k.sim.settle()); await page.waitForTimeout(150); };
const content = () => page.$eval('[data-file-content]', (el) => el.textContent);

test('mount ohne Datei: kein Zustandstext, keine Zone, Titel über api.shell, Abos gezählt', async () => {
  await page.evaluate(async () => {
    const { countingApi } = await import('/v4/tools/port/test-api.mjs');
    window.__c = countingApi(window.__api);
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, {});
  });
  await settle();
  assert.equal(await page.$eval('#win', (el) => el.dataset.app), 'file-viewer');
  assert.equal((await content()).trim(), '');
  assert.equal(await page.evaluate(() => Boolean(document.querySelector('.ctox-syncing, [role="status"], .is-error'))), false);
  assert.deepEqual(await page.evaluate(() => window.__zone()), []);
  assert.equal(await page.evaluate(() => window.__k.shell.window('file-viewer').state.get().title), 'Datei-Viewer');
  // Rundgang RUND1: ohne Datei war „Herunterladen“ klickbar, aber wirkungslos.
  assert.equal(await page.evaluate(() => document.querySelector('#win [data-file-download]')?.disabled), true);
  assert.deepEqual(page.__errors, []);
});

// FIX8 Mangel 3: aus Launcher/Startmenü (ohne Datei) stand nur ein Fehler da. Standard wie Vorschau/Fotos:
// Leerzustand „Keine Datei geöffnet“ + „Datei öffnen…“, Auswahl aus der erlaubten Collection `files`.
test('FIX8: ohne Datei Leerzustand + „Datei öffnen…“; Auswahl aus `files` öffnet die Datei, keine Zone', async () => {
  const visible = (sel) => page.evaluate((sel) => { const el = document.querySelector(sel); return Boolean(el && el.getClientRects().length && !el.closest('[hidden]')); }, sel);
  assert.equal(await visible('#win [data-file-empty]'), true, 'Leerzustand sichtbar');
  assert.equal(await page.$eval('#win [data-file-empty]', (el) => el.textContent.replace(/\s+/g, ' ').trim()), 'Keine Datei geöffnet Datei öffnen…');
  const ref = await page.evaluate(() => window.__upload('Aus der Auswahl', { id: 'f-pick', name: 'auswahl.txt', mime: 'text/plain' }));
  await page.evaluate(() => window.__upload('Andere', { id: 'f-other', name: 'bericht.md', mime: 'text/markdown' }));
  await settle();
  await page.click('#win [data-file-open-picker]');
  await settle();
  assert.equal(await visible('#win [data-file-picker]'), true, 'Auswahl offen');
  const names = () => page.$$eval('#win .file-viewer-picker-item span', (els) => els.map((e) => e.textContent));
  assert.deepEqual((await names()).filter((n) => ['auswahl.txt', 'bericht.md'].includes(n)), ['auswahl.txt', 'bericht.md']);
  await page.fill('#win [data-file-picker-search]', 'auswahl');
  assert.deepEqual(await names(), ['auswahl.txt']);
  await mkdir(join(HERE, 'shots'), { recursive: true });
  await page.screenshot({ path: join(HERE, 'shots', `${APP}-picker-1280.png`) });
  await page.click('#win .file-viewer-picker-item');
  await settle();
  assert.equal(await page.$eval('[data-file-text]', (el) => el.textContent), 'Aus der Auswahl');
  assert.equal(await page.$eval('[data-file-title]', (el) => el.textContent), 'auswahl.txt');
  assert.equal(await visible('#win [data-file-start]'), false, 'Leerzustand weg, sobald eine Datei offen ist');
  assert.equal(await page.evaluate(() => window.__c.subscribers('files')), 0, 'Auswahl-Abo nach der Wahl beendet');
  assert.deepEqual(await page.evaluate(() => window.__zone()), []);
  assert.ok(ref);
});

test('Öffnen per navigate(ref): Inhalt über api.files.get, Kopfzeile und Titel', async () => {
  const ref = await page.evaluate(() => window.__upload('Hallo Viewer', { name: 'notiz.txt', mime: 'text/plain' }));
  await page.evaluate((ref) => window.__explorer.shell.navigate('file-viewer', { ref, name: 'notiz.txt', mime: 'text/plain', size: 12 }), ref);
  await settle();
  assert.equal(await page.$eval('[data-file-text]', (el) => el.textContent), 'Hallo Viewer');
  assert.equal(await page.$eval('[data-file-title]', (el) => el.textContent), 'notiz.txt');
  assert.match(await page.$eval('[data-file-metadata]', (el) => el.textContent), /text\/plain · 12 B/);
  assert.equal(await page.evaluate(() => window.__k.shell.window('file-viewer').state.get().title), 'notiz.txt');
});

test('große Textdatei: Vorschau auf 256 KiB begrenzt, Download liefert alles', async () => {
  const big = 'x'.repeat(300 * 1024);
  const ref = await page.evaluate((big) => window.__upload(big, { name: 'gross.txt', mime: 'text/plain' }), big);
  await page.evaluate((ref) => window.__explorer.shell.navigate('file-viewer', { ref, name: 'gross.txt', mime: 'text/plain', size: 300 * 1024 }), ref);
  await settle();
  assert.equal(await page.$eval('[data-file-text]', (el) => el.textContent.length), 256 * 1024);
  const dl = await Promise.all([page.waitForEvent('download'), page.click('[data-file-download]')]).then(([d]) => d);
  assert.equal(dl.suggestedFilename(), 'gross.txt');
  assert.equal((await stat(await dl.path())).size, 300 * 1024);
});

test('nur file_id: Metadaten kommen live aus `files` (Core-Push), dann wird geladen', async () => {
  const ref = await page.evaluate(() => window.__upload('aus files-Doc', { id: 'f-live', name: 'live.txt', mime: 'text/plain' }));
  await page.evaluate(() => window.__k.sim.push('files', { id: 'f-late', name: 'spaet.txt', mime: 'text/plain', size: 13, ref: '', owner_app: 'explorer' }));
  await settle();
  await page.evaluate(() => window.__explorer.shell.navigate('file-viewer', { file_id: 'f-late', name: 'spaet.txt' }));
  await settle();
  assert.equal((await content()).trim(), '', 'ohne ref nichts gerendert, kein Wartetext');
  await page.evaluate((ref) => window.__k.sim.push('files', { id: 'f-late', name: 'spaet.txt', mime: 'text/plain', size: 13, ref, owner_app: 'explorer' }), ref);
  await settle();
  assert.equal(await page.$eval('[data-file-text]', (el) => el.textContent), 'aus files-Doc');
  assert.equal(await page.evaluate(() => window.__c.subscribers('files')), 0, 'Metadaten-Abo nach Auflösung beendet');
});

test('unbekannte ref: genau ein Zoneneintrag, kein Fehlerkasten im Inhalt', async () => {
  // E30 (Pin 8265bd7): navigate mit ref prüft schon beim Absender die Leseberechtigung; eine unlesbare ref wird mit
  // forbidden abgelehnt und erreicht den Viewer nie (keine Route, kein Zoneneintrag im Viewer).
  const nav = await page.evaluate(async () => {
    const before = window.__k.sim.navigations.length;
    let code = null;
    try { await window.__explorer.shell.navigate('file-viewer', { ref: 'deadbeef', name: 'weg.txt', mime: 'text/plain' }); } catch (e) { code = e?.code ?? String(e); }
    return { code, routed: window.__k.sim.navigations.length - before };
  });
  assert.deepEqual(nav, { code: 'forbidden', routed: 0 });
  await settle();
  assert.deepEqual(await page.evaluate(() => window.__zone()), []);
  // Der Viewer sieht eine unlesbare ref nur noch über einen alten Fensterzustand (restored, Datei inzwischen weg).
  await page.evaluate(async () => {
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, { restored: { target: { ref: 'deadbeef', name: 'weg.txt', mime: 'text/plain' } } });
  });
  await settle();
  const zone = await page.evaluate(() => window.__zone());
  assert.equal(zone.length, 1);
  assert.equal(zone[0].code, 'file-viewer.open');
  assert.equal(zone[0].level, 'error');
  assert.equal((await content()).trim(), '');
});

test('lazy Datei: Befehl ctox.file.materialize ohne Warten; failed → Zone; done mit ref → Inhalt', async () => {
  await page.evaluate(() => window.__k.sim.failCommand('ctox.file.materialize', 'kein Zugriff'));
  await page.evaluate(() => window.__explorer.shell.navigate('file-viewer', { file_id: 'lazy-1', name: 'gross.log', mime: 'text/plain', path: '/CTOX/run/gross.log', content_state: 'lazy' }));
  await settle();
  let cmd = await page.evaluate(() => window.__commands().at(-1));
  assert.equal(cmd.type, 'ctox.file.materialize');
  assert.equal(cmd.payload.file_id, 'lazy-1');
  assert.equal(cmd.payload.path, '/CTOX/run/gross.log');
  assert.equal(cmd.status, 'failed');
  let zone = await page.evaluate(() => window.__zone().filter((e) => e.code === 'file-viewer.command.ctox.file.materialize'));
  assert.equal(zone.length, 1);
  assert.match(zone[0].text, /kein Zugriff/);
  const ref = await page.evaluate(() => window.__upload('materialisiert', { name: 'gross.log', mime: 'text/plain' }));
  await page.evaluate(() => window.__explorer.shell.navigate('file-viewer', { file_id: 'lazy-2', name: 'gross.log', mime: 'text/plain', path: '/CTOX/run/gross.log', content_state: 'lazy' }));
  await settle();
  // ctox.file.materialize ist ein Plattformbefehl: seit f967baf führt der simulierte Core ihn selbst aus (Ergebnis
  // {ok:true}, sim.onCommand greift für Plattformtypen nicht). Das Materialisierungsergebnis {ref} meldet der Core
  // wie im Betrieb über den gepushten Befehlsstatus.
  cmd = await page.evaluate(() => window.__commands().at(-1));
  assert.equal(cmd.payload.file_id, 'lazy-2');
  await page.evaluate(({ cmd, ref }) => window.__k.sim.push('business_commands', { ...cmd, status: 'done', progress: 1, result: { ref } }), { cmd, ref });
  await settle();
  cmd = await page.evaluate(() => window.__commands().at(-1));
  assert.equal(cmd.status, 'done');
  assert.equal(await page.$eval('[data-file-text]', (el) => el.textContent), 'materialisiert');
  zone = await page.evaluate(() => window.__zone().filter((e) => e.code === 'file-viewer.command.ctox.file.materialize'));
  assert.equal(zone.length, 0, 'done räumt den Eintrag');
});

test('Format ohne Vorschau: Inhalt leer, Hinweis nur in der Zone (info)', async () => {
  const ref = await page.evaluate(() => window.__upload('PK', { name: 'archiv.zip', mime: 'application/zip' }));
  await page.evaluate((ref) => window.__explorer.shell.navigate('file-viewer', { ref, name: 'archiv.zip', mime: 'application/zip', size: 2 }), ref);
  await settle();
  assert.equal((await content()).trim(), '');
  const zone = await page.evaluate(() => window.__zone().filter((e) => e.code === 'file-viewer.preview.unsupported'));
  assert.equal(zone.length, 1);
  assert.equal(zone[0].level, 'info');
});

test('snapshot/restored: geöffnete Datei überlebt den Remount; Screenshots 1280/420', async () => {
  const ref = await page.evaluate(() => window.__upload('# Bericht\n\nZeile 1\nZeile 2\n', { name: 'bericht.md', mime: 'text/markdown' }));
  await page.evaluate((ref) => window.__explorer.shell.navigate('file-viewer', { ref, name: 'bericht.md', mime: 'text/markdown', size: 27 }), ref);
  await settle();
  const snap = await page.evaluate(async () => {
    const cloned = structuredClone(window.__app.snapshot());
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, { restored: cloned });
    return cloned;
  });
  assert.equal(snap.target.ref, ref);
  assert.ok(JSON.stringify(snap).length < 1024 * 1024);
  await settle();
  assert.match(await page.$eval('[data-file-text]', (el) => el.textContent), /Zeile 2/);
  assert.equal(await page.$eval('[data-file-title]', (el) => el.textContent), 'bericht.md');
  await mkdir(SHOTS, { recursive: true });
  await page.screenshot({ path: join(SHOTS, 'file-viewer-1280.png') });
  await page.setViewportSize({ width: 420, height: 720 });
  await page.waitForTimeout(100);
  await page.screenshot({ path: join(SHOTS, 'file-viewer-420.png') });
  const overflow = await page.evaluate(() => document.getElementById('win').scrollWidth > window.innerWidth + 1);
  assert.equal(overflow, false, 'kein horizontaler Überlauf bei 420');
  await page.setViewportSize({ width: 1280, height: 720 });
  assert.deepEqual(page.__errors, []);
});

test('unmount meldet alle Abos ab und leert root', async () => {
  const res = await page.evaluate(async () => {
    await window.__app.unmount();
    return { files: window.__c.subscribers('files'), cmds: window.__c.subscribers('business_commands'), children: document.getElementById('win').children.length };
  });
  assert.deepEqual(res, { files: 0, cmds: 0, children: 0 });
  assert.deepEqual(page.__errors, []);
});
