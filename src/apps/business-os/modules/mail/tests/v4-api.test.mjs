// mail gegen den App-API-4.0-Kern-Stub (createStubKernel, Pin cff5ba5) in echtem Chromium.
// Muster: modules/knowledge/tests/v4-api.test.mjs, modules/matching/tests/v4-api.test.mjs. Abo-Zähler über countingApi.
// Lauf: CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/4b0d0ac node --test src/apps/business-os/modules/mail/tests/v4-api.test.mjs
// Screenshots: MAIL_SHOTS=1 zusätzlich → tests/shots/mail-{1280,420}.png
import { test, before, after } from 'node:test';
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
const require = createRequire(import.meta.url);
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || join(homedir(), 'Documents/ctox/src/apps/business-os/node_modules/playwright'));
const TYPES = { '.js': 'text/javascript', '.mjs': 'text/javascript', '.css': 'text/css', '.html': 'text/html', '.json': 'application/json', '.svg': 'image/svg+xml' };
// Gleiche Liste wie MAIL_COLLECTIONS in index.js (je Montage genau ein Abo).
const SUBS = ['business_commands', 'business_apps', 'business_users', 'communication_accounts', 'communication_threads',
  'communication_messages', 'outbound_campaigns', 'outbound_pipeline_items', 'outbound_engagements', 'outbound_messages', 'outbound_approvals',
  'mail_server_state'];
const ACCOUNT = 'email:ada@example.test';

