// tickets gegen den eingefrorenen App-API-4.0-Kern-Stub (createStubKernel) in echtem Chromium.
// Stub: CTOX_APP_API_STUB (Pflicht-Pin ~/.local/state/shell-v4/stub-pins/68b83a3), read-only unter /__stub/.
// Eigene Attrappe (v4/tools/port/test-api.mjs) nur für den Abo-Zähler. Muster: modules/knowledge/tests/v4-api.test.mjs.
// Lauf: CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/68b83a3 node --test src/apps/business-os/modules/tickets/tests/v4-api.test.mjs
// Screenshots (1280 / 420) landen in modules/tickets/tests/shots/.
import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { readFile, mkdir } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { dirname, join, normalize, extname } from 'node:path';
import { homedir } from 'node:os';

const STUB = process.env.CTOX_APP_API_STUB || join(homedir(), '.local/state/shell-v4/stub-pins/68b83a3');
const HERE = dirname(fileURLToPath(import.meta.url));
const BOS = normalize(join(HERE, '../../..'));
const SHOTS = join(HERE, 'shots');
const require = createRequire(import.meta.url);
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || join(homedir(), 'Documents/ctox/src/apps/business-os/node_modules/playwright'));
const TYPES = { '.js': 'text/javascript', '.mjs': 'text/javascript', '.css': 'text/css', '.html': 'text/html', '.json': 'application/json', '.svg': 'image/svg+xml' };

// Die Shell lädt Basis- und App-CSS aus dem Manifest und stellt den Fenster-Container (business-app-window).
const PAGE = `<!doctype html><html><head><meta charset="utf-8">
<link rel="stylesheet" href="/ui-contract/v1/workjet-ui-contract.css">
<link rel="stylesheet" href="/app.css">
<link rel="stylesheet" href="/shared/base.css">
<link rel="stylesheet" href="/themes/ctox-desktop-shell.css">
<link rel="stylesheet" href="/modules/tickets/index.css">
<style>html,body{margin:0;background:var(--surface-1,#f4f4f2)}
#win{width:1280px;height:800px;container-name:business-app-window;container-type:inline-size;position:relative;overflow:hidden}</style>
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
      // Wie der Service Worker (§16.15): ctox:shared/x.js → Shell-Pfad (PORTING.md §4 Falle 2).
      if (/\.m?js$/.test(file)) body = Buffer.from(body.toString('utf8').replace(/(['"])ctox:shared\//g, '$1/shared/'));
      res.writeHead(200, { 'content-type': TYPES[extname(file)] || 'application/octet-stream' }); res.end(body);
    } catch { res.writeHead(404); res.end(); }
  });
  await new Promise((r) => server.listen(0, '127.0.0.1', r));
  base = `http://127.0.0.1:${server.address().port}/`;
  browser = await chromium.launch();
  page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
  const errors = [];
  page.on('pageerror', (e) => errors.push(e.message));
  page.on('console', (m) => { if (m.type() === 'error') console.error('[page]', m.text()); });
  page.on('response', (r) => { if (r.status() >= 400) console.error('[page] HTTP', r.status(), r.url()); });
  page.__errors = errors;
  await page.goto(base);
  await page.evaluate(async () => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    window.__mod = await import('/modules/tickets/index.js');
    const kernel = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false, userId: 'op-1', actor: { id: 'op-1', role: 'admin', permissions: ['apps.source.edit', 'ctox.crew.manage'] } });
    await kernel.ready;
    const now = Date.now();
    await kernel.sim.push('business_apps', [{ id: 'tickets', title: 'Tickets' }]);
    await kernel.sim.push('ctox_ticket_items', [
      { id: 'tk_1', ticket_key: 'SUP-1', title: 'Login defekt', body_text: 'Seit heute kein Login.', remote_status: 'open', source_system: 'zammad', requester: 'kunde@acme.test', priority: 'high', updated_at_ms: now - 1000, updated_at: new Date(now - 1000).toISOString() },
      { id: 'tk_2', ticket_key: 'SUP-2', title: 'Rechnung fehlt', remote_status: 'waiting_customer', source_system: 'email', priority: 'normal', updated_at_ms: now - 2000 },
      { id: 'tk_3', ticket_key: 'SUP-3', title: 'Alter Fall', remote_status: 'closed', source_system: 'ctox', updated_at_ms: now - 3000 },
    ]);
    await kernel.sim.push('ctox_ticket_events', [{ id: 'ev_1', event_key: 'ev_1', ticket_key: 'SUP-1', summary: 'Kunde meldet Fehler', direction: 'inbound', event_type: 'comment', updated_at_ms: now - 900, observed_at: new Date(now - 900).toISOString() }]);
    await kernel.sim.push('ctox_ticket_cases', [{ id: 'case_1', case_id: 'case_1', ticket_key: 'SUP-1', label: 'Login wiederherstellen', state: 'approval_pending', updated_at_ms: now - 800 }]);
    await kernel.sim.settle();
    window.__k = kernel;
    window.__api = kernel.appApi('tickets', '1.0.0');
    window.__zone = () => kernel.statusZone.forApp('tickets').get().filter((e) => e.source === 'app');
    window.__commands = () => window.__api.query('business_commands', { sort: [{ created_at: 'asc' }] }).get();
  });
});

