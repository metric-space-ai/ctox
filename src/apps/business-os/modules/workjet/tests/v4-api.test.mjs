// Origin: CTOX
// License: AGPL-3.0-only
// workjet (Handler-only, E54.3b) gegen den App-API-4.0-Kern-Stub (createStubKernel) in echtem Chromium.
// Stub: CTOX_APP_API_STUB, sonst Pin ~/.local/state/shell-v4/stub-pins/f967baf.
// Lauf: cd src/apps/business-os && node --test modules/workjet/tests/*.test.mjs
//
// Der Stub kann echte Handler mit ./_lib-Importen nicht als Artefakt laden (er importiert eine einzelne Datei als
// data:-URL und reicht nur {app_id} als ctx). Deshalb läuft der ECHTE Handler als Simulator:
// kernel.sim.onCommand('workjet.worker.dispatch', …) → handle(command, ctx). Der sim-ctx des Stubs hat keine Daten-API
// (nur app_id, command_id, model, cap); der Test baut den Handler-ctx (get/read/put/patch/transaction/now) als Adapter
// über die App-API der veröffentlichten workjet-Version (kernel.appApi('workjet', '1')). Damit prüft der Kern-Stub die
// echten Manifest-Angaben: eigene Collections + Schemas, Fremdlesen (Deklaration + Export der Eigentümer ctox/threads).
// Abweichung zum Kern: transaction() hat keine Rücknahme (Adapter), Akteur = api.actor zum Zeitpunkt des Befehls,
// Fehlercode wird für die Prüfung in die Fehlermeldung übernommen („<code>: <message>“).
import { test, before, after } from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
import { createRequire } from 'node:module';
import { fileURLToPath } from 'node:url';
import { dirname, join, normalize, extname } from 'node:path';
import { homedir } from 'node:os';
import { realManifestRoutes } from '../../../v4/tools/port/real-manifest.mjs';

const STUB = process.env.CTOX_APP_API_STUB || join(homedir(), '.local/state/shell-v4/stub-pins/f967baf');
const HERE = dirname(fileURLToPath(import.meta.url));
const BOS = normalize(join(HERE, '../../..'));
const require = createRequire(import.meta.url);
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || join(homedir(), 'Documents/ctox/src/apps/business-os/node_modules/playwright'));
const TYPES = { '.js': 'text/javascript', '.mjs': 'text/javascript', '.json': 'application/json', '.html': 'text/html' };
const APP = 'workjet';
const OWNER = 'owner-1';
const PROJECT = 'proj-1';
const THREAD = 'thread-1';
const ENV = 'env-mac-1';

let server, browser, page;

before(async () => {
  server = createServer(async (req, res) => {
    const path = decodeURIComponent(new URL(req.url, 'http://x').pathname);
    if (path === '/') { res.writeHead(200, { 'content-type': 'text/html' }); res.end('<!doctype html><html><body></body></html>'); return; }
    const [rootDir, rel] = path.startsWith('/__stub/') ? [STUB, path.slice('/__stub'.length)] : [BOS, path];
    const file = normalize(join(rootDir, rel));
    if (!file.startsWith(rootDir)) { res.writeHead(403); res.end(); return; }
    try {
      const body = await readFile(file);
      res.writeHead(200, { 'content-type': TYPES[extname(file)] || 'application/octet-stream' }); res.end(body);
    } catch { res.writeHead(404); res.end(); }
  });
  await new Promise((r) => server.listen(0, '127.0.0.1', r));
  browser = await chromium.launch();
  page = await browser.newPage();
  page.on('console', (m) => { if (m.type() === 'error') console.error('[page]', m.text()); });
  await page.goto(`http://127.0.0.1:${server.address().port}/`);
  // Echte Routen; handlers entfällt, weil der Stub sonst das (nicht veröffentlichte) Artefakt laden will – die Route
  // kommt vom registrierten Simulator (sim.onCommand), der den echten Handler ausführt.
  const routes = { ...realManifestRoutes(APP) };
  assert.deepEqual(routes.handlers, { 'workjet.worker.dispatch': 'handlers/workjet.worker.dispatch.js' });
  delete routes.handlers;
  await page.evaluate((r) => { window.__routes = r; }, routes);
  await page.evaluate(async ({ APP, OWNER, PROJECT, THREAD }) => {
    const { createStubKernel } = await import('/__stub/ctox-app-api-stub.mjs');
    const { default: handle } = await import(`/modules/${APP}/handlers/workjet.worker.dispatch.js`);
    const kernel = createStubKernel({ storage: 'memory', latencyMs: 2, multiTab: false, actor: { id: OWNER, role: 'admin' } });
    await kernel.ready;
    for (const o of window.__routes?._fixture_owners ?? []) if (!(await kernel.sim.inspect('business_apps', o.app))?.data?.active_ver) await kernel.sim.publishAppVersion(o.app, o.ver, o.publish);
    await kernel.sim.publishAppVersion(APP, '1', { title: 'Workjet', manifestExtra: window.__routes });
    const ts = Date.now();
    await kernel.sim.push('business_users', [
      { id: 'bu-owner', user_id: OWNER, role: 'admin', active: true, display_name: 'Owner', created_at_ms: ts, updated_at_ms: ts },
      { id: 'bu-other', user_id: 'other-1', role: 'admin', active: true, display_name: 'Other', created_at_ms: ts, updated_at_ms: ts },
    ]);
    await kernel.sim.push('workjet_projects', [
      { id: PROJECT, name: 'Projekt', status: 'active', owner_user_id: OWNER, created_at_ms: ts, updated_at_ms: ts },
    ]);
    await kernel.sim.push('workjet_project_chats', [
      { id: 'chat-1', project_id: PROJECT, thread_id: THREAD, owner_user_id: OWNER, kind: 'group', created_at_ms: ts, updated_at_ms: ts },
    ]);
    await kernel.sim.settle();
    const api = kernel.appApi(APP, '1');
    const first = (live) => new Promise((resolve) => { let un = null; let done = false; un = live.subscribe((v) => { if (done) return; done = true; resolve(v); queueMicrotask(() => un?.()); }); if (done) un?.(); });
    const write = async (h) => { await h.local; };
    const data = {
      get: async (c, id) => (await first(api.get(c, id))) ?? null,
      read: async (c, q) => first(api.query(c, q)),
      put: async (c, doc) => write(api.put(c, doc)),
      patch: async (c, id, p) => write(api.patch(c, id, p)),
    };
    const ctx = { ...data, transaction: (fn) => fn(data), now: () => Date.now() };
    kernel.sim.onCommand('workjet.worker.dispatch', async (payload, simCtx) => {
      try {
        return await handle({ id: simCtx.command_id, type: 'workjet.worker.dispatch', payload, actor: { id: api.actor.id, role: api.actor.role } }, ctx);
      } catch (err) { throw new Error(`${err.code}: ${err.message}`); }
    });
    window.__k = kernel;
    window.__api = api;
    window.__send = async (payload) => {
      const h = api.command('workjet.worker.dispatch', payload);
      await h.local;
      for (let i = 0; i < 400; i += 1) {
        const s = h.status.get();
        if (s && ['done', 'failed', 'rejected', 'cancelled'].includes(s.status)) return JSON.parse(JSON.stringify(s));
        await new Promise((r) => setTimeout(r, 10));
      }
      return { status: 'timeout' };
    };
    window.__rows = async (c, selector = {}) => first(api.query(c, { selector }));
  }, { APP, OWNER, PROJECT, THREAD });
});

