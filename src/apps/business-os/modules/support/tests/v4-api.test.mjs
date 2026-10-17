// support gegen den eingefrorenen App-API-4.0-Kern-Stub (createStubKernel) in echtem Chromium.
// Stub: CTOX_APP_API_STUB (Pflicht-Pin ~/.local/state/shell-v4/stub-pins/64acc2c), read-only unter /__stub/.
// Eigene Attrappe (v4/tools/port/test-api.mjs) nur für den Abo-Zähler. Muster: modules/knowledge/tests/v4-api.test.mjs.
// Lauf: CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/64acc2c node --test src/apps/business-os/modules/support/tests/v4-api.test.mjs
// Screenshots (1280 / 420) landen in modules/support/tests/shots/.
import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { readFile, mkdir } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { dirname, join, normalize, extname } from 'node:path';
import { homedir } from 'node:os';
import { realManifestRoutes } from '../../../v4/tools/port/real-manifest.mjs';

const STUB = process.env.CTOX_APP_API_STUB || join(homedir(), '.local/state/shell-v4/stub-pins/64acc2c');
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
<link rel="stylesheet" href="/modules/support/index.css">
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
  // Befehlsrouten/Deklarationen der echten support-Version (v4/manifests/support.manifest.json); ohne aktive Version
  // lehnt der Kern seit E26/E27 support.* beim Schreiben mit unknown_command ab.
  await page.evaluate((routes) => { window.__routes = routes; }, realManifestRoutes('support'));
  await page.evaluate(async () => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    window.__mod = await import('/modules/support/index.js');
    const kernel = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false, userId: 'agent-1', actor: { id: 'agent-1', role: 'admin', permissions: ['apps.source.edit'] } });
    await kernel.ready;
    // FIXT-R: Eigentümer der Fremd-Collections (echte Manifeste, inkl. ctox-platform) vor der eigenen Version.
    for (const o of window.__routes?._fixture_owners ?? []) if (!(await kernel.sim.inspect('business_apps', o.app))?.data?.active_ver) await kernel.sim.publishAppVersion(o.app, o.ver, o.publish);
    await kernel.sim.publishAppVersion('support', '0.1.2', { title: 'Support', manifestExtra: window.__routes });
    const now = Date.now();
    await kernel.sim.push('support_conversations', [
      { id: 'conv_a', status: 'open', priority: 'high', inbox_id: 'inbox-mail', assignee_id: '', search_text: 'Alpha GmbH Rechnung fehlt', primary_thread_key: 'mail/alpha', updated_at_ms: now - 2000, last_activity_at_ms: now - 2000 },
      { id: 'conv_b', status: 'open', priority: 'normal', inbox_id: 'inbox-mail', assignee_id: 'agent-2', search_text: 'Beta AG Login defekt', primary_thread_key: 'mail/beta', updated_at_ms: now - 1000, last_activity_at_ms: now - 1000 },
    ]);
    await kernel.sim.push('support_notes', [{ id: 'note_1', conversation_id: 'conv_a', body: 'Kunde ruft morgen an', visibility: 'internal', created_at_ms: now - 1500, updated_at_ms: now - 1500 }]);
    await kernel.sim.push('communication_messages', [
      { id: 'msg_1', thread_key: 'mail/alpha', direction: 'inbound', channel: 'email', body_text: 'Die Rechnung 4711 fehlt.', observed_at: new Date(now - 3000).toISOString() },
      { id: 'msg_b', thread_key: 'mail/beta', direction: 'inbound', channel: 'email', subject: 'Beta AG Login defekt', body_text: 'Seit heute früh geht nichts.', observed_at: new Date(now - 2500).toISOString() },
      { id: 'msg_d', thread_key: 'mail/delta', direction: 'inbound', channel: 'email', subject: 'Delta Put Fall', observed_at: new Date(now - 2400).toISOString() },
    ]);
    await kernel.sim.push('customer_contacts', [{ id: 'ct_gamma', first_name: 'Gerda', last_name: 'Gamma', email: 'gerda@gamma.example', updated_at_ms: now }]);
    await kernel.sim.settle();
    window.__k = kernel;
    window.__api = kernel.appApi('support', '0.1.2');
    window.__zone = () => kernel.statusZone.forApp('support').get().filter((e) => e.source === 'app');
    window.__commands = () => window.__api.query('business_commands', { sort: [{ created_at: 'asc' }] }).get();
  });
});