// Shell lädt base.css/app.css und die App-CSS aus dem Manifest; der Test bildet das nach.
const PAGE = `<!doctype html><html lang="de"><head><meta charset="utf-8">
<link rel="stylesheet" href="/shared/base.css"><link rel="stylesheet" href="/app.css">
<link rel="stylesheet" href="/modules/mail/index.css"><link rel="stylesheet" href="/modules/mail/editor/editor.css">
<style>html,body{margin:0;height:100%} #win{position:relative;display:grid;overflow:hidden;container:business-app-window / inline-size}</style>
</head><body><div id="win" style="width:1280px;height:820px"></div></body></html>`;

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
  page = await browser.newPage({ viewport: { width: 1280, height: 820 } });
  const errors = [];
  page.on('pageerror', (e) => errors.push(e.message));
  page.on('console', (m) => { if (m.type() === 'error') console.error('[page]', m.text()); });
  page.on('response', (r) => { if (r.status() >= 400 && !/\.css$/.test(r.url())) console.error('[page] HTTP', r.status(), r.url()); });
  page.__errors = errors;
  await page.goto(base);
  // FIXT-R: echte Routen/Rechte/Fremd-Collections von mail (v4/manifests/mail.manifest.json) und der Zielapps seiner
  // commands_out (ctox, outbound, support): Querbefehle brauchen seit E39 eine aktive Zielversion mit commands_in + handler.
  await page.evaluate((routes) => { window.__routes = routes; },
    { mail: realManifestRoutes('mail'), targets: ['ctox', 'outbound', 'support'].map((app) => ({ app, routes: realManifestRoutes(app) })) });
  await page.evaluate(async (account) => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    window.__mod = await import('/modules/mail/index.js');
    const kernel = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false,
      actor: { id: 'u-admin', display_name: 'Admin', role: 'admin', permissions: ['apps.source.edit'] } });
    await kernel.ready;
    // FIXT-R: Zielapps (mit ihren Collections, outbound ist zugleich Eigentümer von outbound_*), dann die übrigen Eigentümer
    // der Fremd-Collections (ctox-platform, conversations, documents, …), zuletzt mail selbst – vor den Core-Daten.
    for (const t of window.__routes.targets) await kernel.sim.publishAppVersion(t.app, '1.0.0', { title: t.app, manifestExtra: t.routes });
    for (const o of window.__routes.mail._fixture_owners) if (!(await kernel.sim.inspect('business_apps', o.app))?.data?.active_ver) await kernel.sim.publishAppVersion(o.app, o.ver, o.publish);
    await kernel.sim.publishAppVersion('mail', '1.0.0', { title: 'Mail', manifestExtra: window.__routes.mail });
    // Projektionen des Cores: v4 verlangt `id`; communication_* tragen ihren Schlüssel zusätzlich (V4-PORT.md OFFEN mail-6).
    await kernel.sim.push('communication_accounts', [{ id: account, account_key: account, channel: 'email', address: 'ada@example.test',
      provider: 'stalwart', profile_json: { owner_user_id: 'u-admin' }, created_at: '2026-10-01T08:00:00Z', updated_at: '2026-10-01T08:00:00Z' }]);
    await kernel.sim.push('communication_threads', [
      { id: 't-angebot', thread_key: 't-angebot', account_key: account, channel: 'email', subject: 'Angebot Q4', unread_count: 1,
        participant_keys_json: ['kunde@example.test'], last_message_at: '2026-10-15T09:00:00Z', updated_at: '2026-10-15T09:00:00Z' },
    ]);
    await kernel.sim.push('communication_messages', [
      { id: 'm-angebot', message_key: 'm-angebot', account_key: account, thread_key: 't-angebot', channel: 'email', direction: 'inbound',
        folder_hint: 'inbox', sender_address: 'kunde@example.test', sender_display: 'Kunde AG', subject: 'Angebot Q4',
        preview: 'Bitte um ein Angebot', body_text: 'Bitte um ein Angebot für Q4.', external_created_at: '2026-10-15T09:00:00Z',
        observed_at: '2026-10-15T09:00:01Z' },
    ]);
    // Plattform-Projektion des Mailservers (Kern §16.28): kind=runtime|domain|user, nie Secrets (Strang MAIL2 D).
    await kernel.sim.push('mail_server_state', [
      { id: 'runtime', kind: 'runtime', status: 'running', updated_at_ms: 1,
        data: { config: { enabled: true, hostname: 'mail.example.test', bind_host: '0.0.0.0', smtp_port: 2587, imap_port: 1993,
          outbound_throttle_per_min: 60, max_connections: 8 },
        health: { smtp_reachable: true, imap_reachable: true, queue_pending: 3, delivery_success: 41, delivery_failed: 2 } } },
      { id: 'domain:example.test', kind: 'domain', name: 'example.test', updated_at_ms: 1,
        data: { dkim_selector: 'mail2026', dkim_public_key: 'MIIBIjANBgkq', spf_record: 'v=spf1 mx -all', dmarc_record: 'v=DMARC1; p=quarantine' } },
      { id: 'user:ada@example.test', kind: 'user', name: 'ada@example.test', updated_at_ms: 1,
        data: { owner_user_id: 'u-admin', communication_account_key: account } },
    ]);
    await kernel.sim.push('outbound_campaigns', [{ id: 'camp-herbst', name: 'Herbstaktion', status: 'active', owner_id: 'u-admin',
      payload: {}, created_at_ms: 1, updated_at_ms: 1 }]);
    await kernel.sim.settle();
    window.__k = kernel;
    window.__api = kernel.appApi('mail', '1.0.0');
    window.__zone = () => kernel.statusZone.forApp('mail').get().filter((e) => e.source === 'app');
    window.__commands = () => window.__api.query('business_commands', { sort: [{ created_at: 'asc' }] }).get();
  }, ACCOUNT);
});

after(async () => { await browser?.close(); await new Promise((r) => server?.close(r)); });

const settle = async () => { await page.evaluate(() => window.__k.sim.settle()); await page.waitForTimeout(350); };
const winText = () => page.$eval('#win', (el) => el.innerText);
const subs = () => page.evaluate((names) => names.map((c) => window.__c.subscribers(c)), SUBS);

test('mount: rendert Live-Daten, nur api.*, gescoptes Root, Titel über api.shell', async () => {
  await page.evaluate(async () => {
    const { countingApi } = await import('/v4/tools/port/test-api.mjs');
    window.__c = countingApi(window.__api);
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, {});
  });
  await settle();
  assert.match(await winText(), /Angebot Q4/);
  assert.match(await winText(), /Kunde AG/);
  await page.click('[data-mail-record-kind="thread"][data-mail-record-id="t-angebot"]');
  await settle();
  assert.match(await page.$eval('[data-mail-detail]', (el) => el.innerText), /Bitte um ein Angebot für Q4\./);
  assert.equal(await page.$eval('#win', (el) => el.dataset.app), 'mail');
  assert.equal(await page.evaluate(() => window.__k.shell.window('mail').state.get().title), 'Mail');
  assert.deepEqual(await subs(), SUBS.map(() => 1));
  assert.deepEqual(page.__errors, []);
});

