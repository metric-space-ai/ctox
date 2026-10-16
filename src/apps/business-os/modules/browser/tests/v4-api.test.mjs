// browser gegen den verbindlichen App-API-4.0-Stub des Kerns (createStubKernel, gepinnt über CTOX_APP_API_STUB,
// read-only unter /__stub/ ausgeliefert) in echtem Chromium. Muster: modules/knowledge/tests/v4-api.test.mjs.
// Lauf: CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/68b83a3 node --test src/apps/business-os/modules/browser/tests/v4-api.test.mjs
// Playwright: aus PLAYWRIGHT_MODULE, sonst ~/Documents/ctox/src/apps/business-os/node_modules/playwright.
// SHOTS=1 legt zusätzlich Bildschirmfotos in 1280 und 420 nach tests/shots/.
import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { readFile, mkdir } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { dirname, join, normalize, extname } from 'node:path';
import { homedir } from 'node:os';
import { realManifestRoutes } from '../../../v4/tools/port/real-manifest.mjs';

const STUB = process.env.CTOX_APP_API_STUB || join(homedir(), '.local/state/shell-v4/stub-pins/68b83a3');
const HERE = dirname(fileURLToPath(import.meta.url));
const BOS = normalize(join(HERE, '../../..'));
const require = createRequire(import.meta.url);
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || join(homedir(), 'Documents/ctox/src/apps/business-os/node_modules/playwright'));
const TYPES = { '.js': 'text/javascript', '.mjs': 'text/javascript', '.css': 'text/css', '.html': 'text/html', '.json': 'application/json', '.svg': 'image/svg+xml' };
const PNG = 'iVBORw0KGgoAAAANSUhEUgAAAAQAAAAECAIAAAAmkwkpAAAAEElEQVR4nGMQqTgBRwzEcQAdkhVBSiHpUQAAAABJRU5ErkJggg==';

