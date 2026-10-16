// outbound gegen den App-API-4.0-Kern-Stub (createStubKernel) in echtem Chromium – Muster modules/knowledge.
// Stub: CTOX_APP_API_STUB (Pflicht-Pin ~/.local/state/shell-v4/stub-pins/68b83a3), read-only unter /__stub/.
// Lauf: CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/68b83a3 node --test src/apps/business-os/modules/outbound/tests/v4-api.test.mjs
// Screenshots (1280/420) nach tests/shots/, wenn OUTBOUND_SHOTS=1. #win trägt den Fenster-Container der Shell
// (business-app-window), auf den index.css seine Schmal-Regeln stützt.
import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
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
const SHOTS = process.env.OUTBOUND_SHOTS === '1';
const STATE_TEXT = /synchronisier|wird geladen|werden geladen|Syncing|Loading|warte|wartet|ausstehend|nicht verbunden|Command angelegt/i;
// Fachbezeichnungen im Originalwortlaut (mit `v4-allow: label` gekennzeichnet, Strang LBL): Research-Status je
// Datensatz und Freigabestatus einer Nachricht. Sie sind keine Zustandstexte und werden vor der Prüfung entfernt.
const FACH_LABELS = /CTOX arbeitet(?: gerade nicht)?: \d+ (?:läuft|wartet)|\d+ wartet|wartet in CTOX|läuft gerade|Freigabe ausstehend/g;
const withoutLabels = (s) => String(s).replace(FACH_LABELS, '');

