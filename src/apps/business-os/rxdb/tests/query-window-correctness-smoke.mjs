import { QueryMetaStorage } from '../src/query-meta-storage.mjs';
import { createMemoryMetaBackend } from '../src/query-meta-backend-memory.mjs';

const sidecar = new QueryMetaStorage(createMemoryMetaBackend(), { databaseName: 'query-window-test' });
await putWindow('simple-open', ['ticket-1'], {
  selector: { status: { $eq: 'open' } },
  sort: [],
});
await putWindow('simple-closed', ['ticket-2'], {
  selector: { status: 'closed' },
  sort: [],
});
await putWindow('sorted', ['ticket-3'], {
  selector: { status: 'closed' },
  sort: [{ updated_at_ms: 'desc' }],
});

const invalidated = await sidecar.invalidateQueryWindowsForChanges(
  'tickets',
  [{ id: 'ticket-4', status: 'open' }],
  'id',
);
assert(invalidated === 2, `expected entering equality window + conservative sorted invalidation, got ${invalidated}`);
assert((await getWindow('simple-open')).complete === false, 'new equality member must invalidate its window');
assert((await getWindow('simple-closed')).complete === true, 'unaffected simple equality window must stay complete');
assert((await getWindow('sorted')).complete === false, 'sorted windows must invalidate conservatively');

await sidecar.invalidateQueryWindowsForChanges(
  'tickets',
  [{ id: 'ticket-2', _deleted: true }],
  'id',
);
assert((await getWindow('simple-closed')).complete === false, 'deleting a current member must invalidate equality window');
await sidecar.close();

console.log('ctox-rxdb query-window correctness smoke OK');

function putWindow(fingerprint, documentIds, queryShape) {
  return sidecar.upsertQueryWindow({
    collection: 'tickets',
    queryFingerprint: fingerprint,
    offset: 0,
    limit: 50,
    documentIds,
    complete: true,
    queryShape,
  });
}

function getWindow(fingerprint) {
  return sidecar.getQueryWindow(['tickets', fingerprint, 0, 50]);
}

function assert(condition, message) {
  if (!condition) throw new Error(message);
}
