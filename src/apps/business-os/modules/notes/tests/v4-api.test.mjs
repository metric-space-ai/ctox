// notes gegen den App-API-4.0-Kern-Stub (createStubKernel, read-only unter /__stub/) in echtem Chromium.
// Stub: CTOX_APP_API_STUB (Pflicht-Pin ~/.local/state/shell-v4/stub-pins/4b0d0ac), sonst ~/.local/state/ctox-sync-v4/stub.
// Lauf: CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/4b0d0ac node --test src/apps/business-os/modules/notes/tests/v4-api.test.mjs
// Screenshots (1280/420) nach tests/shots/ mit NOTES_SHOTS=1.
// notes hat keine Befehlstypen (V4-PORT.md): der Befehlsfall entfällt, Schreibfehler gehen in die Zone.
import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { dirname, join, normalize, extname } from 'node:path';
import { homedir } from 'node:os';

const STUB = process.env.CTOX_APP_API_STUB || join(homedir(), '.local/state/ctox-sync-v4/stub');
const HERE = dirname(fileURLToPath(import.meta.url));
const BOS = normalize(join(HERE, '../../..'));
const require = createRequire(import.meta.url);
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || join(homedir(), 'Documents/ctox/src/apps/business-os/node_modules/playwright'));
const TYPES = { '.js': 'text/javascript', '.mjs': 'text/javascript', '.css': 'text/css', '.html': 'text/html', '.json': 'application/json', '.svg': 'image/svg+xml' };
const APP = 'notes';

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
  await page.evaluate(async (APP) => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    window.__mod = await import(`/modules/${APP}/index.js`);
    const kernel = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false });
    await kernel.ready;
    await kernel.sim.push('notes', [
      { id: 'n-alpha', title: 'Alpha Notiz', content: '<h1>Alpha Notiz</h1><p>Alpha Inhalt</p>', folder: 'Notes', notebook: 'Ops', tags: 'team', is_favorite: true, is_trashed: false, is_locked: false, lock_passcode: '', updated_at_ms: 2000 },
      { id: 'n-beta', title: 'Beta Notiz', content: '<h1>Beta Notiz</h1><p>Beta Inhalt</p>', folder: 'Notes', notebook: 'Kunden', tags: '', is_favorite: false, is_trashed: false, is_locked: false, lock_passcode: '', updated_at_ms: 1000 },
    ]);
    await kernel.sim.settle();
    const api = kernel.appApi(APP, '1');
    window.__k = kernel;
    window.__api = api;
    window.__zone = () => kernel.statusZone.forApp(APP).get().filter((e) => e.source === 'app');
    window.__notes = () => api.query('notes', {}).get();
  }, APP);
});

after(async () => { await browser?.close(); await new Promise((r) => server?.close(r)); });

const settle = async () => { await page.evaluate(() => window.__k.sim.settle()); await page.waitForTimeout(250); };
const listText = () => page.$eval('[data-notes-list]', (el) => el.textContent);

test('mount: rendert Live-Daten, nur api.*, gescoptes Root, Titel über api.shell', async () => {
  await page.evaluate(async () => {
    const { countingApi } = await import('/v4/tools/port/test-api.mjs');
    window.__c = countingApi(window.__api);
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, {});
  });
  await settle();
  const list = await listText();
  assert.match(list, /Alpha Notiz/);
  assert.match(list, /Beta Notiz/);
  assert.equal(await page.$eval('#win', (el) => el.dataset.app), 'notes');
  assert.equal(await page.evaluate(() => window.__k.shell.window('notes').state.get().title), 'Notizen');
  assert.equal(await page.evaluate(() => window.__c.subscribers('notes')), 1);
  // Vorauswahl der ersten Notiz wie bisher
  assert.equal(await page.evaluate(() => window.__app.snapshot().activeNoteId), 'n-alpha');
  // Seeding greift nicht, wenn Daten da sind
  assert.equal(await page.evaluate(() => window.__notes().length), 2);
  assert.deepEqual(page.__errors, []);
});

