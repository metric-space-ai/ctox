// Rollback drill: verify that V1.5 activation doesn't disturb the primary
// data path, and that a V1 build (no demand loader) sees byte-identical
// primary data after V1.5 has been used.
//
// This is the in-Node version of the production rollback drill that the
// release process runs against a real browser. The browser version simply
// closes the sidecar IDB database and re-opens the primary; we model that
// here by directly verifying the primary storage is unchanged.

import {
  createMemoryMetaBackend,
  createQueryDemandLoader,
  createSidecarWithMemoryBackend,
  createV1_5StatusState,
} from '../dist/ctox-rxdb-js.mjs';

function makePrimary() {
  const records = new Map();
  return {
    databaseName: 'primary',
    async bulkWrite(rows) { for (const r of rows) records.set(r.id, r); },
    async queryDocuments(query, { matchesSelector, sortDocuments }) {
      let all = Array.from(records.values()).filter((d) => matchesSelector(d, query.selector || {}));
      all = sortDocuments(all, query.sort || []);
      if (query.skip > 0) all = all.slice(query.skip);
      if (Number.isFinite(query.limit)) all = all.slice(0, query.limit);
      return all;
    },
    snapshot() {
      // Order-independent shallow JSON snapshot for parity checks.
      return JSON.stringify(
        Array.from(records.entries()).sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0),
      );
    },
  };
}

const primary = makePrimary();

// === Phase A: V1 build runs, seeds primary. ===
await primary.bulkWrite([
  { id: 'v1-a', module: 'outbound', _meta: { lwt: 1 } },
  { id: 'v1-b', module: 'outbound', _meta: { lwt: 2 } },
  { id: 'v1-c', module: 'outbound', _meta: { lwt: 3 } },
]);
const v1Snapshot = primary.snapshot();

// === Phase B: V1.5 build activates. queryDemandLoadingEnabled=true,
//     loader fetches remote data, materializes into primary. ===
const sidecar = createSidecarWithMemoryBackend({ databaseName: 'sidecar' });
const status = createV1_5StatusState();
status.queryDemandLoadingEnabled = true;
status.queryDemandLoadingActive = true;
status.peerCapabilityQueryFetchV1 = true;

const loader = createQueryDemandLoader({
  storageCollection: primary,
  sidecar,
  collectionName: 'business_records',
  schemaVersion: 1,
  status,
  requestQueryFetch: async () => ({
    documents: [
      { id: 'v15-a', module: 'outbound', _meta: { lwt: 10 } },
      { id: 'v15-b', module: 'outbound', _meta: { lwt: 11 } },
    ],
  }),
});
await loader.resolveQuery({ selector: { module: 'outbound' } });

// === Phase C: Verify V1 records are untouched ===
// The original v1-a, v1-b, v1-c records must still be in the primary store
// with their exact original fields and _meta.lwt. New v15 records are added,
// but never replace or mutate V1 entries.
const recordsAfterV15 = await primary.queryDocuments(
  { selector: {}, sort: [], limit: 1000 },
  { matchesSelector: () => true, sortDocuments: (d) => d },
);
const v1Surviving = recordsAfterV15.filter((r) => r.id.startsWith('v1-'));
assert(v1Surviving.length === 3, `V1 records intact (got ${v1Surviving.length})`);
for (const original of [
  { id: 'v1-a', module: 'outbound', _meta: { lwt: 1 } },
  { id: 'v1-b', module: 'outbound', _meta: { lwt: 2 } },
  { id: 'v1-c', module: 'outbound', _meta: { lwt: 3 } },
]) {
  const live = v1Surviving.find((r) => r.id === original.id);
  assert(JSON.stringify(live) === JSON.stringify(original),
    `V1 record ${original.id} byte-identical after V1.5 use`);
}

// === Phase D: Toggle V1.5 off — sidecar dropped, primary unchanged ===
await sidecar.clear();
status.queryDemandLoadingEnabled = false;
status.queryDemandLoadingActive = false;

// A V1-style query runs against the primary, ignoring the sidecar.
const v1ReadsAfterToggle = await primary.queryDocuments(
  { selector: { id: 'v1-a' }, sort: [], limit: 1 },
  { matchesSelector: (d, s) => d.id === s.id, sortDocuments: (d) => d },
);
assert(v1ReadsAfterToggle.length === 1, 'V1 read works after sidecar cleared');
assert(v1ReadsAfterToggle[0]._meta.lwt === 1, 'V1 record still has original LWT');

// === Phase E: Sidecar is empty after clear() ===
const stats = await sidecar.getCacheStats();
assert(stats.estimatedBytes === 0 || stats.estimatedBytes === undefined, 'sidecar cleared');

// === Phase F: Re-enabling V1.5 from cold sidecar works ===
status.queryDemandLoadingEnabled = true;
status.queryDemandLoadingActive = true;
let recovered = false;
const recoveryLoader = createQueryDemandLoader({
  storageCollection: primary,
  sidecar,
  collectionName: 'business_records',
  schemaVersion: 1,
  status,
  requestQueryFetch: async () => {
    recovered = true;
    return { documents: [{ id: 'v15-recovered', module: 'outbound' }] };
  },
});
await recoveryLoader.resolveQuery({ selector: { module: 'outbound' } });
assert(recovered, 'remote fetch re-runs when sidecar is cold');

console.log('ctox-rxdb-js rollback drill smoke OK');
console.log('   V1 records: byte-identical before/after V1.5 use');
console.log('   Sidecar.clear() preserves primary');
console.log('   V1.5 toggle off → V1-only reads succeed');
console.log('   V1.5 re-enable → cold sidecar triggers fresh fetch');

function assert(c, m) { if (!c) throw new Error(m); }
