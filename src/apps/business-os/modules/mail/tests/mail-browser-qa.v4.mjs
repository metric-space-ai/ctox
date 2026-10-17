#!/usr/bin/env node
// v4-Gegenstück zu mail-browser-qa.mjs (V2, bleibt byte-identisch). Gleiche sichtbare Abläufe gegen die auf
// App-API 4.0 portierte Mail-App: echter mount(root, api, ctx) aus modules/mail/index.js gegen den Kern-Stub
// (createStubKernel), gleiche Seed-Daten wie V2, Bedienung mit echter Maus und Tastatur (Playwright-Eingaben).
//
// Unterschiede zum V2-Harness (Vertrag App-API 4.0, V4-PORT.md):
// - Daten kommen nur aus api.query-Abos; der Core projiziert Befehlsergebnisse (hier sim.onCommand + sim.push).
// - Kein commandBus-Ergebnis-Leseweg, keine Lese-Timeouts/Abbrüche, keine Readiness, kein nativer Konten-
//   Gegenlesepfad, keine Zustandstexte in der App (Fehler/Hinweise gehen in die Zustandszone api.status).
// - Deep-Link über api.shell.navigate('mail', params) (Shell-Route) statt location.hash.
// - Kein ctx.getActionIcon: Mail rendert immer app-eigene Glyphen (mail-action-*).
// Jede V2-Prüfung ohne v4-Gegenstück steht im Inventar unten (NO_V4) mit Grund; nichts wird wegassertiert.
//
// Lauf (aus src/apps/business-os): node modules/mail/tests/mail-browser-qa.v4.mjs
//   CTOX_APP_API_STUB=<dir>          Stub (Standard ~/.local/state/shell-v4/stub-pins/4b0d0ac)
//   CTOX_MAIL_ICON_PROVIDER_MODE     shell|absent|empty (v4: kein Anbieter, alle Modi erwarten mail-action-*)
//   CTOX_MAIL_ICON_QA_ONLY=1         nur Posteingang + Icon-Abnahme
//   CTOX_EVIDENCE_DIR=<dir>          Screenshots (Standard tests/shots)
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { readFile, mkdir } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { dirname, join, normalize, extname } from 'node:path';
import { homedir } from 'node:os';
import { realManifestRoutes } from '../../../v4/tools/port/real-manifest.mjs';

const STUB = process.env.CTOX_APP_API_STUB || join(homedir(), '.local/state/shell-v4/stub-pins/4b0d0ac');
const HERE = dirname(fileURLToPath(import.meta.url));
const BOS = normalize(join(HERE, '../../..'));
const OUT = process.env.CTOX_EVIDENCE_DIR || join(HERE, 'shots');
const require = createRequire(import.meta.url);
// Lader (OLDTEST-Muster): playwright aus dem Checkout (CI: npm ci), sonst PLAYWRIGHT_MODULE, sonst Haupt-Checkout.
function loadPlaywright() {
  try { return require('playwright'); } catch {
    return require(process.env.PLAYWRIGHT_MODULE || join(homedir(), 'Documents/ctox/src/apps/business-os/node_modules/playwright'));
  }
}
const { chromium } = loadPlaywright();
const iconProviderMode = process.env.CTOX_MAIL_ICON_PROVIDER_MODE || 'shell';
if (!['shell', 'absent', 'empty'].includes(iconProviderMode)) {
  throw new Error('CTOX_MAIL_ICON_PROVIDER_MODE must be shell, absent, or empty');
}
const mailIconOnly = process.env.CTOX_MAIL_ICON_QA_ONLY === '1';
const TYPES = { '.js': 'text/javascript', '.mjs': 'text/javascript', '.css': 'text/css', '.html': 'text/html', '.json': 'application/json', '.svg': 'image/svg+xml', '.woff2': 'font/woff2', '.png': 'image/png' };

// Shell lädt base.css/app.css und die App-CSS aus dem Manifest; #win bildet den v4-Fensterinhalt nach
// (data-app setzt die App selbst, Container-Name wie die v4-Shell, V4-PORT OFFEN 10).
const PAGE = `<!doctype html><html lang="de" data-theme="light"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width">
<link rel="icon" href="data:,">
<link rel="stylesheet" href="/shared/base.css"><link rel="stylesheet" href="/app.css">
<link rel="stylesheet" href="/modules/mail/index.css"><link rel="stylesheet" href="/modules/mail/editor/editor.css">
<style>html,body{margin:0;width:100%;height:100%;overflow:hidden} #win{position:fixed;inset:0;display:grid;overflow:hidden;container:business-app-window / inline-size}</style>
</head><body><div id="win"></div></body></html>`;

// Inventar: jede V2-Prüfung (Zeile in mail-browser-qa.mjs) → hier portiert (step) oder ohne v4-Gegenstück.
const NO_V4 = [
  ['L497', 'QUERY_CANCELLED-Lesefehler behält Auswahl, kein Fehlerkasten', 'api.query kann nicht fehlschlagen/abbrechen (synchroner Live-Stand); kein Leseweg mit Fehlerklassen'],
  ['L502', 'Lesefehler zeigt [data-mail-read-error]', 'Lesefehler-Kasten entfernt (PORTING R11); es gibt keinen Lesefehler mehr, der ausgelöst werden könnte'],
  ['L510', 'leere Threads zeigen [data-mail-list-empty]', 'Leer-Kasten entfernt (Textdiät/R11); portiert als „leere Liste ohne Zustandstext“ (Schritt inbox.empty-threads)'],
  ['L802-L820', '„Mail wird synchronisiert“ + Timeout-/Abbruch-/Nebenläufigkeitsgrenzen der Leseanfragen', 'kein Lese-Timeout/AbortController mehr (R12); Readiness entfällt'],
  ['L839-L845', '„Postfach derzeit nicht verfügbar“ + [data-mail-retry-read]', 'Lesefehler-Zustand und Erneut-Knopf entfernt (R11)'],
  ['L846-L859', 'Readiness catching_up hält Daten 10,5 s stabil', 'collectionReadiness entfällt; Stand ist immer der lokale Live-Stand'],
  ['L860-L890', 'Readiness-Listener: leer + catching_up → Fehler, live → „Keine E-Mails“', 'Readiness und Leer-Text entfallen'],
  ['L919-L925 (Mechanik)', 'Entzug über nativen Gegenlesepfad + focus-Ereignis', 'Gegenlesepfad entfällt; portiert als Entzug über die Projektion communication_accounts (Schritt member.revoke-*)'],
  ['L931/L947', 'langsame Fremd-Collection (PENDING) blockiert Entzug nicht', 'kein asynchroner Leseweg; Entzug wirkt synchron mit der Emission'],
  ['L951-L953 / L980-L985', 'native Prüfung nicht verfügbar → Fehlerkasten, Postfach ausgeblendet', 'kein nativer Prüfweg in App-API 4.0 (V4-PORT OFFEN mail-2)'],
  ['L974-L978', 'Remount nach Entzug zeigt „Keine E-Mails“ bei weiterhin lokalen Bodies', 'portiert ohne Leer-Text: Remount nach Entzug zeigt keine Zeilen (Schritt member.remount-after-revoke)'],
  ['L290 icon mode shell', 'ctx.getActionIcon liefert ctox-action-*', 'App-API 4.0 hat keinen Icon-Anbieter; Mail rendert immer mail-action-* (alle Modi gleich)'],
];

// Befunde: V2-Prüfungen, die gegen die v4-App rot sind (App-Code ist nicht Teil dieses Laufs). Streng: der
// gemessene Verstoß muss genau dieser Liste entsprechen; ist er behoben, wird der Lauf rot, bis der Eintrag fällt.
const KNOWN_FINDINGS = {};
const geometryLog = [];

const steps = [];
async function step(id, v2Lines, fn) {
  try { await fn(); } catch (error) { throw new Error(`[${id} ← V2 ${v2Lines}] ${error.message}`, { cause: error }); }
  steps.push({ id, v2Lines });
}