const PAGE = `<!doctype html><html><head><meta charset="utf-8">
<link rel="stylesheet" href="/ui-contract/v1/workjet-ui-contract.css">
<link rel="stylesheet" href="/app.css">
<link rel="stylesheet" href="/shared/base.css">
<link rel="stylesheet" href="/themes/ctox-desktop-shell.css">
<link rel="stylesheet" href="/modules/outbound/index.css">
<style>html,body{margin:0;height:100%;font-family:system-ui,sans-serif}#win{position:relative;overflow:hidden;container:business-app-window/inline-size}</style>
</head><body><div id="win" style="width:1280px;height:800px"></div></body></html>`;

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
      // Wie der Service Worker (§16.15): ctox:shared/x.js → Shell-Pfad (OFFEN-F, native Import-Map greift nicht).
      if (/\.m?js$/.test(file)) body = Buffer.from(body.toString('utf8').replace(/(['"])ctox:shared\//g, '$1/shared/'));
      res.writeHead(200, { 'content-type': TYPES[extname(file)] || 'application/octet-stream' }); res.end(body);
    } catch { res.writeHead(404); res.end(); }
  });
  await new Promise((r) => server.listen(0, '127.0.0.1', r));
  base = `http://127.0.0.1:${server.address().port}/`;
  browser = await chromium.launch({ executablePath: process.env.CHROMIUM_PATH || undefined });
  page = await browser.newPage({ viewport: { width: 1280, height: 800 } });
  const errors = [];
  page.on('pageerror', (e) => errors.push(e.message));
  page.on('console', (m) => { if (m.type() === 'error') console.error('[page]', m.text()); });
  page.on('response', (r) => { if (r.status() >= 400) console.error('[page] HTTP', r.status(), r.url()); });
  page.__errors = errors;
  await page.goto(base);
  // Befehlsrouten/Deklarationen der echten outbound-Version (v4/manifests/outbound.manifest.json); ohne aktive Version
  // lehnt der Kern seit E26/E27 App-Befehle mit unknown_command und Schreibzugriffe ab.
  await page.evaluate((routes) => { window.__routes = routes; }, realManifestRoutes('outbound'));
  await page.evaluate(async () => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    window.__mod = await import('/modules/outbound/index.js');
    const kernel = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false, actor: { role: 'admin', permissions: ['apps.source.edit'] } });
    await kernel.ready;
    // FIXT-R: Eigentümer der Fremd-Collections (echte Manifeste, inkl. ctox-platform) vor der eigenen Version.
    for (const o of window.__routes?._fixture_owners ?? []) if (!(await kernel.sim.inspect('business_apps', o.app))?.data?.active_ver) await kernel.sim.publishAppVersion(o.app, o.ver, o.publish);
    await kernel.sim.publishAppVersion('outbound', '1.0.0', { title: 'Outbound', manifestExtra: window.__routes });
    // knowledge.command ist ein Plattformbefehl des CTOX-Kerns (src/core/business_os/command_plane.rs), keine App-Route:
    // der Prüfstand simuliert ihn wie die anderen Kernbefehle per sim.onCommand (Antwort wie der bisherige Stub-Standard).
    kernel.sim.onCommand('knowledge.command', () => ({ ok: true }));
    const now = Date.now();
    await kernel.sim.push('outbound_campaigns', [
      { id: 'outbound_default_campaign', name: 'Outbound Firmenqualifizierung', status: 'active', payload: {}, created_at_ms: now - 9000, updated_at_ms: now - 9000 },
      { id: 'camp_a', name: 'Alpha Kampagne', status: 'active', market: 'DACH', objective: 'Maschinenbau DACH',
        payload: { communication_account_key: 'email:vertrieb@example.test' }, created_at_ms: now - 5000, updated_at_ms: now },
    ]);
    await kernel.sim.push('outbound_sources', [{ id: 'src_1', campaign_id: 'camp_a', title: 'Messe', source_type: 'text', status: 'imported', row_count: 2, imported_count: 2, payload: {}, created_at_ms: now, updated_at_ms: now }]);
    await kernel.sim.push('outbound_companies', [
      { id: 'comp_1', campaign_id: 'camp_a', source_id: 'src_1', name: 'Muster Maschinen GmbH', domain: 'muster.test', city: 'Ulm', country: 'DE',
        qualification_status: 'qualified', research_status: 'researched', pipeline_status: 'in_pipeline', payload: {}, created_at_ms: now, updated_at_ms: now },
      { id: 'comp_2', campaign_id: 'camp_a', source_id: 'src_1', name: 'Beispiel Antriebe AG', domain: 'beispiel.test', city: 'Linz', country: 'DE',
        qualification_status: 'new', research_status: 'pending', pipeline_status: 'not_started', payload: {}, created_at_ms: now - 1, updated_at_ms: now - 1 },
    ]);
    await kernel.sim.push('outbound_pipeline_items', [
      { id: 'pipe_1', campaign_id: 'camp_a', company_id: 'comp_1', company_name: 'Muster Maschinen GmbH', stage: 'lead', contact_research_status: 'researched',
        outreach_status: 'not_started', lead_status: 'qualified', contact_name: 'Erika Muster', contact_email: 'erika@muster.test', payload: {}, contacts: [], created_at_ms: now, updated_at_ms: now },
    ]);
    await kernel.sim.push('outbound_engagements', [
      { id: 'eng_1', campaign_id: 'camp_a', status: 'awaiting_approval', sender_account_id: 'email:vertrieb@example.test',
        payload: { contact_email: 'max@beispiel.test', contact_name: 'Max Beispiel', company_name: 'Beispiel Antriebe AG', pipeline_id: 'pipe_x' }, created_at_ms: now, updated_at_ms: now },
    ]);
    await kernel.sim.push('outbound_messages', [
      { id: 'msg_1', engagement_id: 'eng_1', campaign_id: 'camp_a', message_type: 'initial', direction: 'outbound', draft_status: 'prepared',
        approval_status: 'awaiting_approval', send_status: 'not_sent', recipient_email: 'max@beispiel.test', subject: 'Kurze Frage zu Antrieben',
        body_text: 'Guten Tag Herr Beispiel,', channel: 'email', payload: {}, created_at_ms: now, updated_at_ms: now },
    ]);
    await kernel.sim.settle();
    const api = kernel.appApi('outbound', '1.0.0');
    window.__k = kernel;
    window.__api = api;
    window.__zone = () => kernel.statusZone.forApp('outbound').get().filter((e) => e.source === 'app');
    window.__commands = () => api.query('business_commands', { sort: [{ created_at: 'asc' }] }).get();
  });
});

after(async () => { await browser?.close(); await new Promise((r) => server?.close(r)); });

