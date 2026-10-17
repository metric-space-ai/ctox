// knowledge gegen den verbindlichen App-API-4.0-Stub des Kerns (createStubKernel,
// ~/.local/state/ctox-sync-v4/stub, read-only unter /__stub/ ausgeliefert) in echtem Chromium.
// Eigene Attrappe (v4/tools/port/test-api.mjs) nur, wo der Stub nicht abdeckt: Zähler der Abos.
// Lauf: node --test src/apps/business-os/modules/knowledge/tests/v4-api.test.mjs
// Playwright: aus PLAYWRIGHT_MODULE, sonst ~/Documents/ctox/src/apps/business-os/node_modules/playwright.
import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { dirname, join, normalize, extname } from 'node:path';
import { homedir } from 'node:os';
import { realManifestRoutes } from '../../../v4/tools/port/real-manifest.mjs';

// Pin 8265bd7: E48 – der Stub veröffentlicht ctox-platform selbst; Fremdlesen braucht die eigene Deklaration (echtes Manifest).
const STUB = process.env.CTOX_APP_API_STUB || join(homedir(), '.local/state/shell-v4/stub-pins/4b0d0ac');
const BOS = normalize(join(dirname(fileURLToPath(import.meta.url)), '../../..'));
const require = createRequire(import.meta.url);
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || join(homedir(), 'Documents/ctox/src/apps/business-os/node_modules/playwright'));
const TYPES = { '.js': 'text/javascript', '.mjs': 'text/javascript', '.css': 'text/css', '.html': 'text/html', '.json': 'application/json', '.svg': 'image/svg+xml' };

const PAGE = `<!doctype html><html><head><meta charset="utf-8">
<link rel="stylesheet" href="/modules/knowledge/index.css">
</head><body><div id="win" style="width:1200px;height:720px"></div></body></html>`;

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
      // Wie der Service Worker (§16.15): ctox:shared/x.js → Shell-Pfad umschreiben. Eine native Import-Map
      // greift für das Schema ctox: in Chromium nicht (gemessen: CORS-Block auf ctox:shared/…).
      if (/\.m?js$/.test(file)) body = Buffer.from(body.toString('utf8').replace(/(['"])ctox:shared\//g, '$1/shared/'));
      res.writeHead(200, { 'content-type': TYPES[extname(file)] || 'application/octet-stream' }); res.end(body); }
    catch { res.writeHead(404); res.end(); }
  });
  await new Promise((r) => server.listen(0, '127.0.0.1', r));
  base = `http://127.0.0.1:${server.address().port}/`;
  browser = await chromium.launch();
  page = await browser.newPage();
  const errors = [];
  page.on('pageerror', (e) => errors.push(e.message));
  page.on('console', (m) => { if (m.type() === 'error') console.error('[page]', m.text()); });
  page.on('requestfailed', (r) => console.error('[page] request failed', r.url()));
  page.on('response', (r) => { if (r.status() >= 400) console.error('[page] HTTP', r.status(), r.url()); });
  page.__errors = errors;
  await page.goto(base);
  await page.evaluate((routes) => { window.__routes = routes; }, realManifestRoutes('knowledge'));
  await page.evaluate(async () => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    window.__mod = await import('/modules/knowledge/index.js');
    const kernel = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false, actor: { role: 'admin', permissions: ['apps.source.edit'] } });
    await kernel.ready;
    await kernel.sim.push('business_apps', [{ id: 'ctox', title: 'CTOX' }, { id: 'knowledge', title: 'Knowledge' }]);
    // E48: echtes Manifest (foreign_collections business_commands) nach der business_apps-Zeile veröffentlichen.
    for (const o of window.__routes?._fixture_owners ?? []) if (!(await kernel.sim.inspect('business_apps', o.app))?.data?.active_ver) await kernel.sim.publishAppVersion(o.app, o.ver, o.publish);
    await kernel.sim.publishAppVersion('knowledge', '1.1.7', { title: 'Knowledge', manifestExtra: window.__routes });
    await kernel.sim.push('knowledge_items', [
      { id: 'skillbook:alpha', kind: 'skillbook', title: 'Alpha Skill', markdown: '# Alpha\n\nAlpha Inhalt', updated_at_ms: 2 },
      { id: 'skillbook:beta', kind: 'skillbook', title: 'Beta Skill', markdown: '# Beta\n\nBeta Inhalt', updated_at_ms: 1 },
    ]);
    await kernel.sim.settle();
    const api = kernel.appApi('knowledge', '1.1.7');
    window.__k = kernel;
    window.__api = api;
    window.__zone = () => kernel.statusZone.forApp('knowledge').get().filter((e) => e.source === 'app');
    window.__commands = () => api.query('business_commands', { sort: [{ created_at: 'asc' }] }).get();
  });
});

