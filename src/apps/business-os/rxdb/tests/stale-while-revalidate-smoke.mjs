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
  queryFingerprint,
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
    // The production storage (storage-indexeddb.mjs) serves membership reads
    // through findDocumentsById; mirror it so membership order/filters match
    // the real path (the queryDocuments fallback truncates by limit BEFORE
    // the membership filter and is not what production exercises).
    async findDocumentsById(ids) {
      const out = {};
      for (const id of ids) {
        const doc = docs.get(String(id));
        if (doc) out[String(id)] = doc;
      }
      return out;
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
    queryGeneration: () => 'swr-authority-generation',
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

// --- 3b. concurrent strict read never inherits stale immediate result -------
{
  const sidecar = createSidecarWithMemoryBackend({ databaseName: 'swr-3b' });
  const storage = makeStorageCollection();
  let release;
  const gate = new Promise((resolve) => { release = resolve; });
  let fetches = 0;
  const loader = createQueryDemandLoader({
    storageCollection: storage,
    sidecar,
    collectionName: 'business_records',
    schemaVersion: 1,
    queryGeneration: () => 'swr-authority-generation',
    requestQueryFetch: async () => {
      fetches += 1;
      if (fetches === 1) {
        return { documents: [{ id: 'a', status: 'open' }], authoritativeRevision: 'r1' };
      }
      await gate;
      return {
        documents: [
          { id: 'a', status: 'open' },
          { id: 'b', status: 'open' },
        ],
        authoritativeRevision: 'r2',
      };
    },
  });
  await loader.resolveQuery({ selector: { status: 'open' } });
  await loader.invalidateDocumentChange(['a']);

  const stale = loader.resolveQuery({ selector: { status: 'open' } });
  let strictResolved = false;
  const strict = loader
    .resolveQuery({ selector: { status: 'open' }, requireRevision: 'r2' })
    .then((docs) => {
      strictResolved = true;
      return docs;
    });
  assert((await stale).length === 1, 'concurrent non-strict read still serves stale data');
  await settle();
  assert(!strictResolved, 'concurrent strict read must await its authoritative refresh');
  release();
  assert((await strict).length === 2, 'concurrent strict read returns the refreshed window');
  assert(fetches === 3, `strict read remains isolated from the untyped SWR refresh (got ${fetches} fetches)`);
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

// --- 5. control-plane status windows: stale serve NOW, refresh behind ------
// business_commands/ctox_queue_tasks keep a short freshness budget, but the
// budget triggers a BACKGROUND revalidation — it must not park the caller on
// a native round-trip after an ordinary reload (every cached window is older
// than the budget then). Regression pin for the issue #211 warm-load finding
// (2026-09-23): Crew/Tickets/Mail first reads blocked minutes behind a
// congested native query plane because control-plane staleness was awaited.
{
  let now = 10_000;
  // Loader and sidecar must share the same clock: window staleness compares
  // the loader clock against sidecar-stamped updatedAt.
  const sidecar = createSidecarWithMemoryBackend({ databaseName: 'swr-5', clock: () => now });
  const storage = makeStorageCollection();
  let fetches = 0;
  let releaseRefresh;
  const refreshGate = new Promise((resolve) => { releaseRefresh = resolve; });
  const loader = createQueryDemandLoader({
    storageCollection: storage,
    sidecar,
    collectionName: 'business_commands',
    schemaVersion: 1,
    clock: () => now,
    requestQueryFetch: async () => {
      fetches += 1;
      if (fetches === 1) {
        return { documents: [{ id: 'cmd-1', status: 'running' }] };
      }
      await refreshGate; // background refresh is gated
      return { documents: [{ id: 'cmd-1', status: 'completed' }] };
    },
  });

  await loader.resolveQuery({ selector: {} }); // cold: awaited
  assert(fetches === 1, 'control-plane cold start fetched remotely');

  now += 60_000; // ordinary reload: every cached window is past the budget
  const started = Date.now();
  const stale = await loader.resolveQuery({ selector: {} });
  const elapsed = Date.now() - started;
  assert(stale.length === 1 && stale[0].status === 'running',
    'stale control-plane window serves the cached row immediately');
  assert(elapsed < 200, `control-plane stale answer must not await the refresh (took ${elapsed}ms)`);
  assert(fetches === 2, 'control-plane background revalidation fetch was started');

  releaseRefresh();
  await settle();
  const fresh = await loader.resolveQuery({ selector: {} });
  assert(fresh[0]?.status === 'completed', 'background refresh updates the control-plane window');
}

// --- 6. strict requireRevision on control-plane still awaits ---------------
{
  let now = 20_000;
  const sidecar = createSidecarWithMemoryBackend({ databaseName: 'swr-6', clock: () => now });
  const storage = makeStorageCollection();
  let fetches = 0;
  let release;
  const gate = new Promise((resolve) => { release = resolve; });
  const loader = createQueryDemandLoader({
    storageCollection: storage,
    sidecar,
    collectionName: 'business_commands',
    schemaVersion: 1,
    clock: () => now,
    queryGeneration: () => 'swr-control-plane-generation',
    requestQueryFetch: async () => {
      fetches += 1;
      if (fetches === 1) {
        return { documents: [{ id: 'cmd-1', status: 'running' }], authoritativeRevision: 'r1' };
      }
      await gate;
      return { documents: [{ id: 'cmd-1', status: 'completed' }], authoritativeRevision: 'r2' };
    },
  });
  await loader.resolveQuery({ selector: {}, requireRevision: 'r1' });
  now += 60_000;

  let strictResolved = false;
  const strict = loader
    .resolveQuery({ selector: {}, requireRevision: 'r2' })
    .then((docs) => { strictResolved = true; return docs; });
  await settle();
  assert(!strictResolved, 'strict control-plane read must await its authoritative refresh');
  release();
  const strictDocs = await strict;
  assert(strictResolved && strictDocs[0]?.status === 'completed',
    'strict control-plane read resolves with the refreshed row');
}

// --- 7. permission-digest boundary: superseded control-plane windows fail
// closed ---------------------------------------------------------------------
// A role/grant change issues a capability token with a bumped epoch, so the
// SYNC-12 read-permission digest changes. replication-webrtc drops retained
// pull checkpoints then, but persisted query windows keep their membership.
// A control-plane window stamped under a superseded digest must NOT serve
// local rows before a newly authorized fetch re-stamps it — neither via SWR
// nor via the complete fast path — while an unchanged digest keeps the
// instant warm render, and an unresolvable current digest stays permissive
// (token-endpoint blip convention, mirroring readPermissionDigestMatches).
{
  let now = 30_000;
  const sidecar = createSidecarWithMemoryBackend({ databaseName: 'swr-7', clock: () => now });
  const storage = makeStorageCollection();
  let currentDigest = 'digest-role-a';
  let fetches = 0;
  let releaseRefresh;
  const refreshGate = new Promise((resolve) => { releaseRefresh = resolve; });
  const loader = createQueryDemandLoader({
    storageCollection: storage,
    sidecar,
    collectionName: 'business_commands',
    schemaVersion: 1,
    clock: () => now,
    readPermissionDigest: () => currentDigest,
    requestQueryFetch: async () => {
      fetches += 1;
      if (fetches === 1) {
        return { documents: [{ id: 'cmd-1', status: 'running', scope: 'role-a' }] };
      }
      await refreshGate; // the authorized fetch under the new identity is gated
      return { documents: [{ id: 'cmd-2', status: 'running', scope: 'role-b' }] };
    },
  });

  await loader.resolveQuery({ selector: {} }); // cold: fetched and stamped digest-role-a
  assert(fetches === 1, 'control-plane cold start fetched remotely (digest boundary)');

  now += 60_000; // ordinary reload age: window past the freshness budget
  currentDigest = 'digest-role-b'; // role/grant change: new capability epoch

  let served = null;
  let servedAt = 0;
  const started = Date.now();
  const pending = loader.resolveQuery({ selector: {} })
    .then((docs) => { served = docs; servedAt = Date.now() - started; return docs; });
  await settle();
  assert(served === null,
    'superseded permission digest must NOT serve the stale control-plane membership');
  assert(fetches === 2, 'superseded window triggers a newly authorized fetch');

  releaseRefresh();
  const refreshed = await pending;
  assert(refreshed.length === 1 && refreshed[0].id === 'cmd-2',
    'after the authorized fetch the new membership is served');
  assert(servedAt >= 0, 'resolved after the authorized fetch completed');

  now += 60_000; // stable warm session under the SAME digest: SWR intact
  const fetchesBeforeWarm = fetches;
  const warmStarted = Date.now();
  const warmDocs = await loader.resolveQuery({ selector: {} });
  assert(warmDocs.length === 1 && warmDocs[0].id === 'cmd-2',
    'unchanged digest keeps the instant local warm render');
  assert(Date.now() - warmStarted < 200, 'warm read under unchanged digest stays sub-200ms');
  assert(fetches === fetchesBeforeWarm + 1, 'unchanged digest revalidates in the background');

  // Unresolvable current digest (token-endpoint blip): permissive, no storm.
  currentDigest = '';
  now += 60_000;
  const blip = await loader.resolveQuery({ selector: {} });
  assert(blip.length === 1 && blip[0].id === 'cmd-2',
    'unresolvable current digest stays permissive (token-blip convention)');
}

// --- 8. pre-stamp-era control-plane window mismatches a known identity once -
// Windows persisted before the permissionDigest stamp existed carry no stamp;
// with a known current identity they must refetch once (fail-closed), then
// serve normally.
{
  let now = 40_000;
  const sidecar = createSidecarWithMemoryBackend({ databaseName: 'swr-8', clock: () => now });
  const storage = makeStorageCollection();
  let fetches = 0;
  let releaseFirst;
  const firstGate = new Promise((resolve) => { releaseFirst = resolve; });
  const loader = createQueryDemandLoader({
    storageCollection: storage,
    sidecar,
    collectionName: 'ctox_queue_tasks',
    schemaVersion: 1,
    clock: () => now,
    readPermissionDigest: () => 'digest-known',
    requestQueryFetch: async () => {
      fetches += 1;
      if (fetches === 1) await firstGate; // hold the authorized fetch
      return { documents: [{ id: 'task-1', status: 'leased' }] };
    },
  });
  // Seed a legacy window directly: complete, members present, NO digest
  // stamp (the pre-fix persisted format). The fingerprint input must match
  // the loader's computation for resolveQuery with an empty selector.
  await storage.bulkWrite([{ id: 'task-legacy', status: 'done' }]);
  const legacyFingerprint = await queryFingerprint({
    collection: 'ctox_queue_tasks',
    schemaVersion: 1,
    selector: {},
    sort: [],
    limit: undefined,
    skip: undefined,
    window: { offset: 0, limit: 200 },
  });
  await sidecar.upsertQueryWindow({
    collection: 'ctox_queue_tasks',
    queryFingerprint: legacyFingerprint,
    offset: 0,
    limit: 200,
    documentIds: ['task-legacy'],
    complete: true,
  });
  now += 60_000; // past the freshness budget, as after an ordinary reload

  // With a known current identity and no stamp, the legacy window must NOT
  // serve locally; it awaits the authorized fetch.
  let legacyServed = null;
  const legacyPending = loader.resolveQuery({ selector: {} })
    .then((docs) => { legacyServed = docs; return docs; });
  await settle();
  assert(legacyServed === null,
    'legacy control-plane window without a digest stamp must not serve under a known identity');
  assert(fetches === 1, 'legacy window triggers the authorized fetch');
  releaseFirst();
  const legacyDocs = await legacyPending;
  assert(legacyDocs.length === 1 && legacyDocs[0].id === 'task-1',
    'legacy window is replaced by the authorized membership');

  // After the re-stamped fetch the same digest serves instantly again.
  now += 60_000;
  const warmAgain = await loader.resolveQuery({ selector: {} });
  assert(warmAgain.length === 1 && warmAgain[0].id === 'task-1',
    're-stamped window resumes stale-while-revalidate under the same identity');
  assert(fetches === 2, 're-stamped window revalidates in the background');
}

console.log('ctox-rxdb stale-while-revalidate smoke OK');
process.exit(0);
