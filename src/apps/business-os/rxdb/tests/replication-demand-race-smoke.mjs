// Replication × demand-loading race audit. Verifies that a remote write
// landing via masterChangesSince concurrently with a query-fetch does NOT
// produce an inconsistent sidecar/primary-store state.
//
// Specifically tested:
//   1. Local write during in-flight fetch — local write must NOT be
//      clobbered by the fetch result.
//   2. Remote change during cached read — invalidateDocumentChange marks
//      the window incomplete and next read triggers re-fetch.
//   3. Concurrent fetches for the same fingerprint dedup correctly even
//      when interleaved with replication writes.

import {
  createQueryDemandLoader,
  createSidecarWithMemoryBackend,
} from '../dist/ctox-rxdb-js.mjs';

function makeStorage() {
  const docs = new Map();
  return {
    docs,
    databaseName: 'race',
    async bulkWrite(rows) {
      for (const r of rows) {
        if (r._deleted) { docs.delete(r.id); continue; }
        const prev = docs.get(r.id);
        // CTOX uses LWT semantics: newer _meta.lwt wins.
        const newer = !prev || ((r._meta?.lwt ?? 0) >= (prev._meta?.lwt ?? 0));
        if (newer) docs.set(r.id, { ...r });
      }
    },
    async queryDocuments() { return Array.from(docs.values()); },
  };
}

// === Case 1: local write during fetch must survive ===
{
  const storage = makeStorage();
  const sidecar = createSidecarWithMemoryBackend({ databaseName: 'race-1' });
  let fetchGate;
  const fetchPromise = new Promise((r) => { fetchGate = r; });
  const loader = createQueryDemandLoader({
    storageCollection: storage,
    sidecar,
    collectionName: 'business_records',
    schemaVersion: 1,
    requestQueryFetch: async () => {
      await fetchPromise;
      return {
        documents: [{ id: 'A', n: 1, _meta: { lwt: 100 } }],
        authoritativeRevision: 'r1',
      };
    },
  });

  // Start a fetch (gated). It will land doc A with lwt=100.
  const resolved = loader.resolveQuery({ selector: { kind: 'x' } });
  // Meanwhile a LOCAL WRITE happens with lwt=200 (newer).
  await storage.bulkWrite([{ id: 'A', n: 999, _meta: { lwt: 200 } }]);
  // Let the fetch return.
  fetchGate();
  await resolved;

  const doc = storage.docs.get('A');
  assert(doc.n === 999, `local write must survive; got n=${doc.n}`);
  assert(doc._meta.lwt === 200, 'LWT must reflect the newer local write');
}

// === Case 2: remote change → invalidate → re-fetch ===
{
  const storage = makeStorage();
  const sidecar = createSidecarWithMemoryBackend({ databaseName: 'race-2' });
  let fetchCount = 0;
  const loader = createQueryDemandLoader({
    storageCollection: storage,
    sidecar,
    collectionName: 'business_records',
    schemaVersion: 1,
    requestQueryFetch: async () => {
      fetchCount += 1;
      return {
        documents: [{ id: 'B', _meta: { lwt: fetchCount } }],
        authoritativeRevision: `r${fetchCount}`,
      };
    },
  });

  await loader.resolveQuery({ selector: { kind: 'y' } });
  assert(fetchCount === 1);
  // Cache hit
  await loader.resolveQuery({ selector: { kind: 'y' } });
  assert(fetchCount === 1, 'cache hit must not re-fetch');

  // Simulate replication delivering a change for doc B.
  await loader.invalidateDocumentChange(['B']);

  await loader.resolveQuery({ selector: { kind: 'y' } });
  assert(fetchCount === 2, `invalidated window must re-fetch (count=${fetchCount})`);
}

// === Case 3: concurrent fetches dedup; concurrent writes don't break it ===
{
  const storage = makeStorage();
  const sidecar = createSidecarWithMemoryBackend({ databaseName: 'race-3' });
  let fetchCount = 0;
  const loader = createQueryDemandLoader({
    storageCollection: storage,
    sidecar,
    collectionName: 'business_records',
    schemaVersion: 1,
    requestQueryFetch: async () => {
      fetchCount += 1;
      await new Promise((r) => setTimeout(r, 10));
      return { documents: [{ id: 'C' }, { id: 'D' }], authoritativeRevision: 'r1' };
    },
  });

  // Run 3 concurrent reads and 2 concurrent writes.
  const work = await Promise.all([
    loader.resolveQuery({ selector: { kind: 'z' } }),
    loader.resolveQuery({ selector: { kind: 'z' } }),
    loader.resolveQuery({ selector: { kind: 'z' } }),
    storage.bulkWrite([{ id: 'C', mutated: true, _meta: { lwt: 999 } }]),
    storage.bulkWrite([{ id: 'E', extra: true, _meta: { lwt: 999 } }]),
  ]);
  assert(fetchCount === 1, `concurrent dedup → 1 fetch (got ${fetchCount})`);
  // The local mutation on C with newer LWT must win over the fetched version.
  assert(storage.docs.get('C').mutated === true, 'local write on C must win');
  assert(storage.docs.has('E'), 'unrelated local write must be present');
}

console.log('ctox-rxdb-js replication × demand race smoke OK');

function assert(c, m) { if (!c) throw new Error(m); }