after(async () => { await browser?.close(); await new Promise((r) => server?.close(r)); });

const settle = async () => { await page.evaluate(() => window.__k.sim.settle()); await page.waitForTimeout(200); };
const text = (sel) => page.$eval(sel, (el) => el.textContent);

test('mount: rendert Live-Daten, nur api.*, gescoptes Root, Titel über api.shell', async () => {
  await page.evaluate(async () => {
    const { countingApi } = await import('/v4/tools/port/test-api.mjs');
    window.__c = countingApi(window.__api);
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, {});
  });
  await settle();
  const list = await text('[data-knowledge-list]');
  assert.match(list, /Alpha Skill/);
  assert.match(list, /Beta Skill/);
  assert.equal(await page.$eval('#win', (el) => el.dataset.app), 'knowledge');
  assert.equal(await page.evaluate(() => window.__k.shell.window('knowledge').state.get().title), 'Knowledge');
  assert.equal(await page.evaluate(() => window.__c.subscribers('knowledge_items')), 1);
  assert.deepEqual(page.__errors, []);
});

test('Rundgang RUND2: Quellen-/Tabellen-Umschalter brechen um statt seitlich zu scrollen', async () => {
  const r = await page.evaluate(() => {
    const host = document.querySelector('#win .knowledge-module') || document.querySelector('#win');
    const sw = document.createElement('div');
    sw.className = 'knowledge-secondary-switcher';
    sw.style.width = '420px';
    for (let i = 0; i < 9; i++) { const b = document.createElement('button'); b.className = 'ctox-chip'; b.textContent = `Quelle Nummer ${i} mit längerem Namen`; sw.append(b); }
    host.append(sw);
    const out = { over: sw.scrollWidth - sw.clientWidth, wrap: getComputedStyle(sw).flexWrap };
    sw.remove();
    return out;
  });
  assert.deepEqual(r, { over: 0, wrap: 'wrap' });
});

test('Live-Änderung vom Core (sim.push) erscheint ohne Neuladen, Löschung verschwindet', async () => {
  await page.evaluate(() => window.__k.sim.push('knowledge_items', { id: 'skillbook:gamma', kind: 'skillbook', title: 'Gamma Live', markdown: '# Gamma', updated_at_ms: 3 }));
  await settle();
  assert.match(await text('[data-knowledge-list]'), /Gamma Live/);
  await page.evaluate(() => window.__k.sim.push('knowledge_items', { id: 'skillbook:gamma', _deleted: true }));
  await settle();
  assert.doesNotMatch(await text('[data-knowledge-list]'), /Gamma Live/);
});

