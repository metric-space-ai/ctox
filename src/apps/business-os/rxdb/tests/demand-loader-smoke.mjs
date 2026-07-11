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

// Demand-only command history stays bounded, but lifecycle projections must
// not become permanent cache hits after the first pending result.
{
  let commandNow = 1_000;
  let commandFetches = 0;
  const commandStorage = makeFakeStorageCollection();
  const commandSidecar = createSidecarWithMemoryBackend({
    databaseName: 'command-status-sidecar',
    clock: () => commandNow,
  });
  const commandLoader = createQueryDemandLoader({
    storageCollection: commandStorage,
    sidecar: commandSidecar,
    collectionName: 'business_commands',
    schemaVersion: 1,
    clock: () => commandNow,
    requestQueryFetch: async () => {
      commandFetches += 1;
      return {
        documents: [{
          id: 'cmd-status-1',
          command_id: 'cmd-status-1',
          status: commandFetches === 1 ? 'pending_sync' : 'accepted',
        }],
        authoritativeRevision: `command-rev-${commandFetches}`,
      };
    },
  });
  const pending = await commandLoader.resolveQuery({ selector: { id: 'cmd-status-1' }, limit: 1 });
  assert(pending[0]?.status === 'pending_sync', 'first command query materializes pending state');
  await commandLoader.resolveQuery({ selector: { id: 'cmd-status-1' }, limit: 1 });
  assert(commandFetches === 1, 'fresh command status window stays local within its freshness budget');
  commandNow += 1_001;
  const accepted = await commandLoader.resolveQuery({ selector: { id: 'cmd-status-1' }, limit: 1 });
  assert(commandFetches === 2, 'stale command status window is revalidated over WebRTC');
  assert(accepted[0]?.status === 'accepted', 'revalidation materializes the native command lifecycle');
}

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

// Broker regression: local in-flight dedup must be installed BEFORE leader
// election. Otherwise the second same-tab query calls claim() while the first
// owns it, waits for an event this exact channel object cannot receive from
// itself, and eventually throws "Timed out waiting for multi-tab query owner".
{
  const brokerSidecar = createSidecarWithMemoryBackend({ databaseName: 'broker-sidecar' });
  const brokerStorage = makeFakeStorageCollection();
  let brokerClaims = 0;
  let brokerFetches = 0;
  const brokerLoader = createQueryDemandLoader({
    storageCollection: brokerStorage,
    sidecar: brokerSidecar,
    collectionName: 'business_commands',
    schemaVersion: 1,
    requestQueryFetch: async () => {
      brokerFetches += 1;
      await new Promise((resolve) => setTimeout(resolve, 20));
      return { documents: [{ id: 'cmd-1', status: 'pending' }] };
    },
    multiTabBroker: {
      closed: false,
      async claim() { brokerClaims += 1; return brokerClaims === 1; },
      async release() {},
      async waitForRemote() { throw new Error('same-tab duplicate must not wait on the broker'); },
    },
  });
  const [first, second] = await Promise.all([
    brokerLoader.resolveQuery({ selector: { status: 'pending' } }),
    brokerLoader.resolveQuery({ selector: { status: 'pending' } }),
  ]);
  assert(first.length === 1 && second.length === 1, 'broker-dedup queries both resolve');
  assert(brokerClaims === 1, `same-tab duplicate performs one broker claim (got ${brokerClaims})`);
  assert(brokerFetches === 1, `same-tab duplicate performs one remote fetch (got ${brokerFetches})`);
}

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