const settle = async () => { await page.evaluate(() => window.__k.sim.settle()); await page.waitForTimeout(250); };
const text = (sel) => page.$eval(sel, (el) => el.textContent);
const click = (sel) => page.evaluate((s) => document.querySelector(s).click(), sel);
const shot = async (name, width) => {
  if (!SHOTS) return;
  await page.setViewportSize({ width, height: 800 });
  await page.evaluate((w) => { document.getElementById('win').style.width = `${w}px`; }, width);
  await page.waitForTimeout(300);
  await page.locator('#win').screenshot({ path: join(HERE, 'shots', `${name}-${width}.png`) });
};

test('mount: Kampagnen und Firmen sofort aus dem lokalen Spiegel, gescoptes Root, Titel über api.shell', async () => {
  await page.evaluate(async () => {
    const { countingApi } = await import('/v4/tools/port/test-api.mjs');
    window.__c = countingApi(window.__api);
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, {});
  });
  await settle();
  assert.match(await text('[data-campaign-list]'), /Alpha Kampagne/);
  await page.evaluate(() => [...document.querySelectorAll('[data-campaign-list] [data-id]')].find((el) => el.dataset.id === 'camp_a')?.click());
  await settle();
  assert.equal(await page.evaluate(() => window.__app.snapshot().selectedCampaignId), 'camp_a');
  const center = await text('.outbound-center');
  assert.match(center, /Muster Maschinen GmbH/);
  assert.match(center, /Beispiel Antriebe AG/);
  assert.equal(await page.$eval('#win', (el) => el.dataset.app), 'outbound');
  assert.equal(await page.evaluate(() => window.__k.shell.window('outbound').state.get().title), 'Outbound');
  for (const c of ['outbound_campaigns', 'outbound_companies', 'outbound_messages', 'business_commands']) {
    assert.equal(await page.evaluate((n) => window.__c.subscribers(n), c), 1, c);
  }
  assert.doesNotMatch(withoutLabels(await text('#win')), STATE_TEXT);
  assert.deepEqual(await page.evaluate(() => window.__zone()), []);
  await shot('campaign', 1280);
  await shot('campaign', 420);
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.evaluate(() => { document.getElementById('win').style.width = '1280px'; });
  assert.deepEqual(page.__errors, []);
});

test('Rundgang RUND2: Reiterleiste „Sequenz & Versand“ bricht um statt seitlich zu überlaufen', async () => {
  const r = await page.evaluate(() => {
    const host = document.querySelector('#win');
    const nav = document.createElement('nav');
    nav.className = 'ctox-pane-tabs outbound-outreach-tabs';
    nav.style.width = '300px';
    for (const l of ['Lead Queue', 'Aktive Engagements', 'Freigabe-Inbox', 'Versand-Queue', 'Antworten', 'Abgeschlossen', 'Skills & Vorlagen']) {
      const b = document.createElement('button'); b.className = 'ctox-pane-tab outbound-outreach-tab'; b.textContent = l; nav.append(b);
    }
    (host.querySelector('[data-app="outbound"]') || host).append(nav);
    const out = { minWidth: getComputedStyle(nav).minWidth, over: nav.scrollWidth - nav.clientWidth, w: Math.round(nav.getBoundingClientRect().width) };
    nav.remove();
    return out;
  });
  assert.equal(r.minWidth, '0px');
  assert.ok(r.w <= 300 && r.over <= 1, JSON.stringify(r));
});

test('Rundgang RUND2: deutsche Texte ohne ASCII-Umschreibungen (fuer/spaeter/Schliessen …)', async () => {
  const de = JSON.parse(await readFile(join(HERE, '..', 'locales', 'de.json'), 'utf8'));
  const bad = /\b(fuer|spaeter|Schliessen|schliessen|pruefen|oeffnen|Oeffentlich|waehlen|aendern|laeuft|verfuegbar|verknuepft|Naechster|Eintraege|Faellige|Vorschlaege|Aktivitaet|ausschliesslich|hinzugefuegten|rechercheert)\b/;
  const hits = Object.entries(de).filter(([, v]) => typeof v === 'string' && bad.test(v)).map(([k]) => k);
  assert.deepEqual(hits, []);
});

