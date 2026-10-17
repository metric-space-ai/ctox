// conversations gegen den eingefrorenen App-API-4.0-Kern-Stub (Pin 9fa9948, read-only unter /__stub/) in echtem
// Chromium. Eigene Attrappe (v4/tools/port/test-api.mjs) nur für den Abo-Zähler.
// Lauf: CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/4b0d0ac \
//       node --test src/apps/business-os/modules/conversations/tests/v4-api.test.mjs
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

const HERE = dirname(fileURLToPath(import.meta.url));
const STUB = process.env.CTOX_APP_API_STUB || join(homedir(), '.local/state/shell-v4/stub-pins/4b0d0ac');
const BOS = normalize(join(HERE, '../../..'));
const SHOTS = join(HERE, 'shots');
const require = createRequire(import.meta.url);
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || join(homedir(), 'Documents/ctox/src/apps/business-os/node_modules/playwright'));
const TYPES = { '.js': 'text/javascript', '.mjs': 'text/javascript', '.css': 'text/css', '.html': 'text/html', '.json': 'application/json', '.svg': 'image/svg+xml', '.png': 'image/png' };

const PAGE = `<!doctype html><html><head><meta charset="utf-8">
<link rel="stylesheet" href="/ui-contract/v1/workjet-ui-contract.css">
<link rel="stylesheet" href="/app.css">
<link rel="stylesheet" href="/shared/base.css">
<link rel="stylesheet" href="/themes/ctox-desktop-shell.css">
<link rel="stylesheet" href="/modules/conversations/index.css">
<style>html,body{margin:0;height:100%;overflow:hidden} #win{position:relative;width:1280px;height:760px;overflow:hidden;display:flex;flex-direction:column;container:business-app-window/inline-size}</style>
</head><body><div id="win"></div></body></html>`;

