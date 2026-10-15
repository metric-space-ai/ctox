// Real IndexedDB cache and authoritative pull freshness. Presentation belongs
// to the V4 shell; this regression only simulates the native RPC boundary.
import assert from 'node:assert/strict';
import http from 'node:http';
import { readFileSync, existsSync } from 'node:fs';
import { resolve, dirname } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
const playwrightModule = process.env.PLAYWRIGHT_MODULE_PATH
  ? pathToFileURL(resolve(process.env.PLAYWRIGHT_MODULE_PATH, 'index.mjs')).href
  : '../../node_modules/playwright/index.mjs';
const { chromium } = await import(playwrightModule);

const appRoot = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const assets = new Map([
  ['/bundle.mjs', readFileSync(resolve(appRoot, 'rxdb/dist/ctox-rxdb-js.mjs'))],

]);
const server = http.createServer((request, response) => {
  const pathname = new URL(request.url, 'http://localhost').pathname;
  if (pathname === '/') {
    response.setHeader('content-type', 'text/html');
    response.end('<!doctype html><meta charset="utf-8"><title>IndexedDB pull contract</title>');
    return;
  }
  const asset = assets.get(pathname);
  response.writeHead(asset ? 200 : 404, { 'content-type': pathname.endsWith('.css') ? 'text/css' : 'text/javascript' });
  response.end(asset || '');
});
await new Promise((ready) => server.listen(0, '127.0.0.1', ready));
const chrome = '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome';
let browser;
try {
  browser = await chromium.launch({ headless: true, ...(existsSync(chrome) ? { executablePath: chrome } : {}) });
  const page = await browser.newPage();
  await page.goto(`http://127.0.0.1:${server.address().port}/`);
  const result = await page.evaluate(async () => {
    const { openCtoxIndexedDbStorage, replicationWebRtcTestInternals, formatHybridLogicalClock } = await import('/bundle.mjs');

    const State = replicationWebRtcTestInternals.getReplicationStateClass();
    const check = (condition, message) => { if (!condition) throw new Error(message); };
    const storage = await openCtoxIndexedDbStorage({ databaseName: `freshness-browser-${Date.now()}` });
    const name = 'outbound_lead_generation_leads';
    const schema = { version: 1, primaryKey: 'id', type: 'object', properties: { id: { type: 'string', maxLength: 100 } } };
    const records = storage.collection(name, { schema });
    const historical = { id: 'fixture-lead', campaign: 'cached-campaign', _rev: '1-native', _meta: { lwt: 1000 } };
    await records.bulkWrite([historical], { replicationOrigin: { role: 'ctox_instance', sessionId: 'fixture-native' } });
    const state = new State({ collection: { name, schema: { primaryPath: 'id' }, storageCollection: records }, topic: 'current-freshness-fixture', pull: { batchSize: 5 }, retryTime: 5000 });
    state.error$.subscribe(() => {});
    state.initialReplication?.catch?.(() => {});
    state.schemaHashValue = 'fixture-schema';
    const protocol = { storageGeneration: 'native-generation', checkpoint: { epoch: 'native-epoch' }, peerSession: { sessionId: 'fixture-native', role: 'ctox_instance' }, collection: { name, schemaHash: 'fixture-schema' } };
    let open = true;
    check(state.periodicPullIntervalMs() === 60_000, 'active data collections revalidate even if no master-change hint arrives');
    let responses = [{ documents: [], checkpoint: { id: historical.id, lwt: 1000 } }];
    let holdResponse = null;
    state.remoteProtocolForPeer = () => protocol;
    state.openPeerIds = () => open ? ['native'] : [];
    state.shared = {
      openSharedPeerIds: () => open ? ['native'] : [],
      isPeerOpen: () => open,
      getTransportStatus: () => ({}),
      unregister() {},
      peer: { async request() { if (holdResponse) await holdResponse; return { ...responses.shift(), peerId: undefined }; } },
    };
    state.peerStates$.next(new Map([['native', { peerId: 'native', remoteProtocol: protocol }]]));
    try {
      await state.pullFromRemotePeers();
      check(state.pullFresh === true, 'a completed current empty pull confirms freshness');
      const historicalCompletion = state.firstPullCompletedAtMs;
      let releaseResponse;
      holdResponse = new Promise((resolve) => { releaseResponse = resolve; });
      // An ordinary empty poll does not flash a warning every second for
      // the command control plane; only evidence of a gap invalidates it.
      responses = [{ documents: [], checkpoint: { id: historical.id, lwt: 1000 } }];
      const quiet = state.pullFromRemotePeers({ revalidate: true });
      check(state.pullFresh === true, 'quiet revalidation preserves current confirmation');
      releaseResponse(); holdResponse = null; await quiet;
      holdResponse = new Promise((resolve) => { releaseResponse = resolve; });
      let releaseWrite;
      let enteredWrite;
      const writeEntered = new Promise((resolve) => { enteredWrite = resolve; });
      const writeGate = new Promise((resolve) => { releaseWrite = resolve; });
      const originalBulkWrite = records.bulkWrite.bind(records);
      records.bulkWrite = async (...args) => {
        enteredWrite();
        await writeGate;
        return originalBulkWrite(...args);
      };
      const newer = { ...historical, campaign: 'current-campaign', _rev: '2-native', _meta: { ctoxHlc: formatHybridLogicalClock({ physicalMs: Date.now() + 1000, logical: 0, nodeId: 'fixture-native' }) } };
      responses = [{ documents: [newer], checkpoint: { id: newer.id, lwt: Date.now() + 1000 } }, { documents: [], checkpoint: { id: newer.id, lwt: Date.now() + 1000 } }];
      const refreshing = state.pullFromRemotePeers({ revalidate: true });
      releaseResponse(); holdResponse = null;
      await writeEntered;
      check(state.pullFresh === false, 'cached data is unconfirmed until the changed pull commits');
      check((await records.getStoredRecord(historical.id)).doc.campaign === 'cached-campaign', 'old cache stays available during catch-up');
      releaseWrite();
      await refreshing;
      records.bulkWrite = originalBulkWrite;
      check((await records.getStoredRecord(historical.id)).doc.campaign === 'current-campaign', 'the native update was committed to real IndexedDB');
      check(state.pullFresh === true, 'only the drained current pull confirms freshness');
      check(state.firstPullCompletedAtMs === historicalCompletion, 'history marker is preserved');
      // A response from a retired connection cannot confirm a replacement.
      let releaseLate;
      holdResponse = new Promise((resolve) => { releaseLate = resolve; });
      responses = [{ documents: [], checkpoint: { id: newer.id, lwt: Date.now() + 1000 } }];
      const late = state.pullFromPeer('native');
      open = false; state.removePeer('native');
      check(state.pullFresh === false, 'disconnect invalidates freshness');
      releaseLate(); holdResponse = null;
      await late;
      open = true; state.publishTransportStatus();
      check(state.pullFresh === false, 'late old-generation drain cannot confirm the reopened connection');
      check((await records.getStoredRecord(historical.id)).doc.campaign === 'current-campaign', 'disconnect and warning never wipe browser cache');
      return { cachePreserved: true, catchupUnconfirmed: true, currentPullConfirmed: true, disconnectInvalidates: true, lateGenerationRejected: true };
    } finally {
      await state.cancel();
      await storage.close();
    }
  });
  assert.deepEqual(result, { cachePreserved: true, catchupUnconfirmed: true, currentPullConfirmed: true, disconnectInvalidates: true, lateGenerationRejected: true });
  console.log('current pull freshness browser smoke OK');
} finally {
  await browser?.close();
  await new Promise((resolve) => server.close(resolve));
}
