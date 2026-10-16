// outbound-lead-generation gegen den eingefrorenen App-API-4.0-Kern-Stub (createStubKernel) in echtem Chromium.
// Muster: modules/knowledge/tests/v4-api.test.mjs. Stub-Pin: CTOX_APP_API_STUB (Pflicht laut Port-Auftrag).
// Lauf: CTOX_APP_API_STUB=~/.local/state/shell-v4/stub-pins/68b83a3 \
//   node --test src/apps/business-os/customer-modules/outbound-lead-generation/tests/v4-api.test.mjs
// Bildschirmfotos (1280/420) landen in tests/shots/.
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
const APP = '/customer-modules/outbound-lead-generation';
const SHOTS = join(HERE, 'shots');
const require = createRequire(import.meta.url);
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || join(homedir(), 'Documents/ctox/src/apps/business-os/node_modules/playwright'));
const TYPES = { '.js': 'text/javascript', '.mjs': 'text/javascript', '.css': 'text/css', '.html': 'text/html', '.json': 'application/json', '.svg': 'image/svg+xml' };

const PAGE = `<!doctype html><html><head><meta charset="utf-8">
<link rel="stylesheet" href="/app.css">
<link rel="stylesheet" href="${APP}/index.css">
<style>html,body{margin:0;height:100%}#win{position:relative;width:100vw;height:100vh;overflow:auto}</style>
</head><body><div id="win"></div></body></html>`;

let server, browser, page, base;

const LEADS = [
  { id: 'lead_alpha', name: 'Alpha Maschinenbau GmbH', campaign: 'Maschinenbau', domain: 'alpha.example', website: 'https://alpha.example', country: 'DE', research_status: 'new', validation_status: 'open', contacts: [], selected_contact_ids: [], payload: {}, data: {}, created_at_ms: 1, updated_at_ms: 2 },
  { id: 'lead_beta', name: 'Beta Chemie AG', campaign: 'Maschinenbau', domain: 'beta.example', website: 'https://beta.example', country: 'DE', research_status: 'new', validation_status: 'open', contacts: [], selected_contact_ids: [], payload: {}, data: {}, created_at_ms: 1, updated_at_ms: 1 },
];

before(async () => {
  server = createServer(async (req, res) => {
    const path = decodeURIComponent(new URL(req.url, 'http://x').pathname);
    if (path === '/') { res.writeHead(200, { 'content-type': 'text/html' }); res.end(PAGE); return; }
    const [rootDir, rel] = path.startsWith('/__stub/') ? [STUB, path.slice('/__stub'.length)] : [BOS, path];
    const file = normalize(join(rootDir, rel));
    if (!file.startsWith(rootDir)) { res.writeHead(403); res.end(); return; }
    try {
      let body = await readFile(file);
      // Wie der Service Worker (§16.15): ctox:shared/x.js → Shell-Pfad (PORTING.md Falle 2).
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
  page.__errors = errors;
  await page.goto(base);
  await page.evaluate(async ({ APP, LEADS }) => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    window.__mod = await import(`${APP}/index.js`);
    const kernel = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false, actor: { role: 'admin', permissions: ['apps.source.edit'] } });
    await kernel.ready;
    await kernel.sim.push('outbound_lead_generation_leads', LEADS);
    await kernel.sim.push('outbound_lead_generation_imports', [{ id: 'import_1', title: 'Maschinenbau', status: 'completed', created_at_ms: 1, updated_at_ms: 1 }]);
    await kernel.sim.settle();
    window.__k = kernel;
    window.__api = kernel.appApi('outbound-lead-generation', '1.0.323');
    window.__zone = () => kernel.statusZone.forApp('outbound-lead-generation').get().filter((e) => e.source === 'app');
    window.__commands = () => window.__api.query('business_commands', { sort: [{ created_at: 'asc' }] }).get();
  }, { APP, LEADS });
});

after(async () => { await browser?.close(); await new Promise((r) => server?.close(r)); });