after(async () => { await browser?.close(); await new Promise((r) => server?.close(r)); });

const send = (payload) => page.evaluate((p) => window.__send(p), payload);
const rows = (c, selector) => page.evaluate(([c, s]) => window.__rows(c, s), [c, selector]);
const ok = async (payload) => {
  const s = await send(payload);
  assert.equal(s.status, 'done', JSON.stringify(s));
  return s.result;
};
const failed = async (payload, code) => {
  const s = await send(payload);
  assert.equal(s.status, 'failed', JSON.stringify(s));
  assert.match(String(s.error), new RegExp(`^${code}:`), JSON.stringify(s));
  return s;
};
const dispatchArgs = (extra = {}) => ({ action: 'dispatch', dispatch_key: 'k1', task: 'Fix the bug', project_id: PROJECT, source_supervisor_thread_id: THREAD, ...extra });

let registration;
let intent;

test('Manifest: Route aus dem echten Manifest, eigene fünf Collections, Fremdlesen über Exporte', async () => {
  const m = await page.evaluate(async () => (await window.__k.sim.inspect('business_app_versions', 'workjet@1'))?.data?.manifest ?? null);
  assert.ok(m, 'workjet@1 veröffentlicht');
  assert.deepEqual(Object.keys(m.collections).sort(), ['workjet_luma_state', 'workjet_mailbox_envelopes', 'workjet_worker_dispatch_intents',
    'workjet_worker_dispatch_outcomes', 'workjet_worker_dispatch_sources']);
  assert.deepEqual(Object.keys(m.foreign_collections).sort(), ['business_users', 'ctox_task_approval_requests', 'workjet_project_chats', 'workjet_projects']);
  assert.deepEqual(await rows('workjet_projects').then((r) => r.map((x) => x.id)), [PROJECT]);
});

test('register_source → Empfang, Zeile in workjet_worker_dispatch_sources', async () => {
  registration = await ok({ action: 'register_source', source_environment_id: ENV, source_supervisor_thread_id: THREAD, project_id: PROJECT });
  assert.equal(registration.contract, 'ctox.workjet.worker-dispatch.v1');
  assert.equal(registration.revision, 1);
  assert.equal(registration.state, 'active');
  const src = await rows('workjet_worker_dispatch_sources');
  assert.equal(src.length, 1);
  assert.equal(src[0].registration_id, registration.registrationId);
});

test('dispatch → pending-Absicht; Replay gleicher Schlüssel = gleiche Absicht ohne zweite Zeile; anderer Inhalt abgelehnt', async () => {
  const first = await ok(dispatchArgs());
  assert.equal(first.state, 'pending');
  intent = first.intent;
  let list = await rows('workjet_worker_dispatch_intents');
  assert.equal(list.length, 1);
  assert.equal(list[0].status, 'pending');
  const replay = await ok(dispatchArgs());
  assert.deepEqual(replay, first);
  list = await rows('workjet_worker_dispatch_intents');
  assert.equal(list.length, 1);
  await failed(dispatchArgs({ task: 'Other task' }), 'conflict');
});