const NOW = Date.now();
const iso = (minutesAgo) => new Date(NOW - minutesAgo * 60000).toISOString();
const OUTBOUND_LINK = { outbound_message_id: 'om-1', engagement_id: 'eng-1', campaign_id: 'camp-1' };

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
      // Wie der Service Worker (§16.15): ctox:shared/x.js → Shell-Pfad (OFFEN-F).
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
  // FIXT-8265 (E48): echte Deklarationen der conversations-Version (v4/manifests/conversations.manifest.json, Fremdlesen
  // der outbound_*-Collections) plus Eigentümer-Manifest outbound; ohne veröffentlichte Version sperrt der Kern Fremdlesen.
  await page.evaluate((routes) => { window.__routes = routes; }, realManifestRoutes('conversations'));
  await page.evaluate(async ({ link, t }) => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    window.__mod = await import('/modules/conversations/index.js');
    const kernel = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false });
    await kernel.ready;
    const sim = kernel.sim;
    await sim.push('business_apps', ['ctox', 'outbound', 'conversations', 'file-viewer'].map((id) => ({ id, title: id })));
    for (const o of window.__routes?._fixture_owners ?? []) if (!(await sim.inspect('business_apps', o.app))?.data?.active_ver) await sim.publishAppVersion(o.app, o.ver, o.publish);
    await sim.publishAppVersion('conversations', '1.0.2', { title: 'Conversations', manifestExtra: window.__routes });
    await sim.push('communication_accounts', [
      { id: 'acct-mail', account_key: 'acct-mail', channel: 'email', address: 'ops@ctox.dev', provider: 'imap', created_at: t[3], updated_at: t[0], last_inbound_ok_at: t[1], last_outbound_ok_at: t[2] },
      { id: 'acct-wa', account_key: 'acct-wa', channel: 'whatsapp', address: '+49 30 555 0101', provider: 'whatsapp-web', created_at: t[3], updated_at: t[0] },
    ]);
    await sim.push('communication_threads', [
      { id: 't-alice', thread_key: 't-alice', channel: 'email', account_key: 'acct-mail', subject: 'Angebot Q4', participant_keys_json: ['alice@example.com'], last_message_at: t[0], message_count: 2, unread_count: 1, updated_at: t[0], metadata_json: { ...link } },
      { id: 't-bob', thread_key: 't-bob', channel: 'whatsapp', account_key: 'acct-wa', subject: 'Rückruf', participant_keys_json: ['+49 170 1234567'], last_message_at: t[2], message_count: 1, unread_count: 0, updated_at: t[2] },
    ]);
    await sim.push('communication_messages', [
      { id: 'm-a1', message_key: 'm-a1', thread_key: 't-alice', channel: 'email', account_key: 'acct-mail', direction: 'inbound', subject: 'Angebot Q4', body_text: 'Hallo, ich habe Interesse am Angebot.', sender_address: 'alice@example.com', recipient_addresses_json: ['ops@ctox.dev'], external_created_at: t[1], observed_at: t[1], status: 'received', route_status: 'queued', seen: 0 },
      { id: 'm-a2', message_key: 'm-a2', thread_key: 't-alice', channel: 'email', account_key: 'acct-mail', direction: 'outbound', subject: 'Re: Angebot Q4', body_text: 'Danke, anbei der Entwurf.', sender_address: 'ops@ctox.dev', recipient_addresses_json: ['alice@example.com'], external_created_at: t[0], observed_at: t[0], status: 'draft', folder_hint: 'drafts', metadata_json: { ...link, approval_status: 'awaiting_approval', send_status: 'awaiting_approval' } },
      { id: 'm-b1', message_key: 'm-b1', thread_key: 't-bob', channel: 'whatsapp', account_key: 'acct-wa', direction: 'inbound', body_text: 'Bitte um Rückruf morgen.', sender_display: 'Bob', external_created_at: t[2], observed_at: t[2], status: 'received' },
    ]);
    await sim.push('outbound_campaigns', [{ id: 'camp-1', name: 'Herbstkampagne', status: 'active', payload: {}, created_at_ms: 1, updated_at_ms: 1 }]);
    await sim.push('outbound_engagements', [{ id: 'eng-1', campaign_id: 'camp-1', status: 'draft', payload: { contact_name: 'Alice Example', company_name: 'Example GmbH' }, created_at_ms: 1, updated_at_ms: 1 }]);
    await sim.push('outbound_messages', [{ id: 'om-1', engagement_id: 'eng-1', campaign_id: 'camp-1', message_type: 'email', direction: 'outbound', draft_status: 'draft', approval_status: 'awaiting_approval', send_status: 'awaiting_approval', subject: 'Re: Angebot Q4', body_text: 'Danke, anbei der Entwurf.', recipient_email: 'alice@example.com', communication_message_key: 'm-a2', thread_key: 't-alice', payload: {}, created_at_ms: 1, updated_at_ms: 1 }]);
    await sim.settle();
    const api = kernel.appApi('conversations', '1.0.2');
    window.__k = kernel;
    // FIXT-8265: conversations deklariert business_commands nicht (nur api.command). Testseitiges Lesen daher über die
    // Befehls-ids aus api.command und kernel.sim.inspect, nicht über die App-API.
    const cmdIds = [];
    window.__api = { ...api, command(type, payload, opts) { const h = api.command(type, payload, opts); cmdIds.push(h.id); return h; } };
    window.__zone = () => kernel.statusZone.forApp('conversations').get().filter((e) => e.source === 'app');
    window.__commands = async () => {
      const rows = [];
      for (const id of cmdIds) { const r = await kernel.sim.inspect('business_commands', id); if (r?.data != null && !r.deleted) rows.push(r.data); }
      return rows.sort((a, b) => String(a.created_at).localeCompare(String(b.created_at)));
    };
  }, { link: OUTBOUND_LINK, t: [iso(5), iso(30), iso(60 * 26), iso(60 * 24 * 30)] });
});

after(async () => { await browser?.close(); await new Promise((r) => server?.close(r)); });

const settle = async () => { await page.evaluate(() => window.__k.sim.settle()); await page.waitForTimeout(150); };
const text = (sel) => page.$eval(sel, (el) => el.textContent);
const STATE_TEXT = /synchronis|syncing|lade|loading|wird vorbereitet|nicht verfügbar|unavailable|keine konversation|no conversation/i;