after(async () => { await browser?.close(); await new Promise((r) => server?.close(r)); });

const settle = async () => { await page.evaluate(() => window.__k.sim.settle()); await page.waitForTimeout(250); };
const text = (sel) => page.$eval(sel, (el) => el.textContent);
const clickSel = (sel) => page.evaluate((s) => document.querySelector(s).click(), sel);
const remount = (opts = {}) => page.evaluate(async (o) => {
  const { withDialogAnswer } = await import('/v4/tools/port/test-api.mjs');
  await window.__app.unmount();
  const api = o.dialog === undefined ? window.__c.api : withDialogAnswer(window.__c.api, o.dialog);
  window.__app = await window.__mod.mount(document.getElementById('win'), api, o.restored ? { restored: o.restored } : {});
}, opts);

test('mount: rendert Live-Daten, nur api.*, gescoptes Root, Titel über api.shell', async () => {
  await page.evaluate(async () => {
    const { countingApi } = await import('/v4/tools/port/test-api.mjs');
    window.__c = countingApi(window.__api);
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, {});
  });
  await settle();
  const list = await text('[data-ticket-list]');
  assert.match(list, /Login defekt/);
  assert.match(list, /Rechnung fehlt/);
  assert.equal(await page.$eval('#win', (el) => el.dataset.app), 'tickets');
  assert.equal(await page.evaluate(() => window.__k.shell.window('tickets').state.get().title), 'Tickets');
  assert.equal(await page.evaluate(() => window.__c.subscribers('ctox_ticket_items')), 1);
  assert.equal(await page.evaluate(() => window.__c.subscribers('ctox_queue_tasks')), 1);
  assert.equal((await text('[data-pg-count="all"]')).trim(), '(3)');
  assert.equal((await text('[data-pg-count="pending"]')).trim(), '(1)');
  assert.equal(await page.$eval('.tickets-left', (el) => el.dataset.pgWired), 'true');
  // Erstes Ticket ist ausgewählt (Fachverhalten), Verlauf sichtbar, Ops-Pane auto-offen (Fall wartet auf Freigabe).
  assert.match(await text('[data-ticket-detail-body]'), /Kunde meldet Fehler/);
  assert.equal(await page.$eval('[data-tickets-root]', (el) => el.classList.contains('is-ops-hidden')), false);
  assert.match(await text('[data-ticket-ops-body]'), /Login wiederherstellen/);
  assert.deepEqual(page.__errors, []);
});

test('Live-Änderung vom Core (sim.push) erscheint ohne Neuladen, Löschung verschwindet', async () => {
  await page.evaluate(() => window.__k.sim.push('ctox_ticket_items', { id: 'tk_live', ticket_key: 'SUP-9', title: 'Live Ticket', remote_status: 'open', updated_at_ms: Date.now() - 5000 }));
  await settle();
  assert.match(await text('[data-ticket-list]'), /Live Ticket/);
  await page.evaluate(() => window.__k.sim.push('ctox_ticket_items', { id: 'tk_live', _deleted: true }));
  await settle();
  assert.doesNotMatch(await text('[data-ticket-list]'), /Live Ticket/);
});