test('Live-Änderung (sim.push) erscheint ohne Nachladen, Löschung verschwindet', async () => {
  await page.evaluate(() => window.__k.sim.push('outbound_companies', { id: 'comp_live', campaign_id: 'camp_a', source_id: 'src_1', name: 'Live Hydraulik KG',
    qualification_status: 'new', research_status: 'pending', pipeline_status: 'not_started', payload: {}, created_at_ms: Date.now(), updated_at_ms: Date.now() + 5 }));
  await settle();
  assert.match(await text('.outbound-center'), /Live Hydraulik KG/);
  await page.evaluate(() => window.__k.sim.push('outbound_companies', { id: 'comp_live', _deleted: true }));
  await settle();
  assert.doesNotMatch(await text('.outbound-center'), /Live Hydraulik KG/);
  const before = await page.evaluate(() => window.__commands().length);
  assert.equal(before, 0, 'Live-Darstellung erzeugt keine Befehle (kein Nachladen über Befehle)');
});

test('leere Collections: kein Zustandstext, keine Zone (nur die Standard-Kampagne wird lokal angelegt)', async () => {
  const empty = await page.evaluate(async () => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    const k2 = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false, instanceId: 'empty' });
    await k2.ready;
    // FIXT-R: Eigentümer der Fremd-Collections (echte Manifeste, inkl. ctox-platform) vor der eigenen Version.
    for (const o of window.__routes?._fixture_owners ?? []) if (!(await k2.sim.inspect('business_apps', o.app))?.data?.active_ver) await k2.sim.publishAppVersion(o.app, o.ver, o.publish);
    await k2.sim.publishAppVersion('outbound', '1.0.0', { title: 'Outbound', manifestExtra: window.__routes });
    k2.sim.onCommand('knowledge.command', () => ({ ok: true }));
    const host = document.createElement('div');
    host.style.cssText = 'width:900px;height:600px';
    document.body.append(host);
    const api2 = k2.appApi('outbound', '1.0.0');
    const app = await window.__mod.mount(host, api2, {});
    await new Promise((r) => setTimeout(r, 300));
    await k2.sim.settle();
    const out = {
      text: host.textContent.replace(/\s+/g, ' ').trim(),
      stateBox: Boolean(host.querySelector('.ctox-syncing, [role="status"], [role="alert"], .outbound-outreach-error')),
      zone: k2.statusZone.forApp('outbound').get().map((e) => e.code),
      campaigns: api2.query('outbound_campaigns', {}).get().map((c) => c.id),
    };
    await app.unmount();
    host.remove();
    await k2.close();
    return out;
  });
  assert.equal(empty.stateBox, false);
  assert.doesNotMatch(withoutLabels(empty.text), STATE_TEXT);
  assert.deepEqual(empty.zone.filter((c) => !c.startsWith('outbound.command.knowledge')), []);
  assert.deepEqual(empty.campaigns, ['outbound_default_campaign']);
  // Modulzustand ist modulweit (wie knowledge OFFEN-4): Hauptfenster neu montieren.
  await page.evaluate(async () => {
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, { restored: { v: 1, selectedCampaignId: 'camp_a' } });
  });
  await settle();
});

test('Freigabe: Befehl outbound.message.approve mit record_id, ohne Warten; failed → genau ein Zoneneintrag', async () => {
  await click('[data-action="set-workbench-mode"][data-workbench-mode="outreach"]');
  await settle();
  await click('[data-action="ao-view"][data-view="approval_inbox"]');
  await settle();
  assert.equal(await page.$eval('[data-action="ao-edit-subject"][data-message-id="msg_1"]', (el) => el.value), 'Kurze Frage zu Antrieben');
  await shot('approval', 1280);
  await shot('approval', 420);
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.evaluate(() => { document.getElementById('win').style.width = '1280px'; });
  await page.evaluate(() => window.__k.sim.failCommand('outbound.message.approve', 'Freigabe abgelehnt (Test)'));
  await click('[data-action="ao-approve"][data-message-id="msg_1"]');
  await settle();
  const cmd = await page.evaluate(() => window.__commands().at(-1));
  assert.equal(cmd.type, 'outbound.message.approve');
  assert.equal(cmd.record_id, 'msg_1');
  assert.equal(cmd.payload.record_id, 'msg_1');
  assert.equal(cmd.payload.message_id, 'msg_1');
  assert.equal(cmd.payload.client_context.source_module, 'outbound');
  assert.equal(cmd.status, 'failed');
  const zone = await page.evaluate(() => window.__zone());
  const codes = zone.map((e) => e.code);
  assert.equal(codes.filter((c) => c === 'outbound.command.outbound.message.approve').length, 1);
  assert.match(zone.find((e) => e.code === 'outbound.command.outbound.message.approve').text, /Freigabe abgelehnt/);
  assert.equal(await page.locator('#win [role="alert"]').count(), 0, 'kein Fehlerkasten im Inhalt');
  // Erfolgreicher zweiter Versuch räumt den Eintrag.
  await click('[data-action="ao-approve"][data-message-id="msg_1"]');
  await settle();
  assert.equal((await page.evaluate(() => window.__commands().at(-1))).status, 'done');
  assert.deepEqual((await page.evaluate(() => window.__zone())).map((e) => e.code).filter((c) => c === 'outbound.command.outbound.message.approve'), []);
});

