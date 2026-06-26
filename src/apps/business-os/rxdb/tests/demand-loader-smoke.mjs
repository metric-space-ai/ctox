import {
  DEFAULT_WINDOW_LIMIT,
  createQueryDemandLoader,
  createSidecarWithMemoryBackend,
  createV1_5StatusState,
} from '../dist/ctox-rxdb-js.mjs';

function makeFakeStorageCollection() {
  const docs = new Map();
  return {
    databaseName: 'fake',
    async bulkWrite(rows) {
      for (const doc of rows) {
        docs.set(doc.id, doc);
      }
    },
    async allDocuments() {
      return Array.from(docs.values());
    },
    async queryDocuments(query, { matchesSelector, sortDocuments }) {
      let all = Array.from(docs.values()).filter((doc) => matchesSelector(doc, query.selector || {}));
      all = sortDocuments(all, query.sort || []);
      if (query.skip > 0) all = all.slice(query.skip);
      if (Number.isFinite(query.limit)) all = all.slice(0, query.limit);
      return all;
    },
  };
}

let fetchCount = 0;
async function fakeFetch({ query, window }) {
  fetchCount += 1;
  // Simulate real-world WebRTC latency. Without a delay, r1 finishes its job
  // (writing the window as complete) before r2/r3 reach their sidecar check,
  // so they take the cache-hit path instead of the dedup path.
  await new Promise((resolve) => setTimeout(resolve, 5));
  const start = window.offset;
  const end = start + window.limit;
  const docs = [];
  for (let i = start; i < Math.min(end, 5); i += 1) {
    docs.push({ id: `doc-${i}`, status: query.selector?.status ?? 'open', n: i });
  }
  return { documents: docs, authoritativeRevision: 'rev-1' };
}

const status = createV1_5StatusState();
const storageCollection = makeFakeStorageCollection();
const sidecar = createSidecarWithMemoryBackend({ databaseName: 'sidecar' });

const loader = createQueryDemandLoader({
  storageCollection,
  sidecar,
  collectionName: 'business_records',
  schemaVersion: 1,
  requestQueryFetch: fakeFetch,
  status,
});

// First resolve: cache miss, exactly one remote fetch.
const r1 = await loader.resolveQuery({ selector: { status: 'open' } });
assert(r1.length === 5, `first resolve should return 5 docs (got ${r1.length})`);
assert(fetchCount === 1, `first resolve = 1 fetch (got ${fetchCount})`);
assert(status.queryFetchSuccessCount === 1, 'success count incremented');
assert(status.queryFetchInFlight === 0, 'in-flight back to zero');

// Second resolve: cache hit, zero remote fetch.
const r2 = await loader.resolveQuery({ selector: { status: 'open' } });
assert(r2.length === 5, 'second resolve same length');
assert(fetchCount === 1, 'second resolve must hit cache');

let scanCount = 0;
const originalScanQueryWindows = sidecar.backend.scanQueryWindows.bind(sidecar.backend);
sidecar.backend.scanQueryWindows = async () => {
  scanCount += 1;
  throw new Error('invalidateDocumentChange must use sidecar document refs');
};
const invalidated = await loader.invalidateDocumentChange(['doc-0']);
assert(invalidated === 1, `invalidateDocumentChange must invalidate one window (got ${invalidated})`);
assert(scanCount === 0, 'invalidateDocumentChange must not scan all query windows when refs exist');
sidecar.backend.scanQueryWindows = originalScanQueryWindows;

// Concurrent identical resolves: share one in-flight promise.
fetchCount = 0;
status.queryFetchDedupHitCount = 0;
await sidecar.clear();
const [a, b, c] = await Promise.all([
  loader.resolveQuery({ selector: { status: 'open' } }),
  loader.resolveQuery({ selector: { status: 'open' } }),
  loader.resolveQuery({ selector: { status: 'open' } }),
]);
assert(a.length === b.length && b.length === c.length, 'concurrent resolves return same shape');
assert(fetchCount === 1, `concurrent resolves dedup to 1 fetch (got ${fetchCount})`);
assert(status.queryFetchDedupHitCount === 2, `dedup hits = 2 (got ${status.queryFetchDedupHitCount})`);

// Error path: failing fetch increments error counter.
status.queryFetchErrorCount = 0;
await sidecar.clear();
let caught = null;
try {
  const failingLoader = createQueryDemandLoader({
    storageCollection,
    sidecar,
    collectionName: 'business_records',
    schemaVersion: 1,
    requestQueryFetch: async () => {
      throw new Error('peer offline');
    },
    status,
  });
  await failingLoader.resolveQuery({ selector: { other: 'x' } });
} catch (error) {
  caught = error;
}
assert(caught && /peer offline/.test(caught.message), 'error from peer must propagate');
assert(status.queryFetchErrorCount === 1, 'error count incremented');

assert(DEFAULT_WINDOW_LIMIT === 200, 'default window limit constant is 200');

console.log('ctox-rxdb-js demand loader smoke OK');

function assert(condition, message) {
  if (!condition) throw new Error(message);
}
