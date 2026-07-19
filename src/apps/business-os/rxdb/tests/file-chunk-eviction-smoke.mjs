// M5: demand-fetched file-chunk rows must count toward the sidecar LRU budget
// and be reclaimable. Before the fix the loader recorded only a single
// `${fileId}-presence` documentAccess with estimatedBytes:0, so the eviction
// budget saw every persisted chunk as 0 bytes and never reclaimed rotated /
// re-fetched file chunks — they accumulated for the DB's lifetime.
//
// It also verifies the correctness follow-through: once chunk rows are evicted,
// the presence blob's resume hint is reconciled so a later fetch re-requests the
// missing sequences instead of returning an incomplete window.

import {
  createFileDemandLoader,
  createMemoryMetaBackend,
  QueryMetaStorage,
} from '../dist/ctox-rxdb-js.mjs';

const COLLECTION = 'desktop_files';
const CHUNK_BYTES = 1000;
const FULL = {
  0: 'A'.repeat(CHUNK_BYTES),
  1: 'B'.repeat(CHUNK_BYTES),
  2: 'C'.repeat(CHUNK_BYTES),
};

// Fake primary documents store: id -> row. Chunks written with a replication
// origin are master state (pushable 0), so they are eligible for eviction.
const primary = new Map();
const storageCollection = {
  async bulkWrite(rows) {
    for (const row of rows) primary.set(row.id, { ...row, pushable: 0 });
  },
  async getStoredRecord(id) {
    return primary.get(id) || null;
  },
  async hardDeleteByIds(ids) {
    for (const id of ids) primary.delete(id);
  },
};

// Mirrors the production primaryDelete wired in replication-webrtc.mjs.
async function primaryDelete(collection, id) {
  if (collection !== COLLECTION) return;
  const stored = await storageCollection.getStoredRecord(id);
  if (!stored || Number(stored.pushable || 0) !== 0) {
    throw new Error(`Refusing to evict locally-unsynced ${collection}/${id}`);
  }
  await storageCollection.hardDeleteByIds([id]);
}

let now = 5_000_000;
const backend = createMemoryMetaBackend();
const sidecar = new QueryMetaStorage(backend, {
  databaseName: 'file-chunk-evict',
  clock: () => now,
  primaryDelete,
});

// The loader shares the SAME backend the sidecar wraps (as in production).
let served = [];
const loader = createFileDemandLoader({
  collectionName: COLLECTION,
  storageCollection,
  sidecarBackend: backend,
  persistChunks: true,
  clock: () => now,
  replicationOrigin: { role: 'ctox_instance', peerId: 'p', sessionId: 's', collection: COLLECTION },
  requestFileFetch: async ({ knownSequences }) => {
    // A real server skips sequences the client claims to already hold.
    const known = new Set((knownSequences || []).map(Number));
    served = [0, 1, 2].filter((seq) => !known.has(seq));
    return served.map((seq) => ({ sequence: seq, bytesBase64: FULL[seq], hash: null }));
  },
});

// --- 1. First full fetch persists chunks AND registers their real weight ---
const first = await loader.fetchFile('f1');
assert(first.length === 3, `first fetch returns all 3 chunks (got ${first.length})`);
assert(primary.has('f1-0') && primary.has('f1-1') && primary.has('f1-2'), 'chunk rows persisted to primary');

const access0 = await backend.getDocumentAccess(COLLECTION, 'f1-0');
assert(access0 !== null, 'chunk row registered in sidecar');
assert(access0.estimatedBytes === CHUNK_BYTES, `chunk documentAccess carries real base64 weight (got ${access0.estimatedBytes})`);

const workingSet = await sidecar.estimateWorkingSetBytes();
assert(workingSet >= 3 * CHUNK_BYTES, `chunk bytes count toward the working set (got ${workingSet}, was 0 before the fix)`);

// --- 2. Over-budget eviction actually deletes the chunk rows ---------------
await sidecar.setBudgetBytes(500); // < one chunk, so all three must go
const removed = await sidecar.runEvictionIfOverBudget({ forceRecount: true });
assert(removed >= 3, `eviction reclaims the chunk rows (removed ${removed})`);
assert(!primary.has('f1-0') && !primary.has('f1-1') && !primary.has('f1-2'), 'evicted chunk rows gone from primary store');
assert((await backend.getDocumentAccess(COLLECTION, 'f1-0')) === null, 'evicted chunk access metadata gone');
// Presence blob survives (its primary row does not exist, so primaryDelete
// refuses it) — that is what the reconcile below leans on.
const presenceAfter = await backend.getDocumentAccess(COLLECTION, 'f1-presence');
assert(presenceAfter !== null, 'presence record survives eviction');
assert(presenceAfter.fileChunkPresence.presentSequences.length === 3, 'stale presence still lists all 3 sequences');

// --- 3. Reconcile: a later fetch re-requests the evicted sequences ---------
served = [];
const refetched = await loader.fetchFile('f1');
assert(served.length === 3, `reconcile drops evicted sequences from knownSequences so the peer re-sends all 3 (server saw ${served.length})`);
assert(refetched.length === 3, `re-fetch returns a complete window after eviction (got ${refetched.length})`);
assert(primary.has('f1-0') && primary.has('f1-1') && primary.has('f1-2'), 'chunk rows re-persisted');

console.log('ctox-rxdb-js file chunk eviction smoke OK', { workingSet, removed });

function assert(c, m) { if (!c) throw new Error(m); }
