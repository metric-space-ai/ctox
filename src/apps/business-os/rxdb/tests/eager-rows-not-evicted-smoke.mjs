// An eagerly pulled collection is a replica, not a demand cache. When query
// demand loading touched more rows than the sidecar budget, LRU eviction
// hard-deleted replicated rows; the incremental pull can never bring them back,
// so every eviction invalidated the retained checkpoint and each reload
// re-pulled the whole collection (customer tenant, 30.09.2026: ~20 MB leads).
import {
  SIDECAR_PIN_RECENT_READ_TTL_MS,
  createMemoryMetaBackend,
  QueryMetaStorage,
} from '../dist/ctox-rxdb-js.mjs';
import { replicationWebRtcTestInternals as internals } from '../src/replication-webrtc.mjs';

const { demandSidecarPrimaryDelete } = internals;
let failures = 0;
function check(name, condition) {
  if (condition) console.log(`ok   ${name}`);
  else { failures += 1; console.log(`FAIL ${name}`); }
}

function fakeState(name, pull) {
  const rows = new Map(['a', 'b', 'c', 'd'].map((id) => [id, { id, pushable: 0 }]));
  const hardDeletes = [];
  return {
    rows,
    hardDeletes,
    pull,
    collection: {
      name,
      storageCollection: {
        async getStoredRecord(id) { return rows.get(id) || null; },
        async hardDeleteByIds(ids) {
          hardDeletes.push(...ids);
          for (const id of ids) rows.delete(id);
          return ids.length;
        },
      },
    },
  };
}

async function evictAll(state) {
  let now = 1_000_000;
  const backend = createMemoryMetaBackend();
  const sidecar = new QueryMetaStorage(backend, {
    databaseName: `evict-${state.collection.name}`,
    clock: () => now,
    primaryDelete: demandSidecarPrimaryDelete(state),
  });
  await sidecar.setBudgetBytes(1024);
  await sidecar.touchDocuments(state.collection.name, ['a', 'b', 'c', 'd'], { estimatedBytes: 1024 });
  now += SIDECAR_PIN_RECENT_READ_TTL_MS + 1;
  const removed = await sidecar.runEvictionIfOverBudget({ forceRecount: true });
  const stats = await sidecar.getCacheStats();
  return { removed, stats };
}

const eager = fakeState('outbound_lead_generation_leads', { batchSize: 10 });
const eagerResult = await evictAll(eager);
check('eager: sidecar still sheds its bookkeeping', eagerResult.removed >= 3);
check('eager: no replicated row is hard-deleted', eager.hardDeletes.length === 0 && eager.rows.size === 4);
check('eager: working set falls back under budget', eagerResult.stats.estimatedBytes <= 1024);

const demandOnly = fakeState('sellify_people', null);
const demandResult = await evictAll(demandOnly);
check('demand-only: evicted rows leave the primary store', demandResult.removed >= 3 && demandOnly.hardDeletes.length === demandResult.removed);

const dirty = fakeState('sellify_records', null);
dirty.rows.set('a', { id: 'a', pushable: 1 });
await evictAll(dirty);
check('demand-only: unsynced row is never evicted', dirty.rows.has('a'));

const other = fakeState('sellify_people', null);
await demandSidecarPrimaryDelete(other)('another_collection', 'a');
check('foreign collection ids are ignored', other.hardDeletes.length === 0);

if (failures) {
  console.error(`eager-rows-not-evicted smoke: ${failures} failure(s)`);
  process.exit(1);
}
console.log('eager-rows-not-evicted smoke OK');