const server = createServer(async (req, res) => {
  const path = decodeURIComponent(new URL(req.url, 'http://x').pathname);
  if (path === '/' || path === '/mail-qa.html') { res.writeHead(200, { 'content-type': 'text/html; charset=utf-8' }); res.end(PAGE); return; }
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
const baseUrl = `http://127.0.0.1:${server.address().port}`;
await mkdir(OUT, { recursive: true });
const browser = await chromium.launch({ headless: true });
const context = await browser.newContext({ viewport: { width: 1440, height: 940 } });
const page = await context.newPage();
const browserErrors = [];
page.on('pageerror', (error) => browserErrors.push(String(error?.stack || error)));
page.on('console', (message) => { if (message.type() === 'error') browserErrors.push(message.text()); });
const shots = [];
async function shot(name) {
  const path = join(OUT, `mail-qa-v4-${name}.png`);
  await page.screenshot({ path });
  shots.push(path);
}
const settle = async (ms = 220) => { await page.evaluate(() => window.__k.sim.settle()); await page.waitForTimeout(ms); };
const assertVisibleText = (text) => page.getByText(text, { exact: false }).first().waitFor({ state: 'visible' });

let exitCode = 0;
mailQa: try {
  await page.goto(`${baseUrl}/mail-qa.html`, { waitUntil: 'domcontentloaded' });
  // PIN-ALL (E48): echte Routen/Rechte/Fremd-Collections aus v4/manifests/mail.manifest.json; ohne aktive mail-Version und
  // ohne Eigentümer-Manifeste sperrt der Kern Plattform- und Fremd-Collections.
  // outbound: Ziel des App-Befehls outbound.message.compose (commands_in/handlers aus dem echten Manifest).
  await page.evaluate(([routes, outbound]) => { window.__routes = routes; window.__outboundRoutes = outbound; }, [realManifestRoutes('mail'), realManifestRoutes('outbound')]);
  await page.evaluate(async () => {
    const now = Date.now();
    const ACCOUNT = 'email:alice@example.test';
    // Seed = V2-Seed (mail-browser-qa.mjs:61-181); v4 adressiert über `id` (= bisheriger Schlüssel, V4-PORT MAIL2 C).
    const rows = {
      business_apps: [{ id: 'mail', title: 'Mail' }, { id: 'sellify', title: 'Sellify' }, { id: 'support', title: 'Support' }, { id: 'outbound', title: 'Outbound' }],
      business_module_catalog: [{ id: 'runtime', ok: true, updated_at_ms: 1, modules: [
        { id: 'mail', title: 'Mail', installed: true }, { id: 'support', title: 'Support', installed: true }, { id: 'customers', title: 'Kunden', installed: true }] }],
      business_users: [
        { id: 'alice', user_id: 'alice', display_name: 'Alice Admin', role: 'admin', active: true, updated_at_ms: 1 },
        { id: 'bob', user_id: 'bob', display_name: 'Bob Beispiel', role: 'member', active: true, updated_at_ms: 1 }],
      communication_accounts: [{ id: ACCOUNT, account_key: ACCOUNT, channel: 'email', address: 'alice@example.test', provider: 'ctox-mailserver',
        profile_json: { owner_user_id: 'alice' }, created_at: '2026-08-06T08:00:00Z', updated_at: '2026-08-06T08:00:00Z' }],
      communication_threads: [
        { thread_key: 'thread-1', subject: 'Projektstatus August', participant_keys_json: ['kunde@example.test'], last_message_at: '2026-08-06T09:00:00Z', unread_count: 1, updated_at: '2026-08-06T09:00:00Z' },
        { thread_key: 'thread-2', subject: 'Rückfrage zur Rechnung', participant_keys_json: ['finance@example.test'], last_message_at: '2026-08-06T08:52:00Z', unread_count: 1, updated_at: '2026-08-06T08:52:00Z' },
        { thread_key: 'thread-3', subject: 'Neuer Supportfall', participant_keys_json: ['help@example.test'], last_message_at: '2026-08-06T08:45:00Z', unread_count: 2, updated_at: '2026-08-06T08:45:00Z' },
        { thread_key: 'thread-sent', subject: 'Versandter Bericht', participant_keys_json: ['kunde@example.test'], last_message_at: '2026-08-06T09:10:00Z', unread_count: 0, updated_at: '2026-08-06T09:10:00Z' },
      ].map((t) => ({ id: t.thread_key, channel: 'email', account_key: ACCOUNT, ...t })),
      communication_messages: [
        { message_key: 'mail-1', thread_key: 'thread-1', direction: 'inbound', folder_hint: 'inbox', sender_display: 'Kunde GmbH', sender_address: 'kunde@example.test', subject: 'Projektstatus August', body_text: 'Können Sie uns den aktuellen Stand schicken?', external_created_at: '2026-08-06T09:00:00Z', observed_at: '2026-08-06T09:00:01Z' },
        { message_key: 'mail-2', thread_key: 'thread-2', direction: 'inbound', folder_hint: 'inbox', sender_display: 'Finance Team', sender_address: 'finance@example.test', subject: 'Rückfrage zur Rechnung', body_text: 'Bitte ordnen Sie die Rechnung unserem Vorgang zu.', external_created_at: '2026-08-06T08:52:00Z', observed_at: '2026-08-06T08:52:01Z' },
        { message_key: 'mail-3', thread_key: 'thread-3', direction: 'inbound', folder_hint: 'inbox', sender_display: 'Help Desk', sender_address: 'help@example.test', subject: 'Neuer Supportfall', body_text: 'Wir benötigen kurzfristig Unterstützung.', external_created_at: '2026-08-06T08:45:00Z', observed_at: '2026-08-06T08:45:01Z' },
        { message_key: 'mail-sent', thread_key: 'thread-sent', direction: 'outbound', folder_hint: 'sent', sender_display: 'Alice Admin', sender_address: 'alice@example.test', recipient_addresses_json: ['kunde@example.test'], subject: 'Versandter Bericht', body_text: 'Der Bericht ist angehängt.', external_created_at: '2026-08-06T09:10:00Z', observed_at: '2026-08-06T09:10:01Z' },
      ].map((m) => ({ id: m.message_key, channel: 'email', account_key: ACCOUNT, ...m })),
      outbound_campaigns: [{ id: 'campaign-1', name: 'August Bestandskunden', objective: 'Status-Updates bündeln', status: 'active', owner_id: 'alice',
        payload: { mail_group_kind: 'newsletter', mail_group: { schema: 'ctox.mail-group.v1', kind: 'newsletter', intake_mode: 'fixed', lifecycle_status: 'active',
          account_key: ACCOUNT, content: { mode: 'email_visual', active_revision_id: '' } } }, created_at_ms: 1, updated_at_ms: 1 }],
      outbound_messages: [{ id: 'message-delivered-1', engagement_id: 'engagement-delivered-1', campaign_id: 'campaign-1', channel: 'email', direction: 'outbound',
        sender_account_id: ACCOUNT, recipient_email: 'kunde@example.test', subject: 'Projektstatus August', body_text: 'Der aktuelle Projektstand ist jetzt verfügbar.',
        approval_status: 'approved', send_status: 'sent', sent_at_ms: now - 120000,
        payload: { provider_dispatch_status: 'delivered', provider_queued_at_ms: now - 180000, provider_completed_at_ms: now - 120000, delivered_at_ms: now - 120000,
          tracking_enabled: true, open_count: 2, first_opened_at_ms: now - 90000, last_opened_at_ms: now - 60000, click_count: 1,
          first_clicked_at_ms: now - 45000, last_clicked_at_ms: now - 45000 },
        created_at_ms: now - 240000, updated_at_ms: now - 45000 }],
      // V2-mailserver-Objekt (mail-browser-qa.mjs:189-209) in Plattformform (Kern §16.28, V4-PORT MAIL2 D).
      mail_server_state: [
        { id: 'runtime', kind: 'runtime', status: 'running', updated_at_ms: 1,
          data: { config: { enabled: true, hostname: 'mail.example.test', bind_host: '127.0.0.1', smtp_port: 2525, imap_port: 1143, outbound_throttle_per_min: 120, max_connections: 10 },
            health: { smtp_reachable: true, imap_reachable: true, queue_pending: 2, delivery_success: 18, delivery_failed: 1 } } },
        { id: 'domain:example.test', kind: 'domain', name: 'example.test', updated_at_ms: 1,
          data: { dkim_selector: 'default', dkim_public_key: 'TESTPUBLICKEY', spf_record: 'v=spf1 mx a ~all', dmarc_record: 'v=DMARC1; p=none; rua=mailto:dmarc@example.test' } },
        { id: 'user:alice@example.test', kind: 'user', name: 'alice@example.test', updated_at_ms: 1, data: { owner_user_id: 'alice', communication_account_key: ACCOUNT } },
      ],
    };
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    window.__mod = await import('/modules/mail/index.js');
    const kernel = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false,
      actor: { id: 'alice', display_name: 'Alice Admin', role: 'admin', permissions: ['apps.source.edit'] } });
    await kernel.ready;
    for (const [name, docs] of Object.entries(rows)) await kernel.sim.push(name, docs);
    // Nach den business_apps-Zeilen veröffentlichen (sonst überschreiben sie active_ver); Eigentümer zuerst.
    for (const o of window.__routes?._fixture_owners ?? []) if (!(await kernel.sim.inspect('business_apps', o.app))?.data?.active_ver) await kernel.sim.publishAppVersion(o.app, o.ver, o.publish);
    // Ein Fremd-App-Befehl braucht beim Ziel commands_in + handlers (sonst forbidden); mit Handler löst der Stub dessen Artefakt
    // vor dem Simulator auf. Der echte Handler (relative Importe) ist kein Einzelartefakt; das Ergebnis liefert sim.onCommand
    // unten, der Platzhalter würde nur ohne Simulator laufen und schlägt dann laut fehl.
    await kernel.sim.publishAppVersion('outbound', 'qa-1', { title: 'Outbound', manifestExtra: window.__outboundRoutes,
      files: { [window.__outboundRoutes.handlers['outbound.message.compose']]: "export default () => { throw new Error('fixture: sim.onCommand simulates outbound.message.compose'); };" } });
    await kernel.sim.publishAppVersion('mail', '1', { title: 'Mail', manifestExtra: window.__routes });
    // Core-Seite der Befehle (V2 commandBus.dispatch, mail-browser-qa.mjs:291-405): Ergebnis wird projiziert.
    const sim = kernel.sim;
    const get = (name, id) => kernel.appApi('mail', '1').query(name, {}).get().find((d) => d.id === id);
    sim.onCommand('ctox.mailserver.save_runtime', async (p) => {
      const cur = get('mail_server_state', 'runtime');
      const enabled = p.enabled !== false;
      await sim.push('mail_server_state', { ...cur, status: enabled ? 'running' : 'stopped', updated_at_ms: Date.now(),
        data: { ...cur.data, config: { ...cur.data.config, ...p }, health: { ...cur.data.health, smtp_reachable: enabled, imap_reachable: enabled } } });
      return { ok: true };
    });
    sim.onCommand('ctox.mailserver.save_domain', async (p) => {
      const { domain_name: name, ...rest } = p;
      await sim.push('mail_server_state', { id: `domain:${name}`, kind: 'domain', name, updated_at_ms: Date.now(), data: { ...rest, dkim_public_key: 'GENERATEDPUBLICKEY' } });
      return { ok: true };
    });
    sim.onCommand('ctox.mailserver.delete_domain', async (p) => { await sim.push('mail_server_state', { id: `domain:${p.domain_name}`, _deleted: true }); return { deleted: true }; });
    sim.onCommand('ctox.mailserver.save_user', async (p) => {
      window.__saveUserPayloads.push(structuredClone(p));
      const key = `email:${p.username}`;
      await sim.push('mail_server_state', { id: `user:${p.username}`, kind: 'user', name: p.username, updated_at_ms: Date.now(), data: { owner_user_id: p.owner_user_id, communication_account_key: key } });
      await sim.push('communication_accounts', { id: key, account_key: key, channel: 'email', address: p.username, provider: 'ctox-mailserver',
        profile_json: { owner_user_id: p.owner_user_id, mailbox_address: p.username, mailbox_status: 'ready', mailserver_managed: true },
        created_at: new Date().toISOString(), updated_at: new Date().toISOString() });
      return { username: p.username };
    });
    sim.onCommand('ctox.mailserver.delete_user', async (p) => {
      await sim.push('mail_server_state', { id: `user:${p.username}`, _deleted: true });
      await sim.push('communication_accounts', { id: `email:${p.username}`, _deleted: true });
      return { username: p.username };
    });
    // outbound.message.compose (Handler modules/outbound/handlers, MAIL2 A) projiziert je Empfänger eine Nachricht.
    sim.onCommand('outbound.message.compose', async (p) => {
      const at = Date.now();
      const mode = p.mode;
      await sim.push('outbound_messages', p.recipients.map((r) => ({ id: r.message_id, engagement_id: r.engagement_id, campaign_id: p.campaign_id || '',
        channel: 'email', direction: 'outbound', sender_account_id: p.sender_account_id, recipient_email: r.recipient_email, subject: p.subject,
        body_text: p.body_text, approval_status: mode === 'draft' ? 'draft' : 'approved', send_status: mode === 'send' ? 'queued_for_provider' : 'not_scheduled',
        payload: mode === 'send' ? { provider_dispatch_status: 'queued_in_mailserver', provider_queued_at_ms: at } : {}, created_at_ms: at, updated_at_ms: at })));
      return { ok: true };
    });
    sim.onCommand('support.conversation.open_from_thread', async () => ({}));
    window.__saveUserPayloads = [];
    await sim.settle();
    window.__k = kernel;
    window.__api = kernel.appApi('mail', '1');
    window.__rows = (name) => window.__api.query(name, {}).get();
    window.__commands = () => window.__api.query('business_commands', { sort: [{ created_at: 'asc' }] }).get();
    window.__zone = () => kernel.statusZone.forApp('mail').get().filter((e) => e.source === 'app');
    window.__remount = async () => {
      await window.__app?.unmount();
      window.__api = kernel.appApi('mail', '1');
      window.__app = await window.__mod.mount(document.getElementById('win'), window.__api, {});
    };
    await window.__remount();
  });
  await settle();

  // ---------- Posteingang (V2 L414-L427) ----------
  await step('inbox.render', 'L414-L415', async () => {
    await page.locator('[data-mail-root]').waitFor({ state: 'visible' });
    await assertVisibleText('Projektstatus August');
  });
  await step('inbox.scope-order', 'L416-L419', async () => {
    assert.deepEqual(await page.locator('[data-mail-scope="queue"]').evaluateAll((nodes) => nodes.slice(0, 3).map((node) => node.dataset.mailScopeId)), ['inbound', 'outbound', 'all']);
  });
  await step('inbox.section-titles', 'L420', async () => {
    assert.deepEqual(await page.locator('.mail-scope-section-title').allTextContents(), ['Ordner', 'Arbeitsabläufe']);
  });
  await shot('1440-inbox');
  await step('inbox.navigation-title', 'L424', async () => {
    assert.equal(await page.locator('[data-mail-navigation-title]').textContent(), 'Postfach');
  });
  await step('inbox.account-select', 'L425-L426', async () => {
    assert.equal(await page.locator('[data-mail-account]').inputValue(), 'all');
    assert.match(await page.locator('[data-mail-account]').textContent(), /alice@example\.test/);
  });
  await step('inbox.sent-scope-label', 'L427', async () => {
    assert.match(await page.locator('[data-mail-scope-id="outbound"]').textContent(), /Gesendet/);
  });

  // ---------- Geteilte Provider-Thread-ID über zwei Postfächer (V2 L428-L467) ----------
  await step('isolation.bob-account-appears', 'L430-L446', async () => {
    await page.evaluate(() => Promise.all([
      window.__k.sim.push('communication_accounts', { id: 'email:bob@example.test', account_key: 'email:bob@example.test', channel: 'email',
        address: 'bob@example.test', provider: 'ctox-mailserver', profile_json: { owner_user_id: 'bob' } }),
      window.__k.sim.push('communication_messages', { id: 'bob-shared-thread', message_key: 'bob-shared-thread', thread_key: 'thread-1', channel: 'email',
        account_key: 'email:bob@example.test', direction: 'outbound', folder_hint: 'sent', sender_address: 'bob@example.test',
        subject: 'Bob private subject', body_text: 'Bob private body', external_created_at: '2026-08-06T10:00:00Z', observed_at: '2026-08-06T10:00:01Z' }),
    ]));
    await page.waitForFunction(() => document.querySelector('[data-mail-account]')?.textContent.includes('bob@example.test'));
  });
  const aliceSharedRow = page.locator('[data-mail-record-kind="thread"][data-mail-record-id="thread-1"]');
  await step('isolation.alice-row-unchanged', 'L447-L449', async () => {
    assert.match(await aliceSharedRow.textContent(), /Projektstatus August/);
    assert.doesNotMatch(await aliceSharedRow.textContent(), /Bob private/);
  });
  await step('isolation.alice-thread-without-bob', 'L450-L452', async () => {
    await aliceSharedRow.click();
    await page.locator('[data-mail-detail]').getByText('Können Sie uns den aktuellen Stand schicken?', { exact: true }).waitFor({ state: 'visible' });
    assert.doesNotMatch(await page.locator('[data-mail-detail]').textContent(), /Bob private/);
  });
  await step('isolation.bob-mailbox-empty', 'L453-L457', async () => {
    await page.locator('[data-mail-left-pane] [data-pg-tray-toggle]').click();
    await page.locator('[data-mail-account]').selectOption('email:bob@example.test');
    assert.equal(await page.locator('[data-mail-record-kind="thread"]').count(), 0);
    await page.locator('[data-mail-detail]').waitFor({ state: 'hidden' });
    assert.doesNotMatch(await page.locator('[data-mail-detail]').textContent(), /Können Sie uns den aktuellen Stand schicken/);
  });
  await step('isolation.bob-removed', 'L458-L467', async () => {
    await page.locator('[data-mail-account]').selectOption('all');
    await page.locator('[data-mail-left-pane] [data-pg-tray-toggle]').click();
    await page.evaluate(() => Promise.all([
      window.__k.sim.push('communication_accounts', { id: 'email:bob@example.test', _deleted: true }),
      window.__k.sim.push('communication_messages', { id: 'bob-shared-thread', _deleted: true }),
    ]));
    await page.waitForFunction(() => !document.querySelector('[data-mail-account]')?.textContent.includes('bob@example.test'));
  });

  // ---------- Gesendet (V2 L468-L488) ----------
  await step('sent.band', 'L468-L469', async () => {
    await page.locator('[data-mail-list-pane] [data-pg-band="outbound"]').click();
    await assertVisibleText('Versandter Bericht');
  });
  await step('sent.scope-selected', 'L470-L477', async () => {
    assert.deepEqual(await page.locator('[data-mail-scope="queue"][aria-selected="true"]').evaluateAll((nodes) => nodes.map((node) => node.dataset.mailScopeId)), ['outbound']);
    assert.deepEqual(await page.locator('[data-mail-scope="queue"].is-active').evaluateAll((nodes) => nodes.map((node) => node.dataset.mailScopeId)), ['outbound']);
  });
  await step('sent.row-status', 'L478', async () => {
    assert.match(await page.locator('[data-mail-record-id="thread-sent"] .mail-record-status').textContent(), /Gesendet/);
  });
  await step('sent.list-title', 'L482', async () => {
    assert.equal(await page.locator('[data-mail-list-title]').textContent(), 'Gesendet');
  });
  await step('sent.band-and-scope-switch', 'L483-L488', async () => {
    await page.locator('[data-mail-list-pane] [data-pg-band="inbound"]').click();
    await assertVisibleText('Projektstatus August');
    await page.locator('[data-mail-scope-id="outbound"]').click();
    await assertVisibleText('Versandter Bericht');
    await page.locator('[data-mail-scope-id="inbound"]').click();
    await page.locator('[data-mail-record-id="thread-1"]').click();
  });

  // ---------- Live-Emission statt Lesefehler (V2 L489-L516; Fehlerkasten ohne v4-Gegenstück, NO_V4) ----------
  await step('inbox.reemission-keeps-selection', 'L489-L497', async () => {
    // v4: eine erneute Emission derselben Daten (Core-Push) behält Zeile und Auswahl, ohne Zustandstext.
    await page.evaluate(() => window.__k.sim.push('communication_threads', window.__rows('communication_threads').find((t) => t.id === 'thread-1')));
    await settle();
    assert.equal(await page.locator('[data-mail-record-id="thread-1"]').count(), 1);
    assert.equal(await page.locator('[data-mail-record-id="thread-1"].is-selected').count(), 1);
    assert.deepEqual(await page.evaluate(() => window.__zone().filter((e) => e.level === 'error')), []);
  });
  await step('inbox.empty-threads', 'L504-L511', async () => {
    await page.evaluate(() => {
      window.__savedThreads = window.__rows('communication_threads').map((t) => ({ ...t }));
      return window.__k.sim.push('communication_threads', window.__savedThreads.map((t) => ({ id: t.id, _deleted: true })));
    });
    await settle();
    assert.equal(await page.locator('[data-mail-record-kind="thread"]').count(), 0);
    // Textdiät: keine Leer-/Fehlerkästen, keine Zone.
    assert.equal(await page.locator('#win .ctox-empty, #win [role="status"], #win [aria-busy="true"]').count(), 0);
    assert.deepEqual(await page.evaluate(() => window.__zone().filter((e) => e.level === 'error')), []);
  });
  await step('inbox.threads-restored', 'L512-L516', async () => {
    await page.evaluate(() => window.__k.sim.push('communication_threads', window.__savedThreads));
    await page.locator('[data-mail-record-id="thread-1"]').waitFor({ state: 'visible' });
  });
  await step('inbox.close-detail', 'L517', async () => {
    await page.locator('[data-mail-close-detail]').click();
  });
  await step('icons.acceptance', 'L518 / L1045-L1096', () => assertMailIconAcceptance(page, iconProviderMode));
  if (mailIconOnly) {
    assert.deepEqual(browserErrors, [], `Mail icon QA must not emit page or console errors (${iconProviderMode} provider)`);
    console.log(`Mail icon QA v4 OK (${iconProviderMode} provider → app-local glyphs): toolbar, progress, navigation/close, and both toggle directions`);
    break mailQa;
  }

  // ---------- Sammel-Routing an Support (V2 L525-L541) ----------
  await step('route.bulk-select', 'L525-L527', async () => {
    await page.locator('[data-mail-record-id="thread-1"][data-mail-record-account="email:alice@example.test"] [data-mail-select-record]').check();
    await page.locator('[data-mail-record-id="thread-2"][data-mail-record-account="email:alice@example.test"] [data-mail-select-record]').check();
    await page.locator('[data-mail-bulkbar]').waitFor({ state: 'visible' });
  });
  await step('route.confirm', 'L528-L531', async () => {
    await page.locator('[data-mail-bulk-route]').click();
    await page.locator('[data-mail-route-destination]').selectOption('support');
    await page.locator('[data-mail-route-note]').fill('Als Supportfälle übernehmen und priorisieren.');
    await page.locator('[data-mail-confirm-route]').click();
    await settle();
  });
  await step('route.notice', 'L532', async () => {
    // v4: Erfolgsmeldung bei done in der Zustandszone (mail.notice) statt als App-Text (PORTING R11).
    const notice = await page.evaluate(() => window.__zone().find((e) => e.code === 'mail.notice'));
    assert.match(notice?.text || '', /2 E-Mails geroutet an Support/);
  });
  await step('route.commands', 'L533-L536', async () => {
    const routed = await page.evaluate(() => window.__commands().filter((c) => c.payload?.client_context?.surface === 'mail.bulk.route'));
    assert.equal(routed.length, 2);
    assert.ok(routed.every((c) => c.type === 'support.conversation.open_from_thread'));
    // V2: kein Altfeld `type` am commandBus-Befehl; v4: Befehlstyp nur in `type`, kein `command_type` im Payload.
    assert.ok(routed.every((c) => c.payload.command_type === undefined));
    assert.ok(routed.every((c) => c.status === 'done'));
  });
  await step('route.close-shows-routed', 'L540-L541', async () => {
    if (await page.locator('[data-mail-close-route]').isVisible()) await page.locator('[data-mail-close-route]').click();
    await assertVisibleText('Geroutete E-Mails');
  });
  await step('thread.open', 'L543-L544', async () => {
    await page.locator('[data-mail-record-id="thread-1"]').click();
    await assertVisibleText('Können Sie uns den aktuellen Stand schicken?');
  });

  // ---------- E-Mail-Gruppen, Versandfortschritt, Tracking (V2 L549-L570) ----------
  await step('campaign.band', 'L549-L550', async () => {
    await page.locator('[data-mail-left-pane] [data-pg-band="campaigns"]').click();
    assert.equal(await page.locator('[data-mail-navigation-title]').textContent(), 'E-Mail-Gruppen');
  });
  await step('campaign.detail', 'L551-L552', async () => {
    await page.locator('[data-mail-scope-id="campaign-1"]').click();
    await assertVisibleText('Status-Updates bündeln');
  });
  const inlineProgress = page.locator('[data-mail-record-id="message-delivered-1"] .mail-record-progress');
  await step('campaign.inline-progress', 'L554-L559', async () => {
    await page.locator('[data-mail-close-detail]').click();
    await inlineProgress.waitFor({ state: 'visible' });
    assert.equal(await inlineProgress.locator('.mail-progress-step').count(), 6);
    assert.equal(await inlineProgress.locator('.mail-progress-step.is-done').count(), 6);
    assert.equal(await inlineProgress.locator('.mail-progress-track > i').getAttribute('style'), 'width:100%');
  });
  await step('campaign.tracking', 'L564-L566', async () => {
    await page.locator('[data-mail-record-id="message-delivered-1"]').click();
    await assertVisibleText('Öffnung erfasst · 2×');
    await assertVisibleText('Link geklickt · 1×');
  });
  await step('campaign.reselect', 'L570', async () => {
    await page.locator('[data-mail-scope-id="campaign-1"]').click();
  });

  // ---------- Visueller Inhaltseditor (V2 L572-L697) ----------
  const editorFrame = page.frameLocator('[data-mail-easy-email-host] iframe');
  const editable = editorFrame.locator('[contenteditable="true"]').first();
  const visibleContent = editorFrame.locator('[contenteditable="true"]:visible').filter({ hasText: 'Echter visueller Kampagneninhalt' }).first();
  await step('editor.open', 'L572-L573', async () => {
    await page.locator('[data-mail-action="edit-group-content"]').click();
    await page.locator('[data-mail-content-surface]').waitFor({ state: 'visible' });
  });
  await step('editor.type', 'L578-L581', async () => {
    await editable.waitFor({ state: 'visible' });
    await editable.click();
    await editable.pressSequentially('Echter visueller Kampagneninhalt', { delay: 15 });
    await visibleContent.waitFor({ state: 'visible', timeout: 5000 });
  });
  await step('editor.undo-redo', 'L582-L587', async () => {
    await page.locator('[data-mail-content-title]').click();
    await page.waitForTimeout(500);
    // Auf die Wirkung warten statt auf feste Zeiten: der Verlauf des Editors läuft asynchron, auf dem langsameren
    // CI-Runner kam „redo“ nach 300 ms vor dem fertigen „undo“ an (17.10.2026, Inhalt blieb bei „…Echter“).
    const editableText = () => editorFrame.locator('[contenteditable="true"]:visible').first().innerText();
    const before = await editableText();
    // Der Formularverlauf von EasyEmail übernimmt das Getippte asynchron; ein undo, das davor ankommt, ist ein Leerlauf
    // (CI 17.10.: Inhalt blieb 10 s unverändert). Deshalb undo wiederholen, bis es wirkt – höchstens 5 Versuche.
    let undone = false;
    for (let attempt = 0; attempt < 5 && !undone; attempt++) {
      await page.locator('[data-mail-editor-history="undo"]').click();
      undone = await expectEventually(async () => (await editableText()) !== before, 'undo ändert den Inhalt', 3000).then(() => true, () => false);
    }
    assert.ok(undone, `undo ändert den Inhalt nicht (vorher: ${JSON.stringify(before.slice(0, 80))})`);
    await page.locator('[data-mail-editor-history="redo"]').click();
    await expectEventually(async () => (await editableText()) === before, 'redo stellt den Inhalt wieder her');
    await visibleContent.waitFor({ state: 'visible' });
  });
  await step('editor.preview-viewports', 'L588-L594', async () => {
    await page.locator('[data-mail-editor-viewport="desktop"]').click();
    assert.equal(await page.locator('[data-mail-editor-viewport="desktop"]').getAttribute('aria-pressed'), 'true');
    await page.locator('[data-mail-editor-viewport="mobile"]').click();
    assert.equal(await page.locator('[data-mail-editor-viewport="mobile"]').getAttribute('aria-pressed'), 'true');
    await page.locator('[data-mail-editor-viewport="edit"]').click();
    await editable.click();
    assert.equal(await editorFrame.locator('.ctox-frame-tools').count(), 0);
  });
  const insertedButton = editorFrame.locator('[contenteditable="true"]').filter({ hasText: /^\s*Button\s*$/ });
  await step('editor.drag-button-block', 'L595-L617', async () => {
    await page.locator('[data-mail-editor-open-panel="blocks"]').click();
    await editorFrame.getByText('Baustein in die E-Mail ziehen', { exact: true }).waitFor({ state: 'visible' });
    const buttonBlock = editorFrame.locator('.ctox-frame-block-item').filter({ hasText: 'Button' }).locator('[draggable="true"]');
    const dropBlock = editorFrame.locator('.email-block').last();
    assert.equal(await insertedButton.count(), 0, 'Campaign canvas must not have a Button before drag');
    await buttonBlock.hover();
    const sourceBox = await buttonBlock.boundingBox();
    const targetBox = await dropBlock.boundingBox();
    assert.ok(sourceBox && targetBox, 'Button and email canvas must be visible for drag');
    const sourceX = sourceBox.x + sourceBox.width / 2;
    const sourceY = sourceBox.y + sourceBox.height / 2;
    await page.mouse.move(sourceX, sourceY);
    await page.mouse.down();
    await page.mouse.move(sourceX + 16, sourceY + 16, { steps: 4 });
    await editorFrame.locator('[data-dragging="true"]').waitFor({ state: 'attached', timeout: 3_000 });
    await page.mouse.move(targetBox.x + targetBox.width / 2, targetBox.y + 6, { steps: 18 });
    await editorFrame.locator('[data-direction]:not([data-direction="none"])').waitFor({ state: 'attached', timeout: 3_000 });
    await page.mouse.up();
    await insertedButton.waitFor({ state: 'visible' });
  });
  await step('editor.design-panel', 'L618-L620', async () => {
    await page.locator('[data-mail-editor-open-panel="design"]').click();
    await editorFrame.getByRole('tab', { name: 'Element', exact: true }).waitFor({ state: 'visible' });
    assert.equal(await editorFrame.locator('#easy-email-rich-text-bar').isVisible(), false);
  });
  await step('editor.source-panel', 'L624-L625', async () => {
    await page.locator('[data-mail-editor-open-panel="source"]').click();
    await editorFrame.getByText('HTML & MJML', { exact: true }).waitFor({ state: 'visible' });
  });
  await step('editor.logic-rule', 'L626-L642', async () => {
    await page.locator('[data-mail-editor-open-panel="logic"]').click();
    await page.locator('[data-mail-logic-editor]').waitFor({ state: 'visible' });
    assert.equal(await editorFrame.locator('#easy-email-rich-text-bar').isVisible(), false);
    await page.locator('[data-logic-action="add-rule"]').first().click();
    const firstLogicRule = page.locator('.mail-logic-rule').first();
    await firstLogicRule.locator('[data-logic-field="field"]').fill('contact.segment');
    await firstLogicRule.locator('[data-logic-field="field"]').press('Tab');
    await firstLogicRule.locator('[data-logic-field="operator"]').selectOption('equals');
    await firstLogicRule.locator('[data-logic-field="valueType"]').selectOption('string');
    await firstLogicRule.locator('[data-logic-field="value"]').fill('kunde');
    await firstLogicRule.locator('[data-logic-field="value"]').press('Tab');
    await page.locator('[data-logic-test-data]').fill('{"contact":{"segment":"kunde"}}');
    await page.getByText('Block wird angezeigt', { exact: true }).waitFor({ state: 'visible' });
    await page.locator('[data-logic-test-data]').fill('{"contact":{"segment":"lead"}}');
    await page.getByText('Block wird für diese Testdaten ausgeblendet', { exact: true }).waitFor({ state: 'visible' });
  });
  await step('editor.controls', 'L643-L647', async () => {
    assert.equal(await page.locator('[data-mail-save-content]').getAttribute('class'), 'ctox-pane-icon');
    for (const panel of ['blocks', 'design', 'source', 'logic']) {
      assert.equal(await page.locator(`[data-mail-editor-open-panel="${panel}"]`).isVisible(), true, `panel ${panel}`);
    }
  });
  await step('editor.theme-bridge-light', 'L648', () => assertEditorThemeBridge(page, editorFrame));
  await step('editor.viewports', 'L649 / L1116-L1149', async () => {
    assert.deepEqual(await measureEditorViewports(page), KNOWN_FINDINGS['editor.viewports'] || [],
      `Editor-Geometrie weicht vom bekannten Stand ab: ${JSON.stringify(geometryLog)}`);
  });
  await step('editor.theme-bridge-dark', 'L653-L658', async () => {
    // V2 nur mit Screenshot-Variable; v4 prüft dunkel immer (Theme setzt die Shell am Dokument).
    await page.evaluate(() => { document.documentElement.dataset.theme = 'dark'; });
    await page.waitForTimeout(200);
    await assertEditorThemeBridge(page, editorFrame);
  });
  await step('editor.theme-bridge-brand', 'L659-L676', async () => {
    await page.evaluate(() => {
      const root = document.documentElement;
      root.dataset.theme = 'light';
      root.style.setProperty('--accent', '#6d28d9');
      root.style.setProperty('--accent-soft', '#ede9fe');
      root.style.setProperty('--focus-ring', '#7c3aed');
    });
    await page.waitForTimeout(200);
    await assertEditorThemeBridge(page, editorFrame);
    await page.evaluate(() => {
      const root = document.documentElement;
      for (const name of ['--accent', '--accent-soft', '--focus-ring']) root.style.removeProperty(name);
      root.dataset.theme = 'light';
    });
    await page.waitForTimeout(100);
  });
  await step('editor.save-revision', 'L679-L691', async () => {
    await visibleContent.waitFor({ state: 'visible' });
    await page.locator('[data-mail-save-content]').click();
    await page.waitForFunction(() => Boolean(window.__rows('outbound_campaigns').find((c) => c.id === 'campaign-1')?.payload?.mail_group?.content?.active_revision_id));
    const savedContent = await page.evaluate(() => window.__rows('outbound_campaigns').find((c) => c.id === 'campaign-1').payload.mail_group.content);
    assert.equal(savedContent.revisions.length, 1);
    assert.equal(savedContent.active_revision_id, savedContent.revisions[0].id);
    assert.match(JSON.stringify(savedContent.editor_envelope.htmlDocument), /Echter visueller Kampagneninhalt/);
    assert.match(JSON.stringify(savedContent.editor_envelope.htmlDocument), /contact\.segment/);
    assert.ok(savedContent.active_revision.compiled_html_ref.content.length > 1000);
  });
  await step('editor.reopen-keeps-content', 'L692-L697', async () => {
    await page.locator('[data-mail-close-content-editor]').click();
    await page.locator('[data-mail-content-surface]').waitFor({ state: 'hidden' });
    await page.locator('[data-mail-action="edit-group-content"]').click();
    await editorFrame.locator('[contenteditable="true"]').first().waitFor({ state: 'visible' });
    await visibleContent.waitFor({ state: 'visible' });
    await page.locator('[data-mail-close-content-editor]').click();
  });

  // ---------- Verfassen und sofort senden (V2 L699-L716) ----------
  let commandsBeforeSend = 0;
  await step('compose.fill', 'L699-L703', async () => {
    await page.locator('[data-mail-compose-primary]').click();
    await page.locator('[data-mail-compose-to]').fill('kontakt@example.test');
    await page.locator('[data-mail-compose-campaign]').selectOption('campaign-1');
    await page.locator('[data-mail-compose-subject]').fill('August Update');
    await page.locator('[data-mail-compose-body]').fill('Hier ist der aktuelle Projektstand.');
    commandsBeforeSend = await page.evaluate(() => window.__commands().length);
  });
  await step('compose.send-now', 'L705-L708', async () => {
    await page.locator('[data-mail-send-now]').click();
    await page.locator('[data-mail-composer]').waitFor({ state: 'hidden' });
    await settle();
    await assertVisibleText('August Update');
    assert.equal(await page.evaluate(() => window.__rows('outbound_messages').length), 2);
  });
  await step('compose.send-chain', 'L709-L716', async () => {
    // V2: fünf Einzelbefehle (engagement.create → … → send_approved); v4 (MAIL2 A): EIN outbound.message.compose mode=send.
    const sent = await page.evaluate((from) => window.__commands().slice(from), commandsBeforeSend);
    assert.deepEqual(sent.map((c) => c.type), ['outbound.message.compose']);
    assert.equal(sent[0].payload.mode, 'send');
    assert.equal(sent[0].payload.campaign_id, 'campaign-1');
    assert.equal(sent[0].payload.subject, 'August Update');
    assert.deepEqual(sent[0].payload.recipients.map((r) => r.recipient_email), ['kontakt@example.test']);
    assert.equal(sent[0].status, 'done');
  });

  // ---------- Neue E-Mail-Gruppe (V2 L718-L722) ----------
  await step('group.create', 'L718-L722', async () => {
    await page.locator('[data-mail-new-group]').click();
    await page.locator('[data-mail-group-name]').fill('Messekontakte August');
    await page.locator('[data-mail-group-description]').fill('Nachfassgruppe für die August-Messe');
    await page.locator('[data-mail-create-group]').click();
    await settle();
    await assertVisibleText('Messekontakte August');
  });

  // ---------- Mailserver-Verwaltung (V2 L724-L758) ----------
  await step('admin.open', 'L724-L725', async () => {
    await page.locator('[data-mail-settings]').click();
    await page.locator('[data-mail-mailbox-admin]').waitFor({ state: 'visible' });
  });
  await step('admin.running-state', 'L726', async () => {
    // V2: Text „Mailserver läuft“; v4: Laufzustand nur als Punkt is-running + Schaltknopf „Stoppen“ (PORTING R11).
    assert.equal(await page.locator('[data-mail-server-dot].is-running').count(), 1);
    assert.equal((await page.locator('[data-mail-toggle-server]').textContent()).trim(), 'Stoppen');
  });
  await step('admin.endpoints', 'L727', async () => assertVisibleText('SMTP 127.0.0.1:2525 · IMAP 127.0.0.1:1143'));
  await step('admin.queue-count', 'L728', async () => {
    assert.equal(await page.locator('[data-mail-queue-count]').textContent(), '2');
  });
  await step('admin.save-throttle', 'L729-L731', async () => {
    await page.locator('[data-mail-server-throttle]').fill('90');
    await page.locator('[data-mail-save-server]').click();
    await page.waitForFunction(() => window.__rows('mail_server_state').find((d) => d.id === 'runtime')?.data?.config?.outbound_throttle_per_min === 90);
  });
  await step('admin.save-domain', 'L732-L736', async () => {
    await page.locator('[data-mail-domain-form] [data-mail-domain-name]').fill('campaign.example.test');
    await page.locator('[data-mail-dmarc-record]').fill('v=DMARC1; p=quarantine');
    await page.locator('[data-mail-save-domain]').click();
    await page.locator('[data-mail-domain-list]').getByText('campaign.example.test', { exact: true }).waitFor({ state: 'visible' });
    await page.locator('[data-mail-domain-list]').getByText('example.test', { exact: true }).waitFor({ state: 'visible' });
  });
  await step('admin.create-mailbox', 'L741-L749', async () => {
    await page.locator('[data-mail-mailbox-username]').fill('bob@example.test');
    await page.locator('[data-mail-mailbox-owner]').selectOption('bob');
    await page.locator('[data-mail-mailbox-password]').fill('only-for-command');
    await page.locator('[data-mail-create-mailbox]').click();
    await settle();
    await assertVisibleText('Zugeordnet zu: Bob Beispiel');
    assert.equal(await page.locator('[data-mail-mailbox-password]').inputValue(), '');
    const createdAccount = await page.evaluate(() => window.__rows('communication_accounts').find((a) => a.id === 'email:bob@example.test'));
    assert.equal(createdAccount.profile_json.owner_user_id, 'bob');
    assert.equal(JSON.stringify(createdAccount).includes('only-for-command'), false);
    // Passwort geht nur an den Core (Secret-Intake), nie in eine Projektion.
    assert.equal(await page.evaluate(() => JSON.stringify(window.__rows('mail_server_state')).includes('only-for-command')), false);
  });
  await step('admin.delete-mailbox', 'L754-L758', async () => {
    await page.locator('[data-mailbox-action="request-delete"][data-mailbox-username="bob@example.test"]').click();
    await page.locator('[data-mailbox-action="confirm-delete"][data-mailbox-username="bob@example.test"]').click();
    await page.waitForFunction(() => !window.__rows('mail_server_state').some((d) => d.id === 'user:bob@example.test'));
    assert.equal(await page.evaluate(() => window.__rows('communication_accounts').some((a) => a.id === 'email:bob@example.test')), false);
    await page.locator('[data-mail-close-mailbox-admin]').click();
  });

  // ---------- Schmales Fenster 700 px, Sellify-Serien-E-Mail (V2 L760-L793) ----------
  await step('narrow.navigation-and-settings', 'L760-L773', async () => {
    await page.setViewportSize({ width: 700, height: 820 });
    if (await page.locator('[data-mail-root]').evaluate((node) => node.classList.contains('is-inspector-open'))) {
      await page.locator('[data-mail-close-detail]').click();
    }
    await page.locator('[data-mail-open-nav]').click();
    await page.locator('.mail-sidebar').waitFor({ state: 'visible' });
    await page.locator('[data-mail-settings]').click();
    await page.locator('[data-mail-mailbox-admin]').waitFor({ state: 'visible' });
    await page.locator('[data-mail-close-mailbox-admin]').click();
    await page.locator('[data-mail-open-nav]').click();
    await page.locator('.mail-sidebar').waitFor({ state: 'visible' });
    await page.locator('[data-mail-close-nav]').click();
  });
  await step('series.handoff', 'L774-L779', async () => {
    // V2: location.hash; v4: Sellify navigiert über die Shell-Route zu Mail (api.shell.navigate → onRoute).
    const routed = await page.evaluate(() => window.__k.appApi('sellify', '1').shell.navigate('mail', {
      action: 'series-email', source_module: 'sellify', campaign_id: 'campaign-1',
      recipients: 'kontakt@example.test,einkauf@example.test', subject: 'August Update' }));
    assert.equal(routed, true);
    await page.locator('[data-mail-composer]').waitFor({ state: 'visible' });
    await page.getByText('Serien-E-Mail', { exact: true }).waitFor({ state: 'visible' });
    assert.equal(await page.locator('[data-mail-compose-to]').inputValue(), 'kontakt@example.test\neinkauf@example.test');
  });
  await step('series.composer-fits', 'L780-L782', async () => {
    await page.waitForTimeout(250);
    const composerBox = await page.locator('[data-mail-composer]').boundingBox();
    assert.ok(composerBox && composerBox.width <= 700 && composerBox.height <= 820, JSON.stringify(composerBox));
  });
  await shot('700-series-composer');
  await step('series.save-draft', 'L787-L793', async () => {
    await page.locator('[data-mail-compose-body]').fill('Serienmail aus Sellify.');
    await page.locator('[data-mail-save-draft]').click();
    await page.locator('[data-mail-composer]').waitFor({ state: 'hidden' });
    await settle();
    assert.deepEqual(await page.evaluate(() => window.__rows('outbound_messages').map((m) => m.recipient_email).sort()),
      ['einkauf@example.test', 'kontakt@example.test', 'kontakt@example.test', 'kunde@example.test']);
    await page.setViewportSize({ width: 1440, height: 940 });
  });

  // ---------- Remount und Live-Änderung (V2 L795-L833; Lese-Timeout/Readiness ohne v4-Gegenstück, NO_V4) ----------
  await step('remount.rows-visible', 'L795-L826', async () => {
    await page.evaluate(() => window.__remount());
    await page.locator('[data-mail-record-id="thread-1"]').waitFor({ state: 'visible' });
    assert.equal(await page.locator('#win [role="status"], #win [aria-busy="true"]').count(), 0);
  });
  await step('live.message-subject-update', 'L828-L833', async () => {
    await page.evaluate(() => {
      const message = window.__rows('communication_messages').find((m) => m.id === 'mail-1');
      return window.__k.sim.push('communication_messages', { ...message, subject: 'Projektstatus nach Sync' });
    });
    await assertVisibleText('Projektstatus nach Sync');
  });

  // ---------- Mitglied, Zweitkonto, Entzug über die Kontoprojektion (V2 L891-L978) ----------
  await step('member.remount', 'L891-L898', async () => {
    await page.evaluate(async () => { window.__k.sim.setActor({ role: 'member' }); await window.__remount(); });
    await page.locator('[data-mail-record-id="thread-1"]').waitFor({ state: 'visible' });
  });
  await step('member.second-account-body', 'L899-L918', async () => {
    await page.evaluate(() => Promise.all([
      window.__k.sim.push('communication_accounts', { id: 'email:second@example.test', account_key: 'email:second@example.test', channel: 'email',
        address: 'second@example.test', profile_json: { owner_user_id: 'alice' } }),
      window.__k.sim.push('outbound_messages', { id: 'message-revoked-b', channel: 'email', direction: 'outbound', sender_account_id: 'email:second@example.test',
        recipient_email: 'private@example.test', subject: 'Second account private mail', body_text: 'PRIVATE SECOND ACCOUNT BODY',
        approval_status: 'approved', send_status: 'sent', created_at_ms: Date.now(), updated_at_ms: Date.now() }),
    ]));
    await page.waitForFunction(() => document.querySelector('[data-mail-account]')?.textContent.includes('second@example.test'));
    await page.locator('[data-mail-scope-id="all"]').click();
    await page.locator('[data-mail-record-id="message-revoked-b"]').click();
    await page.getByText('PRIVATE SECOND ACCOUNT BODY', { exact: true }).waitFor({ state: 'visible' });
  });
  const pushOwner = (key, owner) => page.evaluate(([k, o]) => {
    const account = window.__rows('communication_accounts').find((a) => a.id === k);
    return window.__k.sim.push('communication_accounts', { ...account, profile_json: { ...account.profile_json, owner_user_id: o } });
  }, [key, owner]);
  await step('member.revoke-second-account', 'L919-L926', async () => {
    await pushOwner('email:second@example.test', 'bob');
    await page.getByText('PRIVATE SECOND ACCOUNT BODY', { exact: true }).waitFor({ state: 'hidden', timeout: 8_000 });
    await page.locator('[data-mail-record-id="thread-1"]').waitFor({ state: 'visible' });
  });
  await step('member.revoke-primary-account', 'L927-L935', async () => {
    await pushOwner('email:alice@example.test', 'bob');
    await page.locator('[data-mail-record-id="thread-1"]').waitFor({ state: 'detached', timeout: 8_000 });
    assert.equal(await page.locator('[data-mail-record-kind="thread"]').count(), 0);
  });
  await step('member.restore-primary-account', 'L936-L943', async () => {
    await pushOwner('email:alice@example.test', 'alice');
    await page.locator('[data-mail-record-id="thread-1"]').waitFor({ state: 'visible' });
  });
  await step('member.account-projection-removed', 'L944-L959', async () => {
    // v4-Gegenstück zu „Konto nicht verifizierbar → Postfach ausgeblendet“: der Core entfernt das Konto aus der Projektion.
    await page.evaluate(() => { window.__savedAlice = window.__rows('communication_accounts').find((a) => a.id === 'email:alice@example.test');
      return window.__k.sim.push('communication_accounts', { id: 'email:alice@example.test', _deleted: true }); });
    await page.locator('[data-mail-record-id="thread-1"]').waitFor({ state: 'detached', timeout: 8_000 });
    assert.equal(await page.locator('[data-mail-record-id="thread-sent"]').count(), 0);
    await page.evaluate(() => window.__k.sim.push('communication_accounts', window.__savedAlice));
    await page.locator('[data-mail-record-id="thread-1"]').waitFor({ state: 'visible' });
  });
  await step('member.revoke-keeps-local-bodies-hidden', 'L960-L970', async () => {
    await pushOwner('email:alice@example.test', 'bob');
    await page.locator('[data-mail-record-id="thread-1"]').waitFor({ state: 'detached', timeout: 8_000 });
    assert.equal(await page.locator('[data-mail-record-id="thread-sent"]').count(), 0);
  });
  await step('member.remount-after-revoke', 'L971-L978', async () => {
    await page.evaluate(() => window.__remount());
    await settle();
    assert.equal(await page.locator('[data-mail-record-id="thread-1"]').count(), 0);
    assert.equal(await page.locator('[data-mail-record-id="thread-sent"]').count(), 0);
    assert.ok(await page.evaluate(() => window.__rows('communication_messages').length > 0));
  });
  await step('member.restore-for-evidence', '(v4: Screenshots)', async () => {
    await pushOwner('email:alice@example.test', 'alice');
    await page.locator('[data-mail-record-id="thread-1"]').waitFor({ state: 'visible' });
  });

  // ---------- Belege 1440 / 420 (Auftrag) ----------
  await step('evidence.1440', '(Auftrag)', async () => {
    await page.locator('[data-mail-left-pane] [data-pg-band="queues"]').click();
    await page.locator('[data-mail-scope-id="inbound"]').click();
    await page.locator('[data-mail-record-id="thread-1"]').click();
    await settle();
    await shot('1440-thread');
  });
  await step('evidence.420', '(Auftrag)', async () => {
    await page.setViewportSize({ width: 420, height: 860 });
    await settle();
    await shot('420-thread');
    const overflow = await page.$eval('#win', (el) => el.scrollWidth - el.clientWidth);
    assert.ok(overflow <= 1, `horizontaler Überlauf ${overflow}px bei 420`);
    await page.setViewportSize({ width: 1440, height: 940 });
  });

  await step('no-browser-errors', 'L986', async () => assert.deepEqual(browserErrors, []));
  console.log('Mail browser QA v4 OK: inbox, sent, shared-thread isolation, account revocation via projection, live update, thread, routing, campaign, editor, compose, draft, group, mailbox administration, Sellify series-email handoff, and responsive composer');
} catch (error) {
  exitCode = 1;
  try {
    console.error('[mail QA v4] failure state', JSON.stringify({ browserErrors, zone: await page.evaluate(() => window.__zone?.()) }));
    await shot('failure');
  } catch (diagnosticError) { console.error('[mail QA v4] state capture failed', diagnosticError); }
  console.error(error);
} finally {
  await context.close();
  await browser.close();
  await new Promise((r) => server.close(r));
}
async function iconState(locator) {
  await locator.waitFor({ state: 'visible' });
  return locator.evaluate((node) => {
    const box = node.getBoundingClientRect();
    return { html: node.innerHTML, svgCount: node.querySelectorAll('svg').length, width: box.width, height: box.height };
  });
}