// FIX8 Mangel 6: Einträge zeigten „21.01.1970“ – Sekunden-Zeitstempel wurden als Millisekunden gelesen.
test('FIX8: Datum aus Sekunden, Millisekunden und ISO richtig; ohne Datum nichts (kein 1970, kein Ersatztext)', async () => {
  const sec = Date.UTC(2025, 9, 9, 12) / 1000; // 09.10.2025 in Sekunden
  await page.evaluate((sec) => window.__k.sim.push('knowledge_items', [
    { id: 'skillbook:sec', kind: 'skillbook', title: 'Sekunden Skill', markdown: '# S', updated_at_ms: sec },
    { id: 'skillbook:iso', kind: 'skillbook', title: 'Iso Skill', markdown: '# I', updated_at: '2026-03-05T10:00:00Z' },
    { id: 'skillbook:none', kind: 'skillbook', title: 'Ohne Datum Skill', markdown: '# N' },
  ]), sec);
  await settle();
  const rows = await page.evaluate(() => Object.fromEntries([...document.querySelectorAll('[data-knowledge-list] .bundle-select')]
    .map((b) => [b.querySelector('strong')?.textContent || '', b.textContent.replace(/\s+/g, ' ').trim()])));
  assert.match(rows['Sekunden Skill'] || '', /09\.10\.2025/, JSON.stringify(rows));
  assert.match(rows['Iso Skill'] || '', /05\.03\.2026/, JSON.stringify(rows));
  assert.ok(rows['Ohne Datum Skill'], JSON.stringify(rows));
  for (const [title, row] of Object.entries(rows)) {
    assert.doesNotMatch(row, /1970/, `kein Epoch-Datum bei ${title}`);
    assert.doesNotMatch(row, /Noch nicht bearbeitet|·\s*$/, `kein Ersatztext/kein loser Trenner bei ${title}`);
  }
  await page.evaluate(() => window.__k.sim.push('knowledge_items', [{ id: 'skillbook:sec', _deleted: true }, { id: 'skillbook:iso', _deleted: true }, { id: 'skillbook:none', _deleted: true }]));
  await settle();
});

test('leere Collections rendern keinen Zustandstext', async () => {
  const empty = await page.evaluate(async () => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    const k2 = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false, instanceId: 'empty' });
    await k2.ready;
    const host = document.createElement('div');
    document.body.append(host);
    const app = await window.__mod.mount(host, k2.appApi('knowledge', '1.1.7'), {});
    await new Promise((r) => setTimeout(r, 250));
    const listText = host.querySelector('[data-knowledge-list]').textContent.trim();
    const hasStateBox = Boolean(host.querySelector('.ctox-syncing, [role="status"]'));
    const zone = k2.statusZone.forApp('knowledge').get().map((e) => e.code);
    await app.unmount();
    host.remove();
    await k2.close();
    return { listText, hasStateBox, zone };
  });
  assert.equal(empty.listText, '');
  assert.equal(empty.hasStateBox, false);
  assert.deepEqual(empty.zone, []);
  // Modulzustand ist modulweit (V4-PORT.md OFFEN-4): Hauptfenster neu montieren.
  await page.evaluate(async () => {
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, {});
  });
  await settle();
});

test('Befehl: Markdown speichern → business_commands-Dokument; failed → Zustandszone', async () => {
  await page.evaluate(() => window.__k.sim.failCommand('ctox.knowledge.document.modify', 'kaputt'));
  await page.evaluate(() => document.querySelector('[data-action="edit-markdown"]').click());
  await page.fill('[data-markdown-editor]', '# Alpha\n\nGeändert');
  await page.evaluate(() => document.querySelector('[data-action="save-markdown"]').click());
  await settle();
  const cmd = await page.evaluate(() => window.__commands().at(-1));
  assert.equal(cmd.type, 'ctox.knowledge.document.modify');
  assert.equal(cmd.payload.record_id, await page.evaluate(() => window.__app.snapshot().selectedId));
  assert.match(cmd.payload.record_id, /^skillbook:/);
  assert.match(cmd.payload.markdown, /Geändert/);
  assert.equal(cmd.payload.module, 'knowledge');
  assert.equal(cmd.status, 'failed');
  const zone = await page.evaluate(() => window.__zone());
  assert.equal(zone.length, 1);
  assert.equal(zone[0].code, 'knowledge.command.ctox.knowledge.document.modify');
  assert.match(zone[0].text, /kaputt/);
});

test('Befehl mit Bestätigung: Löschen fragt über api.shell.dialog, ohne Ja kein Befehl', async () => {
  const before = await page.evaluate(() => window.__commands().length);
  await page.evaluate(() => document.querySelector('[data-action="delete-skill"]').click());
  await settle();
  const r1 = await page.evaluate(() => ({ n: window.__commands().length, dialogs: window.__k.sim.dialogs.length }));
  assert.equal(r1.n, before, 'Stub-Dialog liefert default:false → kein Befehl');
  assert.equal(r1.dialogs, 1);
  await page.evaluate(async () => {
    const { withDialogAnswer } = await import('/v4/tools/port/test-api.mjs');
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), withDialogAnswer(window.__c.api, true), {});
  });
  await settle();
  await page.evaluate(() => document.querySelector('[data-action="delete-skill"]').click());
  await settle();
  const cmd = await page.evaluate(() => window.__commands().at(-1));
  assert.equal(cmd.type, 'knowledge.entry.delete');
  assert.equal(cmd.status, 'done');
});