after(async () => { await browser?.close(); await new Promise((r) => server?.close(r)); });

const settle = async () => { await page.evaluate(() => window.__k.sim.settle()); await page.waitForTimeout(250); };
const text = (sel) => page.$eval(sel, (el) => el.textContent);
const clickSel = (sel) => page.evaluate((s) => document.querySelector(s).click(), sel);

test('mount: rendert Live-Daten, nur api.*, gescoptes Root, Titel über api.shell', async () => {
  await page.evaluate(async () => {
    const { countingApi } = await import('/v4/tools/port/test-api.mjs');
    window.__c = countingApi(window.__api);
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, {});
  });
  await settle();
  const list = await text('[data-support-list]');
  // Titel nach Zendesk: Anfang des Texts der ersten Nachricht bzw. ihr Betreff, nie der Suchtext.
  assert.match(list, /Die Rechnung 4711 fehlt/);
  assert.match(list, /Beta AG Login defekt/);
  assert.doesNotMatch(list, /Alpha GmbH Rechnung fehlt/);
  assert.equal(await page.$eval('#win', (el) => el.dataset.app), 'support');
  assert.equal(await page.evaluate(() => window.__k.shell.window('support').state.get().title), 'Support');
  assert.equal(await page.evaluate(() => window.__c.subscribers('support_conversations')), 1);
  // Band-Zählung über die selbst verdrahtete Pane-Grammatik (autoWirePaneGrammar(root)).
  assert.equal((await text('[data-pg-count="open"]')).trim(), '(2)');
  assert.equal(await page.$eval('.support-left', (el) => el.dataset.pgWired), 'true');
  assert.deepEqual(page.__errors, []);
});

test('Auswahl zeigt Timeline aus Nachricht + Notiz und Kontext', async () => {
  await clickSel('[data-support-conversation-id="conv_a"]');
  await settle();
  const timeline = await text('[data-support-timeline]');
  assert.match(timeline, /Rechnung 4711 fehlt/);
  assert.match(timeline, /Kunde ruft morgen an/);
  assert.equal(await page.$eval('[data-support-root]', (el) => el.classList.contains('is-context-hidden')), false);
  assert.match(await text('[data-support-context]'), /high/);
});

test('Live-Änderung vom Core (sim.push) erscheint ohne Neuladen, Löschung verschwindet', async () => {
  await page.evaluate(() => window.__k.sim.push('support_conversations', { id: 'conv_live', status: 'open', priority: 'urgent', search_text: 'Gamma Live Fall', customer_contact_id: 'ct_gamma', updated_at_ms: Date.now() }));
  await settle();
  assert.match(await text('[data-support-list]'), /Anfrage von Gerda Gamma/);
  await page.evaluate(() => window.__k.sim.push('support_conversations', { id: 'conv_live', _deleted: true }));
  await settle();
  assert.doesNotMatch(await text('[data-support-list]'), /Gerda Gamma/);
});