test('mount: Live-Daten, Auswahl, Timeline, Outbound-Kontext; nur api.*, gescoptes Root, Titel über api.shell', async () => {
  await page.evaluate(async () => {
    const { countingApi } = await import('/v4/tools/port/test-api.mjs');
    window.__c = countingApi(window.__api);
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, {});
  });
  await settle();
  const list = await text('[data-conv-thread-list]');
  assert.match(list, /alice@example\.com/);
  assert.match(list, /\+49 170 1234567/);
  assert.equal(await page.$eval('#win', (el) => el.dataset.app), 'conversations');
  assert.equal(await page.evaluate(() => window.__k.shell.window('conversations').state.get().title), 'Konversationen');
  // Neueste Konversation ist ausgewählt, ihre Timeline steht aus der Live-Abfrage.
  assert.equal(await page.evaluate(() => window.__app.snapshot().selectedBucketKey), 'alice@example.com');
  assert.match(await text('[data-conv-timeline]'), /Hallo, ich habe Interesse am Angebot\./);
  assert.match(await text('[data-conv-timeline]'), /Danke, anbei der Entwurf\./);
  // Outbound-Kontext aus den Live-Abos (fremde Collections, nur lesend).
  assert.match(await text('[data-conv-outbound-card-body]'), /Herbstkampagne/);
  assert.equal(await page.$$eval('[data-action="conv-outbound-approve"]', (n) => n.length), 1);
  assert.match(await text('[data-conv-accounts-card-body]'), /ops@ctox\.dev/);
  const subs = await page.evaluate(() => ['communication_threads', 'communication_accounts', 'communication_messages', 'outbound_messages', 'outbound_approvals'].map((c) => window.__c.subscribers(c)));
  assert.deepEqual(subs, [1, 1, 1, 1, 1]);
  // Auto-Zuordnung der eingegangenen Antwort (v3-Verhalten) als Befehl, ohne Warten.
  const match = await page.evaluate(async () => (await window.__commands()).find((c) => c.type === 'outbound.reply.match'));
  assert.equal(match?.payload.engagement_id, 'eng-1');
  assert.equal(match?.payload.reply_message_id, 'm-a1');
  assert.deepEqual(await page.evaluate(() => window.__zone()), []);
  assert.deepEqual(page.__errors, []);
});

test('Live-Änderung vom Core erscheint ohne Neuladen; neue Nachricht im gewählten Thread; Löschung verschwindet', async () => {
  await page.evaluate(() => window.__k.sim.push('communication_threads', { id: 't-carol', thread_key: 't-carol', channel: 'email', account_key: 'acct-mail', subject: 'Neu', participant_keys_json: ['carol@example.com'], last_message_at: new Date(Date.now() - 120 * 60000).toISOString(), message_count: 0, updated_at: new Date().toISOString() }));
  await settle();
  assert.match(await text('[data-conv-thread-list]'), /carol@example\.com/);
  await page.evaluate(() => window.__k.sim.push('communication_messages', { id: 'm-a3', message_key: 'm-a3', thread_key: 't-alice', channel: 'email', account_key: 'acct-mail', direction: 'inbound', body_text: 'Live nachgereicht', external_created_at: new Date().toISOString(), observed_at: new Date().toISOString(), status: 'received' }));
  await settle();
  assert.match(await text('[data-conv-timeline]'), /Live nachgereicht/);
  await page.evaluate(() => window.__k.sim.push('communication_threads', { id: 't-carol', _deleted: true }));
  await settle();
  assert.doesNotMatch(await text('[data-conv-thread-list]'), /carol@example\.com/);
  assert.deepEqual(page.__errors, []);
});