function assertIconState(state, className, label) {
  assert.equal(state.svgCount, 1, `${label} must contain exactly one SVG`);
  assert.match(state.html, new RegExp(className), `${label} must use ${className}`);
  assert.ok(state.width > 0 && state.height > 0, `${label} must be visible`);
}

// V2 L1045-L1096. App-API 4.0 hat keinen Icon-Anbieter: in jedem Modus app-eigene Glyphen (mail-action-*);
// V2 erwartete im Modus shell ctox-action-* (NO_V4).
async function assertMailIconAcceptance(page, _providerMode) {
  const prefix = 'mail-action';
  assert.equal(await page.locator('[data-mail-compose]').count(), 0, 'kein zweiter Neue-Mail-Knopf im Kopf');
  assert.equal(await page.locator('[data-mail-compose-primary]').count(), 1, 'genau ein Neue-Mail-Knopf');
  const toggle = page.locator('[data-mail-left-pane] [data-mail-view-toggle]');
  let toggleState = await iconState(toggle);
  assertIconState(toggleState, `${prefix}-list`, 'cards-to-list toggle');
  assert.equal(await toggle.getAttribute('data-pg-view'), 'cards');
  assert.equal(await toggle.getAttribute('aria-label'), 'Als Liste anzeigen');
  assert.equal(await toggle.getAttribute('title'), 'Als Liste anzeigen');
  assert.equal(await toggle.getAttribute('aria-pressed'), null);
  await toggle.click();
  toggleState = await iconState(toggle);
  assertIconState(toggleState, `${prefix}-grid`, 'list-to-cards toggle');
  assert.equal(await toggle.getAttribute('data-pg-view'), 'list');
  assert.equal(await toggle.getAttribute('aria-label'), 'Als Karten anzeigen');
  assert.equal(await toggle.getAttribute('title'), 'Als Karten anzeigen');
  assert.equal(await toggle.getAttribute('aria-pressed'), null);
  await toggle.click();
  toggleState = await iconState(toggle);
  assertIconState(toggleState, `${prefix}-list`, 'toggle restored');

  await page.locator('[data-mail-left-pane] [data-pg-band="campaigns"]').click();
  await page.locator('[data-mail-scope-id="campaign-1"]').click();
  const progressSteps = page.locator('[data-mail-record-id="message-delivered-1"] .mail-progress-step');
  await progressSteps.first().waitFor({ state: 'visible' });
  for (const [index, name] of ['edit', 'check', 'send', 'check', 'eye', 'link'].entries()) {
    assertIconState(await iconState(progressSteps.nth(index)), `${prefix}-${name}`, `progress ${name}`);
  }

  await page.locator('[data-mail-close-detail]').click();
  await page.setViewportSize({ width: 700, height: 820 });
  await page.waitForTimeout(100);
  assertIconState(await iconState(page.locator('[data-mail-open-nav]')), `${prefix}-columns`, 'navigation open');
  await page.locator('[data-mail-open-nav]').click();
  const sidebar = page.locator('[data-mail-left-pane]');
  await sidebar.waitFor({ state: 'visible' });
  assertIconState(await iconState(page.locator('[data-mail-close-nav]')), `${prefix}-close`, 'navigation close');
  await page.locator('[data-mail-close-nav]').click();
  await sidebar.waitFor({ state: 'hidden' });
  await page.setViewportSize({ width: 1440, height: 940 });
  await page.waitForTimeout(100);
  await page.locator('[data-mail-left-pane] [data-pg-band="queues"]').click();
  await page.locator('[data-mail-scope-id="inbound"]').click();
  await page.locator('[data-mail-record-id="thread-1"]').waitFor({ state: 'visible' });
}