test('leere Collections rendern keinen Zustandstext und keine Zone', async () => {
  const empty = await page.evaluate(async () => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    const k2 = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false, instanceId: 'empty' });
    await k2.ready;
    // E48 (Stub 8265bd7): auch der leere Kern braucht die veröffentlichte Version mit Deklarationen, sonst sperrt er das
    // Lesen von business_commands/ctox_queue_tasks (Eigentümer ctox-platform).
    for (const o of window.__routes?._fixture_owners ?? []) if (!(await k2.sim.inspect('business_apps', o.app))?.data?.active_ver) await k2.sim.publishAppVersion(o.app, o.ver, o.publish);
    await k2.sim.publishAppVersion('support', '0.1.2', { title: 'Support', manifestExtra: window.__routes });
    const host = document.createElement('div');
    document.body.append(host);
    const app = await window.__mod.mount(host, k2.appApi('support', '0.1.2'), {});
    await new Promise((r) => setTimeout(r, 300));
    const out = {
      listText: host.querySelector('[data-support-list]').textContent.trim(),
      timelineText: host.querySelector('[data-support-timeline]').textContent.trim(),
      contextText: host.querySelector('[data-support-context]').textContent.trim(),
      title: host.querySelector('[data-support-title]').textContent.trim(),
      hasStateBox: Boolean(host.querySelector('.ctox-syncing, .ctox-empty, [role="status"]')),
      zone: k2.statusZone.forApp('support').get().map((e) => e.code),
    };
    await app.unmount();
    host.remove();
    await k2.close();
    return out;
  });
  assert.deepEqual(empty, { listText: '', timelineText: '', contextText: '', title: '', hasStateBox: false, zone: [] });
  // Modulzustand ist modulweit (V4-PORT.md OFFEN-3, wie knowledge OFFEN-4): Hauptfenster neu montieren.
  await page.evaluate(async () => {
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, {});
  });
  await settle();
});

test('Befehl: Übernehmen → business_commands-Dokument mit Handler-Payload und record_id; failed → genau ein Zoneneintrag', async () => {
  await page.evaluate(() => window.__k.sim.failCommand('support.conversation.claim', 'already assigned'));
  await clickSel('[data-support-conversation-id="conv_b"]');
  await clickSel('[data-support-action="claim"]');
  await settle();
  const cmd = await page.evaluate(() => window.__commands().at(-1));
  assert.equal(cmd.type, 'support.conversation.claim');
  assert.equal(cmd.app_id, 'support');
  assert.equal(cmd.record_id, 'conv_b');
  assert.equal(cmd.payload.conversation_id, 'conv_b');
  assert.equal(cmd.payload.client_context.surface, 'support.conversation.claim');
  assert.equal(Object.hasOwn(cmd.payload, 'command_type'), false);
  assert.equal(cmd.status, 'failed');
  const zone = await page.evaluate(() => window.__zone());
  assert.equal(zone.length, 1);
  assert.equal(zone[0].code, 'support.command.support.conversation.claim');
  assert.match(zone[0].text, /already assigned/);
  // Optimistische Zuordnung ist zurückgenommen: conv_b bleibt bei agent-2.
  assert.match(await text('[data-support-context]'), /agent-2/);
  // Keine Zustandstexte im Inhalt.
  assert.doesNotMatch(await text('[data-support-root]'), /übergeben|synchronisiert/);
});

test('Befehl: Notiz → support.note.create, lokal sichtbar, done ohne Zoneneintrag', async () => {
  const before = await page.evaluate(() => window.__zone().length);
  await page.fill('[data-support-note]', 'Rückruf vereinbart');
  await clickSel('[data-support-note-submit]');
  assert.match(await text('[data-support-timeline]'), /Rückruf vereinbart/);
  await settle();
  const cmd = await page.evaluate(() => window.__commands().at(-1));
  assert.equal(cmd.type, 'support.note.create');
  assert.deepEqual(
    { conversation_id: cmd.payload.conversation_id, body: cmd.payload.body, visibility: cmd.payload.visibility, source: cmd.payload.source },
    { conversation_id: 'conv_b', body: 'Rückruf vereinbart', visibility: 'internal', source: 'business-os.support' },
  );
  assert.equal(cmd.status, 'done');
  assert.equal(await page.evaluate(() => window.__zone().length), before);
});