test('Live-Änderung vom Core (sim.push) erscheint ohne Neuladen, Löschung verschwindet', async () => {
  await page.evaluate((account) => Promise.all([
    window.__k.sim.push('communication_threads', { id: 't-live', thread_key: 't-live', account_key: account, channel: 'email',
      subject: 'Live Rückfrage', unread_count: 1, last_message_at: '2026-10-15T11:00:00Z', updated_at: '2026-10-15T11:00:00Z' }),
    window.__k.sim.push('communication_messages', { id: 'm-live', message_key: 'm-live', account_key: account, thread_key: 't-live',
      channel: 'email', direction: 'inbound', folder_hint: 'inbox', sender_address: 'neu@example.test', subject: 'Live Rückfrage',
      body_text: 'Neu', external_created_at: '2026-10-15T11:00:00Z', observed_at: '2026-10-15T11:00:00Z' }),
  ]), ACCOUNT);
  await settle();
  assert.match(await winText(), /Live Rückfrage/);
  await page.evaluate(() => Promise.all([
    window.__k.sim.push('communication_threads', { id: 't-live', _deleted: true }),
    window.__k.sim.push('communication_messages', { id: 'm-live', _deleted: true }),
  ]));
  await settle();
  assert.doesNotMatch(await winText(), /Live Rückfrage/);
});

test('Liste zeigt Name/Adresse des Absenders, sonst „Unbekannt“ – nie eine Roh-ID (Textdiät Regel 4)', async () => {
  await page.evaluate((account) => window.__k.sim.push('communication_threads', { id: 't-ohne', thread_key: 'communication--001',
    account_key: account, channel: 'email', subject: 'Ohne Absender', unread_count: 0,
    last_message_at: '2026-10-15T10:00:00Z', updated_at: '2026-10-15T10:00:00Z' }), ACCOUNT);
  await settle();
  const persons = await page.$$eval('[data-mail-record-kind="thread"]', (rows) => Object.fromEntries(rows.map((row) => [
    row.dataset.mailRecordId, row.querySelector('.mail-record-person')?.textContent.trim()])));
  assert.equal(persons['t-angebot'], 'Kunde AG');
  assert.equal(persons['t-ohne'], 'Unbekannt');
  for (const name of Object.values(persons)) assert.doesNotMatch(name, /--|^communication|^t-/);
  await page.evaluate(() => window.__k.sim.push('communication_threads', { id: 't-ohne', _deleted: true }));
  await settle();
});

test('leere Collections rendern keinen Zustandstext und keine Zone', async () => {
  const empty = await page.evaluate(async () => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    const k2 = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false, instanceId: 'empty' });
    await k2.ready;
    await window.__app.unmount();
    const host = document.getElementById('win');
    const app = await window.__mod.mount(host, k2.appApi('mail', '1.0.0'), {});
    await new Promise((r) => setTimeout(r, 400));
    const lists = ['[data-mail-record-list]', '[data-mail-detail]'].map((sel) => host.querySelector(sel)?.innerText.trim() || '');
    const hasStateBox = Boolean(host.querySelector('.ctox-syncing, .ctox-empty, [role="status"], [aria-busy="true"]'));
    const stateWords = /synchronis|geladen|wird geladen|aktualisiert|Loading|Syncing|Noch keine|Keine Mail|Kein E-Mail|nicht verfügbar/i.test(host.innerText);
    const zone = k2.statusZone.forApp('mail').get().map((e) => e.code);
    await app.unmount();
    await k2.close();
    return { lists, hasStateBox, stateWords, zone };
  });
  assert.deepEqual(empty.lists, ['', '']);
  assert.equal(empty.hasStateBox, false);
  assert.equal(empty.stateWords, false);
  assert.deepEqual(empty.zone, []);
  await page.evaluate(async () => {
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, {});
  });
  await settle();
});

test('Schreiben: neue E-Mail-Gruppe geht über api.put nach outbound_campaigns und erscheint live', async () => {
  await page.click('[data-mail-left-pane] [data-pg-band="campaigns"]');
  await settle();
  assert.match(await winText(), /Herbstaktion/);
  await page.click('[data-mail-new-group]');
  await page.fill('[data-mail-group-name]', 'Wintergruppe');
  await page.click('[data-mail-create-group]');
  await settle();
  const rows = await page.evaluate(() => window.__api.query('outbound_campaigns', { selector: { name: 'Wintergruppe' } }).get());
  assert.equal(rows.length, 1);
  assert.equal(rows[0].owner_id, 'u-admin');
  assert.equal(typeof rows[0].updated_at_ms, 'number');
  assert.match(await page.$eval('[data-mail-scope-list]', (el) => el.innerText), /Wintergruppe/);
  assert.deepEqual(await page.evaluate(() => window.__zone().filter((e) => e.level === 'error')), []);
});