test('Rundgang RUND2: Öffnen und Auswählen einer Notiz schreibt nichts (kein Autosave beim Laden in den Editor)', async () => {
  const before = await page.evaluate(() => Object.fromEntries(window.__notes().map((n) => [n.id, n.updated_at_ms])));
  const ids = Object.keys(before);
  assert.ok(ids.length >= 2, 'mindestens zwei Notizen');
  for (const id of ids.slice(0, 3)) {
    await page.evaluate((nid) => document.querySelector(`#win [data-note-id="${nid}"]`)?.click(), id);
    await page.waitForTimeout(400);
  }
  await settle();
  const after = await page.evaluate(() => Object.fromEntries(window.__notes().map((n) => [n.id, n.updated_at_ms])));
  assert.deepEqual(after, before);
});

test('Live-Änderung vom Core (sim.push) erscheint ohne Neuladen, Löschung verschwindet', async () => {
  await page.evaluate(() => window.__k.sim.push('notes', { id: 'n-gamma', title: 'Gamma Live', content: '<p>Gamma</p>', is_trashed: false, updated_at_ms: 500 }));
  await settle();
  assert.match(await listText(), /Gamma Live/);
  await page.evaluate(() => window.__k.sim.push('notes', { id: 'n-gamma', _deleted: true }));
  await settle();
  assert.doesNotMatch(await listText(), /Gamma Live/);
});

test('leere Collection (Erstabgleich läuft): kein Text, keine Zone, kein Seeding; danach Beispielnotizen', async () => {
  const res = await page.evaluate(async () => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    const k2 = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false, instanceId: 'empty' });
    await k2.ready;
    k2.sim.setBacklog(5);
    const host = document.createElement('div');
    document.body.append(host);
    const app = await window.__mod.mount(host, k2.appApi('notes', '1'), {});
    await new Promise((r) => setTimeout(r, 300));
    const listText = host.querySelector('[data-notes-list]').textContent.trim();
    const hasStateBox = Boolean(host.querySelector('.ctox-syncing, .ctox-empty, [role="status"]'));
    const zone = k2.statusZone.forApp('notes').get().map((e) => e.code);
    const docsWhileBacklog = k2.appApi('notes', '1').query('notes', {}).get().length;
    await app.unmount();
    // Ohne Rückstand: Fachverhalten wie bisher – leere Collection wird einmal mit Beispielnotizen gefüllt.
    k2.sim.setBacklog(0);
    await k2.sim.settle();
    const app2 = await window.__mod.mount(host, k2.appApi('notes', '1'), {});
    await k2.sim.settle();
    await new Promise((r) => setTimeout(r, 300));
    const seeded = k2.appApi('notes', '1').query('notes', {}).get().map((d) => d.id).sort();
    const seededList = host.querySelector('[data-notes-list]').textContent;
    await app2.unmount();
    host.remove();
    await k2.close();
    return { listText, hasStateBox, zone, docsWhileBacklog, seeded, seededList };
  });
  assert.equal(res.listText, '');
  assert.equal(res.hasStateBox, false);
  assert.deepEqual(res.zone, []);
  assert.equal(res.docsWhileBacklog, 0);
  assert.deepEqual(res.seeded, ['notes_seed_meeting_followup', 'notes_seed_ops_review', 'notes_seed_product_notes']);
  assert.match(res.seededList, /Operations Review/);
  // Modulzustand ist modulweit: Hauptfenster neu montieren.
  await page.evaluate(async () => {
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, {});
  });
  await settle();
});

test('Suche ohne Treffer rendert nichts (kein „Keine passenden Notizen“)', async () => {
  await page.fill('[data-pg-search]', 'zzz-kein-treffer');
  await settle();
  assert.equal((await listText()).trim(), '');
  await page.fill('[data-pg-search]', '');
  await settle();
  assert.match(await listText(), /Alpha Notiz/);
});