// Strang SKL-A1: „CTOX fragen“ sendet den App-Befehl support.agent.task; Fehlschlag/Abbruch des Agentenauftrags
// (support_agent_requests, vom Abschlussbefehl geschrieben) erscheint live in der Zone, completed räumt.
test('CTOX fragen → support.agent.task; Auftrag failed/cancelled → Zone, completed räumt', async () => {
  await clickSel('[data-support-ask-ctox]');
  await settle();
  const cmd = await page.evaluate(() => window.__commands().at(-1));
  assert.equal(cmd.type, 'support.agent.task');
  assert.equal(cmd.record_id, 'conv_b');
  assert.equal(cmd.payload.conversation_id, 'conv_b');
  assert.equal(cmd.payload.request_kind, 'summary');
  assert.equal(cmd.payload.record_snapshot.conversation.id, 'conv_b');
  assert.equal(Object.hasOwn(cmd.payload, 'instruction'), false);
  const zone = await page.evaluate(async () => {
    const now = Date.now();
    await window.__k.sim.push('support_agent_requests', [
      { id: 'req_old', conversation_id: 'conv_b', request_kind: 'summary', status: 'failed', payload: { error: 'alt' }, updated_at_ms: now - 3_600_000 },
      { id: 'req_1', conversation_id: 'conv_b', request_kind: 'summary', status: 'failed', command_id: 'cmd_x', payload: { error: 'model quota exhausted' }, updated_at_ms: now },
      { id: 'req_2', conversation_id: 'conv_b', request_kind: 'summary', status: 'cancelled', payload: { error: 'support agent task cancelled' }, updated_at_ms: now },
    ]);
    await window.__k.sim.settle();
    await new Promise((r) => setTimeout(r, 250));
    const after = window.__zone().map((e) => ({ code: e.code, level: e.level, text: e.text }));
    await window.__k.sim.push('support_agent_requests', { id: 'req_1', conversation_id: 'conv_b', request_kind: 'summary', status: 'completed', updated_at_ms: Date.now() });
    await window.__k.sim.settle();
    await new Promise((r) => setTimeout(r, 250));
    return { after, cleared: window.__zone().map((e) => e.code) };
  });
  const failed = zone.after.find((e) => e.code === 'support.agent.request.req_1');
  assert.ok(failed, JSON.stringify(zone.after));
  assert.equal(failed.level, 'error');
  assert.match(failed.text, /model quota exhausted/);
  assert.equal(zone.after.find((e) => e.code === 'support.agent.request.req_2')?.level, 'warn');
  assert.ok(!zone.after.some((e) => e.code === 'support.agent.request.req_old'), 'alte Fehlschläge vor der Montage nicht');
  assert.ok(!zone.cleared.includes('support.agent.request.req_1'));
  assert.ok(zone.cleared.includes('support.agent.request.req_2'));
});

test('Import: ungültiges JSON → Zone; gültig → open_from_thread (Payload unverändert, OFFEN-1)', async () => {
  const r = await page.evaluate(() => {
    const hooks = window.__mod.__supportTestHooks;
    const bad = hooks.importSupportText('{kaputt');
    const zoneBad = window.__zone().map((e) => e.code);
    const good = hooks.importSupportText(JSON.stringify([{ primary_thread_key: 'mail/neu', inbox_id: 'inbox-mail' }]));
    return { bad, zoneBad, good, zoneAfter: window.__zone().map((e) => e.code) };
  });
  assert.equal(r.bad, 0);
  assert.ok(r.zoneBad.includes('support.import.invalid'));
  assert.equal(r.good, 1);
  assert.ok(!r.zoneAfter.includes('support.import.invalid'));
  const cmd = await page.evaluate(() => window.__commands().at(-1));
  assert.equal(cmd.type, 'support.conversation.open_from_thread');
  assert.equal(cmd.payload.thread_key, 'mail/neu');
  assert.equal(Object.hasOwn(cmd.payload, 'conversation_id'), false);
});

