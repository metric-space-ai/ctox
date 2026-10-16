// calendar gegen den eingefrorenen App-API-4.0-Kern-Stub (createStubKernel) in echtem Chromium.
// Muster: modules/knowledge/tests/v4-api.test.mjs. Stub-Pfad aus CTOX_APP_API_STUB (Pflicht-Pin
// ~/.local/state/shell-v4/stub-pins/68b83a3), read-only unter /__stub/ ausgeliefert.
// Lauf: CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/68b83a3 node --test src/apps/business-os/modules/calendar/tests/v4-api.test.mjs
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

const APP = 'calendar';
const STUB = process.env.CTOX_APP_API_STUB || join(homedir(), '.local/state/shell-v4/stub-pins/68b83a3');
const HERE = dirname(fileURLToPath(import.meta.url));
const BOS = normalize(join(HERE, '../../..'));
const require = createRequire(import.meta.url);
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || join(homedir(), 'Documents/ctox/src/apps/business-os/node_modules/playwright'));
const TYPES = { '.js': 'text/javascript', '.mjs': 'text/javascript', '.css': 'text/css', '.html': 'text/html', '.json': 'application/json', '.svg': 'image/svg+xml' };
const COLLECTIONS = ['calendar_calendars', 'calendar_events', 'calendar_booking_pages', 'calendar_booking_holds', 'calendar_bookings'];

