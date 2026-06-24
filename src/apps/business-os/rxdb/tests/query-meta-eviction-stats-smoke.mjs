import {
  SIDECAR_PIN_RECENT_READ_TTL_MS,
  createSidecarWithMemoryBackend,
} from '../dist/ctox-rxdb-js.mjs';

let now = 3_000_000;
const clock = () => now;
const storage = createSidecarWithMemoryBackend({ databaseName: 'evict-stats-test', clock });

await storage.setBudgetBytes(2048);
for (let i = 0; i < 4; i += 1) {
  await storage.touchDocuments('business_records', [`doc-${i}`], { estimatedBytes: 1024 });
  now += 10;
}

const before = await storage.getCacheStats();
assert(before.estimatedBytes === 4096, `touchDocuments must maintain estimatedBytes, got ${before.estimatedBytes}`);

now += SIDECAR_PIN_RECENT_READ_TTL_MS + 1;
const removed = await storage.runEvictionIfOverBudget();
assert(removed >= 2, `eviction should remove enough docs without manual recordEstimatedBytes, removed ${removed}`);

const after = await storage.getCacheStats();
assert(after.estimatedBytes <= after.budgetBytes, 'eviction should bring estimated bytes under budget');

console.log('ctox-rxdb query-meta eviction stats smoke OK');

function assert(condition, message) {
  if (!condition) throw new Error(message);
}