test('Ablehnen fragt über api.shell.dialog: ohne Antwort kein Befehl, mit Antwort reject + comment', async () => {
  await click('[data-action="ao-view"][data-view="approval_inbox"]');
  await settle();
  const n0 = await page.evaluate(() => window.__commands().length);
  await click('[data-action="ao-reject"][data-message-id="msg_1"]');
  await settle();
  assert.equal(await page.evaluate(() => window.__commands().length), n0, 'Stub-Dialog liefert null → kein Befehl');
  assert.ok(await page.evaluate(() => window.__k.sim.dialogs.length) >= 1);
  await page.evaluate(async () => {
    const { withDialogAnswer } = await import('/v4/tools/port/test-api.mjs');
    const snap = structuredClone(window.__app.snapshot());
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), withDialogAnswer(window.__c.api, 'Ton zu förmlich'), { restored: snap });
  });
  await settle();
  await click('[data-action="ao-view"][data-view="approval_inbox"]');
  await settle();
  await click('[data-action="ao-reject"][data-message-id="msg_1"]');
  await settle();
  const cmd = await page.evaluate(() => window.__commands().at(-1));
  assert.equal(cmd.type, 'outbound.message.reject');
  assert.equal(cmd.record_id, 'msg_1');
  assert.equal(cmd.payload.comment, 'Ton zu förmlich');
  assert.equal(cmd.status, 'done');
});

test('Engagement starten: create → assign_sender → draft.prepare als Kette auf done (Fachablauf unverändert)', async () => {
  await click('[data-action="ao-view"][data-view="lead_queue"]');
  await settle();
  assert.match(await text('.outbound-outreach'), /Erika Muster|Muster Maschinen/);
  const n0 = await page.evaluate(() => window.__commands().length);
  // Offline: Befehl bleibt in der Outbox (queued) – die UI blockiert nicht, der Folgeschritt wartet auf done.
  await page.evaluate(() => window.__k.sim.setLink({ state: 'offline' }));
  await click('[data-action="ao-auto-draft"]');
  await page.waitForTimeout(150);
  const firstOnly = await page.evaluate((n) => window.__commands().slice(n).map((c) => [c.type, c.status]), n0);
  assert.deepEqual(firstOnly, [['outbound.engagement.create', 'queued']], 'Folgebefehl erst nach done des ersten');
  assert.equal(await page.locator('[data-action="ao-view"]').count() > 0, true, 'UI bleibt bedienbar');
  await page.evaluate(() => window.__k.sim.setLink({ state: 'online', rtt_ms: 5, relay: false }));
  for (let i = 0; i < 6; i += 1) await settle();
  const types = await page.evaluate((n) => window.__commands().slice(n).map((c) => [c.type, c.record_id, c.payload.engagement_id]), n0);
  assert.deepEqual(types.map((t) => t[0]), ['outbound.engagement.create', 'outbound.engagement.assign_sender', 'outbound.draft.prepare']);
  assert.ok(types.every((t) => t[2] === types[0][1]), 'alle Schritte am selben Engagement');
  const draft = await page.evaluate(() => window.__commands().at(-1));
  assert.equal(draft.payload.sender_account_id, 'email:vertrieb@example.test');
  assert.equal(draft.payload.recipient_email, 'erika@muster.test');
  assert.equal(draft.record_id, draft.payload.message_id);
});

