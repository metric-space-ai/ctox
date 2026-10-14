import assert from 'node:assert/strict';
import { createDemandLoadingTransport } from '../src/demand-loading-transport.mjs';
import { createQueryDemandLoader, createSidecarWithMemoryBackend } from '../src/index.mjs';

const deadline = (ms) => new Promise((_, reject) => setTimeout(
  () => reject(new Error(`strict native query was not admitted within ${ms}ms`)), ms,
));
const waitFor = async (predicate, ms = 1000) => {
  const until = Date.now() + ms;
  while (!predicate() && Date.now() < until) await new Promise(resolve => setTimeout(resolve, 5));
  assert.ok(predicate(), 'expected ordinary window fetches to start');
};
const envelope = (id) => ({
  requestId: id,
  collectionName: 'outbound_lead_generation_leads',
  schemaVersion: 1,
  queryFingerprint: id,
  query: { selector: { id: { $in: ['lead-fixture'] } }, sort: [{ id: 'asc' }], limit: 1, skip: 0 },
  window: { offset: 0, limit: 1 },
});
const peerFor = (started, getTransport, beforeAck = async () => {}) => ({
  connections: new Map([['native', { channel: { readyState: 'open' }, peer: { connectionState: 'connected' } }]]),
  async request(_peerId, method, params) {
    assert.equal(method, 'rxdb.query.fetch');
    const request = params[0];
    started.push(request.requestId);
    await beforeAck(request);
    queueMicrotask(() => getTransport().requestHandlers['rxdb.query.chunk']({
      params: [{
        requestId: request.requestId, sequence: 0,
        documents: request.requestId.startsWith('ordinary-') ? [] : [{ id: 'lead-fixture' }],
        complete: true,
      }],
    }));
    return { ack: true };
  },
});
let windowedStrictMs = null;

// Seven ordinary windows may be waiting on the native peer. The eighth slot
// must stay available for a requireRevision user action on that SAME collection.
{
  let releaseOrdinary;
  const ordinaryAck = new Promise(resolve => { releaseOrdinary = resolve; });
  const started = [];
  let transport;
  const peer = peerFor(started, () => transport, request =>
    request.requestId.startsWith('ordinary-') ? ordinaryAck : undefined);
  transport = createDemandLoadingTransport({ getPeerId: () => 'native' });
  transport.attach(peer);
  const ordinary = Array.from({ length: 8 }, (_, i) =>
    transport.requestQueryFetch(envelope(`ordinary-${i}`)));
  const stored = new Map();
  const storageCollection = {
    databaseName: 'thesen-strict-query-fixture',
    primaryPath: 'id',
    async bulkWrite(rows) {
      for (const row of rows) {
        const document = row?.document || row;
        stored.set(document.id, document);
      }
    },
    async findDocumentsById(ids) {
      return Object.fromEntries(ids.filter(id => stored.has(id)).map(id => [id, stored.get(id)]));
    },
    async queryDocuments(query, { matchesSelector, sortDocuments } = {}) {
      const matches = [...stored.values()].filter(document => matchesSelector(document, query.selector || {}));
      return sortDocuments(matches, query.sort || []);
    },
  };
  const loader = createQueryDemandLoader({
    storageCollection,
    sidecar: createSidecarWithMemoryBackend({ databaseName: storageCollection.databaseName }),
    collectionName: 'outbound_lead_generation_leads',
    schemaVersion: 1,
    queryGeneration: () => 'fixture-generation',
    requestQueryFetch: (request, options) => transport.requestQueryFetch(request, options),
  });
  let strict;
  try {
    await waitFor(() => started.length >= 7);
    const startedAt = performance.now();
    strict = loader.resolveQuery({
      selector: { id: { $in: ['lead-fixture'] } },
      sort: [{ id: 'asc' }], limit: 1, requireRevision: 'fixture:new:full:0',
    });
    const documents = await Promise.race([strict, deadline(1500)]);
    windowedStrictMs = Math.round(performance.now() - startedAt);
    assert.ok(windowedStrictMs < 2000, 'windowed requireRevision read must finish under 2s');
    assert.equal(documents[0]?.id, 'lead-fixture');
    assert.ok(started[7] && !started[7].startsWith('ordinary-'),
      'a strict read must reach native before the queued ordinary window');
  } finally {
    releaseOrdinary();
    await Promise.allSettled([...ordinary, strict].filter(Boolean));
  }
}

// The browser rate bucket can also fill while ordinary windows of this same
// collection queue. The first refill belongs to the strict read, not the FIFO
// tail of a long list of normal Outbound windows.
{
  delete globalThis[Symbol.for('ctox.rxdb.query-stream-state.v1')];
  const started = [];
  let transport;
  const peer = peerFor(started, () => transport);
  transport = createDemandLoadingTransport({ getPeerId: () => 'native' });
  transport.attach(peer);
  const ordinary = Array.from({ length: 90 }, (_, i) =>
    transport.requestQueryFetch(envelope(`ordinary-rate-${i}`)));
  await waitFor(() => started.length === 48);
  const strict = transport.requestQueryFetch(envelope('strict-rate'), { priority: 'strict' });
  await Promise.race([strict, deadline(1600)]);
  assert.equal(started[48], 'strict-rate', 'the first refill token must go to the strict read');
  await Promise.all(ordinary);
}

console.log(`query strict admission smoke OK; windowed requireRevision ${windowedStrictMs}ms`);