test('leere Collections rendern keinen Zustandstext und keine Zone', async () => {
  const empty = await page.evaluate(async () => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    const k2 = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false, instanceId: 'empty' });
    await k2.ready;
    const host = document.createElement('div');
    document.body.append(host);
    const app = await window.__mod.mount(host, k2.appApi('tickets', '1.0.0'), {});
    await new Promise((r) => setTimeout(r, 300));
    const out = {
      listText: host.querySelector('[data-ticket-list]').textContent.trim(),
      detailText: host.querySelector('[data-ticket-detail-body]').textContent.trim(),
      opsText: host.querySelector('[data-ticket-ops-body]').textContent.trim(),
      hasStateBox: Boolean(host.querySelector('.ctox-syncing, .ctox-empty, [role="status"], [data-ticket-command-status]')),
      zone: k2.statusZone.forApp('tickets').get().map((e) => e.code),
    };
    await app.unmount();
    host.remove();
    await k2.close();
    return out;
  });
  assert.deepEqual(empty, { listText: '', detailText: '', opsText: '', hasStateBox: false, zone: [] });
  // Modulzustand ist modulweit (V4-PORT.md OFFEN-4): Hauptfenster neu montieren.
  await remount();
  await settle();
});

test('Befehl: Freigeben → ctox.ticket.approve mit Handler-Payload und record_id; failed → genau ein Zoneneintrag', async () => {
  await page.evaluate(() => window.__k.sim.failCommand('ctox.ticket.approve', 'case is not approval_pending'));
  await clickSel('[data-detail-actions] [data-ticket-action="approve"]');
  await settle();
  const cmd = await page.evaluate(() => window.__commands().at(-1));
  assert.equal(cmd.type, 'ctox.ticket.approve');
  assert.equal(cmd.app_id, 'tickets');
  assert.equal(cmd.record_id, 'case_1');
  assert.equal(cmd.payload.case_id, 'case_1');
  assert.equal(cmd.payload.status, 'approved');
  assert.equal(cmd.payload.client_context.actor.id, 'op-1');
  assert.equal(Object.hasOwn(cmd.payload, 'command_type'), false);
  assert.equal(cmd.status, 'failed');
  const zone = await page.evaluate(() => window.__zone());
  assert.equal(zone.length, 1);
  assert.equal(zone[0].code, 'tickets.command.ctox.ticket.approve');
  assert.match(zone[0].text, /approval_pending/);
  assert.doesNotMatch(await text('[data-tickets-root]'), /ausgeführt|abgeschlossen|wird verarbeitet/);
});

test('Prompt-Aktionen über api.shell.dialog: ohne Antwort kein Befehl, mit Antwort Befehl', async () => {
  const before = await page.evaluate(() => window.__commands().length);
  await clickSel('[data-action="new"]');
  await settle();
  assert.equal(await page.evaluate(() => window.__commands().length), before, 'Stub-Dialog liefert default → Abbruch');
  assert.ok(await page.evaluate(() => window.__k.sim.dialogs.length) >= 1);
  await remount({ dialog: 'Drucker streikt' });
  await settle();
  await clickSel('[data-action="new"]');
  await settle();
  const cmd = await page.evaluate(() => window.__commands().at(-1));
  assert.equal(cmd.type, 'ctox.ticket.local.create');
  assert.equal(cmd.record_id, 'local:Drucker streikt');
  assert.deepEqual({ title: cmd.payload.title, status: cmd.payload.status, priority: cmd.payload.priority }, { title: 'Drucker streikt', status: 'open', priority: 'normal' });
  assert.equal(cmd.status, 'done');
  await remount();
  await settle();
});

test('Deep-Link über api.shell.onRoute: bekannter Fall fokussiert, unbekannter → Zone', async () => {
  await page.evaluate(() => window.__api.shell.navigate('tickets', { record: 'case_1' }));
  await settle();
  assert.equal(await page.evaluate(() => window.__app.snapshot().selectedId), 'tk_1');
  assert.equal(await page.evaluate(() => window.__app.snapshot().focusedCaseId), 'case_1');
  await page.evaluate(() => window.__api.shell.navigate('tickets', { record: 'gibt-es-nicht' }));
  await settle();
  const zone = await page.evaluate(() => window.__zone().map((e) => e.code));
  assert.ok(zone.includes('tickets.route.unknown_record'));
});

