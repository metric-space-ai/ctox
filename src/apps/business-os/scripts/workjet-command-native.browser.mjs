// Isolated browser -> WebRTC -> Rust/SQLite protocol fixture. No operator state,
// credentials or production instance. The daemon's demo collection stores seeded
// terminal receipts; it does NOT execute the Business OS project-list handler.
import assert from 'node:assert/strict';
import http from 'node:http';
import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, resolve, sep } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { decodeChunk } from '../rxdb/src/chunk-decoder.mjs';

const args = process.argv.slice(2);
const option = name => args[args.indexOf(name) + 1];
const binary = option('--native');
assert.ok(args.includes('--native') && binary?.startsWith('/'), 'explicit isolated native wire-daemon binary required');
const baseline = args.includes('--baseline') ? option('--baseline') : null;
const shell = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const app = readFileSync(resolve(shell, 'app.js'), 'utf8');
const helper = app.slice(app.indexOf('async function dispatchWorkjetProjectList('),
  app.indexOf('async function workjetProjectControl('));
const { chromium } = await import(pathToFileURL(resolve(process.env.PLAYWRIGHT_MODULE_PATH, 'index.mjs')).href);
const child = spawn(binary, [], { stdio: ['pipe', 'pipe', 'inherit'] });
const lines = createInterface({ input: child.stdout });
const pending = new Map();
function awaitMessage(key, match, done = () => true) {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => { pending.delete(key); reject(new Error('native fixture deadline: ' + key)); }, 5000);
    pending.set(key, { match, docs: [], resolve: value => { clearTimeout(timer); resolve(value); }, done });
  });
}
lines.on('line', line => {
  const message = JSON.parse(line);
  for (const [key, slot] of pending) {
    if (!slot.match(message)) continue;
    slot.docs.push(message);
    if (slot.done(message)) { pending.delete(key); slot.resolve(slot.docs); }
  }
});
const ready = awaitMessage('ready', message => message.kind === 'ready');
const send = value => child.stdin.write(JSON.stringify(value) + '\n');
const nativeQueries = [];
async function nativeRead(request) {
  const started = performance.now();
  if (request.fixtureSeed) {
    const written = awaitMessage(request.requestId, message => message.kind === 'seeded');
    send({ kind: 'seed', collection: 'demo', docs: request.fixtureSeed });
    await written;
    return true;
  }
  const nativeRequest = { ...request, collectionName: 'demo', schemaVersion: 0 };
  const response = awaitMessage(request.requestId,
    message => message.kind === 'wire' && message.frame?.method === 'rxdb.query.chunk'
      && message.frame.params?.[0]?.requestId === request.requestId,
    message => message.frame.params[0].complete === true);
  send({ kind: 'request', peerIdentity: 'isolated-guest-browser', message: {
    id: request.requestId, method: 'rxdb.query.fetch', params: [nativeRequest],
  } });
  const frames = await response;
  const documents = [];
  for (const frame of frames) documents.push(...await decodeChunk(frame.frame.params[0]));
  nativeQueries.push({ durationMs: performance.now() - started, documents: documents.length });
  return { documents, authoritativeRevision: 'isolated-native-fixture' };
}
const server = http.createServer((request, response) => {
  try {
    const url = new URL(request.url, 'http://localhost');
    if (url.pathname === '/') {
      response.writeHead(200, { 'content-type': 'text/html' });
      response.end('<!doctype html><title>Isolated Workjet command wait</title>'); return;
    }
    if (url.pathname === '/timing-helper.mjs') {
      response.writeHead(200, { 'content-type': 'text/javascript' });
      response.end(helper + '\nexport { dispatchWorkjetProjectList };'); return;
    }
    const relative = url.pathname.replace(/^\/(?:baseline\/)?/, '');
    const path = resolve(shell, relative);
    assert.ok(path.startsWith(shell + sep));
    const content = baseline && url.pathname === '/baseline/shared/command-bus.js'
      ? readFileSync(baseline) : readFileSync(path);
    response.writeHead(200, { 'content-type': 'text/javascript' }); response.end(content);
  } catch { response.writeHead(404); response.end(); }
});
let browser;
const report = { schema: 'ctox.workjet-command-native-fixture.v1',
  scope: 'real Chromium IndexedDB and WebRTC DataChannel plus Rust SQLite query dispatcher; seeded terminal receipt, not Business OS command execution/enrollment or installed acceptance',
  nativeBinary: binary, samples: [], nativeQueries };