// V2 L1102-L1114: die Editor-iframe übernimmt die Design-Token des Elterndokuments.
async function assertEditorThemeBridge(page, editorFrame) {
  const names = ['--surface', '--surface-2', '--text', '--muted', '--line', '--accent'];
  const parentTokens = await page.evaluate((list) => {
    const styles = getComputedStyle(document.documentElement);
    return list.map((name) => [name, styles.getPropertyValue(name).trim()]);
  }, names);
  const frameTokens = await editorFrame.locator('html').first().evaluate((element, list) => {
    const styles = getComputedStyle(element);
    return list.map((name) => [name, styles.getPropertyValue(name).trim()]);
  }, names);
  assert.deepEqual(frameTokens, parentTokens);
}

// V2 L1116-L1149: Editor-Fläche und Kopfleiste ohne Überlauf in fünf Größen. Liefert je Größe die Verstöße,
// damit bekannte Befunde (KNOWN_FINDINGS) streng gegen den gemessenen Stand geprüft werden.
async function measureEditorViewports(page) {
  const violations = [];
  for (const viewport of [{ width: 1180, height: 820 }, { width: 960, height: 720 }, { width: 640, height: 480 }, { width: 390, height: 844 }, { width: 360, height: 760 }]) {
    await page.setViewportSize(viewport);
    await page.waitForTimeout(150);
    const geometry = await page.locator('[data-mail-content-surface]').evaluate((surface) => {
      const drawer = surface.querySelector('.mail-content-editor-drawer');
      const commandbar = surface.querySelector('.mail-content-editor-commandbar');
      const surfaceBox = surface.getBoundingClientRect();
      const drawerBox = drawer?.getBoundingClientRect();
      return {
        surface: { left: surfaceBox.left, right: surfaceBox.right, width: surfaceBox.width },
        drawer: drawerBox ? { left: drawerBox.left, right: drawerBox.right, width: drawerBox.width } : null,
        commandbarOverflow: commandbar ? commandbar.scrollWidth - commandbar.clientWidth : 0,
      };
    });
    const size = `${viewport.width}x${viewport.height}`;
    if (!(geometry.surface.left >= 0 && geometry.surface.right <= viewport.width + 1)) violations.push(`${size} surface outside window`);
    if (!(geometry.drawer && geometry.drawer.left >= geometry.surface.left - 1 && geometry.drawer.right <= viewport.width + 1)) violations.push(`${size} drawer outside surface`);
    if (!(geometry.commandbarOverflow <= 1)) violations.push(`${size} commandbar overflow`);
    geometryLog.push({ size, ...geometry });
  }
  await page.setViewportSize({ width: 1440, height: 940 });
  await page.waitForTimeout(100);
  return violations;
}

for (const [lines, check, reason] of NO_V4) console.log(`ohne v4-Gegenstück  V2 ${lines}: ${check} — ${reason}`);
for (const [id, list] of Object.entries(KNOWN_FINDINGS)) console.log(`Befund (streng gemessen) ${id}: ${list.join('; ')}`);
console.log(`Mail browser QA v4: ${steps.length} Schritte portiert, ${NO_V4.length} V2-Prüfgruppen ohne v4-Gegenstück; Screenshots: ${shots.join(', ')}`);
process.exitCode = exitCode;

async function expectEventually(probe, what, timeoutMs = 10000) {
  const until = Date.now() + timeoutMs;
  for (;;) {
    if (await probe()) return;
    if (Date.now() > until) throw new Error(`nicht eingetreten: ${what}`);
    await new Promise((r) => setTimeout(r, 100));
  }
}