const PAGE = `<!doctype html><html><head><meta charset="utf-8">
<link rel="stylesheet" href="/ui-contract/v1/workjet-ui-contract.css">
<link rel="stylesheet" href="/app.css">
<link rel="stylesheet" href="/shared/base.css">
<link rel="stylesheet" href="/themes/ctox-desktop-shell.css">
<link rel="stylesheet" href="/modules/browser/index.css">
<style>html,body{margin:0;overflow:hidden;background:var(--bg,#f4f4f2)}#win{position:relative;width:1280px;height:760px;display:flex;flex-direction:column;overflow:hidden}#win>*{flex:1;min-height:0}</style>
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
      // Wie der Service Worker (§16.15): ctox:shared/x.js → Shell-Pfad (PORTING §4 Falle 2).
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
  // E26/E27: Befehlsrouten + Deklarationen der echten browser-Version (v4/manifests/browser.manifest.json);
  // ohne aktive Version lehnt der Kern App-Befehle beim Schreiben mit unknown_command ab.
  await page.evaluate((routes) => { window.__routes = routes; }, realManifestRoutes('browser'));
  await page.evaluate(async (png) => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    window.__mod = await import('/modules/browser/index.js');
    const kernel = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false, userId: 'michael', actor: { id: 'michael', display_name: 'Michael', role: 'admin' } });
    await kernel.ready;
    const now = Date.now();
    await kernel.sim.push('business_apps', [{ id: 'browser', title: 'Browser' }, { id: 'outbound', title: 'Outbound' }]);
    // FIXT-R: Eigentümer der Fremd-Collections (echte Manifeste, inkl. ctox-platform) vor der eigenen Version.
    for (const o of window.__routes?._fixture_owners ?? []) if (!(await kernel.sim.inspect('business_apps', o.app))?.data?.active_ver) await kernel.sim.publishAppVersion(o.app, o.ver, o.publish);
    await kernel.sim.publishAppVersion('browser', '0.3.0', { title: 'Browser', manifestExtra: window.__routes });
    await kernel.sim.push('browser_sessions', [
      { id: 'browser_session_michael_1', owner_user_id: 'michael', controller_user_id: 'michael', controller_lease_id: 'lease-1', controller_lease_expires_at_ms: now + 110_000, status: 'active', runtime_status: 'active', title: 'Example Domain', current_url: 'https://example.com/', current_tab_id: 'browser_tab_1', profile_mode: 'persistent', created_at_ms: now - 5000, updated_at_ms: now - 1000 },
      { id: 'browser_session_michael_0', owner_user_id: 'michael', status: 'disconnected', runtime_status: 'disconnected', title: 'Alte Sitzung', current_url: 'https://old.example.org/', created_at_ms: now - 90000, updated_at_ms: now - 80000 },
      { id: 'browser_session_fremd', owner_user_id: 'jemand', status: 'active', runtime_status: 'active', title: 'Fremde Sitzung', created_at_ms: now, updated_at_ms: now },
    ]);
    await kernel.sim.push('browser_tabs', [{ id: 'browser_tab_1', session_id: 'browser_session_michael_1', status: 'open', title: 'Example Domain', url: 'https://example.com/', created_at_ms: now, updated_at_ms: now }]);
    await kernel.sim.push('browser_frames', [{ id: 'frame_1', session_id: 'browser_session_michael_1', tab_id: 'browser_tab_1', seq: 7, mime_type: 'image/png', encoding: 'base64', data: png, width: 4, height: 4, captured_at_ms: now, expires_at_ms: now + 3_600_000, updated_at_ms: now }]);
    await kernel.sim.settle();
    const api = kernel.appApi('browser', '0.3.0');
    window.__k = kernel;
    window.__api = api;
    window.__zone = () => kernel.status.forApp('browser').get().filter((e) => e.source === 'app');
    window.__commands = () => api.query('business_commands', { sort: [{ created_at: 'asc' }] }).get();
  }, PNG);
});

after(async () => { await browser?.close(); await new Promise((r) => server?.close(r)); });

const settle = async () => { await page.evaluate(() => window.__k.sim.settle()); await page.waitForTimeout(220); };
const railText = () => page.$eval('[data-browser-sessions]', (el) => el.textContent);
const LIVE = ['business_commands', 'browser_sessions', 'browser_tabs', 'browser_frames', 'browser_input_events', 'ctox_queue_tasks'];
const STATE_TEXT = /lad|load|sync|warte|wait|verbind|connect|vorbereit|prepar|keine|no browser|noch keine/i;

async function shot(name, width) {
  if (!process.env.SHOTS) return;
  await mkdir(join(HERE, 'shots'), { recursive: true });
  await page.setViewportSize({ width, height: 760 });
  await page.$eval('#win', (el, w) => { el.style.width = `${w}px`; }, width);
  await page.waitForTimeout(250);
  await page.screenshot({ path: join(HERE, 'shots', `${name}-${width}.png`) });
  await page.setViewportSize({ width: 1280, height: 760 });
  await page.$eval('#win', (el) => { el.style.width = '1280px'; });
}

test('mount: Live-Sitzungen des Nutzers, Live-Bild, gescoptes Root, Titel über api.shell', async () => {
  await page.evaluate(async () => {
    const { countingApi } = await import('/v4/tools/port/test-api.mjs');
    window.__c = countingApi(window.__api);
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, {});
  });
  await settle();
  const rail = await railText();
  assert.match(rail, /Example Domain/);
  assert.match(rail, /Alte Sitzung/);
  assert.doesNotMatch(rail, /Fremde Sitzung/, 'persönliche Sitzungen anderer Nutzer bleiben verborgen');
  assert.equal(await page.$eval('#win', (el) => el.dataset.app), 'browser');
  assert.equal(await page.evaluate(() => window.__k.shell.window('browser').state.get().title), 'Browser');
  assert.deepEqual(await page.evaluate((l) => l.map((c) => window.__c.subscribers(c)), LIVE), [1, 1, 1, 1, 1, 1]);
  // Ausgewählt ist die jüngste Sitzung; ihr Bild aus browser_frames liegt auf der Bühne, kein Bühnentext.
  const stage = await page.evaluate(() => ({
    selected: window.__app.snapshot().selectedSessionId,
    canvas: [document.querySelector('[data-browser-canvas]').width, document.querySelector('[data-browser-canvas]').height],
    emptyHidden: document.querySelector('[data-browser-empty]').hidden,
    title: document.querySelector('[data-browser-status-title]').textContent,
  }));
  assert.equal(stage.selected, 'browser_session_michael_1');
  assert.deepEqual(stage.canvas, [4, 4]);
  assert.equal(stage.emptyHidden, true);
  assert.equal(stage.title, 'Example Domain');
  assert.deepEqual(await page.evaluate(() => window.__zone().map((e) => e.code)), []);
  assert.deepEqual(page.__errors, []);
  await shot('browser', 1280);
  await shot('browser', 420);
});

test('Live-Änderung vom Core (sim.push) erscheint ohne Neuladen, Löschung verschwindet', async () => {
  await page.evaluate(() => window.__k.sim.push('browser_sessions', { id: 'browser_session_michael_live', owner_user_id: 'michael', status: 'active', runtime_status: 'active', title: 'Gamma Live', created_at_ms: 1, updated_at_ms: 2 }));
  await settle();
  assert.match(await railText(), /Gamma Live/);
  await page.evaluate(() => window.__k.sim.push('browser_sessions', { id: 'browser_session_michael_live', _deleted: true }));
  await settle();
  assert.doesNotMatch(await railText(), /Gamma Live/);
  // Gemeinsame Web-Stack-Anmeldesitzung eines anderen Kontos erscheint (Owner-Entscheidung 04.09.2026).
  await page.evaluate(() => window.__k.sim.push('browser_sessions', { id: 'browser_session_web_stack_auth_xing-com', owner_user_id: 'jemand', status: 'disconnected', title: 'Xing Login', payload: { purpose: 'web_stack_auth', source_id: 'xing.com' }, created_at_ms: 1, updated_at_ms: 3 }));
  await settle();
  assert.match(await railText(), /Xing Login/);
});

// FIX8 Mangel 5: ohne Sitzung war die Bühne eine Sackgasse (RANK-A: Leerzustand verwies auf „oben rechts“, der
// Knopf sitzt oben links). Jetzt genau ein Satz + Aktion auf der Bühne, ohne Ortsangabe, sonst kein Zustandstext.
test('leere Collections: kein Zustandstext und keine Zone; Bühne zeigt genau einen Satz + „Neue Sitzung“ (FIX8)', async () => {
  await page.evaluate(async () => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    const k2 = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false, instanceId: 'empty' });
    await k2.ready;
    // Leere App-Collections, aber eine aktive Version mit Routen (sonst unknown_command für browser.session.start).
    // FIXT-R: Eigentümer der Fremd-Collections (echte Manifeste, inkl. ctox-platform) vor der eigenen Version.
    for (const o of window.__routes?._fixture_owners ?? []) if (!(await k2.sim.inspect('business_apps', o.app))?.data?.active_ver) await k2.sim.publishAppVersion(o.app, o.ver, o.publish);
    await k2.sim.publishAppVersion('browser', '0.3.0', { title: 'Browser', manifestExtra: window.__routes });
    const host = document.createElement('div');
    host.style.cssText = 'width:900px;height:600px';
    document.body.append(host);
    host.setAttribute('data-fix8-empty-host', '');
    const app = await window.__mod.mount(host, k2.appApi('browser', '0.3.0'), {});
    await new Promise((r) => setTimeout(r, 300));
    window.__fix8Empty = { app, host, k2 };
  });
  await mkdir(join(HERE, 'shots'), { recursive: true });
  await page.locator('[data-fix8-empty-host]').screenshot({ path: join(HERE, 'shots', 'browser-empty-900.png') });
  const empty = await page.evaluate(async () => {
    const { app, host, k2 } = window.__fix8Empty;
    const texts = ['[data-browser-sessions]', '[data-browser-status-title]', '[data-browser-status-chip]', '[data-browser-session-card]', '[data-browser-command-history]', '[data-browser-handoff-history]']
      .map((sel) => host.querySelector(sel)?.textContent.trim() || '');
    const hasStateBox = Boolean(host.querySelector('.ctox-syncing, [role="status"], [data-browser-notice]'));
    const stage = host.querySelector('[data-browser-empty]');
    const stageVisible = !stage.hidden && stage.getClientRects().length > 0;
    const stageText = stage.querySelector('[data-browser-empty-text]')?.textContent || '';
    const stageAction = stage.querySelector('[data-browser-empty-start]')?.textContent || '';
    const emptyCount = [...host.querySelectorAll('.ctox-empty, [data-browser-empty]')].filter((el) => !el.hidden && el.textContent.trim()).length;
    const zone = k2.status.forApp('browser').get().map((e) => e.code);
    stage.querySelector('[data-browser-empty-start]')?.click();
    await new Promise((r) => setTimeout(r, 200));
    const started = k2.appApi('browser', '0.3.0').query('business_commands', {}).get().map((c) => c.type);
    await app.unmount();
    host.remove();
    await k2.close();
    return { texts, hasStateBox, stageVisible, stageText, stageAction, emptyCount, zone, started };
  });
  for (const value of empty.texts) assert.equal(value, '');
  assert.equal(empty.hasStateBox, false);
  assert.equal(empty.stageVisible, true, 'Bühne zeigt den Leerzustand');
  assert.equal(empty.stageText, 'Keine Browser-Sitzung.');
  assert.equal(empty.stageAction, 'Neue Sitzung');
  assert.doesNotMatch(empty.stageText, /oben|unten|rechts|links/, 'keine Ortsangabe');
  assert.equal(empty.emptyCount, 1, 'Leerzustand genau einmal');
  assert.deepEqual(empty.zone, []);
  assert.ok(empty.started.includes('browser.session.start'), `Aktion startet eine Sitzung (${empty.started})`);
});

test('Befehl: Neue Sitzung → business_commands browser.session.start, nie warten; failed → genau ein Zoneneintrag', async () => {
  await page.evaluate(() => window.__k.sim.failCommand('browser.session.start', 'kein Chromium'));
  const before = await page.evaluate(() => window.__commands().length);
  // Kein Warten: das Dokument liegt sofort lokal. Ein Doppelklick erzeugt keinen zweiten Start, solange der
  // erste läuft (16.08.2026: sieben Startbefehle, 21 Chrome-Prozesse).
  const immediate = await page.evaluate(() => {
    document.querySelector('[data-browser-start]').click();
    document.querySelector('[data-browser-start]').click();
    return window.__commands().length;
  });
  assert.equal(immediate, before + 1);
  await settle();
  const cmd = await page.evaluate(() => window.__commands().at(-1));
  assert.equal(cmd.type, 'browser.session.start');
  assert.match(cmd.payload.session_id, /^browser_session_michael_\d+$/);
  assert.equal(cmd.payload.record_id, cmd.payload.session_id);
  assert.equal(cmd.record_id, cmd.payload.session_id, 'record_id als Befehls-Option am Dokument');
  assert.ok(cmd.payload.lease_id, 'Start trägt die neue Steuerpacht');
  assert.equal(cmd.payload.profile_mode, 'persistent');
  assert.equal(cmd.payload.new_session, undefined);
  assert.equal(cmd.status, 'failed');
  const zone = await page.evaluate(() => window.__zone());
  assert.equal(zone.length, 1);
  assert.equal(zone[0].code, 'browser.command.browser.session.start');
  assert.match(zone[0].text, /kein Chromium/);
  assert.equal(zone[0].ref, cmd.id);
  assert.equal(await page.evaluate(() => window.__commands().filter((c) => c.type === 'browser.session.start').length), 1);
  // Zurück zur laufenden Sitzung (für die folgenden Tests).
  await page.evaluate(() => document.querySelector('[data-browser-session-id="browser_session_michael_1"]').click());
  await settle();
  assert.equal(await page.evaluate(() => window.__app.snapshot().selectedSessionId), 'browser_session_michael_1');
});

test('Befehl mit Steuerpacht: Adresse öffnen → browser.navigate mit Pacht; done räumt die Zone', async () => {
  await page.evaluate(() => window.__k.sim.onCommand('browser.navigate', () => ({ url: 'https://example.org/next' })));
  await page.fill('[data-browser-address]', 'example.org/next');
  await page.press('[data-browser-address]', 'Enter');
  await settle();
  const cmd = await page.evaluate(() => window.__commands().at(-1));
  assert.equal(cmd.type, 'browser.navigate');
  assert.equal(cmd.payload.url, 'https://example.org/next');
  assert.equal(cmd.payload.session_id, 'browser_session_michael_1');
  assert.equal(cmd.payload.lease_id, 'lease-1', 'eigene Pacht nach Remount wiedererkannt');
  assert.equal(cmd.status, 'done');
  assert.deepEqual(await page.evaluate(() => window.__zone().map((e) => e.code).filter((c) => c.includes('navigate'))), []);
});

test('Schreiben: Eingabe auf der Bühne → api.put browser_input_events, vom Core angenommen', async () => {
  await page.click('[data-browser-canvas]');
  await page.keyboard.press('a');
  await settle();
  const events = await page.evaluate(() => window.__api.query('browser_input_events', { sort: [{ seq: 'asc' }] }).get());
  const key = events.find((e) => e.type === 'keyDown' && e.key === 'a');
  assert.ok(key, 'keyDown a geschrieben');
  assert.equal(key.session_id, 'browser_session_michael_1');
  assert.equal(key.lease_id, 'lease-1');
  assert.equal(key.text, 'a');
  assert.ok(events.some((e) => e.type === 'mouseDown'));
  const result = await page.evaluate(async () => {
    const h = window.__api.put('browser_input_events', { id: 'probe', session_id: 'browser_session_michael_1', seq: 1, type: 'mouseMove', status: 'pending', created_at_ms: 1, updated_at_ms: 1 });
    await h.local;
    return h.result;
  });
  assert.equal(result.status, 'accepted');
  assert.deepEqual(page.__errors, []);
});

test('Dialog statt window.prompt: Beobachter ohne Antwort → kein Befehl, Dialog im Fenster protokolliert', async () => {
  const before = await page.evaluate(() => ({ n: window.__commands().length, d: window.__k.sim.dialogs.length }));
  await page.evaluate(() => document.querySelector('[data-browser-observer-grant]').click());
  await settle();
  const after = await page.evaluate(() => ({ n: window.__commands().length, d: window.__k.sim.dialogs.length, last: window.__k.sim.dialogs.at(-1) }));
  assert.equal(after.n, before.n);
  assert.equal(after.d, before.d + 1);
  assert.equal(after.last.app_id, 'browser');
  assert.equal(after.last.spec.kind, 'prompt');
});

test('Route: Start-Wunsch einer anderen App (api.shell.navigate) startet die Anmeldesitzung', async () => {
  const ok = await page.evaluate(() => window.__k.appApi('outbound', '1').shell.navigate('browser', {
    session_id: 'browser_session_web_stack_auth_dnb-com', purpose: 'web_stack_auth', target_url: 'https://dnbhoovers.com/login', source_id: 'dnbhoovers.com',
  }));
  assert.equal(ok, true);
  await settle();
  const cmd = await page.evaluate(() => window.__commands().at(-1));
  assert.equal(cmd.type, 'browser.session.start');
  assert.equal(cmd.payload.session_id, 'browser_session_web_stack_auth_dnb-com');
  assert.equal(cmd.payload.url, 'https://dnbhoovers.com/login');
  assert.equal(cmd.payload.purpose, 'web_stack_auth');
  // Startform von outbound (modules/outbound/index.js: navigate('browser', {title, mode, args}) mit
  // browserAuthAssistLaunchArgs): die Argumente stecken in params.args und müssen genauso ankommen.
  const okOutbound = await page.evaluate(() => window.__k.appApi('outbound', '1').shell.navigate('browser', {
    title: 'Browser', mode: 'maximized', args: {
      purpose: 'web_stack_auth', session_id: 'browser_session_web_stack_auth_xing-com', tab_id: 'browser_tab_browser_session_web_stack_auth_xing-com',
      source_id: 'xing.com', target_url: 'https://login.xing.com/', allowed_domains: ['xing.com', 'login.xing.com'],
      capture_script: 'xing.company_capture.v1', verify_selector: '#profile', secret_name: 'XING_BROWSER_LOGIN',
      auth_assist_command_id: 'cmd-auth-1', auth_assist_task_id: 'task-1', requesting_task_id: 'task-0', instruction: 'Bitte anmelden',
      auth_assist_status: 'pending', profile_mode: 'persistent', secret_value_in_rxdb: false,
    },
  }));
  assert.equal(okOutbound, true);
  await settle();
  const fromOutbound = await page.evaluate(() => window.__commands().at(-1));
  assert.equal(fromOutbound.type, 'browser.session.start');
  assert.equal(fromOutbound.payload.session_id, 'browser_session_web_stack_auth_xing-com');
  assert.equal(fromOutbound.payload.tab_id, 'browser_tab_browser_session_web_stack_auth_xing-com');
  assert.equal(fromOutbound.payload.url, 'https://login.xing.com/');
  assert.equal(fromOutbound.payload.source_id, 'xing.com');
  assert.deepEqual(fromOutbound.payload.allowed_domains, ['xing.com', 'login.xing.com']);
  assert.equal(fromOutbound.payload.capture_script, 'xing.company_capture.v1');
  assert.equal(fromOutbound.payload.secret_name, 'XING_BROWSER_LOGIN');
  assert.equal(fromOutbound.payload.auth_assist_command_id, 'cmd-auth-1');
  assert.equal(fromOutbound.payload.secret_value_in_rxdb, false);
  assert.equal(fromOutbound.payload.title, undefined, 'Fenster-Optionen der Startform gehören nicht in den Befehl');
  assert.equal(await page.evaluate(() => window.__app.snapshot().selectedSessionId), 'browser_session_web_stack_auth_xing-com');
  // Unbekannte Sitzungs-ID → Zone statt stillem Rückgriff.
  await page.evaluate(() => window.__k.appApi('outbound', '1').shell.navigate('browser', { session_id: 'kaputt' }));
  await settle();
  assert.ok((await page.evaluate(() => window.__zone().map((e) => e.code))).includes('browser.route.unknown_session'));
  // Zurück auf die laufende Sitzung.
  await page.evaluate(() => document.querySelector('[data-browser-session-id="browser_session_michael_1"]').click());
  await settle();
});

test('snapshot/restored: offene Adresseingabe, Auswahl, Pacht und Leiste überleben den Remount', async () => {
  await page.evaluate(() => document.querySelector('[data-browser-session-id="browser_session_michael_0"]').click());
  await settle();
  await page.fill('[data-browser-address]', 'halb getippte-adresse.de/pfad');
  await page.evaluate(() => document.querySelector('[data-browser-rail-toggle]').click());
  const snap = await page.evaluate(async () => {
    const cloned = structuredClone(window.__app.snapshot()); // Shell-Ablauf Entwurf §5
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, { restored: cloned });
    return cloned;
  });
  assert.equal(snap.addressDraft, 'halb getippte-adresse.de/pfad');
  assert.equal(snap.selectedSessionId, 'browser_session_michael_0');
  assert.equal(snap.railCollapsed, true);
  assert.ok(JSON.stringify(snap).length < 1024 * 1024);
  await settle();
  const after = await page.evaluate(() => ({
    address: document.querySelector('[data-browser-address]').value,
    selected: window.__app.snapshot().selectedSessionId,
    rail: document.querySelector('[data-browser-root]').classList.contains('is-rail-collapsed'),
    marked: document.querySelector('[data-browser-session-id].is-selected')?.dataset.browserSessionId,
  }));
  assert.equal(after.address, 'halb getippte-adresse.de/pfad');
  assert.equal(after.selected, 'browser_session_michael_0');
  assert.equal(after.marked, 'browser_session_michael_0');
  assert.equal(after.rail, true);
  assert.deepEqual(await page.evaluate((l) => l.map((c) => window.__c.subscribers(c)), LIVE), [1, 1, 1, 1, 1, 1], 'alte Abos sind abgemeldet');
  // Eine beendete Sitzung meldet ihren Grund in der Zone, nicht auf der Bühne.
  const zone = await page.evaluate(() => window.__zone().find((e) => e.code === 'browser.frame'));
  assert.match(zone?.text || '', /Kein laufender Browser-Prozess/);
  assert.doesNotMatch(await page.$eval('[data-browser-workbench], .browser-workbench', (el) => el.innerText), STATE_TEXT);
  assert.deepEqual(page.__errors, []);
  await page.evaluate(() => document.querySelector('[data-browser-rail-toggle]').click());
});

test('Scraping-Band: Adapter beider Sammlungen live, Abos nur solange das Band offen ist', async () => {
  await page.evaluate(() => window.__k.sim.push('outbound_lead_generation_adapters', { id: 'a1', source_id: 'linkedin.com', label: 'LinkedIn', url: 'https://linkedin.com', adapter_kind: 'playwright', status: 'ok', enabled: true, payload: {}, created_at_ms: 1, updated_at_ms: 2 }));
  await page.evaluate(() => window.__k.sim.push('outbound_research_adapters', { id: 'r1', source_id: 'xing.com', label: 'Xing', url: 'https://xing.com', adapter_kind: 'playwright', status: 'ok', enabled: true, payload: {}, created_at_ms: 1, updated_at_ms: 3 }));
  await settle();
  await page.evaluate(() => document.querySelector('[data-pg-band="adapters"]').click());
  await settle();
  const rail = await railText();
  assert.match(rail, /LinkedIn/);
  assert.match(rail, /Xing/);
  assert.equal(await page.evaluate(() => window.__c.subscribers('outbound_lead_generation_adapters')), 1);
  // Rundgang RUND1: lange Adapterzeilen (nowrap) ließen die Schiene seitlich scrollen; Fußzeile blieb beim Sitzungsband.
  await page.evaluate(() => window.__k.sim.push('outbound_research_adapters', { id: 'r2', source_id: 'sehr-lange-quelle-mit-vielen-zeichen.example.com', label: 'Rahmenvertrag über Wartung und Instandhaltung der Produktionsanlagen am Standort Augsburg', url: 'https://example.com', adapter_kind: 'playwright', status: 'ok', enabled: true, latency_ms: 1234, payload: {}, created_at_ms: 1, updated_at_ms: 4 }));
  await settle();
  const geo = await page.evaluate(() => { const r = document.querySelector('#win .browser-session-rail'); const p = document.querySelector('#win .browser-sessions'); return { rail: r.scrollWidth - r.clientWidth, pane: p.scrollWidth - p.clientWidth, footer: p.querySelector('[data-pg-footer]')?.textContent || '' }; });
  assert.ok(geo.rail <= 0 && geo.pane <= 0, JSON.stringify(geo));
  // Textdiät: kein Fußzähler mehr; der RUND1-Fehler (veralteter „N Sitzungen · Alle“-Fuß unter der Adapterliste) bleibt geprüft.
  assert.doesNotMatch(geo.footer, /Sitzung/);
  assert.equal(geo.footer, '');
  await page.evaluate(() => document.querySelector('[data-pg-band="all"]').click());
  await settle();
  assert.equal(await page.evaluate(() => window.__c.subscribers('outbound_lead_generation_adapters')), 0);
});

test('unmount meldet alle Abos ab und leert root', async () => {
  const res = await page.evaluate(async (l) => {
    await window.__app.unmount();
    await window.__k.sim.push('browser_sessions', { id: 'browser_session_michael_late', owner_user_id: 'michael', status: 'active', created_at_ms: 1, updated_at_ms: 10 });
    await window.__k.sim.settle();
    return {
      subs: [...l, 'outbound_research_adapters', 'outbound_lead_generation_adapters'].map((c) => window.__c.subscribers(c)),
      children: document.getElementById('win').children.length,
    };
  }, LIVE);
  assert.deepEqual(res.subs, [0, 0, 0, 0, 0, 0, 0, 0]);
  assert.equal(res.children, 0);
  assert.deepEqual(page.__errors, []);
});