try {
  await ready;
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  browser = await chromium.launch({ headless: true });
  const context = await browser.newContext();
  const page = await context.newPage();
  const nativePage = await context.newPage();
  const origin = `http://127.0.0.1:${server.address().port}`;
  await Promise.all([page.goto(origin), nativePage.goto(origin)]);
  await nativePage.exposeFunction('nativeRead', nativeRead);
  await nativePage.evaluate(() => {
    globalThis.pc = new RTCPeerConnection({ iceServers: [] });
    pc.ondatachannel = event => {
      globalThis.channel = event.channel;
      channel.onmessage = async event => {
        const { id, request } = JSON.parse(event.data);
        try { channel.send(JSON.stringify({ id, result: await nativeRead(request) })); }
        catch (error) { channel.send(JSON.stringify({ id, error: String(error) })); }
      };
    };
  });
  const offer = await page.evaluate(async () => {
    globalThis.pc = new RTCPeerConnection({ iceServers: [] });
    globalThis.channel = pc.createDataChannel('ctox-query-fixture');
    globalThis.requests = new Map();
    channel.onmessage = event => {
      const reply = JSON.parse(event.data); const slot = requests.get(reply.id);
      if (!slot) return; requests.delete(reply.id);
      reply.error ? slot.reject(new Error(reply.error)) : slot.resolve(reply.result);
    };
    globalThis.queryNative = request => new Promise((resolve, reject) => {
      requests.set(request.requestId, { resolve, reject });
      channel.send(JSON.stringify({ id: request.requestId, request }));
    });
    await pc.setLocalDescription(await pc.createOffer());
    await new Promise(resolve => { if (pc.iceGatheringState === 'complete') resolve();
      else pc.onicegatheringstatechange = () => { if (pc.iceGatheringState === 'complete') resolve(); }; });
    return pc.localDescription.toJSON();
  });
  const answer = await nativePage.evaluate(async offer => {
    await pc.setRemoteDescription(offer); await pc.setLocalDescription(await pc.createAnswer());
    await new Promise(resolve => { if (pc.iceGatheringState === 'complete') resolve();
      else pc.onicegatheringstatechange = () => { if (pc.iceGatheringState === 'complete') resolve(); }; });
    return pc.localDescription.toJSON();
  }, offer);
  await page.evaluate(answer => pc.setRemoteDescription(answer), answer);
  await page.waitForFunction(() => channel.readyState === 'open', { timeout: 5000 });
  for (const mode of baseline ? ['baseline', 'fixed'] : ['fixed']) {
    for (const retained of [false, true]) {
      const id = `cmd_workjet_project_list_${mode}_${retained ? 'retained' : 'fresh'}`;

      const sample = await page.evaluate(async ({ mode, retained, id }) => {
        const prefix = mode === 'baseline' ? '/baseline' : '';
        const timing = await import(prefix + '/shared/command-bus.js');
        const { createRxDatabase, getCtoxIndexedDbStorage, createQueryDemandLoader,
          createSidecarWithMemoryBackend } = await import('/rxdb/src/index.mjs');
        const { withTimeout } = await import('/shared/async-timeout.js');
        const db = await createRxDatabase({ name: 'isolated-' + id, storage: getCtoxIndexedDbStorage() });
        await db.addCollections({ business_commands: { schema: { version: 0, primaryKey: 'id',
          type: 'object', properties: { id: { type: 'string', maxLength: 160 } }, required: ['id'], additionalProperties: true } } });
        const commands = db.business_commands;
        let requestsStarted = 0;
        let held;
        const loader = createQueryDemandLoader({ collectionName: 'business_commands', schemaVersion: 0,
          storageCollection: commands.storageCollection, sidecar: createSidecarWithMemoryBackend({ databaseName: id }),
          queryGeneration: () => 'isolated-current-peer', readPermissionDigest: () => 'isolated-fixture-reader',
          requestQueryFetch: request => {
            requestsStarted++;
            if (retained && requestsStarted === 1) return new Promise(resolve => { held = resolve; });
            return queryNative(request);
          },
        });
        commands.setDemandLoader(loader);
        if (retained) {
          commands.findOne(id).exec().catch(() => {});
          while (!held) await new Promise(resolve => setTimeout(resolve, 1));
        }
        // Known source-test fixture token, never an installed credential.
        globalThis.CTOX_BUSINESS_OS_SESSION = { capability_token: 'projection-smoke-capability-token',
          capability_expires_at_ms: Date.now() + 3600000 };
        const sync = { async startCollection() { return { state: { demandStatus: { peerConnected: true },
          async pushDocumentsToRemotePeers() {
            return queryNative({ requestId: id + '-receipt-seed', fixtureSeed: [{ id, status: 'completed',
              replication_phase: 'native_observed', execution_phase: 'terminal', terminal_status: 'completed',
              result: { ok: true, collection: 'workjet_projects', count: 0, truncated: false },
              _rev: '1-fixture', _deleted: false, _meta: { lwt: Date.now() }, _attachments: {} }] });
          } } }; } };
        const bus = timing.createCommandBus({ db: { raw: db }, sync });
        const helperSource = await fetch('/timing-helper.mjs').then(response => response.text());
        const helper = new Function('state', 'loadCommandBusModule', 'awaitWorkjetProjectListStep',
          helperSource.replace('export { dispatchWorkjetProjectList };', 'return dispatchWorkjetProjectList;'))(
          { commandBus: bus }, async () => timing,
          (value, deadline) => withTimeout(value, Math.max(1, deadline - Date.now()), { code: 'fixture_command_deadline' }));
        let diagnostic;
        let status; let error;
        const started = performance.now();
        try {
          const receipt = await helper({ id, module: 'ctox', command_type: 'ctox.workjet.project.list',
            payload: { limit: 10 }, client_context: {} },
          { until: 'terminal', timeoutMs: 1000, sync_queue_tasks: false }, Date.now() + 600,
          { onCommandTiming: sample => { diagnostic = sample; } });
          status = receipt.status;
        } catch (failure) { error = failure.code || failure.name; }
        const elapsedMs = performance.now() - started;
        // Release only this fixture's old request and close its database.
        held?.({ documents: [], authoritativeRevision: 'fixture-retired-read' });
        loader.abortAllInFlight('fixture-complete');
        await new Promise(resolve => setTimeout(resolve, 100));
        await db.close(); delete globalThis.CTOX_BUSINESS_OS_SESSION;
        return { mode, retained, elapsedMs, status, error, requestsStarted, diagnostic,
          channel: channel.readyState, databases: (await indexedDB.databases()).map(entry => entry.name) };
      }, { mode, retained, id });
      report.samples.push(sample);
      if (mode === 'baseline' && retained) assert.equal(sample.error, 'fixture_command_deadline');
      else { assert.equal(sample.status, 'completed'); assert.ok(sample.elapsedMs < 600); }
      assert.ok(sample.diagnostic?.stagesMs);
      assert.ok(Object.hasOwn(sample.diagnostic.stagesMs, 'push'));
    }
  }
  await page.evaluate(() => { channel.close(); pc.close(); });
  await nativePage.evaluate(() => { channel.close(); pc.close(); });
  await context.close();
  if (args.includes('--output')) writeFileSync(option('--output'), JSON.stringify(report, null, 2) + '\n');
  console.log(JSON.stringify(report));
} finally {
  await browser?.close();
  await new Promise(resolve => server.close(resolve));
  if (child.exitCode === null && child.signalCode === null) {
    const exited = new Promise(resolve => child.once('exit', resolve));
    send({ kind: 'shutdown' }); child.stdin.end();
    const stopTimer = setTimeout(() => child.kill('SIGTERM'), 2000);
    const killTimer = setTimeout(() => child.kill('SIGKILL'), 5000);
    await exited;
    clearTimeout(stopTimer); clearTimeout(killTimer);
  }
  lines.close();
}