const settle = async () => { await page.evaluate(() => window.__k.sim.settle()); await page.waitForTimeout(250); };

test('mount: Live-Daten sichtbar, gescoptes Root, Titel über api.shell, nur api.*', async () => {
  await page.evaluate(async () => {
    const { countingApi } = await import('/v4/tools/port/test-api.mjs');
    window.__c = countingApi(window.__api);
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, {});
  });
  await settle();
  assert.equal(await page.$eval('#win', (el) => el.dataset.app), 'outbound-lead-generation');
  assert.match(await page.$eval('[data-campaigns-body]', (el) => el.textContent), /Maschinenbau/);
  assert.match(await page.$eval('[data-leads-body]', (el) => el.textContent), /Alpha Maschinenbau GmbH/);
  assert.equal(await page.evaluate(() => window.__k.shell.window('outbound-lead-generation').state.get().title), 'Outbound Lead Generation');
  assert.equal(await page.evaluate(() => window.__c.subscribers('outbound_lead_generation_leads')), 1);
  // Startpflege legt die mitgelieferten Quellen über api.put an.
  const quellen = await page.evaluate(() => window.__api.query('outbound_lead_generation_sources', {}).get().length);
  assert.ok(quellen > 10, `mitgelieferte Quellen angelegt: ${quellen}`);
  assert.deepEqual(page.__errors, []);
});