test('Workjet-Quelle liest offene Absichten per api.query (statt MCP-poll); poll-Aktion liefert dieselbe', async () => {
  const open = await rows('workjet_worker_dispatch_intents', { owner_user_id: OWNER, source_environment_id: ENV, status: 'pending' });
  assert.deepEqual(open.map((r) => r.intent_id), [intent.intentId]);
  const polled = await ok({ action: 'poll', source_environment_id: ENV });
  assert.deepEqual(polled.intents, [intent]);
  assert.equal(polled.registration.registrationId, registration.registrationId);
});

test('complete zweimal: erstes gewinnt, abweichendes zweites abgelehnt', async () => {
  const result = {
    schemaVersion: 1, status: 'dispatched', environmentId: ENV, workerThreadId: intent.intentId, computerId: 'mac-1',
    branch: `workjet/worker/${intent.intentId}`, worktreePath: '/tmp/wt', parent: { environmentId: ENV, threadId: THREAD },
    modelSelection: { instanceId: 'codex', model: 'gpt' }, enabledCapabilityIds: [],
  };
  const base = { action: 'complete', registration_id: registration.registrationId, revision: 1, intent_id: intent.intentId };
  assert.deepEqual((await ok({ ...base, result })).result, result);
  await failed({ ...base, result: { schemaVersion: 1, status: 'failed', reason: 'create-failed' } }, 'conflict');
  const [row] = await rows('workjet_worker_dispatch_intents', { intent_id: intent.intentId });
  assert.deepEqual(row.result, result);
  assert.equal(row.status, 'completed');
});

test('report_outcome → Zeile in workjet_worker_dispatch_outcomes; observe zeigt Bestätigung und Bericht', async () => {
  const receipt = {
    schema: 'ctox.workjet.worker-outcome.v1', worker_thread_id: intent.intentId, environment_id: ENV, computer_id: 'mac-1',
    branch: `workjet/worker/${intent.intentId}`, execution_stopped: true,
    pull_request: { provider: 'github', number: 7, url: 'https://github.com/metric-space-ai/ctox/pull/7', head_oid: 'b'.repeat(40), state: 'merged' },
  };
  const out = await ok({ action: 'report_outcome', registration_id: registration.registrationId, revision: 1, intent_id: intent.intentId, receipt });
  assert.equal(out.provenance, 'authenticated_source_report');
  assert.deepEqual(out.receipt, receipt);
  assert.equal((await rows('workjet_worker_dispatch_outcomes')).length, 1);
  const seen = await ok({ action: 'observe', project_id: PROJECT, source_supervisor_thread_id: THREAD });
  assert.equal(seen.observations.length, 1);
  assert.equal(seen.observations[0].acknowledgement.status, 'dispatched');
  assert.deepEqual(seen.observations[0].reportedOutcome.receipt, receipt);
});

test('task > 16384 → invalid', async () => {
  await failed(dispatchArgs({ dispatch_key: 'big', task: 'x'.repeat(16385) }), 'invalid');
});

test('129. offene Absicht → abgelehnt', async () => {
  // Die erste Absicht ist abgeschlossen: 128 neue offene anlegen, die 129. offene wird abgelehnt.
  for (let i = 0; i < 128; i += 1) await ok(dispatchArgs({ dispatch_key: `cap-${i}` }));
  await failed(dispatchArgs({ dispatch_key: 'cap-overflow' }), 'conflict');
  const pending = await rows('workjet_worker_dispatch_intents', { status: 'pending' });
  assert.equal(pending.length, 128);
});

test('fremde Sitzung → forbidden', async () => {
  await page.evaluate(() => window.__k.sim.setActor({ id: 'other-1', role: 'admin' }));
  try {
    await failed(dispatchArgs({ dispatch_key: 'intruder' }), 'forbidden');
    await failed({ action: 'observe', project_id: PROJECT, source_supervisor_thread_id: THREAD }, 'forbidden');
    await failed({ action: 'register_source', source_environment_id: ENV, source_supervisor_thread_id: THREAD, project_id: PROJECT }, 'forbidden');
  } finally {
    await page.evaluate((id) => window.__k.sim.setActor({ id, role: 'admin' }), OWNER);
  }
});

test('revoke_source → widerrufen, danach kein poll-Kopf und kein dispatch', async () => {
  const revoked = await ok({ action: 'revoke_source', registration_id: registration.registrationId, revision: 1 });
  assert.equal(revoked.state, 'revoked');
  assert.equal(revoked.revision, 2);
  assert.deepEqual((await ok({ action: 'poll', source_environment_id: ENV })).intents, []);
  await failed(dispatchArgs({ dispatch_key: 'after-revoke' }), 'conflict');
  const [src] = await rows('workjet_worker_dispatch_sources');
  assert.equal(src.state, 'revoked');
});