test('leere Collections rendern keinen Zustandstext und keine Zone', async () => {
  const empty = await page.evaluate(async () => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    const k2 = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false, instanceId: 'empty' });
    await k2.ready;
    const host = document.createElement('div');
    host.style.cssText = 'position:absolute;left:-2000px;width:900px;height:500px';
    document.body.append(host);
    const app = await window.__mod.mount(host, k2.appApi('conversations', '1.0.2'), {});
    await new Promise((r) => setTimeout(r, 250));
    const out = {
      listText: host.querySelector('[data-conv-thread-list]').textContent.trim(),
      timelineText: host.querySelector('[data-conv-timeline]').textContent.trim(),
      centerText: host.querySelector('.conv-center-body').textContent.trim(),
      stateBox: Boolean(host.querySelector('.ctox-syncing, .ctox-empty, [role="status"]')),
      zone: k2.statusZone.forApp('conversations').get().map((e) => e.code),
    };
    await app.unmount();
    host.remove();
    await k2.close();
    return out;
  });
  assert.equal(empty.listText, '');
  assert.equal(empty.timelineText, '');
  assert.equal(empty.centerText, '');
  assert.equal(empty.stateBox, false);
  assert.deepEqual(empty.zone, []);
});

test('Freigeben: Befehl über api.command (gehört outbound), kein lokales Upsert; done → Projektion in communication_messages', async () => {
  await page.click('[data-action="conv-outbound-approve"]');
  const immediate = await page.evaluate(async () => (await window.__commands()).filter((c) => c.type === 'outbound.message.approve'));
  assert.equal(immediate.length, 1, 'Befehl liegt sofort lokal an, ohne Warten');
  await settle();
  const cmd = await page.evaluate(async () => (await window.__commands()).find((c) => c.type === 'outbound.message.approve'));
  assert.equal(cmd.payload.message_id, 'om-1');
  assert.equal(cmd.payload.record_id, 'om-1');
  assert.equal(cmd.payload.client_context.source_module, 'conversations');
  assert.equal(cmd.status, 'done');
  // Entscheidung + Audit schreibt der outbound-Handler; die UI fasst outbound_* nicht an.
  const outbound = await page.evaluate(() => ({
    message: window.__api.get('outbound_messages', 'om-1').get(),
    approvals: window.__api.query('outbound_approvals', {}).get(),
  }));
  assert.equal(outbound.message.approval_status, 'awaiting_approval');
  assert.deepEqual(outbound.approvals, []);
  const projected = await page.evaluate(() => window.__api.get('communication_messages', 'm-a2').get().metadata_json);
  assert.equal(projected.approval_status, 'approved');
  assert.equal(projected.send_status, 'approved_not_sent');
  assert.equal(projected.outbound_message_id, 'om-1');
  assert.equal(projected.outbound_engagement_id, 'eng-1');
  assert.equal(projected.outbound_campaign_id, 'camp-1');
  assert.deepEqual(await page.evaluate(() => window.__zone()), []);
});

test('Ablehnen schlägt fehl → genau ein Eintrag in der Zustandszone, keine Projektion', async () => {
  await page.evaluate(() => window.__k.sim.failCommand('outbound.message.reject', 'kaputt'));
  await page.click('[data-action="conv-outbound-reject"]');
  await settle();
  const cmd = await page.evaluate(async () => (await window.__commands()).find((c) => c.type === 'outbound.message.reject'));
  assert.equal(cmd.payload.message_id, 'om-1');
  assert.equal(cmd.payload.comment, 'Rejected from Conversations');
  assert.equal(cmd.status, 'failed');
  const zone = await page.evaluate(() => window.__zone());
  assert.equal(zone.length, 1);
  assert.equal(zone[0].code, 'conversations.command.outbound.message.reject');
  assert.match(zone[0].text, /kaputt/);
  const projected = await page.evaluate(() => window.__api.get('communication_messages', 'm-a2').get().metadata_json);
  assert.equal(projected.approval_status, 'approved', 'fehlgeschlagener Befehl ändert nichts');
  await page.evaluate(() => window.__api.status.clear('conversations.command.outbound.message.reject'));
});

test('Reply-Klasse: Befehl outbound.reply.classify mit gewählter Klasse', async () => {
  await page.selectOption('.conv-outbound-reply-classifier select', 'positive');
  await page.click('[data-action="conv-outbound-reply-classify"]');
  await settle();
  const cmd = await page.evaluate(async () => (await window.__commands()).filter((c) => c.type === 'outbound.reply.classify').at(-1));
  assert.deepEqual({ e: cmd.payload.engagement_id, c: cmd.payload.classification, r: cmd.payload.reply_message_id }, { e: 'eng-1', c: 'positive', r: 'm-a3' }, 'neueste eingegangene Antwort (m-a3 kam live)');
});