test('Rundgang RUND2: kein Wächter-Kommentar („// v4-allow“) als sichtbarer Text in HTML-Vorlagen', async () => {
  const src = await readFile(join(HERE, '..', 'index.js'), 'utf8');
  // Ein Zeilenkommentar direkt hinter einem HTML-Tag steht innerhalb eines Template-Literals und wird gerendert
  // (Lead-Karten zeigten „// v4- allow: label“). Richtig: ${'' /* v4-allow: label */}.
  const leaks = src.split('\n').map((l, i) => [i + 1, l]).filter(([, l]) => /<\/?[a-z][^>]*>\s*\/\/\s*v4-allow/.test(l) && !/`\s*\/\/\s*v4-allow/.test(l)).map(([n]) => n);
  assert.deepEqual(leaks, []);
  assert.equal(await page.evaluate(() => document.querySelector('#win')?.innerText.includes('v4-allow') ?? false), false);
});

test('Live-Änderung vom Core erscheint ohne Neuladen, Löschung verschwindet', async () => {
  await page.evaluate(() => window.__k.sim.push('outbound_lead_generation_leads', { id: 'lead_gamma', name: 'Gamma Live KG', campaign: 'Maschinenbau', research_status: 'new', contacts: [], selected_contact_ids: [], payload: {}, data: {}, updated_at_ms: 9 }));
  await settle();
  assert.match(await page.$eval('[data-leads-body]', (el) => el.textContent), /Gamma Live KG/);
  await page.evaluate(() => window.__k.sim.push('outbound_lead_generation_leads', { id: 'lead_gamma', _deleted: true }));
  await settle();
  assert.doesNotMatch(await page.$eval('[data-leads-body]', (el) => el.textContent), /Gamma Live KG/);
});

test('leere Collections rendern keinen Zustandstext und keine Zone', async () => {
  const leer = await page.evaluate(async () => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    const k2 = createStubKernel({ storage: 'memory', latencyMs: 5, multiTab: false, instanceId: 'leer' });
    await k2.ready;
    const host = document.createElement('div');
    document.body.append(host);
    const app = await window.__mod.mount(host, k2.appApi('outbound-lead-generation', '1.0.323'), {});
    await new Promise((r) => setTimeout(r, 300));
    const kampagnen = host.querySelector('[data-campaigns-body]').textContent.trim();
    const leads = host.querySelector('[data-leads-body]').textContent.trim();
    const statusBox = Boolean(host.querySelector('.ctox-syncing, [role="status"]'));
    const zone = k2.statusZone.forApp('outbound-lead-generation').get().map((e) => e.code);
    await app.unmount();
    host.remove();
    await k2.close();
    return { kampagnen, leads, statusBox, zone };
  });
  assert.equal(leer.kampagnen, '');
  assert.equal(leer.statusBox, false);
  assert.deepEqual(leer.zone, []);
  assert.doesNotMatch(leer.leads, /geladen|synchronis/i);
  // Modulzustand ist modulweit: Hauptfenster neu montieren.
  await page.evaluate(async () => {
    await window.__app.unmount();
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, {});
  });
  await settle();
});

test('Schreiben: api.put erscheint live und wird vom Core angenommen', async () => {
  const result = await page.evaluate(async () => {
    const h = window.__api.put('outbound_lead_generation_leads', { id: 'lead_delta', name: 'Delta Put GmbH', campaign: 'Maschinenbau', research_status: 'new', contacts: [], selected_contact_ids: [], payload: {}, data: {}, updated_at_ms: 10 });
    await h.local;
    return h.result;
  });
  assert.equal(result.status, 'accepted');
  await settle();
  assert.match(await page.$eval('[data-leads-body]', (el) => el.textContent), /Delta Put GmbH/);
});

test('Befehl: Update-Vorschau legt business_commands-Dokument an; failed → genau ein Zoneneintrag', async () => {
  await page.evaluate(() => window.__k.sim.failCommand('outbound.update_digest.send_now', 'kaputt'));
  const vorher = await page.evaluate(() => window.__commands().length);
  await page.evaluate(() => window.__mod.__leadgenOutboundTestHooks.__render.getState().port.befehl({
    id: 'cmd_test_digest', command_type: 'outbound.update_digest.send_now', record_id: 'update-digest', module: 'outbound',
    payload: { dry_run: true }, client_context: { source_module: 'outbound-lead-generation' },
  }));
  await settle();
  const cmds = await page.evaluate(() => window.__commands());
  assert.equal(cmds.length, vorher + 1);
  const cmd = cmds.at(-1);
  assert.equal(cmd.type, 'outbound.update_digest.send_now');
  assert.equal(cmd.payload.command_id, 'cmd_test_digest');
  assert.equal(cmd.payload.record_id, 'update-digest');
  assert.equal(cmd.status, 'failed');
  const zone = await page.evaluate(() => window.__zone().filter((e) => e.code.startsWith('outbound-lead-generation.command.')));
  assert.equal(zone.length, 1);
  assert.equal(zone[0].code, 'outbound-lead-generation.command.outbound.update_digest.send_now');
  assert.match(zone[0].text, /kaputt/);
});

test('Bestätigung über api.shell.dialog: ohne Ja kein Löschen', async () => {
  const vorher = await page.evaluate(() => window.__api.query('outbound_lead_generation_sources', {}).get().length);
  const dialoge = await page.evaluate(() => window.__k.sim.dialogs.length);
  await page.evaluate(() => {
    const knopf = document.createElement('button');
    knopf.dataset.action = 'delete-source';
    knopf.dataset.id = 'northdata.de';
    document.getElementById('win').append(knopf);
    return window.__mod.__leadgenOutboundTestHooks.handleClick({ target: knopf, preventDefault() {}, stopPropagation() {} }).catch(() => {}).finally(() => knopf.remove());
  });
  await settle();
  assert.equal(await page.evaluate(() => window.__k.sim.dialogs.length), dialoge + 1);
  assert.equal(await page.evaluate(() => window.__api.query('outbound_lead_generation_sources', {}).get().length), vorher);
});

test('snapshot/restored: Auswahl, Suche und offener Prompt-Entwurf überleben den Remount', async () => {
  await page.fill('[data-lead-search]', 'Beta');
  await page.evaluate(() => {
    const s = window.__mod.__leadgenOutboundTestHooks.__render.getState();
    s.selectedLeadId = 'lead_beta';
    s.researchPolicyDraft = 'halb fertiger Rechercheablauf';
  });
  const snap = await page.evaluate(async () => {
    const cloned = structuredClone(window.__app.snapshot());
    await window.__app.unmount();
    const s = window.__mod.__leadgenOutboundTestHooks.__render.getState();
    s.search = ''; s.selectedLeadId = ''; s.researchPolicyDraft = '';
    window.__app = await window.__mod.mount(document.getElementById('win'), window.__c.api, { restored: cloned });
    return cloned;
  });
  assert.ok(JSON.stringify(snap).length < 1024 * 1024);
  await settle();
  const nach = await page.evaluate(() => {
    const s = window.__mod.__leadgenOutboundTestHooks.__render.getState();
    return { search: s.search, selected: s.selectedLeadId, draft: s.researchPolicyDraft, feld: document.querySelector('[data-lead-search]')?.value };
  });
  assert.equal(nach.search, 'Beta');
  assert.equal(nach.feld, 'Beta');
  assert.equal(nach.selected, 'lead_beta');
  assert.equal(nach.draft, 'halb fertiger Rechercheablauf');
});

test('Scrape-Registry live aus den OLG-Collections, ohne registry_read-Befehl', async () => {
  const r = await page.evaluate(async () => {
    const hooks = window.__mod.__leadgenOutboundTestHooks;
    const s = hooks.__render.getState();
    const vorher = window.__commands().length;
    const upload = window.__api.files.put(new Blob(['// northdata adapter\nconsole.log(1);\n'], { type: 'text/plain' }), { name: 'rev0001', mime: 'text/plain' });
    await upload.local;
    const ref = await upload.ref;
    await window.__k.sim.push('outbound_lead_generation_scrape_targets', [{
      id: 'northdata-de', target_id: 'scrape_target-nd', target_key: 'northdata-de', display_name: 'Northdata', start_url: 'https://www.northdata.de/',
      target_kind: 'generic', status: 'active', config: {}, output_schema: {}, workspace_dir: 'outbound-lead-generation/scrape/targets/northdata-de',
      latest_script_revision_no: 1, created_at: '2026-10-15T08:00:00.000Z', updated_at: '2026-10-15T08:00:00.000Z', updated_at_ms: 1,
    }]);
    await window.__k.sim.push('outbound_lead_generation_scrape_scripts', [{
      id: 'northdata-de:r1', target_key: 'northdata-de', target_id: 'scrape_target-nd', revision_no: 1, language: 'javascript',
      script_sha256: 'x', script_ref: ref, created_at: '2026-10-15T08:00:00.000Z',
    }]);
    await window.__k.sim.push('outbound_lead_generation_scrape_runs', [{
      id: 'run_1', run_id: 'run_1', target_id: 'scrape_target-nd', target_key: 'northdata-de', trigger_kind: 'research',
      started_at: '2026-10-15T08:01:00.000Z', finished_at: '2026-10-15T08:02:00.000Z', status: 'succeeded', result: {},
    }]);
    await window.__k.sim.settle();
    await new Promise((res) => setTimeout(res, 100));
    const erst = s.registry.get('northdata-de');
    // Live: ein neuer Lauf und ein inaktives Anbieterkonto kommen ohne Befehl und ohne Neuladen an.
    await window.__k.sim.push('outbound_lead_generation_scrape_runs', [{
      id: 'run_2', run_id: 'run_2', target_id: 'scrape_target-nd', target_key: 'northdata-de', trigger_kind: 'test',
      started_at: '2026-10-15T09:00:00.000Z', finished_at: null, status: 'failed', result: { failure_mode: 'portal_drift' },
    }]);
    await window.__k.sim.push('outbound_lead_generation_scrape_account_states', [{
      id: 'northdata-de', target_id: 'scrape_target-nd', target_key: 'northdata-de', generation: 1, reason: 'Konto gesperrt',
      causal_run_id: 'run_2', last_probe_run_id: 'run_2', failed_probes: 1, next_probe_at_ms: 0, probe_lease_until_ms: 0, updated_at_ms: 2,
    }]);
    await window.__k.sim.settle();
    await new Promise((res) => setTimeout(res, 100));
    const dann = s.registry.get('northdata-de');
    s.adapterInspectorSourceId = 'northdata.de';
    await hooks.ladeAdapterSkript('northdata.de');
    const skript = s.adapterInspectorScript;
    s.adapterInspectorSourceId = '';
    const neueBefehle = window.__commands().slice(vorher).map((c) => c.type);
    return { erst, dann, skript, neueBefehle };
  });
  assert.equal(r.erst.status, 'active');
  assert.equal(r.erst.last_run.run_id, 'run_1');
  assert.equal(r.erst.last_successful_run.run_id, 'run_1');
  assert.equal(r.erst.account_state, null);
  assert.equal(r.dann.last_run.run_id, 'run_2');
  assert.equal(r.dann.last_run.failure_mode, 'portal_drift');
  assert.equal(r.dann.last_successful_run.run_id, 'run_1');
  assert.equal(r.dann.account_state.status, 'provider_account_inactive');
  assert.equal(r.dann.account_state.reason, 'Konto gesperrt');
  assert.equal(r.skript.available, true);
  assert.equal(r.skript.revision_no, 1);
  assert.match(r.skript.text, /northdata adapter/);
  assert.ok(!r.neueBefehle.includes('outbound.research_source.registry_read'), `kein registry_read: ${r.neueBefehle}`);
});

test('navigate: Browser-Anmeldung flach in der Routenform der Browser-App', async () => {
  const r = await page.evaluate(async () => {
    await window.__k.sim.publishAppVersion('browser', '1', {});
    const routen = [];
    const off = window.__k.appApi('browser', '1').shell.onRoute((ev) => routen.push(ev));
    const hooks = window.__mod.__leadgenOutboundTestHooks;
    const ok = await hooks.openSourceAuthorization({ id: 'dnbhoovers.com' }, {
      session_id: 'browser_session_dnb_1', target_url: 'https://app.dnbhoovers.com/login', source_id: 'dnbhoovers.com',
      allowed_domains: ['dnbhoovers.com'], required_secret_name: 'DNB_LOGIN', command_id: 'cmd_auth_1', task_id: 'task_1',
    });
    off();
    return { ok, routen: structuredClone(routen) };
  });
  assert.equal(r.ok, true);
  assert.equal(r.routen.length, 1);
  const { from, to, params } = r.routen[0];
  assert.equal(from, 'outbound-lead-generation');
  assert.equal(to, 'browser');
  // modules/browser/index.js: browserRouteArgs(params) → params (kein args-Träger), browserAuthRequestFromArgs braucht
  // session_id (browser_session_…), purpose web_stack_auth und target_url auf oberster Ebene.
  assert.equal(params.args, undefined);
  assert.equal(params.title, undefined);
  assert.equal(params.mode, undefined);
  assert.match(params.session_id, /^browser_session_[a-z0-9_-]+$/i);
  assert.equal(params.purpose, 'web_stack_auth');
  assert.equal(params.target_url, 'https://app.dnbhoovers.com/login');
  assert.equal(params.tab_id, 'browser_tab_browser_session_dnb_1');
  assert.equal(params.secret_name, 'DNB_LOGIN');
  assert.equal(params.auth_assist_command_id, 'cmd_auth_1');
  assert.deepEqual(params.allowed_domains, ['dnbhoovers.com']);
});

test('navigate: Excel-Export öffnet Spreadsheets mit file_ref und Herkunft in deren Routenform', async () => {
  const r = await page.evaluate(async () => {
    await window.__k.sim.publishAppVersion('spreadsheets', '1', {});
    const sheetsApi = window.__k.appApi('spreadsheets', '1');
    const routen = [];
    const off = sheetsApi.shell.onRoute((ev) => routen.push(ev));
    const hooks = window.__mod.__leadgenOutboundTestHooks;
    const s = hooks.__render.getState();
    const leads = s.leads.filter((lead) => ['lead_alpha', 'lead_beta'].includes(lead.id));
    await hooks.exportResearchXlsx(leads, 'Maschinenbau');
    off();
    const params = routen[0]?.params || {};
    // Wie modules/spreadsheets/index.js fileFromRef: Bytes über api.files der Spreadsheet-App.
    const blob = params.file_ref ? await sheetsApi.files.get(params.file_ref) : null;
    const bytes = blob ? new Uint8Array(await blob.arrayBuffer()) : new Uint8Array();
    const sha = Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256', bytes)), (b) => b.toString(16).padStart(2, '0')).join('');
    return { anzahl: routen.length, to: routen[0]?.to, params: structuredClone(params), groesse: bytes.length, sha };
  });
  assert.equal(r.anzahl, 1);
  assert.equal(r.to, 'spreadsheets');
  assert.equal(r.params.openFile, undefined);
  assert.equal(typeof r.params.file_ref, 'string');
  assert.ok(r.groesse > 0, 'Excel-Bytes über api.files lesbar');
  assert.match(r.params.name, /\.xlsx$/);
  assert.equal(r.params.source_kind, 'research_generated');
  assert.equal(r.params.open_purpose, 'snapshot_report');
  const snap = r.params.report_snapshot;
  assert.equal(snap.source_module, 'outbound-lead-generation');
  assert.equal(snap.source_collection, 'outbound_lead_generation_leads');
  assert.deepEqual([...snap.source_record_ids].sort(), ['lead_alpha', 'lead_beta']);
  assert.ok(Number.isSafeInteger(snap.captured_at_ms) && snap.captured_at_ms > 0);
  // assertSpreadsheetSnapshotBytes: der Hash im Snapshot muss zu den abgelegten Bytes passen.
  assert.equal(snap.file_sha256, r.sha);
});

test('Bildschirmfotos 1280 und 420', async () => {
  await mkdir(SHOTS, { recursive: true });
  await page.fill('[data-lead-search]', '');
  await page.evaluate(() => { window.__mod.__leadgenOutboundTestHooks.__render.getState().search = ''; window.__mod.__leadgenOutboundTestHooks.__render.render(); });
  await settle();
  await page.setViewportSize({ width: 1280, height: 800 });
  await page.waitForTimeout(200);
  await page.screenshot({ path: join(SHOTS, 'olg-1280.png') });
  await page.setViewportSize({ width: 420, height: 860 });
  await page.waitForTimeout(200);
  await page.screenshot({ path: join(SHOTS, 'olg-420.png') });
  const quer = await page.evaluate(() => document.getElementById('win').scrollWidth - document.getElementById('win').clientWidth);
  assert.ok(quer <= 1, `kein horizontales Überlaufen bei 420 px (${quer})`);
});

// main #725 (2f7771874) nachgezogen (Strang OLG725): scheitert das Lesen des Leads (RATE_LIMITED), bleibt der Entwurf
// im Editor erhalten; erst ein lokal bestätigter Schreibvorgang (api.put → handle.local) schließt die Maske.
test('Lead-Editor: rate-limitiertes Lesen behält den Entwurf, erneuter Versuch speichert', async () => {
  const r = await page.evaluate(async () => {
    const hooks = window.__mod.__leadgenOutboundTestHooks;
    const s = hooks.testState();
    const leads = s.collections.leads;
    const echtesEins = leads.eins;
    let ablehnen;
    leads.eins = () => new Promise((_, reject) => { ablehnen = reject; });
    s.leadEditorId = 'lead_beta';
    s.leadEditorOpen = true;
    s.leadEditorContactId = '';
    s.adapterInspectorSourceId = '';
    s.sellifyImportOpen = false;
    s.leadDraft = { name: 'Beta Chemie AG Neu', campaign: 'Maschinenbau', country: 'DE' };
    s.leadDraftOriginal = { name: 'Beta Chemie AG', campaign: 'Maschinenbau', country: 'DE' };
    const dialog = () => s.root.querySelector('[data-app-dialog] [data-lead-editor]');
    const save = hooks.saveLeadEditor();
    const waehrend = {
      saving: s.leadEditorSaving,
      open: s.leadEditorOpen,
      busy: dialog()?.getAttribute('aria-busy'),
      saveDisabled: dialog()?.querySelector('[data-action="save-lead-editor"]')?.disabled,
      inputsDisabled: [...(dialog()?.querySelectorAll('[data-lead-edit-field]') || [])].every((el) => el.disabled),
    };
    // Schließen während des Speicherns wird ignoriert (sonst verlöre der Entwurf seine einzige Kopie).
    const schliessen = dialog()?.querySelector('[data-action="close-lead-editor"]');
    if (schliessen) await hooks.handleClick({ target: schliessen, preventDefault() {}, stopPropagation() {} }).catch(() => {});
    const nachSchliessversuch = s.leadEditorOpen;
    ablehnen?.(Object.assign(new Error('RATE_LIMITED: per-peer query-fetch rate limit reached'), { code: 'RATE_LIMITED' }));
    await save;
    const nachFehler = {
      saving: s.leadEditorSaving,
      open: s.leadEditorOpen,
      name: s.leadDraft?.name,
      feld: dialog()?.querySelector('[data-lead-edit-field="name"]')?.value,
      feldGesperrt: dialog()?.querySelector('[data-lead-edit-field="name"]')?.disabled,
      zone: window.__k.statusZone.forApp('outbound-lead-generation').get().map((e) => e.text).join(' | '),
      gespeichert: window.__api.get('outbound_lead_generation_leads', 'lead_beta').get()?.name,
    };
    leads.eins = echtesEins;
    await hooks.saveLeadEditor();
    await window.__k.sim.settle();
    return {
      waehrend, nachSchliessversuch, nachFehler,
      danach: { open: s.leadEditorOpen, name: window.__api.get('outbound_lead_generation_leads', 'lead_beta').get()?.name },
    };
  });
  assert.deepEqual(r.waehrend, { saving: true, open: true, busy: 'true', saveDisabled: true, inputsDisabled: true });
  assert.equal(r.nachSchliessversuch, true, 'Schließen während des Speicherns ist gesperrt');
  assert.equal(r.nachFehler.saving, false);
  assert.equal(r.nachFehler.open, true, 'ein abgelehntes Lesen darf den Entwurf nicht schließen');
  assert.equal(r.nachFehler.name, 'Beta Chemie AG Neu');
  assert.equal(r.nachFehler.feld, 'Beta Chemie AG Neu', 'Editor zeigt den Entwurf, nicht den alten Stand');
  assert.equal(r.nachFehler.feldGesperrt, false, 'nach dem Fehler wieder bearbeitbar');
  assert.match(r.nachFehler.zone, /fehlgeschlagen/);
  assert.equal(r.nachFehler.gespeichert, 'Beta Chemie AG');
  assert.deepEqual(r.danach, { open: false, name: 'Beta Chemie AG Neu' });
});

test('patchLead: fehlender Lead ist kein stiller Erfolg (main #725)', async () => {
  const meldung = await page.evaluate(async () => {
    try { await window.__mod.__leadgenOutboundTestHooks.patchLead('lead_gibt_es_nicht', { name: 'x' }); return 'kein Fehler'; } catch (e) { return String(e.message); }
  });
  assert.match(meldung, /LEAD_NOT_FOUND/);
});

test('unmount meldet alle Abos ab und leert root', async () => {
  const r = await page.evaluate(async () => {
    await window.__app.unmount();
    const namen = ['outbound_lead_generation_sources', 'outbound_lead_generation_adapters', 'outbound_lead_generation_imports',
      'outbound_lead_generation_research_policies', 'outbound_lead_generation_leads', 'business_commands'];
    return { subs: namen.map((n) => window.__c.subscribers(n)), leer: document.getElementById('win').childElementCount };
  });
  assert.deepEqual(r.subs, [0, 0, 0, 0, 0, 0]);
  assert.equal(r.leer, 0);
  assert.deepEqual(page.__errors, []);
});
