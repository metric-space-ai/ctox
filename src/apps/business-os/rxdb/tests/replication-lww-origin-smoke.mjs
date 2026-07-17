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
//   2. replication-over-LOCAL: the unsynced local write wins while it
//      outranks the master row — ordered by HLC when both sides carry
//      `_meta.ctoxHlc` (SYNC-11), by wall-clock lwt otherwise — except
//      authoritative command lifecycle progress.
//   3. local writes keep plain LWW semantics.
//   4. an unsynced local edit that LOSES the gate is journaled as an
//      `update_vs_update` conflict unless the master row acknowledges it
//      (own push round-trip / identical content).

import {
  createQueryDemandLoader,
  createSidecarWithMemoryBackend,
  ctoxIndexedDbStorageTestInternals,
  formatHybridLogicalClock,
} from '../dist/ctox-rxdb-js.mjs';

const { shouldAcceptDocumentWrite, lwwOverwriteConflict } = ctoxIndexedDbStorageTestInternals;
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

const locallyNewerPendingCommand = {
  lwt: 5000,
  doc: {
    id: 'command-2',
    command_id: 'command-2',
    status: 'pending_sync',
    updated_at_ms: 5000,
    _meta: { lwt: 5000 },
  },
};
assert(
  shouldAcceptDocumentWrite(
    locallyNewerPendingCommand,
    4000,
    origin,
    {
      id: 'command-2',
      command_id: 'command-2',
      status: 'completed',
      execution_phase: 'terminal',
      replication_phase: 'native_observed',
      updated_at_ms: 4000,
    },
    'business_commands',
  ) === true,
  'native command progress must beat a newer browser timestamp caused by clock skew',
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

// --- 2b. SYNC-11: the local veto is ordered by HLC when both sides carry one
// Push conflicts arbitrate by `_meta.ctoxHlc`
// (`resolveWholeDocumentLwwConflicts`); the pull gate must use the SAME
// ordering, otherwise two concurrent LWW edits resolve differently depending
// on relay-vs-push interleaving.
const hlc = (physicalMs, nodeId) => formatHybridLogicalClock({ physicalMs, logical: 0, nodeId });
const localHlcRecord = (lwt, hlcValue) => ({
  lwt,
  replicationOriginRole: '',
  doc: { id: 'doc-1', purpose: 'local-edit', updated_at_ms: lwt, _meta: { lwt, ctoxHlc: hlcValue } },
});
const masterDoc = (hlcValue, extra = {}) => ({
  id: 'doc-1',
  purpose: 'master-edit',
  ...extra,
  ...(hlcValue ? { _meta: { ctoxHlc: hlcValue } } : {}),
});

// (a) HLC-newer local unsynced edit survives a master pull that carries a
// NEWER wall-clock lwt but an OLDER HLC (the live SYNC-02 failure: the
// relayed row was re-stamped with the native commit wall-clock).
assert(
  shouldAcceptDocumentWrite(localHlcRecord(1000, hlc(5000, 'tab-a')), 9000, origin, masterDoc(hlc(4000, 'native'))) === false,
  'an HLC-newer local unsynced edit must veto a master row with a newer wall-clock but older HLC',
);
// (b-gate) HLC-older local edit is overwritten even when its wall-clock lwt
// is newer (the converse interleaving).
assert(
  shouldAcceptDocumentWrite(localHlcRecord(9000, hlc(4000, 'tab-a')), 1000, origin, masterDoc(hlc(5000, 'native'))) === true,
  'an HLC-older local edit must lose to a master row with a newer HLC regardless of wall-clock lwt',
);
// Equal HLC = the local write's own push round-tripping back: accept.
assert(
  shouldAcceptDocumentWrite(localHlcRecord(1000, hlc(4000, 'tab-a')), 9000, origin, masterDoc(hlc(4000, 'tab-a'))) === true,
  'a master row carrying the SAME HLC (own push echo) must be accepted',
);
// (c) missing-HLC fallback keeps today's wall-clock behavior on either side.
assert(
  shouldAcceptDocumentWrite(localHlcRecord(2000, hlc(1000, 'tab-a')), 1000, origin, masterDoc(null)) === false
  && shouldAcceptDocumentWrite(localHlcRecord(1000, hlc(1000, 'tab-a')), 2000, origin, masterDoc(null)) === true
  && shouldAcceptDocumentWrite(localRecord(2000), 1000, origin, masterDoc(hlc(5000, 'native'))) === false
  && shouldAcceptDocumentWrite(localRecord(1000), 2000, origin, masterDoc(hlc(5000, 'native'))) === true,
  'when either side lacks an HLC the gate keeps the wall-clock lwt fallback (mixed-version safety)',
);
// Clock skew: a strongly-future local HLC must not silently win (mirrors the
// push side, which accepts master and journals the skewed local row).
const futureHlc = hlc(Date.now() + 10 * 60 * 1000, 'tab-skewed');
assert(
  shouldAcceptDocumentWrite(localHlcRecord(1000, futureHlc), 9000, origin, masterDoc(hlc(4000, 'native'))) === true,
  'a local HLC more than five minutes in the future must not veto the master row',
);

// --- 2c. SYNC-11: the losing unsynced local edit is journaled ----------------
// `lwwOverwriteConflict` builds the conflict-store entry the bulk-write path
// persists (`update_vs_update`, the delete_vs_update pattern generalized).
{
  const previous = localHlcRecord(9000, hlc(4000, 'tab-a'));
  const master = masterDoc(hlc(5000, 'native'), { updated_at_ms: 1000 });
  const conflict = lwwOverwriteConflict({
    previous,
    incomingDocument: master,
    collectionName: 'business_consents',
    conflictStrategy: 'lww',
    replicationOrigin: origin,
  });
  assert(conflict, 'an overwritten unsynced local edit must produce a conflict record');
  assert(conflict.conflictType === 'update_vs_update', 'the conflict type generalizes delete_vs_update to update_vs_update');
  assert(conflict.code === 'structured_conflict_requires_resolution', 'the conflict uses the structured-conflict code');
  assert(conflict.collection === 'business_consents', 'the conflict carries the collection');
  assert(
    conflict.local?.purpose === 'local-edit' && conflict.local?.updated_at_ms === 9000
    && conflict.local?._meta?.ctoxHlc === hlc(4000, 'tab-a'),
    'the conflict preserves the losing local doc with its exact field values',
  );
  assert(conflict.master?.purpose === 'master-edit', 'the conflict carries the winning master row');

  // A strongly-future local HLC loses as clock skew, consistent with the
  // push-side journaling philosophy.
  const skewConflict = lwwOverwriteConflict({
    previous: localHlcRecord(1000, futureHlc),
    incomingDocument: master,
    collectionName: 'business_consents',
    conflictStrategy: 'lww',
    replicationOrigin: origin,
  });
  assert(skewConflict?.code === 'clock_skew_detected', 'a skewed losing local edit is journaled as clock_skew_detected');
  assert(skewConflict?.conflictType === 'update_vs_update', 'the skewed loss still lands as update_vs_update');
}

// --- 2d. SYNC-11: NO conflict record when nothing is lost --------------------
{
  const noConflict = (overrides) => lwwOverwriteConflict({
    previous: localHlcRecord(1000, hlc(4000, 'tab-a')),
    incomingDocument: masterDoc(hlc(5000, 'native')),
    collectionName: 'business_consents',
    conflictStrategy: 'lww',
    replicationOrigin: origin,
    ...overrides,
  });
  // (d) already-pushed local write: the master row carries the local write's
  // own HLC (push round-trip acknowledgement) — not data loss.
  assert(
    noConflict({ incomingDocument: masterDoc(hlc(4000, 'tab-a')) }) === null,
    'an already-pushed local write (master echoes its HLC) must not produce a conflict record',
  );
  // Identical content without HLCs (mixed-version acknowledgement).
  const plainLocal = { lwt: 1000, replicationOriginRole: '', doc: { id: 'doc-1', purpose: 'same', _meta: { lwt: 1000 } } };
  assert(
    lwwOverwriteConflict({
      previous: plainLocal,
      incomingDocument: { id: 'doc-1', purpose: 'same', _meta: { lwt: 1000 } },
      collectionName: 'business_consents',
      conflictStrategy: 'lww',
      replicationOrigin: origin,
    }) === null,
    'an incoming row identical to the local row must not produce a conflict record',
  );
  // Already-synced rows are replication-over-replication, not data loss.
  assert(
    noConflict({
      previous: {
        lwt: 1000,
        replicationOriginRole: 'ctox_instance',
        doc: { id: 'doc-1', _meta: { lwt: 1000, ctoxHlc: hlc(4000, 'tab-a'), ctoxReplicationOrigin: { role: 'ctox_instance' } } },
      },
    }) === null,
    'a stored master-origin row must never produce an overwrite conflict record',
  );
  // Field-merge collections preserve local fields via three-way merge.
  assert(
    noConflict({ conflictStrategy: 'field-merge' }) === null,
    'field-merge collections must not journal whole-doc overwrite conflicts',
  );
  // Master tombstones stay on the delete_vs_update path.
  assert(
    noConflict({ incomingDocument: { ...masterDoc(hlc(5000, 'native')), _deleted: true } }) === null,
    'master tombstones are journaled as delete_vs_update, not update_vs_update',
  );
  // Native-authoritative lifecycle rewrites are server-owned, not data loss.
  assert(
    noConflict({ collectionName: 'business_commands' }) === null
    && noConflict({ collectionName: 'ctox_queue_tasks' }) === null,
    'native-authoritative collections must not journal overwrite conflicts',
  );
  // Local writes never journal against themselves.
  assert(
    noConflict({ replicationOrigin: null }) === null,
    'local writes must not produce overwrite conflict records',
  );
}

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
