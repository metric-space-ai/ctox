// Incident reproduction: real browser IndexedDB and replication state machine;
// only the native RPC boundary is simulated. No live tenant or signaling.
import http from 'node:http';
import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const playwrightModule = process.env.PLAYWRIGHT_MODULE_PATH
  ? pathToFileURL(resolve(process.env.PLAYWRIGHT_MODULE_PATH, 'index.mjs')).href
  : '../../node_modules/playwright/index.mjs';
const { chromium } = await import(playwrightModule);
const testDir = dirname(fileURLToPath(import.meta.url));
const bundle = readFileSync(resolve(testDir, '../dist/ctox-rxdb-js.mjs'));
const server = http.createServer((request, response) => {
  response.writeHead(200, { 'content-type': request.url === '/bundle.mjs' ? 'text/javascript' : 'text/html' });
  response.end(request.url === '/bundle.mjs' ? bundle : '<!doctype html><title>Initial sync stale browser probe</title>');
});
await new Promise((ready) => server.listen(0, '127.0.0.1', ready));
const systemChrome = '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
const browser = await chromium.launch({
  headless: true,
  ...(existsSync(systemChrome) ? { executablePath: systemChrome } : {}),
});
try {
  const page = await browser.newPage();
  await page.goto(`http://127.0.0.1:${server.address().port}/`);
  const results = await page.evaluate(async () => {
    const { openCtoxIndexedDbStorage, replicationWebRtcTestInternals, formatHybridLogicalClock } = await import('/bundle.mjs');
    const State = replicationWebRtcTestInternals.getReplicationStateClass();
    const assert = (condition, message) => { if (!condition) throw new Error(message); };
    const results = [];
    for (const scenario of ['newer-on-pull', 'deleted-on-pull', 'newer-on-push', 'deleted-on-push', 'permanent-revision-conflict']) {
      const storage = await openCtoxIndexedDbStorage({ databaseName: `initial-probe-${scenario}-${Date.now()}` });
      const name = 'outbound_lead_generation_leads';
      const schema = { version: 1, primaryKey: 'id', type: 'object', properties: { id: { type: 'string', maxLength: 100 } } };
      const records = storage.collection(name, { schema });
      const local = { id: 'lead-a', label: 'browser-local', updated_at_ms: 1000, _rev: '1-browser' };
      await records.bulkWrite([local]);
      const seeded = await records.getStoredRecord(local.id);
      assert(seeded.pushable === 1, 'fixture must begin with a real unsynced IndexedDB row');
      const permanent = scenario === 'permanent-revision-conflict';
      const master = {
        ...local, label: 'server-newer', _rev: '2-native', updated_at_ms: Date.now() + 1000,
        _deleted: scenario.startsWith('deleted'),
        ...(!permanent ? { _meta: { ctoxHlc: formatHybridLogicalClock({ physicalMs: Date.now() + 1000, logical: 0, nodeId: 'native-probe' }) } } : {}),
      };
      const state = new State({
        collection: { name, schema: { primaryPath: 'id' }, storageCollection: records },
        topic: `initial-probe-${scenario}`, pull: { batchSize: 10 }, push: { batchSize: 10 }, retryTime: 5000,
      });
      const errors = [];
      state.error$.subscribe((error) => errors.push(String(error?.message || error)));
      const initial = state.awaitInitialReplication(); // subscribe BEFORE peer-ready, like the shell
      initial.catch(() => {});
      let pulls = 0;
      let pushes = 0;
      let conflictsReturned = 0;
      const trace = [];
      const peerId = 'native-probe';
      state.openPeerIds = () => [peerId];
      state.shared = {
        getTransportStatus: () => ({ status: 'connected' }),
        openSharedPeerIds: () => [peerId],
        unregister() {},
        peer: { async request(_peer, method, [value]) {
          trace.push(method);
          if (method === 'masterChangesSince') {
            pulls += 1;
            // Push scenarios model a native update immediately after the pull
            // snapshot; the stale local row MUST reach masterWrite.
            return { documents: scenario.endsWith('on-pull') && pulls === 1 ? [master] : [], checkpoint: { id: master.id, lwt: 2000 } };
          }
          assert(method === 'masterWrite', `unexpected RPC ${method}`);
          pushes += 1;
          assert(value[0].newDocumentState.id === local.id, 'wrong pushed document');
          assert(value[0].newDocumentState.label === local.label, 'must push the seeded local write');
          conflictsReturned += 1;
          return [master]; // native rejects the write and returns its current revision
        } },
      };
      const started = performance.now();
      let deadline;
      try {
        await Promise.race([
          Promise.all([initial, state.onPeerReady(peerId, {
            peerSession: { role: 'ctox_instance', sessionId: 'native-probe-session' },
            checkpoint: { state: 'ready', epoch: 'native-probe-epoch' },
            collection: { name, schemaHash: 'probe-schema' },
          }, false)]),
          new Promise((_, reject) => { deadline = setTimeout(() => reject(new Error(`initial replication stuck: ${scenario}; trace=${trace}; errors=${errors}`)), 3000); }),
        ]);
        const stored = await records.getStoredRecord(local.id);
        if (permanent) {
          assert(pushes === 3, `permanent conflict must stop after three attempts, got ${pushes}`);
          assert(errors.some((error) => error.includes('masterWrite conflicts remained')), 'permanent rejection must emit an error');
          assert(state.pushRetryTimer, 'permanent revision conflict must schedule a retry');
          assert(!state.pushCheckpointsByPeer.get(peerId), 'rejected batch must not advance the push checkpoint');
          assert(stored.pushable === 1, 'rejected local write must remain recoverable');
        } else {
          assert(pushes === (scenario.endsWith('on-push') ? 1 : 0), `unexpected pushes for ${scenario}: ${pushes}`);
          assert(stored.pushable === 0, 'accepted server state must be non-pushable');
          assert(stored.doc._rev === master._rev, 'browser must retain the authoritative server revision');
          assert(Boolean(stored.doc._deleted) === Boolean(master._deleted), 'browser must converge to server deletion state');
          assert(errors.length === 0, `unexpected sync error: ${errors}`);
          assert(!state.pushRetryTimer, 'resolved conflict must not arm retry');
          assert(state.pushCheckpointsByPeer.has(peerId), 'completed push scan must record its checkpoint, including null for an empty scan');
        }
        results.push({ scenario, initial: 'complete', elapsedMs: Math.round(performance.now() - started), pulls, pushes, conflictsReturned, pushable: stored.pushable, deleted: Boolean(stored.doc._deleted), errors });
      } finally {
        clearTimeout(deadline);
        await state.cancel();
        storage.close();
      }
    }
    return results;
  });
  for (const result of results) console.log(JSON.stringify(result));
  console.log('ctox-rxdb initial stale browser probe OK');
} finally {
  await browser.close();
  server.close();
}
