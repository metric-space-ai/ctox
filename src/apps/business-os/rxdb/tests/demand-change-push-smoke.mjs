// Pushed remote writes on demand-only collections must reach subscribers as
// named changes (id + revision), and a window refreshed by our own read must
// carry the revisions it served. Without this, demand-only apps re-paged whole
// collections on every write (thesen 07.10.2026: Outbound lead list re-read
// every second while pushed master changes were dropped).
import assert from 'node:assert/strict';

const source = process.argv.includes('--source');
const mod = await import(source ? '../src/index.mjs' : '../dist/ctox-rxdb-js.mjs');
const { createQueryDemandLoader, createSidecarWithMemoryBackend } = mod;
const internals = (await import(source ? '../src/replication-webrtc.mjs' : '../dist/ctox-rxdb-js.mjs')).replicationWebRtcTestInternals;

function memoryStorage() {
  const docs = new Map();
  return {
    docs, databaseName: 'push', primaryPath: 'id',
    async bulkWrite(rows) { for (const row of rows) docs.set(row.id, { ...row }); return { success: {}, error: {} }; },
    async queryDocuments() { return [...docs.values()]; },
    async findDocumentsById(ids) { return Object.fromEntries(ids.filter(id => docs.has(id)).map(id => [id, docs.get(id)])); },
  };
}

// 1. A pushed remote write names its rows with revisions, even when no cached
//    window references them yet (a new row must reach a list that never saw it).
{
  const notes = [];
  const loader = createQueryDemandLoader({
    storageCollection: memoryStorage(),
    sidecar: createSidecarWithMemoryBackend({ databaseName: 'push-1' }),
    collectionName: 'outbound_lead_generation_leads',
    schemaVersion: 0,
    requestQueryFetch: async () => ({ documents: [], authoritativeRevision: 'r0' }),
    onQueryWindowChanged: change => notes.push(change),
  });
  await loader.invalidateDocuments([
    { id: 'lead_new', _rev: '1-a', status: 'x' },
    { id: 'lead_gone', _rev: '3-b', _deleted: true },
  ]);
  assert.equal(notes.length, 1, 'one notification per pushed batch');
  assert.deepEqual(notes[0].changes, [
    { id: 'lead_new', rev: '1-a', deleted: false },
    { id: 'lead_gone', rev: '3-b', deleted: true },
  ]);
}

// 2. A demand-only replication state (no pull stream) turns a pushed master
//    change into a demand-cache invalidation instead of dropping it.
{
  const State = internals.getReplicationStateClass();
  const invalidated = [];
  let pulled = 0;
  let resynced = 0;
  const context = {
    cancelled: false,
    pull: null,
    masterChange$: { next() {} },
    invalidateDemandCacheForRemoteWrite: async documents => { invalidated.push(documents); },
    pullFromRemotePeers: async () => { pulled += 1; },
    collection: { notifyQueryWindowChange: () => { resynced += 1; } },
  };
  State.prototype.onMasterChange.call(context, { result: { documents: [{ id: 'lead_1', _rev: '2-c' }], checkpoint: {} } });
  State.prototype.onMasterChange.call(context, { result: 'RESYNC' });
  await new Promise(resolve => setImmediate(resolve));
  assert.deepEqual(invalidated, [[{ id: 'lead_1', _rev: '2-c' }]], 'pushed documents invalidate the demand cache');
  assert.equal(resynced, 1, 'a RESYNC push invalidates without naming rows');
  assert.equal(pulled, 0, 'no pull for a demand-only collection');
}

// 3. An id-only invalidation of a newly inserted row must notify even when no
// cached window contained it. Exercise both the indexed sidecar and scan path.
for (const indexed of [true, false]) {
  const notes = [];
  const sidecar = createSidecarWithMemoryBackend({databaseName:'id-only-' + indexed});
  if (!indexed) sidecar.invalidateQueryWindowsForDocuments = undefined;
  const loader = createQueryDemandLoader({
    storageCollection:memoryStorage(), sidecar, collectionName:'leads', schemaVersion:0,
    requestQueryFetch:async () => { throw new Error('invalidation must not fetch rows'); },
    onQueryWindowChanged:change => notes.push(change),
  });
  assert.equal(await loader.invalidateDocumentChange(['new-row']), 0);
  assert.deepEqual(notes, [{changes:[{id:'new-row'}]}], 'new id-only rows reach subscribers without a cached window');
  await loader.invalidateDocumentChange([]);
  assert.equal(notes.length, 1, 'no changed ids means no invented notification');
}

console.log('demand change push smoke PASS: known row ids survive document and id-only invalidations');