test('Schreiben: api.put erscheint live und wird vom Core angenommen', async () => {
  const result = await page.evaluate(async () => {
    const h = window.__api.put('knowledge_items', { id: 'skillbook:delta', kind: 'skillbook', title: 'Delta Put', markdown: '# Delta', updated_at_ms: 9 });
    await h.local;
    return h.result;
  });
  assert.equal(result.status, 'accepted');
  await settle();
  assert.match(await text('[data-knowledge-list]'), /Delta Put/);
});

test('snapshot/restored: offener Editor-Inhalt und Auswahl überleben den Remount', async () => {
  await page.evaluate(() => {
    const row = [...document.querySelectorAll('[data-knowledge-list] *')].find((el) => el.textContent.trim() === 'Beta Skill');
    (row?.closest('[data-knowledge-id], [data-id], button, li, article') || row)?.click();
  });
  await settle();
  const selectedBefore = await page.evaluate(() => window.__app.snapshot().selectedId);
  await page.evaluate(() => document.querySelector('[data-action="edit-markdown"]').click());
  await page.fill('[data-markdown-editor]', 'halb fertiger Entwurf');
  const snap = await page.evaluate(async () => {
    const cloned = structuredClone(window.__app.snapshot()); // Shell-Ablauf Entwurf §5
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, { restored: cloned });
    return cloned;
  });
  assert.equal(snap.editing, true);
  assert.equal(snap.markdownDraft, 'halb fertiger Entwurf');
  assert.ok(JSON.stringify(snap).length < 1024 * 1024);
  await settle();
  const after = await page.evaluate(() => ({
    editor: document.querySelector('[data-markdown-editor]').value,
    editorHidden: document.querySelector('[data-markdown-editor]').hidden,
    selected: window.__app.snapshot().selectedId,
  }));
  assert.equal(after.editor, 'halb fertiger Entwurf');
  assert.equal(after.editorHidden, false);
  assert.equal(after.selected, selectedBefore);
  assert.equal(await page.evaluate(() => window.__c.subscribers('knowledge_items')), 1, 'alte Abos sind abgemeldet');
  assert.deepEqual(page.__errors, []);
});

// E11: knowledge_table_rows = Projektion eines Sichtfensters. Die App schreibt knowledge_views per api.put; eine
// simulierte App-Laufzeit ruft für jede Änderung den echten Trigger-Handler (handlers/knowledge.view.window.js) mit
// simulierter Fähigkeit cap.dataframe.window und spielt dessen Schreibtransaktion als Core-Änderung (sim.push) ein.
const NO_STATE_TEXT = /lad|load|sync|warte|wait|keine|no rows|empty/i;

async function startTriggerRuntime() {
  await page.evaluate(async () => {
    const { default: handle } = await import('/modules/knowledge/handlers/knowledge.view.window.js');
    const { createDataframeSim, measuredRows } = await import('/modules/knowledge/handlers/tests/_dataframe-sim.mjs');
    const k = window.__k;
    const core = k.appApi('knowledge', '1.1.7');
    const sim = createDataframeSim({ 'kdt-big': measuredRows(300) });
    const queue = [];
    const seen = new Map();
    const results = [];
    let paused = true;
    let running = false;
    let seq = 0;
    let cursor = null;
    const read = async (collection, query = {}) => core.query(collection, query).get();
    const get = async (collection, id) => core.get(collection, id).get();
    const ctx = {
      read,
      get,
      async transaction(fn) {
        const ops = new Map();
        const stage = (collection, doc) => { if (!ops.has(collection)) ops.set(collection, []); ops.get(collection).push(doc); return { status: 'accepted' }; };
        const value = await fn({ read, get, put: async (c, doc) => stage(c, doc), remove: async (c, id) => stage(c, { id, _deleted: true }) });
        for (const [collection, docs] of ops) await k.sim.push(collection, docs);
        return value;
      },
      cap: { dataframe: { window: (request) => sim.window(request) } },
      cursor: { get: async () => cursor, set: async (value) => { cursor = value; } },
      now: () => Date.now(),
      log() {},
    };
    async function pump() {
      if (paused || running) return;
      running = true;
      try {
        while (queue.length && !paused) results.push(await handle(queue.shift(), ctx));
      } finally { running = false; }
    }
    window.__viewDocs = () => core.query('knowledge_views', {}).get();
    window.__rowsDocs = () => core.query('knowledge_table_rows', { sort: [{ row_index: 'asc' }] }).get();
    window.__runtimeOff = core.query('knowledge_views', {}).subscribe((docs) => {
      for (const doc of docs) {
        const json = JSON.stringify(doc);
        if (seen.get(doc.id) === json) continue;
        const op = seen.has(doc.id) ? 'update' : 'insert';
        seen.set(doc.id, json);
        queue.push({ type: 'trigger', trigger_id: `knowledge.view.window:${op}`, change: { collection: 'knowledge_views', op, doc, seq: ++seq } });
      }
      pump();
    });
    window.__runtime = {
      sim,
      results,
      resume() { paused = false; return pump(); },
      async idle() {
        for (let i = 0; i < 200 && (queue.length || running); i += 1) await new Promise((r) => setTimeout(r, 10));
      },
    };
  });
}

