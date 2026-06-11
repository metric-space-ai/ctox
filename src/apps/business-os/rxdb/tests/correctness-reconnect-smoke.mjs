import {
  createQueryDemandLoader,
  createSidecarWithMemoryBackend,
  createV1_5StatusState,
} from '../dist/ctox-rxdb-js.mjs';

function makeStorageCollection() {
  const docs = new Map();
  return {
    databaseName: 'recon',
    async bulkWrite(rows) { for (const r of rows) docs.set(r.id, r); },
    async queryDocuments(query, { matchesSelector, sortDocuments }) {
      let all = Array.from(docs.values()).filter((doc) => matchesSelector(doc, query.selector || {}));
      all = sortDocuments(all, query.sort || []);
      if (query.skip > 0) all = all.slice(query.skip);
      if (Number.isFinite(query.limit)) all = all.slice(0, query.limit);
      return all;
    },
  };
}

// === Invalidation test ===
{
  const sidecar = createSidecarWithMemoryBackend({ databaseName: 'reset-inv' });
  const storage = makeStorageCollection();
  const status = createV1_5StatusState();
  const loader = createQueryDemandLoader({
    storageCollection: storage,
    sidecar,
    collectionName: 'business_records',
    schemaVersion: 1,
    requestQueryFetch: async ({ query }) => ({
      documents: [
        { id: 'a', n: 1, status: query.selector.status },
        { id: 'b', n: 2, status: query.selector.status },
        { id: 'c', n: 3, status: query.selector.status },
      ],
    }),
    status,
  });

  await loader.resolveQuery({ selector: { status: 'open' } });
  let fetchedAgain = 0;
  const loaderWithRefresh = createQueryDemandLoader({
    storageCollection: storage,
    sidecar,
    collectionName: 'business_records',
    schemaVersion: 1,
    requestQueryFetch: async () => {
      fetchedAgain += 1;
      return { documents: [{ id: 'a' }, { id: 'b' }, { id: 'c', changed: true }] };
    },
    status,
  });

  // Invalidate window because doc 'c' changed remotely.
  const invalidated = await loaderWithRefresh.invalidateDocumentChange(['c']);
  assert(invalidated === 1, `invalidate should mark 1 window incomplete (got ${invalidated})`);

  await loaderWithRefresh.resolveQuery({ selector: { status: 'open' } });
  // Contract evolution (stale-while-revalidate, see
  // stale-while-revalidate-smoke.mjs): an invalidated EVER-complete window
  // serves local results immediately and revalidates in the BACKGROUND. The
  // fetch still MUST happen — give the async job a beat to run.
  await new Promise((resolve) => setTimeout(resolve, 25));
  assert(fetchedAgain === 1, 'invalidated window must trigger a fresh remote fetch (background revalidation)');
}

// === Reconnect-abort test ===
{
  const sidecar = createSidecarWithMemoryBackend({ databaseName: 'reset-recon' });
  const storage = makeStorageCollection();
  let cancelled = [];
  const slowFetch = () => new Promise(() => {}); // hangs forever
  const loader = createQueryDemandLoader({
    storageCollection: storage,
    sidecar,
    collectionName: 'business_records',
    schemaVersion: 1,
    requestQueryFetch: slowFetch,
    requestCancel: async ({ requestId, reason }) => { cancelled.push({ requestId, reason }); },
  });

  loader.resolveQuery({ selector: { status: 'pending' } }).catch(() => {});
  await new Promise((r) => setTimeout(r, 5));
  assert(loader.inflightSize() === 1, 'expected one in-flight before abort');
  await loader.abortAllInFlight('reconnect');
  assert(loader.inflightSize() === 0, 'abort must drop all in-flight');
  assert(cancelled.length === 1, 'requestCancel must be called once');
  assert(cancelled[0].reason === 'reconnect', 'reason propagated');

  // Sidecar must NOT have a complete window (the aborted fetch didn't finish).
  const all = await sidecar.backend.scanQueryWindows();
  const completed = all.filter((w) => w.complete);
  assert(completed.length === 0, 'no completed window after abort');
}

// === Multi-tab leader test ===
{
  const sidecar = createSidecarWithMemoryBackend({ databaseName: 'reset-mt' });
  const storage = makeStorageCollection();
  const claims = new Set();
  const broker = {
    async claim(key) {
      if (claims.has(key)) return false;
      claims.add(key);
      return true;
    },
    async release(key) { claims.delete(key); },
  };
  const loader = createQueryDemandLoader({
    storageCollection: storage,
    sidecar,
    collectionName: 'business_records',
    schemaVersion: 1,
    requestQueryFetch: async () => ({ documents: [] }),
    multiTabBroker: broker,
  });
  const ok = await loader.leaderClaim('window-a');
  assert(ok === true, 'first tab claims successfully');
  const dup = await loader.leaderClaim('window-a');
  assert(dup === false, 'second tab denied');
  await loader.leaderRelease('window-a');
  const re = await loader.leaderClaim('window-a');
  assert(re === true, 'after release, re-claim allowed');
}

console.log('ctox-rxdb-js correctness/reconnect/multi-tab smoke OK');

function assert(c, m) { if (!c) throw new Error(m); }