test('Befehl: Entwurf speichern → EIN outbound.message.compose ohne Warten; failed → genau ein Zoneneintrag', async () => {
  await page.evaluate(() => window.__k.sim.failCommand('outbound.message.compose', 'Mailserver lehnt ab'));
  await page.click('[data-mail-compose-primary]');
  await page.fill('[data-mail-compose-to]', 'kunde@example.test');
  await page.fill('[data-mail-compose-subject]', 'Ihr Angebot');
  await page.fill('[data-mail-compose-body]', 'Anbei das Angebot.');
  const before = await page.evaluate(() => window.__commands().length);
  await page.click('[data-mail-save-draft]');
  await settle();
  const cmds = await page.evaluate((n) => window.__commands().slice(n), before);
  // Strang MAIL2: die Verfasskette ist EIN Befehl (Handler modules/outbound/handlers/outbound.message.compose.js).
  assert.deepEqual(cmds.map((c) => c.type), ['outbound.message.compose']);
  const [compose] = cmds;
  assert.equal(compose.payload.mode, 'draft');
  assert.equal(compose.payload.sender_account_id, 'email:ada@example.test');
  assert.equal(compose.payload.subject, 'Ihr Angebot');
  assert.equal(compose.payload.body_text, 'Anbei das Angebot.');
  assert.deepEqual(compose.payload.recipients.map((r) => r.recipient_email), ['kunde@example.test']);
  assert.match(compose.payload.recipients[0].message_id, /^msg_mail_/);
  assert.match(compose.payload.recipients[0].engagement_id, /^eng_mail_/);
  assert.equal(compose.payload.client_context.source_module, 'mail');
  assert.equal(compose.status, 'failed');
  const errors = await page.evaluate(() => window.__zone().filter((e) => e.level === 'error'));
  assert.equal(errors.length, 1);
  assert.equal(errors[0].code, 'mail.command.outbound.message.compose');
  assert.match(errors[0].text, /Mailserver lehnt ab/);
  assert.equal(errors[0].ref, compose.id);
  assert.deepEqual(page.__errors, []);
});

test('Befehl done: Erfolgsmeldung erst bei done, Mailserver-Befehl ohne Leseweg', async () => {
  await page.evaluate(() => window.__k.sim.onCommand('ctox.mailserver.save_domain', () => ({ ok: true })));
  await page.click('[data-mail-settings]');
  await settle();
  // Einstellungen aus mail_server_state gefüllt (Strang MAIL2 D): Domain mit DNS-Werten, Postfach, Laufzeit, Zähler.
  const admin = await page.evaluate(() => ({
    domains: document.querySelector('[data-mail-domain-list]')?.innerText || '',
    mailboxes: document.querySelector('[data-mail-mailbox-list]')?.innerText || '',
    hostname: document.querySelector('[data-mail-server-hostname]')?.value,
    smtp: document.querySelector('[data-mail-server-smtp-port]')?.value,
    queue: document.querySelector('[data-mail-queue-count]')?.textContent,
  }));
  assert.match(admin.domains, /example\.test/);
  assert.match(admin.domains, /mail2026\._domainkey/);
  assert.match(admin.domains, /v=spf1 mx -all/);
  assert.match(admin.mailboxes, /ada@example\.test/);
  assert.equal(admin.hostname, 'mail.example.test');
  assert.equal(admin.smtp, '2587');
  assert.equal(admin.queue, '3');
  await page.fill('[data-mail-domain-name]', 'example.test');
  await page.click('[data-mail-save-domain]');
  await settle();
  const cmd = await page.evaluate(() => window.__commands().at(-1));
  assert.equal(cmd.type, 'ctox.mailserver.save_domain');
  assert.equal(cmd.payload.domain_name, 'example.test');
  assert.equal(cmd.payload.dkim_selector, 'default');
  const notice = await page.evaluate(() => window.__zone().find((e) => e.code === 'mail.notice'));
  assert.match(notice?.text || '', /Versanddomain gespeichert/);
  // Kein get_config-Leseweg: Domains/Postfächer brauchen eine Collection (OFFEN mail-1).
  assert.equal(await page.evaluate(() => window.__commands().some((c) => c.type === 'ctox.mailserver.get_config')), false);
  await page.click('[data-mail-close-mailbox-admin]');
});