const runtimeIdle = async () => { await page.evaluate(() => window.__runtime.idle()); await settle(); };
const tableState = () => page.evaluate(() => {
  const host = document.querySelector('[data-dataframe-host]');
  return {
    hostText: host.textContent.trim(),
    rows: [...host.querySelectorAll('tbody tr')].map((tr) => tr.querySelector('td')?.textContent.trim()),
    meta: document.querySelector('[data-table-meta]').textContent,
    zone: window.__zone().map((e) => e.code).filter((code) => code.startsWith('knowledge.table')),
  };
});

test('Katalog-Tabelle (E11): Fenster schreiben → leeres Fenster ohne Lade-Text → eingespielte Zeilen erscheinen', async () => {
  await page.evaluate(async () => {
    await window.__k.sim.push('knowledge_tables', {
      id: 'table:kdt-big', kind: 'dataframe', title: 'Big Load Table', updated_at_ms: 30,
      projection_version: 2, rows_source: 'rxdb.rows.fetch', row_count: 300, rows_complete: true,
      columns: [{ key: 'row_id', label: 'Row' }, { key: 'rpm', label: 'RPM' }, { key: 'note', label: 'Note' }],
    });
    await window.__k.sim.settle();
  });
  await startTriggerRuntime();
  await page.evaluate(async () => {
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, {
      restored: { v: 1, selectedId: 'table:kdt-big', selectedTableId: 'table:kdt-big', activeTab: 'data' },
    });
  });
  await settle();

  // Fenster als Dokument geschrieben (api.put), Form nach collections.schema.json.
  const schema = await page.evaluate(async () => (await (await fetch('/modules/knowledge/collections.schema.json')).json()).collections);
  const views = await page.evaluate(() => window.__viewDocs());
  assert.equal(views.length, 1);
  const view = views[0];
  for (const key of schema.knowledge_views.required) assert.ok(key in view, `knowledge_views.${key}`);
  assert.equal(view.table, 'kdt-big');
  assert.deepEqual({ skip: view.skip, limit: view.limit, filter: view.filter, sort: view.sort }, { skip: 0, limit: 120, filter: {}, sort: [] });
  assert.equal(view.actor_id, await page.evaluate(() => window.__api.actor.id));

  // Leeres Fenster (Trigger noch nicht gelaufen): Kopf ohne Zeilen, kein Lade-/Zustandstext, keine Zone.
  const empty = await tableState();
  assert.deepEqual(empty.rows, []);
  assert.doesNotMatch(empty.hostText, NO_STATE_TEXT);
  assert.doesNotMatch(empty.meta, NO_STATE_TEXT);
  assert.deepEqual(empty.zone, []);
  assert.equal(await page.evaluate(() => window.__c.subscribers('knowledge_table_rows')), 1);

  // Laufzeit läuft: der Handler projiziert das Fenster, die Zeilen erscheinen live.
  await page.evaluate(() => window.__runtime.resume());
  await runtimeIdle();
  const shown = await tableState();
  assert.equal(shown.rows.length, 120);
  assert.equal(shown.rows[0], 'MLP-0001');
  assert.match(shown.meta, /Zeilen 1-120 von 300/);
  assert.deepEqual(await page.evaluate(() => window.__runtime.sim.calls.at(-1)), { table: 'kdt-big', skip: 0, limit: 120 });
  const rowDocs = await page.evaluate(() => window.__rowsDocs());
  assert.equal(rowDocs.length, 120);
  for (const key of schema.knowledge_table_rows.required) assert.ok(key in rowDocs[0], `knowledge_table_rows.${key}`);
  assert.equal(rowDocs[0].view_id, view.id);
  assert.deepEqual(page.__errors, []);
});