test('Navigation über api.shell.navigate: CTOX-Flow (Task-Chip) und Outbound', async () => {
  // Chips zeigen Nutzerbegriff + Titel, keine Kennung (Textdiät Regel 4); die Kennung bleibt Navigationsziel.
  const chipTexts = await page.$$eval('.conv-message-task-chip', (nodes) => nodes.map((n) => n.innerText.replace(/\s+/g, ' ')));
  assert.ok(chipTexts.some((text) => /Aufgabe/.test(text)), 'Aufgaben-Chip trägt den Nutzerbegriff');
  for (const text of chipTexts) assert.doesNotMatch(text, /\bm-a\d|\bTASK\b|\bWORK\b/);
  const replyLine = await page.$eval('.conv-outbound-reply-classifier small', (n) => n.textContent).catch(() => '');
  assert.doesNotMatch(replyLine, /\bm-a\d/);
  await page.evaluate(() => [...document.querySelectorAll('.conv-message-task-chip')].find((n) => n.dataset.kind === 'task').click());
  let nav = await page.evaluate(() => window.__k.sim.navigations.at(-1));
  assert.equal(nav.to, 'ctox');
  assert.equal(nav.params.source, 'conversations');
  assert.match(nav.params.task_id, /^m-a/);
  await page.click('[data-action="conv-outbound-open"]');
  nav = await page.evaluate(() => window.__k.sim.navigations.at(-1));
  assert.deepEqual({ to: nav.to, ...nav.params }, { to: 'outbound', campaign_id: 'camp-1', engagement_id: 'eng-1', message_id: 'om-1' });
});

test('Deep Link über onRoute: Auswahl + Hervorhebung; unbekanntes Ziel → Zustandszone statt stillem Rückgriff', async () => {
  await page.evaluate(() => window.__k.appApi('outbound', '1').shell.navigate('conversations', { thread_key: 't-unknown' }));
  await settle();
  let zone = await page.evaluate(() => window.__zone().map((e) => e.code));
  assert.deepEqual(zone, ['conversations.route.not_found']);
  await page.evaluate(() => window.__k.appApi('outbound', '1').shell.navigate('conversations', { message_key: 'm-b1' }));
  await settle();
  assert.equal(await page.evaluate(() => window.__app.snapshot().selectedBucketKey), '+49 170 1234567');
  assert.equal(await page.$eval('.conv-message.is-highlighted', (el) => el.dataset.messageKey), 'm-b1');
  zone = await page.evaluate(() => window.__zone().map((e) => e.code));
  assert.deepEqual(zone, []);
});

test('Schreiben: api.put erscheint live und wird vom Core angenommen', async () => {
  const result = await page.evaluate(async () => {
    const h = window.__api.put('communication_threads', { id: 't-dora', thread_key: 't-dora', channel: 'email', account_key: 'acct-mail', participant_keys_json: ['dora@example.com'], last_message_at: new Date(Date.now() - 300 * 60000).toISOString(), updated_at: new Date().toISOString() });
    await h.local;
    return h.result;
  });
  assert.equal(result.status, 'accepted');
  await settle();
  assert.match(await text('[data-conv-thread-list]'), /dora@example\.com/);
});