test('snapshot/restored: Auswahl, Ansicht und offener Entwurf überleben den Remount', async () => {
  await page.click('[data-mail-left-pane] [data-pg-band="queues"]');
  await settle();
  await page.click('[data-mail-scope="queue"][data-mail-scope-id="inbound"]');
  await settle();
  await page.click('[data-mail-record-kind="thread"][data-mail-record-id="t-angebot"]');
  await page.click('[data-mail-compose-primary]');
  await page.fill('[data-mail-compose-subject]', 'halber Entwurf');
  const snap = await page.evaluate(async () => {
    const cloned = structuredClone(window.__app.snapshot());
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, { restored: cloned });
    return cloned;
  });
  assert.equal(snap.v, 1);
  assert.equal(snap.selectedKind, 'thread');
  assert.equal(snap.selectedId, 't-angebot');
  assert.equal(snap.composer.composeSubject, 'halber Entwurf');
  assert.ok(JSON.stringify(snap).length < 1024 * 1024);
  await settle();
  const after = await page.evaluate(() => ({
    subject: document.querySelector('[data-mail-compose-subject]')?.value,
    composerOpen: !document.querySelector('[data-mail-composer]')?.hidden,
    detail: document.querySelector('[data-mail-detail]')?.innerText || '',
  }));
  assert.equal(after.subject, 'halber Entwurf');
  assert.equal(after.composerOpen, true);
  assert.match(after.detail, /Bitte um ein Angebot für Q4\./);
  assert.deepEqual(await subs(), SUBS.map(() => 1), 'alte Abos sind abgemeldet');
  await page.click('[data-mail-close-composer]');
  assert.deepEqual(page.__errors, []);
});

test('Screenshots 1280 und 420 (MAIL_SHOTS=1)', { skip: !process.env.MAIL_SHOTS }, async () => {
  const dir = join(HERE, 'shots');
  await mkdir(dir, { recursive: true });
  await settle();
  await page.screenshot({ path: join(dir, 'mail-1280.png') });
  // Mailserver-Einstellungen aus mail_server_state (Strang MAIL2 D).
  await page.click('[data-mail-settings]');
  await settle();
  await page.screenshot({ path: join(dir, 'mail-1280-settings.png') });
  await page.click('[data-mail-close-mailbox-admin]');
  await settle();
  await page.setViewportSize({ width: 420, height: 820 });
  await page.$eval('#win', (el) => { el.style.width = '420px'; });
  await settle();
  await page.screenshot({ path: join(dir, 'mail-420.png') });
  const overflow = await page.$eval('#win', (el) => el.scrollWidth - el.clientWidth);
  // Liste ohne geöffneten Inspektor (bei 420 liegt der Inspektor als Blatt im App-Fenster).
  await page.click('[data-mail-close-detail]');
  await settle();
  await page.screenshot({ path: join(dir, 'mail-420-list.png') });
  const listOverflow = await page.$eval('#win', (el) => el.scrollWidth - el.clientWidth);
  assert.ok(listOverflow <= 1, `horizontaler Überlauf ${listOverflow}px bei 420 (Liste)`);
  await page.setViewportSize({ width: 1280, height: 820 });
  await page.$eval('#win', (el) => { el.style.width = '1280px'; });
  assert.ok(overflow <= 1, `horizontaler Überlauf ${overflow}px bei 420`);
});