test('Katalog-Tabelle (E11): Blättern und Filtern aktualisieren dasselbe View-Dokument, Zeilen werden ersetzt', async () => {
  await page.evaluate(() => document.querySelector('[data-action="next-rows"]').click());
  await settle();
  let views = await page.evaluate(() => window.__viewDocs());
  assert.equal(views.length, 1, 'ein View je Nutzer und Tabelle');
  assert.equal(views[0].skip, 120);
  await runtimeIdle();
  let shown = await tableState();
  assert.equal(shown.rows[0], 'MLP-0121');
  assert.match(shown.meta, /Zeilen 121-240 von 300/);

  await page.fill('[data-table-filter]', 'peak');
  await page.waitForTimeout(400);
  await settle();
  views = await page.evaluate(() => window.__viewDocs());
  assert.deepEqual({ skip: views[0].skip, filter: views[0].filter }, { skip: 0, filter: { text: 'peak' } });
  await runtimeIdle();
  shown = await tableState();
  assert.equal(shown.rows.length, 43);
  assert.match(shown.meta, /von 43/);
  assert.equal(await page.evaluate(() => window.__rowsDocs().length), 43, 'alte Fensterzeilen sind ersetzt');

  // Leeres Ergebnis: Fenster leer, weiterhin kein Zustandstext.
  await page.fill('[data-table-filter]', 'gibt-es-nicht');
  await page.waitForTimeout(400);
  await runtimeIdle();
  const none = await tableState();
  assert.deepEqual(none.rows, []);
  assert.doesNotMatch(none.hostText, NO_STATE_TEXT);
  assert.deepEqual(none.zone, []);

  const snap = await page.evaluate(() => structuredClone(window.__app.snapshot()));
  assert.equal(snap.tableFilter, 'gibt-es-nicht');
  assert.equal(await page.evaluate(() => window.__c.subscribers('knowledge_table_rows')), 1);
  await page.evaluate(() => window.__runtimeOff());
  assert.deepEqual(page.__errors, []);
});