test('snapshot/restored: Suche, Auswahl und offene Reply-Klasse überleben den Remount', async () => {
  await page.evaluate(() => [...document.querySelectorAll('.conv-thread-item')].find((n) => n.dataset.bucketKey === 'alice@example.com').click());
  await settle();
  await page.fill('[data-pg-search]', 'alice');
  await settle();
  await page.selectOption('.conv-outbound-reply-classifier select', 'objection');
  const snap = await page.evaluate(async () => {
    const cloned = structuredClone(window.__app.snapshot());
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, { restored: cloned });
    return cloned;
  });
  assert.equal(snap.searchText, 'alice');
  assert.equal(snap.selectedBucketKey, 'alice@example.com');
  assert.deepEqual(snap.classificationDraft, { engagementId: 'eng-1', value: 'objection' });
  assert.ok(JSON.stringify(snap).length < 1024 * 1024);
  await settle();
  const after = await page.evaluate(() => ({
    search: document.querySelector('[data-pg-search]').value,
    items: [...document.querySelectorAll('.conv-thread-item')].map((n) => n.dataset.bucketKey),
    selected: window.__app.snapshot().selectedBucketKey,
    classification: document.querySelector('.conv-outbound-reply-classifier select')?.value,
  }));
  assert.equal(after.search, 'alice');
  assert.deepEqual(after.items, ['alice@example.com']);
  assert.equal(after.selected, 'alice@example.com');
  assert.equal(after.classification, 'objection');
  const subs = await page.evaluate(() => ['communication_threads', 'communication_accounts', 'communication_messages'].map((c) => window.__c.subscribers(c)));
  assert.deepEqual(subs, [1, 1, 1], 'alte Abos sind abgemeldet');
  assert.deepEqual(page.__errors, []);
});

test('Screenshots 1280 und 420 gegen den Stub', async () => {
  await page.fill('[data-pg-search]', '');
  await settle();
  const all = await page.$$eval('.conv-thread-item', (n) => n.length);
  assert.ok(all >= 3, `Liste nach geleerter Suche vollständig (${all})`);
  const visibleText = await page.$eval('#win', (el) => el.innerText);
  assert.doesNotMatch(visibleText, STATE_TEXT);
  await page.screenshot({ path: join(SHOTS, 'conversations-1280.png'), clip: { x: 0, y: 0, width: 1280, height: 760 } });
  await page.setViewportSize({ width: 420, height: 820 });
  await page.evaluate(() => { const w = document.getElementById('win'); w.style.width = '420px'; w.style.height = '820px'; });
  await page.waitForTimeout(300);
  await page.screenshot({ path: join(SHOTS, 'conversations-420.png'), clip: { x: 0, y: 0, width: 420, height: 820 } });
  const overflow = await page.$eval('#win', (el) => el.scrollWidth - el.clientWidth);
  assert.ok(overflow <= 1, `kein waagrechter Überlauf (${overflow}px)`);
  // Timeline: Chips (Nutzerbegriff + Titel) kürzen statt seitlich zu scrollen.
  const timeline = await page.$eval('.conv-timeline', (el) => {
    const box = el.getBoundingClientRect();
    const widest = [...el.querySelectorAll('*')].map((n) => ({ n, r: n.getBoundingClientRect() }))
      .filter(({ r }) => r.width > 0).sort((a, b) => b.r.right - a.r.right)[0];
    return { over: el.scrollWidth - el.clientWidth, widest: widest ? `${widest.n.className} ${Math.round(widest.r.right - box.left)}` : '' };
  });
  assert.ok(timeline.over <= 1, `Timeline ohne seitlichen Scroll: ${JSON.stringify(timeline)}`);
  await page.setViewportSize({ width: 1280, height: 760 });
  await page.evaluate(() => { const w = document.getElementById('win'); w.style.width = '1280px'; w.style.height = '760px'; });
});

test('unmount meldet alle Abos ab und leert root', async () => {
  const res = await page.evaluate(async () => {
    await window.__app.unmount();
    await window.__k.sim.push('communication_threads', { id: 't-late', thread_key: 't-late', channel: 'email', account_key: 'acct-mail', participant_keys_json: ['late@example.com'], last_message_at: new Date().toISOString(), updated_at: new Date().toISOString() });
    await window.__k.sim.settle();
    return {
      subs: ['communication_threads', 'communication_accounts', 'communication_messages', 'outbound_messages', 'outbound_approvals', 'outbound_engagements', 'outbound_campaigns'].map((c) => window.__c.subscribers(c)),
      children: document.getElementById('win').children.length,
    };
  });
  assert.deepEqual(res.subs, [0, 0, 0, 0, 0, 0, 0]);
  assert.equal(res.children, 0);
  assert.deepEqual(page.__errors, []);
});