test('Schreiben: api.put erscheint live und wird vom Core angenommen', async () => {
  const result = await page.evaluate(async () => {
    const h = window.__api.put('support_conversations', { id: 'conv_put', status: 'open', priority: 'low', search_text: 'delta', primary_thread_key: 'mail/delta', last_activity_at_ms: Date.now(), updated_at_ms: Date.now() });
    await h.local;
    return h.result;
  });
  assert.equal(result.status, 'accepted');
  await settle();
  assert.match(await text('[data-support-list]'), /Delta Put Fall/);
});

test('snapshot/restored: Notiz-Entwurf, Auswahl, Band und Suche überleben den Remount', async () => {
  await clickSel('[data-support-conversation-id="conv_a"]');
  await page.fill('[data-support-note]', 'halb fertige Notiz');
  await page.fill('[data-pg-search]', 'alpha');
  await settle();
  const snap = await page.evaluate(async () => {
    const cloned = structuredClone(window.__app.snapshot()); // Shell-Ablauf Entwurf §5
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, { restored: cloned });
    return cloned;
  });
  assert.equal(snap.selectedId, 'conv_a');
  assert.equal(snap.noteDraft, 'halb fertige Notiz');
  assert.equal(snap.search, 'alpha');
  assert.ok(JSON.stringify(snap).length < 1024 * 1024);
  await settle();
  const after = await page.evaluate(() => ({
    note: document.querySelector('[data-support-note]').value,
    search: document.querySelector('[data-pg-search]').value,
    selected: document.querySelector('[data-support-conversation-id].is-selected')?.getAttribute('data-support-conversation-id'),
    rows: [...document.querySelectorAll('[data-support-conversation-id]')].map((el) => el.getAttribute('data-support-conversation-id')),
    snapSelected: window.__app.snapshot().selectedId,
  }));
  assert.equal(after.note, 'halb fertige Notiz');
  assert.equal(after.search, 'alpha');
  assert.equal(after.selected, 'conv_a');
  assert.deepEqual(after.rows, ['conv_a']);
  assert.equal(after.snapSelected, 'conv_a');
  assert.equal(await page.evaluate(() => window.__c.subscribers('support_conversations')), 1, 'alte Abos sind abgemeldet');
  assert.deepEqual(page.__errors, []);
});