test('Schreiben: api.put erscheint live und wird vom Core angenommen', async () => {
  await click('[data-action="set-workbench-mode"][data-workbench-mode="campaign"]');
  await settle();
  const result = await page.evaluate(async () => {
    const h = window.__api.put('outbound_companies', { id: 'comp_put', campaign_id: 'camp_a', source_id: 'src_1', name: 'Put Pumpen GmbH',
      qualification_status: 'new', research_status: 'pending', pipeline_status: 'not_started', payload: {}, created_at_ms: Date.now(), updated_at_ms: Date.now() + 9 });
    await h.local;
    return h.result;
  });
  assert.equal(result.status, 'accepted');
  await settle();
  assert.match(await text('.outbound-center'), /Put Pumpen GmbH/);
});

test('snapshot/restored: offener Nachrichtenentwurf und Auswahl überleben den Remount', async () => {
  await click('[data-action="set-workbench-mode"][data-workbench-mode="outreach"]');
  await settle();
  await click('[data-action="ao-view"][data-view="approval_inbox"]');
  await settle();
  await page.fill('[data-action="ao-edit-body"][data-message-id="msg_1"]', 'Halb fertiger Entwurf an Herrn Beispiel');
  const snap = await page.evaluate(async () => {
    const cloned = structuredClone(window.__app.snapshot());
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, { restored: cloned });
    return cloned;
  });
  assert.equal(snap.selectedCampaignId, 'camp_a');
  assert.equal(snap.outreachView, true);
  assert.equal(snap.activeOutreach.view, 'approval_inbox');
  assert.ok(JSON.stringify(snap).length < 1024 * 1024);
  await settle();
  const after = await page.evaluate(() => ({
    body: document.querySelector('[data-action="ao-edit-body"][data-message-id="msg_1"]')?.value,
    campaign: window.__app.snapshot().selectedCampaignId,
  }));
  assert.equal(after.body, 'Halb fertiger Entwurf an Herrn Beispiel');
  assert.equal(after.campaign, 'camp_a');
  assert.equal(await page.evaluate(() => window.__c.subscribers('outbound_messages')), 1, 'alte Abos abgemeldet');
  // Entwurf speichern → update_draft mit dem restaurierten Text.
  await click('[data-action="ao-save-draft"][data-message-id="msg_1"]');
  await settle();
  const cmd = await page.evaluate(() => window.__commands().at(-1));
  assert.equal(cmd.type, 'outbound.message.update_draft');
  assert.equal(cmd.payload.body_text, 'Halb fertiger Entwurf an Herrn Beispiel');
  assert.deepEqual(page.__errors, []);
});

