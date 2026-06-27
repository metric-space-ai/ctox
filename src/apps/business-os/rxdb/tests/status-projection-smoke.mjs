import {
  V1_5_STATUS_FIELDS,
  createSidecarWithMemoryBackend,
  createV1_5StatusState,
  projectStatusFromSidecar,
  snapshotV1_5Status,
} from '../dist/ctox-rxdb-js.mjs';

const sidecar = createSidecarWithMemoryBackend({ databaseName: 'status-test' });
await sidecar.setBudgetBytes(8192);
await sidecar.touchDocuments('business_records', ['a', 'b'], { estimatedBytes: 1024 });
await sidecar.recordEstimatedBytes(2048);

const baseline = createV1_5StatusState();
const stats = await sidecar.getCacheStats();
const projected = projectStatusFromSidecar(baseline, stats, { pinnedDocCount: 2, pinnedBytes: 2048 });

assert(projected.indexedDbWorkingSetBytes === 2048, 'working set bytes projected');
assert(projected.pinnedDocCount === 2, 'pinned doc count projected');
assert(projected.pinnedBytes === 2048, 'pinned bytes projected');
assert(projected.rxdbProtocolVersion === '1', 'baseline preserved');

const snapshot = snapshotV1_5Status(projected);
for (const field of V1_5_STATUS_FIELDS) {
  assert(field in snapshot, `snapshot includes ${field}`);
}
assert(snapshot.activeFileStreams === 0, 'file streams default to zero');
assert(snapshot.pendingQueryFetchCollectors === 0, 'pending query collectors default to zero');
assert(snapshot.pendingFileFetchCollectors === 0, 'pending file collectors default to zero');
assert(snapshot.fileBytesReceived === 0, 'file bytes default to zero');
assert(snapshot.localPushChangedSinceCalls === 0, 'local push changed-since calls default to zero');
assert(snapshot.localPushChangedSinceScannedRows === 0, 'local push changed-since scanned rows default to zero');
assert(snapshot.localPushChangedSinceScanLimitHits === 0, 'local push scan-limit hits default to zero');
assert(snapshot.localPushChangedSinceMaxScannedRows === 0, 'local push max scanned rows default to zero');

console.log('ctox-rxdb-js status projection smoke OK');

function assert(c, m) { if (!c) throw new Error(m); }