test('Schreiben: Entwurf speichern → api.put, angenommen, live in der Liste; Hinweis nur in der Zone', async () => {
  await page.click('[data-action="create-note"]');
  await settle();
  await page.click('[data-action="save-draft-note"]');
  await settle();
  const res = await page.evaluate(() => ({ notes: window.__notes(), zone: window.__zone(), outbox: window.__k.sim.outbox ? null : null }));
  assert.equal(res.notes.length, 3);
  const created = res.notes.find((n) => n.id !== 'n-alpha' && n.id !== 'n-beta');
  assert.ok(created, 'neue Notiz liegt in der Collection');
  assert.equal('__notes_draft' in created, false);
  assert.ok(res.zone.some((e) => e.code === 'notes.notice' && /Entwurf gespeichert/.test(e.text)));
  assert.match(await listText(), new RegExp(created.title));
  // Direkter put über die API wird vom Core angenommen
  const result = await page.evaluate(async () => {
    const h = window.__api.put('notes', { id: 'n-delta', title: 'Delta Put', content: '<p>Delta</p>', is_trashed: false, updated_at_ms: 3000 });
    await h.local;
    return h.result;
  });
  assert.equal(result.status, 'accepted');
  await settle();
  assert.match(await listText(), /Delta Put/);
});

test('Ablehnung: ungültiger Write geht genau einmal in die Zone (statt Text im Inhalt)', async () => {
  // Papierkorb verlangt Bestätigung über api.shell.dialog; Stub antwortet default:false → nichts passiert.
  const before = await page.evaluate(() => window.__notes().filter((n) => n.is_trashed).length);
  await page.evaluate(() => { const id = window.__app.snapshot().activeNoteId; void id; document.querySelector('[data-action="delete-note"]').click(); });
  await settle();
  const r = await page.evaluate(() => ({ trashed: window.__notes().filter((n) => n.is_trashed).length, dialogs: window.__k.sim.dialogs.length }));
  assert.equal(r.trashed, before);
  assert.ok(r.dialogs >= 1);
  // Mit Ja: Papierkorb per patch, Rückgängig-Knopf im Fenster, Text in der Zone.
  await page.evaluate(async () => {
    const { withDialogAnswer } = await import('/v4/tools/port/test-api.mjs');
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), withDialogAnswer(window.__c.api, true), {});
  });
  await settle();
  const target = await page.evaluate(() => window.__app.snapshot().activeNoteId);
  await page.evaluate(() => document.querySelector('[data-action="delete-note"]').click());
  await settle();
  const after = await page.evaluate((id) => ({ doc: window.__notes().find((n) => n.id === id), zone: window.__zone(), toast: document.querySelector('.nn-action-toast')?.textContent.trim() }), target);
  assert.equal(after.doc.is_trashed, true);
  assert.ok(after.zone.some((e) => e.code === 'notes.notice' && /Papierkorb/.test(e.text)));
  assert.equal(after.toast, 'Rückgängig');
  await page.click('.nn-action-toast button');
  await settle();
  assert.equal(await page.evaluate((id) => window.__notes().find((n) => n.id === id).is_trashed, target), false);
  assert.equal((await page.evaluate(() => window.__zone())).filter((e) => e.code === 'notes.notice').length, 0);
});

test('snapshot/restored: offener Entwurf, Suche und Auswahl überleben den Remount', async () => {
  await page.evaluate(() => {
    const card = document.querySelector('[data-note-id="n-beta"]');
    card?.click();
  });
  await settle();
  assert.equal(await page.evaluate(() => window.__app.snapshot().activeNoteId), 'n-beta');
  await page.click('[data-action="create-note"]');
  await settle();
  const snap = await page.evaluate(async () => {
    const cloned = structuredClone(window.__app.snapshot());
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, { restored: cloned });
    return cloned;
  });
  assert.ok(snap.draft && snap.draft.id, 'Entwurf im Snapshot');
  assert.ok(JSON.stringify(snap).length < 1024 * 1024);
  await settle();
  const after = await page.evaluate(() => ({
    active: window.__app.snapshot().activeNoteId,
    draft: window.__app.snapshot().draft?.id,
    banner: Boolean(document.querySelector('.nn-draft-actions [data-action="save-draft-note"]'))
      && !document.querySelector('.nn-draft-actions .ctox-callout, .nn-draft-banner')
      && document.querySelector('.nn-draft-actions [data-action="save-draft-note"]').title === 'Entwurf',
  }));
  assert.equal(after.active, snap.draft.id);
  assert.equal(after.draft, snap.draft.id);
  assert.equal(after.banner, true);
  assert.equal(await page.evaluate(() => window.__c.subscribers('notes')), 1, 'alte Abos sind abgemeldet');
  // Entwurf verwerfen, Auswahl auf Beta, Remount hält Auswahl
  await page.click('[data-action="discard-draft-note"]');
  await settle();
  await page.evaluate(() => document.querySelector('[data-note-id="n-beta"]')?.click());
  await settle();
  const snap2 = await page.evaluate(async () => {
    const cloned = structuredClone(window.__app.snapshot());
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, { restored: cloned });
    return cloned;
  });
  await settle();
  assert.equal(snap2.activeNoteId, 'n-beta');
  assert.equal(await page.evaluate(() => window.__app.snapshot().activeNoteId), 'n-beta');
  assert.deepEqual(page.__errors, []);
});