test('unmount meldet alle Abos ab und leert root', async () => {
  const res = await page.evaluate(async () => {
    await window.__app.unmount();
    await window.__k.sim.push('outbound_companies', { id: 'comp_late', campaign_id: 'camp_a', name: 'Spät AG', payload: {}, created_at_ms: 1, updated_at_ms: 1 });
    await window.__k.sim.settle();
    const names = ['outbound_campaigns', 'outbound_sources', 'outbound_companies', 'outbound_pipeline_items', 'outbound_research_runs',
      'outbound_research_adapters', 'business_commands', 'ctox_queue_tasks', 'outbound_engagements', 'outbound_messages',
      'outbound_approvals', 'outbound_sequences', 'outbound_sender_assignments', 'outbound_meeting_requests',
      'outbound_suppression_entries', 'outbound_account_limits', 'outbound_skillbooks', 'outbound_letter_templates'];
    return { subs: names.map((c) => window.__c.subscribers(c)), children: document.getElementById('win').children.length };
  });
  assert.deepEqual(res.subs, new Array(18).fill(0));
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

// OUTRES (TEXT-E Frage 2): je Firmenzeile höchstens eine Hauptaktion, der Rest im „…“-Menü des Rahmens
// (api.shell.menu → window-sheet.js); je Kampagne nur „Bearbeiten“ + „…“. Alle bisherigen Aktionen bleiben erreichbar.
test('OUTRES: Firmenzeile eine Hauptaktion + „…“-Menü (Tastatur Enter/Pfeile/Esc, Maus), Kampagne Bearbeiten + „…“', async () => {
  await mountWithWindowSheet();
  await settle();
  const rows = Object.fromEntries(await page.$$eval('#win tr[data-company-id]', (trs) => trs.map((tr) => [tr.dataset.companyId,
    [...tr.querySelectorAll('.outbound-cell-actions button')].map((b) => b.dataset.action)])));
  assert.deepEqual(rows.comp_2, ['qualify-company', 'company-more'], 'offene Firma: Qualifizieren + „…“');
  assert.deepEqual(rows.comp_1, ['send-pipeline', 'company-more'], 'qualifizierte Firma: In Pipeline + „…“');
  const more = '#win tr[data-company-id="comp_2"] [data-action="company-more"]';
  await page.focus(more);
  await page.keyboard.press('Enter');
  await page.waitForSelector('#win [data-v4-menu]');
  const items = await page.$$eval('#win [data-v4-menu] [data-v4-menu-item]', (bs) => bs.map((b) => b.dataset.v4MenuItem));
  assert.deepEqual(items, ['research-company', 'validate-company-research', 'download-company-research-log', 'hide-company', 'reject-company']);
  const focused = () => page.evaluate(() => document.activeElement?.dataset.v4MenuItem || document.activeElement?.dataset.action || '');
  assert.equal(await focused(), 'research-company');
  await page.keyboard.press('ArrowDown');
  assert.equal(await focused(), 'validate-company-research');
  await page.keyboard.press('Escape');
  assert.equal(await page.$('#win [data-v4-menu]'), null, 'Esc schließt das Menü');
  assert.equal(await focused(), 'company-more', 'Fokus zurück auf „…“');
  await page.click(more);
  await page.waitForSelector('#win [data-v4-menu]');
  await page.click('#win [data-v4-menu] [data-v4-menu-item="reject-company"]');
  await settle();
  const status = await page.evaluate(() => window.__api.query('outbound_companies', {}).get().find((c) => c.id === 'comp_2')?.qualification_status);
  assert.equal(status, 'rejected', 'Ausschließen über das Menü wirkt');
  const campaign = await page.$eval('#win .outbound-campaign-item[data-campaign-id="camp_a"] .outbound-campaign-actions', (el) => [...el.querySelectorAll('button')].map((b) => b.dataset.action));
  assert.deepEqual(campaign, ['edit-campaign', 'campaign-more']);
  await page.click('#win .outbound-campaign-item[data-campaign-id="camp_a"] [data-action="campaign-more"]');
  await page.waitForSelector('#win [data-v4-menu]');
  const campaignItems = await page.$$eval('#win [data-v4-menu] [data-v4-menu-item]', (bs) => bs.map((b) => b.dataset.v4MenuItem));
  assert.deepEqual(campaignItems, ['import-source', 'open-campaign-runbook', 'delete-campaign']);
  await page.keyboard.press('Escape');
  assert.deepEqual(page.__errors, []);
  await unmountWindowSheet();
});

test('FR5-1 Rahmen-Dialog wirkt: „Kampagne löschen“ fragt im Fenster (destruktiv rot), Enter löscht die Kampagne', async () => {
  await mountWithWindowSheet();
  await settle();
  // OUTRES: „Kampagne löschen“ liegt im „…“-Menü der Kampagne (Rahmen-Menü im Fenster), nicht mehr als Symbol.
  const id = await page.$eval('#win [data-action="campaign-more"]', (b) => b.dataset.id);
  await page.click(`#win [data-action="campaign-more"][data-id="${id}"]`);
  await page.waitForSelector('#win [data-v4-menu]');
  await page.click('#win [data-v4-menu] [data-v4-menu-item="delete-campaign"]');
  await page.waitForSelector('#win [data-v4-sheet]');
  const s = await sheetState();
  const danger = await page.$eval('#win [data-v4-sheet-ok]', (b) => b.classList.contains('is-danger'));
  assert.deepEqual({ inAppRoot: s.inAppRoot, inert: s.inert, ok: s.ok, danger }, { inAppRoot: false, inert: true, ok: 'Löschen', danger: true });
  await page.keyboard.press('Enter');
  await settle();
  assert.equal(await sheetState(), null);
  const ids = await page.evaluate(() => window.__api.query('outbound_campaigns', {}).get().map((c) => c.id));
  assert.ok(!ids.includes(id), `Kampagne ${id} gelöscht (übrig: ${ids.join(',')})`);
  await unmountWindowSheet();
});