test('FIX7 Titel wie Zendesk: Betreff > Textanfang (≤ 60) > „Anfrage von <Name>“, nie Suchtext oder ID', async () => {
  const r = await page.evaluate(async () => {
    const now = Date.now();
    const search = document.querySelector('[data-support-root] [data-pg-search]');
    const prevSearch = search.value;
    search.value = '';
    search.dispatchEvent(new Event('input', { bubbles: true }));
    await window.__k.sim.push('communication_messages', [
      { id: 'm_t1a', thread_key: 'mail/t1', subject: '  Rückfrage   zur Lieferung  ', body_text: 'Text', observed_at: new Date(now - 5000).toISOString() },
      { id: 'm_t1b', thread_key: 'mail/t1', subject: 'AW: später', observed_at: new Date(now - 1000).toISOString() },
      { id: 'm_t2', thread_key: 'mail/t2', body_text: 'Guten Tag, unser Zugang zum Kundenportal funktioniert seit dem Update am Montag nicht mehr richtig.', observed_at: new Date(now - 4000).toISOString() },
    ]);
    await window.__k.sim.push('support_conversations', [
      { id: 'conv_t1', status: 'open', search_text: 'suchtext eins', customer_account_id: 'account-001', primary_thread_key: 'mail/t1', updated_at_ms: now },
      { id: 'conv_t2', status: 'open', search_text: 'suchtext zwei', primary_thread_key: 'mail/t2', updated_at_ms: now },
      { id: 'conv_t3', status: 'open', search_text: 'suchtext drei', customer_contact_id: 'ct_gamma', customer_account_id: 'account-001', updated_at_ms: now },
      { id: 'conv_t4', status: 'open', customer_account_id: 'account-001', team_id: 'team-001', ticket_case_id: 'case-001', updated_at_ms: now },
    ]);
    await window.__k.sim.settle();
    await new Promise((res) => setTimeout(res, 250));
    const label = (id) => document.querySelector(`[data-support-conversation-id="${id}"] .support-row-title`)?.textContent || '';
    const rows = [...document.querySelectorAll('[data-support-conversation-id]')].map((el) => el.dataset.supportConversationId);
    if (!rows.includes('conv_t4')) return { rows, search: document.querySelector('[data-pg-search]')?.value };
    document.querySelector('[data-support-conversation-id="conv_t4"]').click();
    await new Promise((res) => setTimeout(res, 250));
    const out = { t1: label('conv_t1'), t2: label('conv_t2'), t3: label('conv_t3'), t4: label('conv_t4'), context: document.querySelector('[data-support-context]').textContent, title: document.querySelector('[data-support-title]').textContent };
    for (const id of ['conv_t1', 'conv_t2', 'conv_t3', 'conv_t4']) await window.__k.sim.push('support_conversations', { id, _deleted: true });
    await window.__k.sim.settle();
    search.value = prevSearch;
    search.dispatchEvent(new Event('input', { bubbles: true }));
    return out;
  });
  assert.ok(!r.rows, JSON.stringify(r));
  assert.equal(r.t1, 'Rückfrage zur Lieferung', 'Betreff der ERSTEN Nachricht, Leerraum normalisiert');
  assert.ok(r.t2.length <= 60, `Textanfang max. 60 Zeichen (${r.t2.length})`);
  assert.match(r.t2, /^Guten Tag, unser Zugang zum Kundenportal.*…$/);
  assert.equal(r.t3, 'Anfrage von Gerda Gamma');
  assert.equal(r.t4, 'Anfrage', 'ohne Nachricht und Namen: neutraler Titel statt ID');
  assert.equal(r.title, 'Anfrage');
  for (const v of [r.t1, r.t2, r.t3, r.t4, r.context]) assert.doesNotMatch(v, /suchtext|account-001|team-001|case-001|conv_t/);
});

test('Screenshots 1280 und 420 (gegen den Stub)', async () => {
  await mkdir(SHOTS, { recursive: true });
  await page.fill('[data-pg-search]', '');
  await page.dispatchEvent('[data-pg-search]', 'input');
  await clickSel('[data-support-conversation-id="conv_a"]');
  await settle();
  for (const [w, h] of [[1280, 800], [420, 860]]) {
    await page.setViewportSize({ width: w, height: h });
    await page.$eval('#win', (el, size) => { el.style.width = `${size[0]}px`; el.style.height = `${size[1]}px`; }, [w, h]);
    await page.waitForTimeout(300);
    const overflow = await page.$eval('#win', (el) => el.scrollWidth - el.clientWidth);
    assert.ok(overflow <= 1, `kein horizontaler Überlauf bei ${w}px (gemessen ${overflow})`);
    await page.locator('#win').screenshot({ path: join(SHOTS, `support-${w}.png`) });
  }
  // 420: Kontext eingeklappt → Queue + Konversation gestapelt.
  await clickSel('.support-right [data-support-toggle-context]');
  await page.waitForTimeout(200);
  assert.ok(await page.$eval('.support-left', (el) => el.getBoundingClientRect().height > 100));
  await page.locator('#win').screenshot({ path: join(SHOTS, 'support-420-queue.png') });
});

test('unmount meldet alle Abos ab und leert root', async () => {
  const res = await page.evaluate(async () => {
    await window.__app.unmount();
    await window.__k.sim.push('support_conversations', { id: 'conv_late', status: 'open', search_text: 'Late', updated_at_ms: Date.now() });
    await window.__k.sim.settle();
    return {
      subs: ['support_conversations', 'support_notes', 'business_commands', 'communication_messages'].map((c) => window.__c.subscribers(c)),
      children: document.getElementById('win').children.length,
    };
  });
  assert.deepEqual(res.subs, [0, 0, 0, 0]);
  assert.equal(res.children, 0);
  assert.deepEqual(page.__errors, []);
});
