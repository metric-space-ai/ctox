// REGRESSION: master pulls are authoritative in the browser store's LWW gate.
//
// Master rows arrive WITHOUT `_meta.lwt` (keep_meta=false on the wire), so
// `documentLwt` falls back to the app-level `updated_at_ms` payload field.
// `shouldAcceptDocumentWrite` used to apply plain last-write-wins over that
// heuristic for ALL writes: any master change whose payload timestamp did not
// advance was silently dropped — while the pull checkpoint advanced past it,
// a permanent divergence (rxdb-soak file-chunk-stale-generation mode caught
// it: a corrupted master row replicated to the browser never landed).
//
// Contract pinned here:
//   1. replication-over-replication: ALWAYS accepted (master checkpoint
//      iteration only moves forward; the payload heuristic must not veto).
//   2. replication-over-LOCAL: the unsynced local write wins while its lwt
//      is newer (it pushes and round-trips through the master).
//   3. local writes keep plain LWW semantics.

import {
  createQueryDemandLoader,
  createSidecarWithMemoryBackend,
  ctoxIndexedDbStorageTestInternals,
} from '../dist/ctox-rxdb-js.mjs';

const { shouldAcceptDocumentWrite } = ctoxIndexedDbStorageTestInternals;
const origin = { role: 'ctox_instance', peerId: 'peer-native' };

const replicatedRecord = (lwt) => ({
  lwt,
  doc: { id: 'doc-1', _meta: { lwt, ctoxReplicationOrigin: { role: 'ctox_instance' } } },
});
const localRecord = (lwt) => ({
  lwt,
  doc: { id: 'doc-1', _meta: { lwt } },
});

const assert = (condition, message) => {
  if (!condition) throw new Error(message);
};

// --- 1. replication-over-replication always wins -----------------------------
assert(
  shouldAcceptDocumentWrite(replicatedRecord(2000), 1000, origin) === true,
  'master state with a non-advancing payload timestamp must still be accepted',
);
assert(
  shouldAcceptDocumentWrite(replicatedRecord(1000), 2000, origin) === true,
  'master state with a newer timestamp is accepted',
);

// A live accepted/terminal command update can overtake an older in-flight
// pull page. The older master row must not regress server-owned lifecycle
// state even though replication-over-replication normally always wins.
const acceptedCommand = {
  lwt: 2000,
  doc: {
    id: 'command-1',
    command_id: 'command-1',
    status: 'accepted',
    _meta: { lwt: 2000, ctoxReplicationOrigin: { role: 'ctox_instance' } },
  },
};
assert(
  shouldAcceptDocumentWrite(
    acceptedCommand,
    3000,
    origin,
    { id: 'command-1', command_id: 'command-1', status: 'pending_sync' },
    'business_commands',
  ) === false,
  'an older pending command pull must not regress an accepted native command',
);
assert(
  shouldAcceptDocumentWrite(
    acceptedCommand,
    3000,
    origin,
    { id: 'command-1', command_id: 'command-1', status: 'completed' },
    'business_commands',
  ) === true,
  'a forward native command transition must still be accepted',
);
const completedCommand = {
  ...acceptedCommand,
  doc: { ...acceptedCommand.doc, status: 'completed' },
};
assert(
  shouldAcceptDocumentWrite(
    completedCommand,
    4000,
    origin,
    { id: 'command-1', command_id: 'command-1', status: 'accepted' },
    'business_commands',
  ) === false,
  'an accepted replay must not reopen a terminal native command',
);

// --- 2. unsynced local write survives a replication write -------------------
assert(
  shouldAcceptDocumentWrite(localRecord(2000), 1000, origin) === false,
  'an unsynced newer LOCAL write must not be clobbered by an older master row',
);
assert(
  shouldAcceptDocumentWrite(localRecord(1000), 2000, origin) === true,
  'master state newer than the local write is accepted',
);

// --- 3. local writes keep plain LWW ------------------------------------------
assert(
  shouldAcceptDocumentWrite(replicatedRecord(2000), 1000, null) === false,
  'local write older than the stored row is rejected',
);
assert(
  shouldAcceptDocumentWrite(replicatedRecord(2000), 2000, null) === true,
  'local write at the same lwt is accepted (>= keeps upsert semantics)',
);
assert(
  shouldAcceptDocumentWrite(null, 1, origin) === true && shouldAcceptDocumentWrite(null, 1, null) === true,
  'first write for an id is always accepted',
);

// --- 4. demand loader stamps the replication origin on its writes ----------
// Demand-fetched documents are master state. Unstamped, they counted as
// unsynced local writes: the push pipeline echoed them (and cache-eviction
// tombstones — i.e. DELETES) back to the master, and the LWW gate above let
// them veto later master pulls.
{
  const writes = [];
  const storage = {
    databaseName: 'lww-origin',
    async bulkWrite(rows, options = {}) {
      writes.push({ rows, options });
    },
    async queryDocuments() { return []; },
    async allDocuments() { return []; },
  };
  const sidecar = createSidecarWithMemoryBackend({ databaseName: 'lww-origin' });
  const loader = createQueryDemandLoader({
    storageCollection: storage,
    sidecar,
    collectionName: 'business_records',
    schemaVersion: 1,
    requestQueryFetch: async () => ({
      documents: [{ id: 'A', n: 1, _meta: { lwt: 100 } }],
      authoritativeRevision: 'r1',
    }),
    replicationOrigin: () => origin,
  });
  await loader.resolveQuery({ selector: { kind: 'x' } });
  const materialized = writes.find(({ rows }) => rows.some((row) => row.id === 'A'));
  assert(materialized, 'demand loader materialized the fetched document');
  assert(
    materialized.options?.replicationOrigin?.role === 'ctox_instance',
    'demand-fetched documents must carry the replication origin stamp',
  );
}

console.log('ctox-rxdb replication LWW origin smoke OK');
process.exit(0);
