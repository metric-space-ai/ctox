import {
  QueryMetaStorage,
  createMemoryMetaBackend,
} from '../dist/ctox-rxdb-js.mjs';

const backend = createMemoryMetaBackend();
let accessScans = 0;
const originalScanDocumentAccess = backend.scanDocumentAccess.bind(backend);
backend.scanDocumentAccess = async () => {
  accessScans += 1;
  return originalScanDocumentAccess();
};

const storage = new QueryMetaStorage(backend, { databaseName: 'eviction-idle-test' });
await storage.setBudgetBytes(4096);
await storage.touchDocuments('business_records', ['a', 'b'], { estimatedBytes: 1024 });

const before = await storage.getCacheStats();
assert(before.estimatedBytes === 2048, `expected stats to track 2048 bytes, got ${before.estimatedBytes}`);

const removed = await storage.runEvictionIfOverBudget();
assert(removed === 0, 'under-budget eviction should be a no-op');
assert(accessScans === 0, 'under-budget eviction must not scan document access records');

await storage.recordEstimatedBytes(8192);
const removedOverBudget = await storage.runEvictionIfOverBudget();
assert(accessScans === 1, 'over-budget eviction should scan document access exactly once');
assert(removedOverBudget >= 0, 'over-budget eviction should complete');

console.log('ctox-rxdb query-meta eviction idle smoke OK');

function assert(condition, message) {
  if (!condition) throw new Error(message);
}