test('Import: ungültiges JSON → Zone; gültig → ctox.ticket.local.create', async () => {
  const r = await page.evaluate(() => {
    const hooks = window.__mod.__ticketTestHooks;
    const bad = hooks.importTicketsText('{kaputt');
    const zoneBad = window.__zone().map((e) => e.code);
    const good = hooks.importTicketsText(JSON.stringify([{ title: 'Import A', body: 'x' }, { body: 'ohne Titel' }]));
    return { bad, zoneBad, good, zoneAfter: window.__zone().map((e) => e.code) };
  });
  assert.equal(r.bad, 0);
  assert.ok(r.zoneBad.includes('tickets.import.invalid'));
  assert.equal(r.good, 1);
  assert.ok(!r.zoneAfter.includes('tickets.import.invalid'));
  const cmd = await page.evaluate(() => window.__commands().at(-1));
  assert.equal(cmd.type, 'ctox.ticket.local.create');
  assert.equal(cmd.payload.title, 'Import A');
});

test('Schreiben: api.put erscheint live und wird vom Core angenommen', async () => {
  const result = await page.evaluate(async () => {
    const h = window.__api.put('ctox_ticket_items', { id: 'tk_put', ticket_key: 'SUP-PUT', title: 'Put Ticket', remote_status: 'open', updated_at_ms: Date.now() - 10000 });
    await h.local;
    return h.result;
  });
  assert.equal(result.status, 'accepted');
  await settle();
  assert.match(await text('[data-ticket-list]'), /Put Ticket/);
});

test('snapshot/restored: Auswahl, Ops-Modus, Suche und Filterfach überleben den Remount', async () => {
  await clickSel('[data-context-record-id="tk_2"]');
  await page.fill('[data-pg-search]', 'rechnung');
  await page.evaluate(() => document.querySelector('[data-pg-tray-toggle]').click());
  await settle();
  const snap = await page.evaluate(async () => {
    const cloned = structuredClone(window.__app.snapshot());
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, { restored: cloned });
    return cloned;
  });
  assert.equal(snap.selectedId, 'tk_2');
  assert.equal(snap.search, 'rechnung');
  assert.equal(snap.trayOpen, true);
  assert.ok(JSON.stringify(snap).length < 1024 * 1024);
  await settle();
  const after = await page.evaluate(() => ({
    search: document.querySelector('[data-pg-search]').value,
    tray: !document.querySelector('[data-pg-tray]').hidden,
    selected: document.querySelector('[data-ticket-list] .is-selected')?.getAttribute('data-context-record-id'),
    rows: [...document.querySelectorAll('[data-ticket-list] [data-context-record-id]')].map((el) => el.getAttribute('data-context-record-id')),
  }));
  assert.deepEqual(after, { search: 'rechnung', tray: true, selected: 'tk_2', rows: ['tk_2'] });
  assert.equal(await page.evaluate(() => window.__c.subscribers('ctox_ticket_items')), 1, 'alte Abos sind abgemeldet');
  assert.deepEqual(page.__errors, []);
});

test('Screenshots 1280 und 420 (gegen den Stub)', async () => {
  await mkdir(SHOTS, { recursive: true });
  await page.fill('[data-pg-search]', '');
  await page.dispatchEvent('[data-pg-search]', 'input');
  await clickSel('[data-context-record-id="tk_1"]');
  await settle();
  for (const [w, h] of [[1280, 800], [420, 860]]) {
    await page.setViewportSize({ width: w, height: h });
    await page.$eval('#win', (el, size) => { el.style.width = `${size[0]}px`; el.style.height = `${size[1]}px`; }, [w, h]);
    await page.waitForTimeout(300);
    const overflow = await page.$eval('#win', (el) => el.scrollWidth - el.clientWidth);
    assert.ok(overflow <= 1, `kein horizontaler Überlauf bei ${w}px (gemessen ${overflow})`);
    await page.locator('#win').screenshot({ path: join(SHOTS, `tickets-${w}.png`) });
  }
  await page.evaluate(() => document.querySelector('[data-ops-actions] [data-action="close-ops"]')?.click());
  await page.waitForTimeout(200);
  await page.locator('#win').screenshot({ path: join(SHOTS, 'tickets-420-detail.png') });
});