test('Listenkopf: Hauptaktionen sichtbar, Import/Export/Sperren im „…“-Menü (Textdiät Regel 6)', async () => {
  const head = await page.evaluate(() => {
    const actions = document.querySelector('.notes-left > .ctox-pane-header .ctox-pane-actions');
    const visible = [...actions.querySelectorAll('button')].filter((b) => !b.closest('[data-list-more-menu]')).map((b) => b.dataset.action);
    return { visible };
  });
  assert.deepEqual(head.visible, ['create-note', 'create-notebook', 'create-tag', 'toggle-list-more']);
  await page.click('[data-action="toggle-list-more"]');
  const open = await page.evaluate(() => {
    const menu = document.querySelector('[data-list-more-menu]');
    return { hidden: menu.hidden, items: [...menu.querySelectorAll('.nn-dropdown-item')].map((b) => b.dataset.action),
      expanded: document.querySelector('[data-action="toggle-list-more"]').getAttribute('aria-expanded') };
  });
  assert.deepEqual(open, { hidden: false, items: ['import', 'export', 'lock-app'], expanded: 'true' });
  await page.keyboard.press('Escape');
  assert.equal(await page.evaluate(() => document.querySelector('[data-list-more-menu]').hidden), true);
});

test('Screenshots 1280 und 420', { skip: !process.env.NOTES_SHOTS && !process.env.V4_SHOTS }, async () => {
  const shots = join(HERE, 'shots');
  await page.screenshot({ path: join(shots, `${APP}-1280.png`) });
  await page.evaluate(() => { const w = document.getElementById('win'); w.style.width = '420px'; w.style.height = '760px'; });
  await page.setViewportSize({ width: 420, height: 760 });
  await page.waitForTimeout(300);
  await page.screenshot({ path: join(shots, `${APP}-420.png`) });
  await page.setViewportSize({ width: 1280, height: 760 });
  await page.evaluate(() => { const w = document.getElementById('win'); w.style.width = '1280px'; });
});

test('unmount meldet alle Abos ab und leert root', async () => {
  const res = await page.evaluate(async () => {
    await window.__app.unmount();
    await window.__k.sim.push('notes', { id: 'n-late', title: 'Late', updated_at_ms: 10 });
    await window.__k.sim.settle();
    return { subs: window.__c.subscribers('notes'), children: document.getElementById('win').children.length };
  });
  assert.equal(res.subs, 0);
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

test('FR5-1 Rahmen-Dialog wirkt: „Neues Notizbuch“ fragt den Namen im Fenster, Enter legt Notizbuch + Notiz an', async () => {
  await mountWithWindowSheet();
  await settle();
  const before = await page.evaluate(() => window.__notes().length);
  await page.evaluate(() => document.querySelector('#win [data-action="create-notebook"]').click());
  await page.waitForSelector('#win [data-v4-sheet] [data-v4-sheet-input]');
  const s = await sheetState();
  assert.deepEqual({ inAppRoot: s.inAppRoot, inert: s.inert, focus: s.focus }, { inAppRoot: false, inert: true, focus: true });
  await page.keyboard.type('Projekt FR5');
  await page.keyboard.press('Enter');
  await settle();
  assert.equal(await sheetState(), null);
  // Neues Notizbuch ist gewählt; die neue Notiz ist ein lokaler Entwurf (gespeichert erst beim Schreiben).
  const picked = await page.evaluate(() => [...document.querySelectorAll('#win select')].some((sel) => sel.value === 'Projekt FR5'));
  assert.equal(picked, true, 'Notizbuch „Projekt FR5“ angelegt und gewählt');
  assert.equal(await page.evaluate(() => window.__notes().length), before, 'Entwurf ohne Schreibvorgang');
  await unmountWindowSheet();
});