const PAGE = `<!doctype html><html lang="de"><head><meta charset="utf-8">
<link rel="stylesheet" href="/app.css">
<link rel="stylesheet" href="/shared/base.css">
<link rel="stylesheet" href="/modules/calendar/vendor/event-calendar/event-calendar.min.css">
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
    const kernel = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false });
    await kernel.ready;
    await kernel.sim.push('business_apps', [{ id: APP, title: 'Kalender' }]);
    await kernel.sim.push('calendar_sources', [{ id: 'src-1', type: 'local', name: 'Lokal', updated_at_ms: 1 }]);
    await kernel.sim.push('calendar_calendars', [
      { id: 'cal-alpha', source_id: 'src-1', title: 'Alpha Kalender', color: '#2563eb', visibility: true, updated_at_ms: 2 },
      { id: 'cal-beta', source_id: 'src-1', title: 'Beta Team', color: '#16a34a', visibility: true, updated_at_ms: 1 },
    ]);
    const start = new Date(); start.setHours(10, 0, 0, 0);
    const end = new Date(start.getTime() + 3600e3);
    await kernel.sim.push('calendar_events', [
      { id: 'ev-1', calendar_id: 'cal-alpha', title: 'Planungsrunde', start_time: start.getTime(), end_time: end.getTime(), all_day: false, created_at_ms: 1, updated_at_ms: 1 },
    ]);
    await kernel.sim.settle();
    window.__k = kernel;
    window.__api = kernel.appApi(APP, '1');
    window.__zone = () => kernel.statusZone.forApp(APP).get().filter((e) => e.source === 'app');
    window.__commands = () => window.__api.query('business_commands', { sort: [{ created_at: 'asc' }] }).get();
  }, APP);
});

after(async () => { await browser?.close(); await new Promise((r) => server?.close(r)); });

const settle = async () => { await page.evaluate(() => window.__k.sim.settle()); await page.waitForTimeout(250); };
const listText = () => page.$eval('[data-calendar-left-list]', (el) => el.textContent);

test('mount: rendert Live-Daten, nur api.*, gescoptes Root, Titel über api.shell', async () => {
  await page.evaluate(async () => {
    const { countingApi } = await import('/v4/tools/port/test-api.mjs');
    window.__c = countingApi(window.__api);
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, {});
  });
  await settle();
  const list = await listText();
  assert.match(list, /Alpha Kalender/);
  assert.match(list, /Beta Team/);
  assert.equal(await page.$eval('#win', (el) => el.dataset.app), APP);
  assert.equal(await page.evaluate(() => window.__k.shell.window('calendar').state.get().title), 'Kalender');
  assert.equal(await page.evaluate(() => window.__c.subscribers('calendar_calendars')), 1);
  assert.match(await page.$eval('[data-calendar-root]', (el) => el.textContent), /Planungsrunde/);
  assert.deepEqual(page.__errors, []);
});

test('Live-Änderung vom Core (sim.push) erscheint ohne Neuladen, Löschung verschwindet', async () => {
  await page.evaluate(() => window.__k.sim.push('calendar_calendars', { id: 'cal-live', source_id: 'src-1', title: 'Gamma Live', color: '#999999', visibility: true, updated_at_ms: 3 }));
  await settle();
  assert.match(await listText(), /Gamma Live/);
  await page.evaluate(() => window.__k.sim.push('calendar_calendars', { id: 'cal-live', _deleted: true }));
  await settle();
  assert.doesNotMatch(await listText(), /Gamma Live/);
});

test('leere Collections: kein Lade-/Leertext, keine Zone', async () => {
  // Modulzustand ist modulweit (V4-PORT.md OFFEN-3): Hauptfenster abmelden, leer montieren, wieder montieren.
  const res = await page.evaluate(async (APP) => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    await window.__app.unmount();
    const k2 = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false, instanceId: 'empty' });
    await k2.ready;
    const host = document.createElement('div');
    host.style.cssText = 'width:900px;height:600px';
    document.body.append(host);
    const app = await window.__mod.mount(host, k2.appApi(APP, '1'), {});
    await new Promise((r) => setTimeout(r, 150));
    const stateBox = Boolean(host.querySelector('.ctox-syncing, [role="status"]'));
    const text = host.textContent;
    const zone = k2.statusZone.forApp(APP).get().filter((e) => e.source === 'app').map((e) => e.code);
    await app.unmount();
    host.remove();
    await k2.close();
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, {});
    return { stateBox, synced: /synchronisiert|wird geladen|Syncing|Keine Einträge|keine Datenbank/i.test(text), zone };
  }, APP);
  await settle();
  assert.equal(res.stateBox, false);
  assert.equal(res.synced, false);
  assert.deepEqual(res.zone, []);
});

test('Schreiben: neuer Kalender über das Formular → api.put, live sichtbar, vom Core angenommen', async () => {
  await page.evaluate(() => document.querySelector('#btnLeftNew').click());
  await page.fill('#drawerCalendarForm [name="title"]', 'Delta Neu');
  await page.evaluate(() => document.querySelector('#drawerCalendarForm [data-submit-action]').click());
  await settle();
  assert.match(await listText(), /Delta Neu/);
  const res = await page.evaluate(async () => ({
    stored: window.__api.query('calendar_calendars', { selector: { title: 'Delta Neu' } }).get().length,
    outbox: (await window.__k.sim.outbox()).length,
  }));
  assert.equal(res.stored, 1);
  assert.equal(res.outbox, 0);
});

// Rundgang RUND1: das frisch geöffnete leere Formular zeigte sofort „Kalendertitel ist erforderlich.“ in Rot.
test('Formular: kein Fehlertext vor der ersten Eingabe, Speichern gesperrt; nach Leeren des Felds erscheint der Hinweis', async () => {
  await page.evaluate(() => document.querySelector('#btnLeftNew').click());
  await settle();
  const fresh = await page.evaluate(() => {
    const f = document.querySelector('#drawerCalendarForm');
    return { errors: [...f.querySelectorAll('[data-error-for]')].filter((n) => !n.hidden && n.textContent.trim()).length, invalid: f.querySelectorAll('[aria-invalid="true"]').length, submitDisabled: f.querySelector('[data-submit-action]').disabled };
  });
  assert.deepEqual(fresh, { errors: 0, invalid: 0, submitDisabled: true });
  await page.fill('#drawerCalendarForm [name="title"]', 'x');
  await page.fill('#drawerCalendarForm [name="title"]', '');
  await settle();
  assert.match(await page.evaluate(() => document.querySelector('#drawerCalendarForm').textContent), /Kalendertitel ist erforderlich/);
  await page.keyboard.press('Escape');
  await page.evaluate(() => document.querySelector('[data-calendar-root] [data-action="close-drawer"], [data-calendar-root] .ctox-drawer [aria-label="Schließen"]')?.click());
  await settle();
});

test('Löschen fragt über api.shell.dialog: ohne Ja nichts, mit Ja api.remove', async () => {
  const openCal = async () => {
    await page.evaluate(() => document.querySelector('[data-action="edit-cal"][data-id="cal-beta"]').click());
    await page.waitForSelector('#btnDeleteCal', { timeout: 3000 });
  };
  await openCal();
  await page.evaluate(() => document.querySelector('#btnDeleteCal').click());
  await settle();
  assert.equal(await page.evaluate(() => window.__k.sim.dialogs.length), 1);
  assert.ok(await page.evaluate(() => window.__api.get('calendar_calendars', 'cal-beta').get()), 'default:false → nichts gelöscht');
  await page.evaluate(async () => {
    const { withDialogAnswer } = await import('/v4/tools/port/test-api.mjs');
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), withDialogAnswer(window.__c.api, true), {});
  });
  await settle();
  await openCal();
  await page.evaluate(() => document.querySelector('#btnDeleteCal').click());
  await settle();
  assert.equal(await page.evaluate(() => window.__api.get('calendar_calendars', 'cal-beta').get()), null);
  assert.doesNotMatch(await listText(), /Beta Team/);
});

test('Meldung statt ctx.notifications: ungültiger Import → genau ein Zoneneintrag', async () => {
  const [chooser] = await Promise.all([
    page.waitForEvent('filechooser'),
    page.evaluate(() => document.querySelector('[data-calendar-root] [data-action="import"]').click()),
  ]);
  await chooser.setFiles({ name: 'x.json', mimeType: 'application/json', buffer: Buffer.from('kein json') });
  await settle();
  const zone = await page.evaluate(() => window.__zone());
  assert.equal(zone.length, 1);
  assert.equal(zone[0].code, 'calendar.import.invalid');
  assert.equal(zone[0].level, 'error');
});

test('snapshot/restored: Ansicht und offenes Formular samt Eingabe überleben den Remount', async () => {
  await page.evaluate(() => document.querySelector('[data-calendar-view="week"]')?.click());
  await page.evaluate(() => document.querySelector('#btnLeftNew').click());
  await page.fill('#drawerCalendarForm [name="title"]', 'halb fertiger Kalender');
  const snap = await page.evaluate(async () => {
    const cloned = structuredClone(window.__app.snapshot());
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, { restored: cloned });
    return cloned;
  });
  assert.equal(snap.editing.type, 'calendar');
  assert.equal(snap.editing.fields.title, 'halb fertiger Kalender');
  assert.equal(snap.calendarView, 'week');
  assert.ok(JSON.stringify(snap).length < 1024 * 1024);
  await settle();
  const after = await page.evaluate(() => ({
    title: document.querySelector('#drawerCalendarForm [name="title"]')?.value,
    view: window.__app.snapshot().calendarView,
  }));
  assert.equal(after.title, 'halb fertiger Kalender');
  assert.equal(after.view, 'week');
  assert.equal(await page.evaluate(() => window.__c.subscribers('calendar_calendars')), 1, 'alte Abos sind abgemeldet');
  await page.evaluate(() => document.querySelector('#btnCancelDrawer')?.click());
  assert.deepEqual(page.__errors, []);
});

// FIX8 Mangel 4: Klick auf einen Termin öffnete nichts. Standard macOS-Kalender/Outlook: Klick → Termindetails
// (Seitenblatt, nur lesen), Doppelklick → Termin bearbeiten. Keine Speicherung ohne Nutzeraktion.
test('FIX8: Klick auf Termin → Details (nur lesen), Doppelklick → Bearbeiten; keine Schreibvorgänge', async () => {
  await page.evaluate(() => document.querySelector('[data-calendar-view="month"]')?.click());
  await settle();
  const writes = await page.evaluate(() => window.__api.query('calendar_events', {}).get().map((e) => e.updated_at_ms).join(','));
  const locate = () => page.evaluate(() => {
    const el = [...document.querySelectorAll('#win .ec-event')].find((e) => /Planungsrunde/.test(e.textContent));
    el?.scrollIntoView({ block: 'center' });
    const r = el?.getBoundingClientRect();
    return r ? { x: r.x + Math.min(20, r.width / 2), y: r.y + r.height / 2 } : null;
  });
  let box = await locate();
  assert.ok(box, 'Termin gerendert');
  const drawerVisible = () => page.evaluate(() => {
    const d = document.querySelector('#calendarInspectorDrawer');
    const r = d?.getBoundingClientRect();
    const w = document.getElementById('win').getBoundingClientRect();
    return Boolean(d?.classList.contains('is-open') && r && r.width > 40 && r.height > 40 && r.right > w.left && r.left < w.right && r.bottom > w.top && r.top < w.bottom && getComputedStyle(d).visibility !== 'hidden');
  });
  await page.mouse.click(box.x, box.y);
  await page.waitForTimeout(400);
  assert.equal(await drawerVisible(), true, 'Klick öffnet das Seitenblatt');
  const details = await page.evaluate(() => ({
    view: Boolean(document.querySelector('#calendarInspectorDrawer [data-calendar-event-details]')),
    form: Boolean(document.querySelector('#calendarInspectorDrawer form')),
    text: document.querySelector('#calendarInspectorDrawer')?.textContent.replace(/\s+/g, ' ').trim(),
  }));
  assert.equal(details.view, true, 'Termindetails statt Formular');
  assert.equal(details.form, false, 'Klick öffnet kein Bearbeitungsformular');
  assert.match(details.text, /Planungsrunde/);
  assert.match(details.text, /Alpha Kalender/);
  assert.match(details.text, /10:00/);
  assert.doesNotMatch(details.text, /ev-1|cal-alpha/, 'keine IDs');
  await mkdir(join(HERE, 'shots'), { recursive: true });
  await page.screenshot({ path: join(HERE, 'shots', `${APP}-details-1280.png`) });
  // „Bearbeiten“ im Seitenblatt führt ebenfalls ins Formular.
  await page.evaluate(() => document.querySelector('#calendarInspectorDrawer [data-calendar-event-edit]')?.click());
  await page.waitForTimeout(200);
  assert.equal(await page.$eval('#drawerEventForm [name="title"]', (el) => el.value), 'Planungsrunde');
  await page.evaluate(() => document.querySelector('#btnCancelDrawer')?.click());
  await page.waitForTimeout(600); // Schließ-Animation des Seitenblatts abwarten
  box = await locate(); // das Seitenblatt hat den Monat neu gerendert
  await page.mouse.dblclick(box.x, box.y);
  await page.waitForTimeout(400);
  assert.equal(await drawerVisible(), true);
  assert.equal(await page.$eval('#drawerEventForm [name="title"]', (el) => el.value), 'Planungsrunde', 'Doppelklick öffnet zum Bearbeiten');
  await page.evaluate(() => document.querySelector('#btnCancelDrawer')?.click());
  await settle();
  assert.equal(await page.evaluate(() => window.__api.query('calendar_events', {}).get().map((e) => e.updated_at_ms).join(',')), writes, 'nichts gespeichert');
  assert.deepEqual(page.__errors, []);
});

test('Screenshots 1280/420', async () => {
  await mkdir(join(HERE, 'shots'), { recursive: true });
  await page.evaluate(() => document.querySelector('[data-calendar-view="month"]')?.click());
  await settle();
  await page.screenshot({ path: join(HERE, 'shots', `${APP}-1280.png`) });
  await page.setViewportSize({ width: 420, height: 860 });
  await page.$eval('#win', (el) => { el.style.width = '420px'; el.style.height = '860px'; });
  await page.waitForTimeout(300);
  await page.screenshot({ path: join(HERE, 'shots', `${APP}-420.png`) });
  const overflow = await page.$eval('#win', (el) => el.scrollWidth - el.clientWidth);
  assert.ok(overflow <= 1, `kein horizontaler Überlauf (${overflow}px)`);
  await page.setViewportSize({ width: 1280, height: 760 });
  await page.$eval('#win', (el) => { el.style.width = '1280px'; el.style.height = '760px'; });
});

test('unmount meldet alle Abos ab und leert root', async () => {
  const res = await page.evaluate(async (COLLECTIONS) => {
    await window.__app.unmount();
    await window.__k.sim.push('calendar_calendars', { id: 'cal-late', title: 'Late', updated_at_ms: 10 });
    await window.__k.sim.settle();
    return { subs: COLLECTIONS.map((c) => window.__c.subscribers(c)), children: document.getElementById('win').children.length };
  }, COLLECTIONS);
  assert.deepEqual(res.subs, COLLECTIONS.map(() => 0));
  assert.equal(res.children, 0);
  assert.deepEqual(page.__errors, []);
});