test('FIX9a: Verfassen-Schublade – Esc leer schließt sofort, mit Inhalt „Entwurf behalten?“ (Behalten/Verwerfen/Abbrechen); Schließen-Knopf außerhalb der Schutzzone', async () => {
  // Echtes Rahmen-Blatt (v4/frame/window-sheet.js) statt Stub-Antwort; Schutzzone wie im Rahmen (frame.css: 96 × 44 px).
  await page.evaluate(async () => {
    await window.__app.unmount();
    const win = document.getElementById('win');
    win.style.setProperty('--shell-controls-zone-height', '44px');
    win.style.setProperty('--shell-controls-inset', '96px');
    const { withWindowSheet } = await import('/v4/tools/port/test-api.mjs');
    const { api } = await withWindowSheet(window.__c.api, win, {});
    window.__app = await window.__mod.mount(win, api, {});
  });
  await settle();
  const composerOpen = () => page.$eval('[data-mail-composer]', (el) => !el.hidden);
  const sheetOpen = () => page.evaluate(() => !!document.querySelector('[data-v4-sheet-overlay]'));
  const commandCount = () => page.evaluate(() => window.__commands().filter((c) => c.type === 'outbound.message.compose').length);
  const openFresh = async () => { await page.$eval('[data-mail-compose-primary]', (el) => el.click()); await page.waitForTimeout(30); assert.equal(await composerOpen(), true); };

  // Schutzzone: Schließen-Knopf schneidet das Rechteck der Fensterknöpfe (oben rechts) bei 1280 und 420 nicht.
  for (const width of [1280, 420]) {
    await page.$eval('#win', (el, w) => { el.style.width = `${w}px`; }, width);
    await openFresh();
    const geo = await page.evaluate(() => {
      const w = document.getElementById('win').getBoundingClientRect();
      const b = document.querySelector('[data-mail-close-composer]').getBoundingClientRect();
      const zone = { left: w.right - 96, right: w.right, top: w.top, bottom: w.top + 44 };
      return { hit: b.left < zone.right && b.right > zone.left && b.top < zone.bottom && b.bottom > zone.top, b: [b.left, b.top, b.right, b.bottom], w: w.right };
    });
    assert.equal(geo.hit, false, `Schließen-Knopf liegt in der Schutzzone bei ${width}: ${JSON.stringify(geo)}`);
    await page.click('[data-mail-close-composer]');
    assert.equal(await composerOpen(), false, `× schließt die leere Schublade bei ${width}`);
  }
  await page.$eval('#win', (el) => { el.style.width = '1280px'; });

  // Leer: Esc schließt sofort, ohne Rückfrage.
  await openFresh();
  await page.focus('[data-mail-compose-subject]');
  await page.keyboard.press('Escape');
  assert.equal(await sheetOpen(), false);
  assert.equal(await composerOpen(), false, 'Esc schließt die leere Schublade');

  // Inhalt + Esc → Rückfrage; Esc im Blatt (Abbrechen) lässt die Schublade samt Inhalt offen.
  await openFresh();
  await page.fill('[data-mail-compose-subject]', 'Halbfertig');
  await page.keyboard.press('Escape');
  await page.waitForSelector('[data-v4-sheet-overlay]');
  assert.match(await page.$eval('[data-v4-sheet-overlay]', (el) => el.innerText), /Entwurf behalten\?[\s\S]*Als Entwurf speichern[\s\S]*Verwerfen/);
  await page.keyboard.press('Escape');
  assert.equal(await sheetOpen(), false);
  assert.equal(await composerOpen(), true);
  assert.equal(await page.$eval('[data-mail-compose-subject]', (el) => el.value), 'Halbfertig');

  // Verwerfen: zu, kein Befehl.
  const before = await commandCount();
  await page.focus('[data-mail-compose-subject]');
  await page.keyboard.press('Escape');
  await page.waitForSelector('[data-v4-sheet-overlay]');
  await page.check('[data-v4-sheet-option="discard"]');
  await page.click('[data-v4-sheet-ok]');
  await page.waitForTimeout(30);
  assert.equal(await composerOpen(), false, 'Verwerfen schließt');
  await settle();
  assert.equal(await commandCount(), before, 'Verwerfen speichert nichts');

  // Behalten: Entwurf wird gespeichert (EIN compose-Befehl), Schublade zu.
  await openFresh();
  await page.fill('[data-mail-compose-to]', 'kunde@example.test');
  await page.fill('[data-mail-compose-subject]', 'Entwurf per Esc');
  await page.fill('[data-mail-compose-body]', 'Text');
  await page.keyboard.press('Escape');
  await page.waitForSelector('[data-v4-sheet-overlay]');
  await page.click('[data-v4-sheet-ok]');
  await settle();
  assert.equal(await composerOpen(), false, 'Behalten speichert und schließt');
  assert.equal(await commandCount(), before + 1);
  assert.deepEqual(page.__errors, []);
});