test('Aufträge (KNOW2): Fehler je Ziel einmal in der Zone, „Ausblenden“ quittiert am Datensatz, neuer Erfolg räumt', async () => {
  const r = await page.evaluate(async () => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    const k = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false, instanceId: 'tasks', actor: { id: 'u-chef', role: 'admin' } });
    await k.ready;
    const task = (id, status, record_id, created, extra = {}) => ({ id, command_id: `cmd-${id}`, command_type: 'ctox.knowledge.book.import', record_id, title: `Import ${id}`, status, created_at_ms: created, updated_at_ms: created, ...(status === 'completed' ? {} : { error: { message: `kaputt ${id}` } }), ...extra });
    await k.sim.push('knowledge_tasks', [
      task('t0', 'failed', 'ziel-a', 5),                                   // überholt vom jüngeren Fehler t1
      task('t1', 'failed', 'ziel-a', 10),                                  // → genau ein Eintrag
      task('t2', 'cancelled', 'ziel-b', 20, { acknowledged_at: '2026-10-15T10:00:00.000Z' }), // quittiert
      task('t3', 'failed', 'ziel-c', 30),
      task('t4', 'completed', 'ziel-c', 40),                               // neuerer Erfolg desselben Ziels
    ]);
    await k.sim.settle();
    const base = k.appApi('knowledge', '1.1.7');
    const actions = new Map();
    const api = { ...base, status: {
      report(item) { if (item.action) actions.set(item.code, item.action); base.status.report(item); },
      clear(code) { actions.delete(code); base.status.clear(code); },
    } };
    // Modulzustand ist modulweit (V4-PORT.md OFFEN-4): Hauptfenster vorher abbauen, danach neu montieren.
    await window.__app.unmount();
    const zone = () => k.statusZone.forApp('knowledge').get().filter((e) => e.source === 'app' && e.code.startsWith('knowledge.task.')).map((e) => ({ code: e.code, ref: e.ref, level: e.level }));
    const wait = async () => { await k.sim.settle(); await new Promise((res) => setTimeout(res, 200)); };
    const host = document.createElement('div');
    document.body.append(host);
    const out = {};
    let app = await window.__mod.mount(host, api, {});
    await wait();
    out.first = zone();
    out.label = [...actions.values()].map((a) => a.label);
    await app.unmount();
    app = await window.__mod.mount(host, api, {});
    await wait();
    out.remount = zone();
    out.ran = await [...actions.values()][0].run();
    await wait();
    out.acked = k.appApi('knowledge', '1.1.7').get('knowledge_tasks', 't1').get();
    out.afterAck = zone();
    await app.unmount();
    app = await window.__mod.mount(host, api, {});
    await wait();
    out.afterAckRemount = zone();
    await k.sim.push('knowledge_tasks', [task('t5', 'failed', 'ziel-d', 50)]);
    await wait();
    out.newFailure = zone();
    await k.sim.push('knowledge_tasks', [task('t6', 'completed', 'ziel-d', 60)]);
    await wait();
    out.afterSuccess = zone();
    await app.unmount();
    host.remove();
    await k.close();
    return out;
  });
  const one = [{ code: 'knowledge.task.ctox.knowledge.book.import:ziel-a', ref: 'cmd-t1', level: 'error' }];
  assert.deepEqual(r.first, one);
  assert.deepEqual(r.label, ['Ausblenden']);
  assert.deepEqual(r.remount, one);
  assert.equal(r.ran, true);
  assert.equal(typeof r.acked.acknowledged_at, 'string');
  assert.equal(r.acked.acknowledged_by, 'u-chef');
  assert.equal(r.acked.status, 'failed');
  assert.deepEqual(r.afterAck, []);
  assert.deepEqual(r.afterAckRemount, []);
  assert.deepEqual(r.newFailure, [{ code: 'knowledge.task.ctox.knowledge.book.import:ziel-d', ref: 'cmd-t5', level: 'error' }]);
  assert.deepEqual(r.afterSuccess, []);
  assert.deepEqual(page.__errors, []);
  await page.evaluate(async () => {
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, {});
  });
  await settle();
});

test('unmount meldet alle Abos ab und leert root', async () => {
  const res = await page.evaluate(async () => {
    await window.__app.unmount();
    await window.__k.sim.push('knowledge_items', { id: 'skillbook:late', kind: 'skillbook', title: 'Late', updated_at_ms: 10 });
    await window.__k.sim.settle();
    return {
      subs: ['knowledge_items', 'knowledge_runbooks', 'knowledge_tables', 'knowledge_table_rows'].map((c) => window.__c.subscribers(c)),
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

test('FR5-1 Rahmen-Dialog wirkt: „Löschen“ fragt im Fenster (rot), Esc bricht ab, Enter löscht', async () => {
  await mountWithWindowSheet();
  await settle();
  const before = await page.evaluate(() => window.__commands().length);
  await page.click('#win [data-action="delete-skill"]');
  await page.waitForSelector('#win [data-v4-sheet]');
  const s = await sheetState();
  assert.equal(s.inAppRoot, false);
  assert.equal(s.inert, true);
  assert.equal(s.ok, 'Löschen');
  await page.keyboard.press('Escape');
  await settle();
  assert.equal(await sheetState(), null);
  assert.equal(await page.evaluate(() => window.__commands().length), before, 'Esc → kein Befehl');
  await page.click('#win [data-action="delete-skill"]');
  await page.waitForSelector('#win [data-v4-sheet]');
  await page.keyboard.press('Enter');
  await settle();
  const cmd = await page.evaluate(() => window.__commands().at(-1));
  assert.equal(cmd.type, 'knowledge.entry.delete');
  await unmountWindowSheet();
});
