import {
  SIDECAR_PIN_RECENT_READ_TTL_MS,
  createSidecarWithMemoryBackend,
} from '../dist/ctox-rxdb-js.mjs';

let now = 2_000_000;
const clock = () => now;
const storage = createSidecarWithMemoryBackend({ databaseName: 'evict-test', clock });

await storage.setBudgetBytes(4096);

// Seed five docs at 1 KB each, totalling 5 KB → over budget.
for (let i = 0; i < 5; i += 1) {
  await storage.touchDocuments('business_records', [`doc-${i}`], { estimatedBytes: 1024 });
  now += 100; // each touch one tick later
}
await storage.recordEstimatedBytes(5 * 1024);

// Mark doc-2 dirty (must NOT be evicted).
await storage.markDirty('business_records', 'doc-2', true);

// Mark doc-4 was just touched, still inside pin TTL.
// Advance clock past TTL so the other four are eligible.
now += SIDECAR_PIN_RECENT_READ_TTL_MS + 1;
// Refresh doc-4 to keep it pinned by recent-read.
await storage.touchDocuments('business_records', ['doc-4'], { estimatedBytes: 1024 });

const removed = await storage.runEvictionIfOverBudget();
assert(removed >= 1, 'eviction must remove at least one doc when over budget');
assert(
  (await storage.getDocumentAccess('business_records', 'doc-2')) !== null,
  'dirty doc-2 must survive eviction',
);
assert(
  (await storage.getDocumentAccess('business_records', 'doc-4')) !== null,
  'freshly touched doc-4 must survive eviction',
);

const finalStats = await storage.getCacheStats();
assert(
  finalStats.estimatedBytes <= finalStats.budgetBytes,
  `eviction must bring bytes (${finalStats.estimatedBytes}) under budget (${finalStats.budgetBytes})`,
);
assert(finalStats.lastEvictionAt !== null, 'lastEvictionAt must be set');

// Running again when under budget is a no-op.
const removedAgain = await storage.runEvictionIfOverBudget();
assert(removedAgain === 0, 'no-op when already under budget');

console.log('ctox-rxdb-js eviction scheduler smoke OK');

function assert(condition, message) {
  if (!condition) throw new Error(message);
}
