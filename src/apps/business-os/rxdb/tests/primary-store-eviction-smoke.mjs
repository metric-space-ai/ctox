// Regression for the review finding: sidecar eviction must actually delete
// documents from the primary store, not just the sidecar metadata. Without
// the `primaryDelete` hook the browser cache grows unbounded across
// "evictions" because the docs themselves never leave the documents store.

import {
  SIDECAR_PIN_RECENT_READ_TTL_MS,
  createMemoryMetaBackend,
  QueryMetaStorage,
} from '../dist/ctox-rxdb-js.mjs';

// Fake primary store: a Map that pretends to be the documents store.
const primary = new Map();
primary.set('business_records|a', { id: 'a' });
primary.set('business_records|b', { id: 'b' });
primary.set('business_records|c', { id: 'c' });
primary.set('business_records|d', { id: 'd' });

const primaryDeleteCalls = [];
async function primaryDelete(collection, id) {
  primaryDeleteCalls.push({ collection, id });
  primary.delete(`${collection}|${id}`);
}

let now = 4_000_000;
const sidecar = new QueryMetaStorage(createMemoryMetaBackend(), {
  databaseName: 'primary-evict',
  clock: () => now,
  primaryDelete,
});

await sidecar.setBudgetBytes(2048);
await sidecar.touchDocuments('business_records', ['a', 'b', 'c', 'd'], { estimatedBytes: 1024 });
await sidecar.recordEstimatedBytes(4096);
// Mark `a` dirty — it must NOT be evicted from primary.
await sidecar.markDirty('business_records', 'a', true);
// Move time past the recently-read TTL so b, c, d become eligible.
now += SIDECAR_PIN_RECENT_READ_TTL_MS + 1;
// Refresh d to keep it pinned.
await sidecar.touchDocuments('business_records', ['d'], { estimatedBytes: 1024 });

const removed = await sidecar.runEvictionIfOverBudget();
assert(removed >= 1, `at least one doc must be evicted (got ${removed})`);

// THE key assertion the review demanded: docs actually gone from primary.
assert(primary.has('business_records|a'), 'dirty `a` must survive in primary store');
assert(primary.has('business_records|d'), 'freshly touched `d` must survive in primary store');
// Either `b` or `c` (or both) must have been evicted from primary.
const primaryEvicted = ['b', 'c'].filter((id) => !primary.has(`business_records|${id}`));
assert(primaryEvicted.length >= 1, `at least one of b/c must be gone from primary (got ${primaryEvicted})`);

// primaryDelete must have been called for each evicted doc — not just sidecar
// metadata cleared.
assert(primaryDeleteCalls.length >= 1, `primaryDelete must be invoked (got ${primaryDeleteCalls.length})`);
assert(primaryDeleteCalls.every((c) => c.collection === 'business_records'),
       'all primaryDelete calls scoped to the right collection');

// If primaryDelete throws, sidecar metadata stays so we don't orphan the doc.
const stayBackend = createMemoryMetaBackend();
const stayStorage = new QueryMetaStorage(stayBackend, {
  databaseName: 'fail-evict',
  clock: () => now,
  primaryDelete: async () => { throw new Error('IDB write failed'); },
});
await stayStorage.touchDocuments('c2', ['x'], { estimatedBytes: 1024 });
now += SIDECAR_PIN_RECENT_READ_TTL_MS + 1;
const failResult = await stayStorage.evictDocuments([{ collection: 'c2', id: 'x' }]);
assert(failResult === 0, 'failed primary-delete must not remove sidecar metadata');
const still = await stayBackend.getDocumentAccess('c2', 'x');
assert(still !== null, 'sidecar entry stays when primary-delete fails');

console.log('ctox-rxdb-js primary-store eviction smoke OK', {
  evictedFromPrimary: primaryEvicted,
  primaryDeleteCalls: primaryDeleteCalls.length,
  primarySurvivors: [...primary.keys()],
});

function assert(c, m) { if (!c) throw new Error(m); }