test('FIX10: Mail-Einstellungen schließen per Esc und liegen nie über einer anderen offenen Schublade', async () => {
  // Läuft im Rahmen-Blatt aus dem FIX9a-Test (Rückfrage „Entwurf behalten?“ über das echte Blatt).
  const open = () => page.evaluate(() => ['[data-mail-composer]', '[data-mail-mailbox-admin]', '[data-mail-group-editor]']
    .filter((sel) => { const el = document.querySelector(sel); return el && !el.hidden && el.getBoundingClientRect().height > 0; }));
  const click = async (sel) => { await page.$eval(sel, (el) => el.click()); await page.waitForTimeout(30); };
  const sheetOpen = () => page.evaluate(() => !!document.querySelector('[data-v4-sheet-overlay]'));

  // Esc schließt die Einstellungen; Fokus zurück auf den Knopf.
  await click('[data-mail-settings]');
  await settle();
  assert.deepEqual(await open(), ['[data-mail-mailbox-admin]']);
  await page.focus('[data-mail-domain-name]');
  await page.keyboard.press('Escape');
  assert.deepEqual(await open(), [], 'Esc schließt die Mail-Einstellungen');
  assert.equal(await page.evaluate(() => document.activeElement?.hasAttribute('data-mail-settings')), true, 'Fokus zurück auf Einstellungen');

  // Einstellungen offen → Verfassen: Einstellungen schließen, nur die Verfassen-Schublade bleibt.
  await click('[data-mail-settings]');
  await click('[data-mail-compose-primary]');
  assert.deepEqual(await open(), ['[data-mail-composer]'], 'Verfassen schließt die Einstellungen');
  // Verfassen leer → Neue Gruppe: nur der Gruppen-Editor.
  await click('[data-mail-new-group]');
  assert.deepEqual(await open(), ['[data-mail-group-editor]']);
  // Gruppen-Editor → Einstellungen: nur die Einstellungen.
  await click('[data-mail-settings]');
  assert.deepEqual(await open(), ['[data-mail-mailbox-admin]']);
  await page.keyboard.press('Escape');

  // Verfassen mit Inhalt → Einstellungen: erst „Entwurf behalten?“, Abbrechen lässt den Entwurf offen, keine zweite Schublade.
  await click('[data-mail-compose-primary]');
  await page.fill('[data-mail-compose-subject]', 'Noch nicht fertig');
  await page.$eval('[data-mail-settings]', (el) => el.click());
  await page.waitForSelector('[data-v4-sheet-overlay]');
  await page.keyboard.press('Escape');
  await page.waitForTimeout(30);
  assert.equal(await sheetOpen(), false);
  assert.deepEqual(await open(), ['[data-mail-composer]'], 'Abbrechen: Entwurf bleibt, Einstellungen öffnen nicht darüber');
  assert.equal(await page.$eval('[data-mail-compose-subject]', (el) => el.value), 'Noch nicht fertig');
  // Verwerfen → Einstellungen öffnen allein.
  await page.$eval('[data-mail-settings]', (el) => el.click());
  await page.waitForSelector('[data-v4-sheet-overlay]');
  await page.check('[data-v4-sheet-option="discard"]');
  await page.click('[data-v4-sheet-ok]');
  await page.waitForTimeout(30);
  await settle();
  assert.deepEqual(await open(), ['[data-mail-mailbox-admin]']);
  await page.keyboard.press('Escape');
  assert.deepEqual(await open(), []);
  assert.deepEqual(page.__errors, []);
});

test('unmount meldet alle Abos ab und leert root', async () => {
  const res = await page.evaluate(async (names) => {
    await window.__app.unmount();
    await window.__k.sim.push('communication_threads', { id: 't-late', thread_key: 't-late', account_key: 'email:ada@example.test',
      channel: 'email', subject: 'Late', last_message_at: '2026-10-15T12:00:00Z', updated_at: '2026-10-15T12:00:00Z' });
    await window.__k.sim.settle();
    return { subs: names.map((c) => window.__c.subscribers(c)), children: document.getElementById('win').children.length };
  }, SUBS);
  assert.deepEqual(res.subs, SUBS.map(() => 0));
  assert.equal(res.children, 0);
  await page.waitForTimeout(400);
  assert.deepEqual(page.__errors, []);
});
