// CONTRACT: demand-loader stale-while-revalidate (perf-critical for module
// load times in Business OS).
//
// A query window that was EVER complete has its member documents in the
// primary store, and replication keeps those documents fresh — an
// invalidation only means the window MEMBERSHIP may have changed. The
// loader therefore serves local results immediately and revalidates in the
// background. Strict await semantics remain for (a) windows that were never
// complete (cold start — the data may not exist locally at all) and (b)
// callers passing `requireRevision` (explicit consistency demand).
//
// Also pinned: the reconnect-abort path must NOT tombstone members of
// ever-complete windows — those documents are replicated state, not partial
// orphans of an aborted fetch.

import {
  createQueryDemandLoader,
  createSidecarWithMemoryBackend,
} from '../dist/ctox-rxdb-js.mjs';

function makeStorageCollection() {
  const docs = new Map();
  return {
    docs,
    databaseName: 'swr',
    async bulkWrite(rows) {
      for (const r of rows) {
        const doc = r?.document || r;
        if (doc._deleted) { docs.set(doc.id, { ...doc }); continue; }
        docs.set(doc.id, { ...doc });
      }
    },
    async queryDocuments(query, { matchesSelector, sortDocuments }) {
      let all = Array.from(docs.values())
        .filter((doc) => !doc._deleted)
        .filter((doc) => matchesSelector(doc, query.selector || {}));
      all = sortDocuments(all, query.sort || []);
      if (query.skip > 0) all = all.slice(query.skip);
      if (Number.isFinite(query.limit)) all = all.slice(0, query.limit);
      return all;
    },
  };
}

const assert = (condition, message) => {
  if (!condition) throw new Error(message);
};
const settle = () => new Promise((resolve) => setTimeout(resolve, 10));

// --- 1. invalidated ever-complete window: local answer NOW, refresh behind --
{
  const sidecar = createSidecarWithMemoryBackend({ databaseName: 'swr-1' });
  const storage = makeStorageCollection();
  let fetches = 0;
  let releaseRefresh;
  const refreshGate = new Promise((resolve) => { releaseRefresh = resolve; });
  const loader = createQueryDemandLoader({
    storageCollection: storage,
    sidecar,
    collectionName: 'business_records',
    schemaVersion: 1,
    requestQueryFetch: async () => {
      fetches += 1;
      if (fetches === 1) {
        return { documents: [{ id: 'a', status: 'open' }, { id: 'b', status: 'open' }] };
      }
      await refreshGate; // background refresh is gated
      return { documents: [{ id: 'a', status: 'open' }, { id: 'b', status: 'open' }, { id: 'c', status: 'open' }] };
    },
  });

  await loader.resolveQuery({ selector: { status: 'open' } }); // cold: awaited
  assert(fetches === 1, 'cold start fetched remotely');

  await loader.invalidateDocumentChange(['a']);

  const started = Date.now();
  const stale = await loader.resolveQuery({ selector: { status: 'open' } });
  const elapsed = Date.now() - started;
  assert(stale.length === 2, `stale answer serves the 2 local docs (got ${stale.length})`);
  assert(elapsed < 200, `stale answer must not wait for the gated refresh (took ${elapsed}ms)`);
  assert(fetches === 2, 'background revalidation fetch was started');

  releaseRefresh();
  await settle();
  const fresh = await loader.resolveQuery({ selector: { status: 'open' } });
  assert(fresh.length === 3, 'after the background refresh the window is complete again');
  assert(fetches === 2, 'a re-completed window does not refetch');
}

// --- 2. requireRevision keeps strict await semantics ------------------------
{
  const sidecar = createSidecarWithMemoryBackend({ databaseName: 'swr-2' });
  const storage = makeStorageCollection();
  let release;
  const gate = new Promise((resolve) => { release = resolve; });
  let fetches = 0;
  const loader = createQueryDemandLoader({
    storageCollection: storage,
    sidecar,
    collectionName: 'business_records',
    schemaVersion: 1,
    requestQueryFetch: async () => {
      fetches += 1;
      if (fetches === 1) return { documents: [{ id: 'a', status: 'open' }], authoritativeRevision: 'r1' };
      await gate;
      return { documents: [{ id: 'a', status: 'open' }], authoritativeRevision: 'r2' };
    },
  });
  await loader.resolveQuery({ selector: { status: 'open' } });
  await loader.invalidateDocumentChange(['a']);

  let resolved = false;
  const strict = loader
    .resolveQuery({ selector: { status: 'open' }, requireRevision: 'r2' })
    .then((docs) => { resolved = true; return docs; });
  await settle();
  assert(!resolved, 'requireRevision on a stale window must AWAIT the remote fetch');
  release();
  await strict;
  assert(resolved, 'requireRevision resolves once the fetch lands');
}

// --- 3. never-complete window stays awaited (cold start) --------------------
{
  const sidecar = createSidecarWithMemoryBackend({ databaseName: 'swr-3' });
  const storage = makeStorageCollection();
  let release;
  const gate = new Promise((resolve) => { release = resolve; });
  const loader = createQueryDemandLoader({
    storageCollection: storage,
    sidecar,
    collectionName: 'business_records',
    schemaVersion: 1,
    requestQueryFetch: async () => {
      await gate;
      return { documents: [{ id: 'a', status: 'open' }] };
    },
  });
  let resolved = false;
  const cold = loader.resolveQuery({ selector: { status: 'open' } }).then((docs) => { resolved = true; return docs; });
  await settle();
  assert(!resolved, 'a never-complete window must await the remote fetch');
  release();
  assert((await cold).length === 1, 'cold fetch returns the fetched doc');
}

// --- 4. reconnect-abort never tombstones ever-complete window members -------
{
  const sidecar = createSidecarWithMemoryBackend({ databaseName: 'swr-4' });
  const storage = makeStorageCollection();
  let fetches = 0;
  const loader = createQueryDemandLoader({
    storageCollection: storage,
    sidecar,
    collectionName: 'business_records',
    schemaVersion: 1,
    requestQueryFetch: async () => {
      fetches += 1;
      if (fetches === 1) return { documents: [{ id: 'a', status: 'open' }, { id: 'b', status: 'open' }] };
      return new Promise(() => {}); // background refresh hangs
    },
    requestCancel: async () => {},
  });
  await loader.resolveQuery({ selector: { status: 'open' } });
  await loader.invalidateDocumentChange(['a']);
  await loader.resolveQuery({ selector: { status: 'open' } }); // starts hanging refresh
  await loader.abortAllInFlight('reconnect');
  const docs = await storage.queryDocuments(
    { selector: { status: 'open' } },
    {
      matchesSelector: (doc, selector) => Object.entries(selector).every(([k, v]) => doc[k] === v),
      sortDocuments: (list) => list,
    },
  );
  assert(
    docs.length === 2,
    `abort must not tombstone replicated members of an ever-complete window (got ${docs.length})`,
  );
}

console.log('ctox-rxdb stale-while-revalidate smoke OK');
process.exit(0);
