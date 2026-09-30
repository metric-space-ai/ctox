// Real browser cache and visible warning; only the native RPC boundary is simulated.
import assert from 'node:assert/strict';
import http from 'node:http';
import { readFileSync, existsSync } from 'node:fs';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { chromium } from '../../node_modules/playwright/index.mjs';

const appRoot = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const assets = new Map([
  ['/bundle.mjs', readFileSync(resolve(appRoot, 'rxdb/dist/ctox-rxdb-js.mjs'))],
  ['/collection-freshness.js', readFileSync(resolve(appRoot, 'shared/collection-freshness.js'))],
  ['/sync-contract.js', readFileSync(resolve(appRoot, 'shared/sync-contract.js'))],
]);
const server = http.createServer((request, response) => {
  const pathname = new URL(request.url, 'http://localhost').pathname;
  if (pathname === '/') {
    response.setHeader('content-type', 'text/html');
    response.end('<!doctype html><span data-warning role="status" hidden></span>');
    return;
  }
  const asset = assets.get(pathname);
  response.writeHead(asset ? 200 : 404, { 'content-type': 'text/javascript' });
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
    const { renderCollectionFreshnessWarning } = await import('/collection-freshness.js');
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
    const warning = document.querySelector('[data-warning]');
    const subscription = state.transportStatus$.subscribe((status) => renderCollectionFreshnessWarning(warning, {
      collections: [name], diagnostics: { mode: 'webrtc', collections: { [name]: { frameTransport: status } } }, language: 'de',
    }));
    try {
      await state.pullFromRemotePeers();
      check(warning.hidden, 'a completed current empty pull hides the warning');
      const historicalCompletion = state.firstPullCompletedAtMs;
      let releaseResponse;
      holdResponse = new Promise((resolve) => { releaseResponse = resolve; });
      const newer = { ...historical, campaign: 'current-campaign', _rev: '2-native', _meta: { ctoxHlc: formatHybridLogicalClock({ physicalMs: Date.now() + 1000, logical: 0, nodeId: 'fixture-native' }) } };
      responses = [{ documents: [newer], checkpoint: { id: newer.id, lwt: Date.now() + 1000 } }, { documents: [], checkpoint: { id: newer.id, lwt: Date.now() + 1000 } }];
      const refreshing = state.pullFromRemotePeers();
      check(!warning.hidden && warning.textContent.includes('noch nicht bestätigt'), 'cached data is visibly unconfirmed while pull is held');
      check((await records.getStoredRecord(historical.id)).doc.campaign === 'cached-campaign', 'old cache stays available during catch-up');
      releaseResponse(); holdResponse = null;
      await refreshing;
      check((await records.getStoredRecord(historical.id)).doc.campaign === 'current-campaign', 'the native update was committed to real IndexedDB');
      check(warning.hidden, 'only the drained current pull clears the warning');
      check(state.firstPullCompletedAtMs === historicalCompletion, 'history marker is preserved');
      // A response from a retired connection cannot confirm a replacement.
      let releaseLate;
      holdResponse = new Promise((resolve) => { releaseLate = resolve; });
      responses = [{ documents: [], checkpoint: { id: newer.id, lwt: Date.now() + 1000 } }];
      const late = state.pullFromPeer('native');
      open = false; state.removePeer('native');
      check(!warning.hidden && warning.textContent.startsWith('Offline:'), 'disconnect keeps a visible offline warning');
      releaseLate(); holdResponse = null;
      await late;
      open = true; state.publishTransportStatus();
      check(!warning.hidden && state.pullFresh === false, 'late old-generation drain cannot confirm the reopened connection');
      check((await records.getStoredRecord(historical.id)).doc.campaign === 'current-campaign', 'disconnect and warning never wipe browser cache');
      renderCollectionFreshnessWarning(warning, { collections: [], diagnostics: { mode: 'webrtc' } });
      check(warning.hidden, 'leaving data views clears their warning');
      return { cachePreserved: true, catchupVisible: true, currentPullConfirmed: true, offlineVisible: true, lateGenerationRejected: true };
    } finally {
      subscription.unsubscribe?.();
      await state.cancel();
      await storage.close();
    }
  });
  assert.deepEqual(result, { cachePreserved: true, catchupVisible: true, currentPullConfirmed: true, offlineVisible: true, lateGenerationRejected: true });
  console.log('current pull freshness browser smoke OK');
} finally {
  await browser?.close();
  await new Promise((resolve) => server.close(resolve));
}