test('unmount meldet alle Abos ab und leert root', async () => {
  const res = await page.evaluate(async () => {
    await window.__app.unmount();
    await window.__k.sim.push('ctox_ticket_items', { id: 'tk_late', title: 'Late', updated_at_ms: 99 });
    await window.__k.sim.settle();
    return {
      subs: ['ctox_ticket_items', 'ctox_ticket_cases', 'ctox_queue_tasks', 'ctox_crew_members'].map((c) => window.__c.subscribers(c)),
      children: document.getElementById('win').children.length,
    };
  });
  assert.deepEqual(res.subs, [0, 0, 0, 0]);
  assert.equal(res.children, 0);
  assert.deepEqual(page.__errors, []);
});

// FR5-1 (Rahmen-Dialog): api.shell.dialog über das Blatt des Rahmens (v4/frame/window-sheet.js) statt Stub-Antwort.
// Nachbau des Fensters: #win = Fensterelement, darin der Inhaltsbereich als App-root; das Blatt hängt im Fenster.
const mountWithWindowSheet = () => page.evaluate(async () => {
  const { withWindowSheet } = await import('/v4/tools/port/test-api.mjs');
  if (!document.querySelector('link[data-fr5-css]')) {
    const l = document.createElement('link'); l.rel = 'stylesheet'; l.href = '/v4/frame/frame.css'; l.dataset.fr5Css = '';
    document.head.append(l);
  }
  await window.__app?.unmount?.();
  const winEl = document.getElementById('win');
  winEl.style.position = 'relative';
  const content = document.createElement('div');
  content.dataset.fr5Content = '';
  content.style.cssText = 'position:absolute;inset:0;overflow:auto';
  winEl.replaceChildren(content);
  const { api } = await withWindowSheet(window.__c.api, winEl, { root: content });
  window.__app = await window.__mod.mount(content, api, {});
});
const sheetState = () => page.evaluate(() => {
  const s = document.querySelector('#win [data-v4-sheet]');
  const content = document.querySelector('#win [data-fr5-content]');
  return s ? { inWindow: true, inAppRoot: content.contains(s), inert: !!content.inert, focus: s.contains(document.activeElement), ok: s.querySelector('[data-v4-sheet-ok]')?.textContent || '' } : null;
});
const unmountWindowSheet = () => page.evaluate(async () => { await window.__app?.unmount?.(); window.__app = null; document.getElementById('win').replaceChildren(); });

test('FR5-1 Rahmen-Dialog wirkt: „Ticket anlegen“ öffnet das Eingabeblatt im Fenster, Enter legt das Ticket an', async () => {
  await mountWithWindowSheet();
  await settle();
  const before = await page.evaluate(() => window.__commands().length);
  await page.click('#win [data-action="new"]');
  await page.waitForSelector('#win [data-v4-sheet] [data-v4-sheet-input]');
  const s = await sheetState();
  assert.deepEqual({ ...s, ok: undefined }, { inWindow: true, inAppRoot: false, inert: true, focus: true, ok: undefined });
  await page.keyboard.type('Scanner klemmt');
  await page.keyboard.press('Enter');
  // Zweite Frage (Beschreibung) folgt erst nach der ersten (Warteschlange des Blatts).
  await page.waitForFunction(() => document.querySelector('#win [data-v4-sheet] .v4-sheet-title')?.textContent === 'Beschreibung');
  await page.keyboard.type('Fach 2');
  await page.keyboard.press('Enter');
  await settle();
  assert.equal(await sheetState(), null);
  const cmds = await page.evaluate(() => window.__commands());
  assert.equal(cmds.length, before + 1);
  assert.equal(cmds.at(-1).type, 'ctox.ticket.local.create');
  assert.equal(cmds.at(-1).payload.title, 'Scanner klemmt');
  await unmountWindowSheet();
});
