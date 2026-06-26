import {
  SIDECAR_DATABASE_NAME,
  SIDECAR_PIN_RECENT_READ_TTL_MS,
  createSidecarWithMemoryBackend,
} from '../dist/ctox-rxdb-js.mjs';

let now = 1_000_000;
const clock = () => now;
const storage = createSidecarWithMemoryBackend({
  databaseName: SIDECAR_DATABASE_NAME,
  clock,
});

const collection = 'business_records';
const fingerprint = 'fingerprint-1';

assert((await storage.getQueryWindow([collection, fingerprint, 0, 200])) === null, 'window missing initially');

const window = await storage.upsertQueryWindow({
  collection,
  queryFingerprint: fingerprint,
  offset: 0,
  limit: 200,
  documentIds: ['a', 'b', 'c'],
  complete: true,
  authoritativeRevision: 'rev-1',
});
assert(window.complete === true, 'window marked complete');
assert(window.documentIds.join(',') === 'a,b,c', 'window has documentIds');
assert(window.createdAt === 1_000_000 && window.updatedAt === 1_000_000, 'created/updated timestamps set');

now = 1_001_000;
const fetched = await storage.getQueryWindow([collection, fingerprint, 0, 200]);
assert(fetched.lastAccessedAt === 1_001_000, 'getQueryWindow updates lastAccessedAt');

let scanCount = 0;
const originalScanQueryWindows = storage.backend.scanQueryWindows.bind(storage.backend);
storage.backend.scanQueryWindows = async () => {
  scanCount += 1;
  return originalScanQueryWindows();
};
const invalidatedByDocument = await storage.invalidateQueryWindowsForDocuments(collection, ['b']);
assert(invalidatedByDocument === 1, 'document-index invalidation finds matching window');
assert(scanCount === 0, 'document-index invalidation must not scan all query windows');
const invalidatedByIndex = await storage.getQueryWindow([collection, fingerprint, 0, 200]);
assert(invalidatedByIndex.complete === false, 'document-index invalidation clears complete flag');

await storage.upsertQueryWindow({
  collection,
  queryFingerprint: fingerprint,
  offset: 0,
  limit: 200,
  documentIds: ['c'],
  complete: true,
  authoritativeRevision: 'rev-2',
});
const invalidatedByRemovedRef = await storage.invalidateQueryWindowsForDocuments(collection, ['b']);
assert(invalidatedByRemovedRef === 0, 'replaced window refs remove stale document links');
const stillComplete = await storage.getQueryWindow([collection, fingerprint, 0, 200]);
assert(stillComplete.complete === true, 'stale document ref must not invalidate replaced window');

await storage.touchDocuments(collection, ['a', 'b'], { estimatedBytes: 4096 });
const accessA = await storage.getDocumentAccess(collection, 'a');
assert(accessA.lastAccessedAt === 1_001_000, 'access time set');
assert(accessA.estimatedBytes === 4096, 'estimated bytes recorded');
assert(accessA.pinReason === 'recently-read', 'recently-read pin default');

await storage.markDirty(collection, 'a', true);
const dirtyA = await storage.getDocumentAccess(collection, 'a');
assert(dirtyA.dirty === true && dirtyA.pinReason === 'dirty', 'dirty pin protects from eviction');

// Eviction must not remove dirty docs, must not remove fresh recently-read docs.
now = 1_002_000;
const removedFirst = await storage.evictDocuments([
  { collection, id: 'a' },
  { collection, id: 'b' },
]);
assert(removedFirst === 0, 'fresh + dirty docs must not be evicted yet');

// Past TTL, recently-read pin expires; dirty still survives.
now = 1_002_000 + SIDECAR_PIN_RECENT_READ_TTL_MS + 1;
const removedSecond = await storage.evictDocuments([
  { collection, id: 'a' },
  { collection, id: 'b' },
]);
assert(removedSecond === 1, 'expired recently-read pin can be evicted');
assert((await storage.getDocumentAccess(collection, 'a')) !== null, 'dirty document still present');
assert((await storage.getDocumentAccess(collection, 'b')) === null, 'evicted document is gone');

const bytesBefore = await storage.estimateWorkingSetBytes();
assert(bytesBefore === 4096, `working-set bytes after eviction = ${bytesBefore}`);

await storage.invalidateQueryWindow([collection, fingerprint, 0, 200]);
const invalidated = await storage.getQueryWindow([collection, fingerprint, 0, 200]);
assert(invalidated.complete === false, 'invalidate clears complete flag');

await storage.setBudgetBytes(8192);
const stats = await storage.getCacheStats();
assert(stats.budgetBytes === 8192, 'budget recorded');
assert(stats.lastEvictionAt !== null, 'eviction timestamp recorded');

await storage.clear();
assert(
  (await storage.getQueryWindow([collection, fingerprint, 0, 200])) === null,
  'clear removes windows',
);
assert(
  (await storage.getDocumentAccess(collection, 'a')) === null,
  'clear removes document access',
);

console.log('ctox-rxdb-js sidecar storage smoke OK');

function assert(condition, message) {
  if (!condition) throw new Error(message);
}
