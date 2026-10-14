// REGRESSION (SYNC-40): a native TERMINAL authz/schema rejection of a browser
// push must roll the local doc back to master + journal it, and STOP re-pushing.
//
// The native side returns an authz/schema denial as a replication-scope
// `ctoxError` VALUE (the response frame's `error` is null, so `peer.request`
// RESOLVES with the object rather than throwing) — index_mod.rs
// `replication_error_result` with messages like "peer is not authorized to
// write collection". Commands already do native-authoritative accept-master
// rollback; ordinary data writes used to leave the denied doc in the store,
// re-pushed and re-denied forever (a permanently divergent local mirror).
//
// Contract pinned here:
//   1. classification: authz/schema `ctoxError` results are TERMINAL; a normal
//      conflict ARRAY and a transient replication-io result ("no master
//      handler") are NOT (they keep the existing retry).
//   2. wiring: a terminal rejection in `pushToPeer` reconciles the batch
//      (rolls back + journals), advances the checkpoint, surfaces a non-fatal
//      `ctox_replication_push_rejected` signal, pulls-and-replaces, and does
//      NOT throw (throwing re-arms the infinite retry) — masterWrite is NOT
//      retried for the denied batch.
//   3. a transient THROWN transport error still rejects (retry preserved).
//   4. storage rollback: `reconcileRejectedLocalWrites` journals the rejected
//      local version and force-writes the master's last-confirmed state (merge
//      base, else a tombstone) as an origin write that clears pushable.

import { replicateWebRTC, replicationWebRtcTestInternals } from '../src/replication-webrtc.mjs';
import { CtoxIndexedDbCollection } from '../src/storage-indexeddb.mjs';

const { terminalPushRejection } = replicationWebRtcTestInternals;

const assert = (condition, message) => {
  if (!condition) throw new Error(message);
};
const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

// --- 1. classification --------------------------------------------------------
{
  const authzCollection = {
    type: 'ctoxError', scope: 'replication', rxdb: true,
    code: 'RC_WEBRTC_PEER', phase: 'replication-io', direction: 'push',
    collection: 'business_records', message: 'peer is not authorized to write collection',
  };
  const authzPerDoc = { ...authzCollection, message: 'peer is not authorized to write one or more documents' };
  const schema = {
    type: 'ctoxError', scope: 'replication', rxdb: true,
    code: 'RC_WEBRTC_SCHEMA', phase: 'replication-io', direction: 'push',
    collection: 'business_records', status: '422', message: 'document failed schema validation',
  };
  // The native masterWrite path wraps a 422 bulk-write validation error in
  // SNH, then returns that RxError inside a replication-scope RC_PUSH value.
  const nestedSchema = {
    type: 'ctoxError', scope: 'replication', rxdb: true,
    code: 'RC_PUSH', phase: 'replication-push', direction: 'push',
    collection: 'outbound_lead_generation_leads',
    errors: [{ code: 'SNH', parameters: {
      error: { status: 422, message: "required field 'evidence' is missing" },
    } }],
  };
  const noMaster = {
    type: 'ctoxError', scope: 'replication', rxdb: true,
    code: 'RC_WEBRTC_PEER', phase: 'replication-io', direction: 'unknown',
    collection: 'business_records', message: 'no master handler registered for collection',
  };

  assert(terminalPushRejection(authzCollection)?.kind === 'authz', 'collection-level authz denial is terminal (authz)');
  assert(terminalPushRejection(authzPerDoc)?.kind === 'authz', 'per-document authz denial is terminal (authz)');
  assert(terminalPushRejection(schema)?.kind === 'schema', 'schema/422 rejection is terminal (schema)');
  assert(terminalPushRejection(nestedSchema)?.kind === 'schema', 'nested native bulk-write 422 is terminal (schema)');
  assert(terminalPushRejection({
    ...nestedSchema,
    errors: [{ code: 'SNH', parameters: { error: { status: 500 } } }],
  }) === null, 'nested native 500 stays retryable');
  assert(terminalPushRejection({
    ...nestedSchema,
    errors: [{ code: 'SNH', parameters: { pushRows: [{ status: 422 }] } }],
  }) === null, 'document fields cannot classify a transport error as terminal');
  assert(terminalPushRejection(noMaster) === null, 'transient "no master handler" is NOT terminal (keeps retrying)');
  assert(terminalPushRejection([]) === null, 'a normal conflict ARRAY is not a rejection');
  assert(terminalPushRejection([{ id: 'x' }]) === null, 'a conflict doc array is not a rejection');
  assert(terminalPushRejection(null) === null, 'null result is not a rejection');
  assert(terminalPushRejection({ type: 'ctoxError', scope: 'control-plane' }) === null, 'control-plane errors are not push rejections');
}

// A native-shaped nested 422 must reach the existing quarantine/checkpoint
// path, not retry the same bad lead ahead of every later document.
{
  const state = await makeState('outbound_lead_generation_leads');
  let reads = 0;
  let writes = 0;
  let reconciled = 0;
  let pulls = 0;
  const rejected = { id: 'invalid-lead', updated_at_ms: 100 };
  state.collection.storageCollection.getChangedDocumentsSince = async () => {
    reads += 1;
    return reads === 1
      ? { documents: [rejected], checkpoint: { lwt: 100, id: rejected.id } }
      : { documents: [], checkpoint: { lwt: 100, id: rejected.id } };
  };
  state.collection.storageCollection.reconcileRejectedLocalWrites = async (documents) => {
    reconciled += 1;
    assert(documents[0]?.id === rejected.id, 'the rejected lead is quarantined');
    return [rejected.id];
  };
  state.remoteProtocolForPeer = () => ({ peerSession: { sessionId: 'native-1', role: 'ctox_instance' } });
  state.pullFromRemotePeers = async () => { pulls += 1; };
  state.shared.peer = { request: async () => {
    writes += 1;
    return {
      type: 'ctoxError', scope: 'replication', rxdb: true,
      code: 'RC_PUSH', phase: 'replication-push', direction: 'push',
      collection: 'outbound_lead_generation_leads',
      errors: [{ code: 'SNH', parameters: {
        error: { status: 422, message: "required field 'evidence' is missing" },
      } }],
    };
  } };
  await state.pushToPeer('p1');
  assert(writes === 1 && reconciled === 1 && pulls === 1,
    'nested 422 is reconciled once and a clean pull follows');
  assert(state.pushCheckpointsByPeer.get('p1'), 'the push checkpoint advances past the rejected lead');
  await state.cancel();
}

// --- 2. pushToPeer wiring: terminal rejection reconciles, does not re-push ----
function mockCollection(name) {
  return {
    name,
    schema: { version: 0, primaryPath: 'id', hash: async () => `hash-${name}` },
    observe() { return { unsubscribe() {} }; },
    storageCollection: {
      conflictStrategy: 'lww',
      replicationCheckpointStatus: async () => ({ epoch: 'e1', state: 'ready' }),
      getChangedDocumentsSince: async () => ({ documents: [], checkpoint: null }),
      bulkWrite: async () => ({}),
    },
  };
}

async function makeState(name) {
  const state = await replicateWebRTC({
    collection: mockCollection(name),
    topic: `room-${name}-abcdef`,
    connectionHandlerCreator: {
      kind: 'ctox-native-webrtc',
      signalingServerUrl: 'wss://signaling.invalid/?token=t&token_iat=1&token_exp=2',
      config: {},
    },
    pull: { batchSize: 5 },
    push: { batchSize: 5 },
    retryTime: 60,
  });
  state.initialReplication?.catch?.(() => {});
  return state;
}

{
  const state = await makeState('business_records');
  const localDoc = { id: 'rec-1', title: 'unauthorized local edit', updated_at_ms: 100 };
  let reads = 0;
  let reconcileArgs = null;
  let pulls = 0;
  const masterWriteCalls = [];

  state.collection.storageCollection.getChangedDocumentsSince = async () => {
    reads += 1;
    if (reads === 1) return { documents: [localDoc], checkpoint: { lwt: 100, id: 'rec-1' }, scanned: 1, scanLimitReached: false };
    return { documents: [], checkpoint: { lwt: 100, id: 'rec-1' }, scanned: 0, scanLimitReached: false };
  };
  state.collection.storageCollection.reconcileRejectedLocalWrites = async (documents, options) => {
    reconcileArgs = { documents, options };
    return documents.map((doc) => doc.id);
  };
  state.remoteProtocolForPeer = () => ({ peerSession: { sessionId: 'native-1', role: 'ctox_instance' } });
  state.pullFromRemotePeers = async () => { pulls += 1; };
  state.shared.peer = {
    request: async (_peerId, method, params) => {
      assert(method === 'masterWrite', `expected masterWrite, got ${method}`);
      masterWriteCalls.push(params[0]);
      return {
        type: 'ctoxError', scope: 'replication', rxdb: true,
        code: 'RC_WEBRTC_PEER', phase: 'replication-io', direction: 'push',
        collection: 'business_records',
        message: 'peer is not authorized to write collection',
      };
    },
  };
  const errors = [];
  state.error$.subscribe((error) => { if (error) errors.push(error); });

  // Must NOT throw (throwing re-arms the infinite push retry the finding is about).
  await state.pushToPeer('p1');

  assert(masterWriteCalls.length === 1, `masterWrite must not be retried for a terminal rejection (got ${masterWriteCalls.length})`);
  assert(reconcileArgs, 'the rejected batch is reconciled');
  assert(reconcileArgs.documents[0].id === 'rec-1', 'the denied local doc is handed to reconcile');
  assert(reconcileArgs.options?.origin?.role === 'ctox_instance', 'reconcile uses the peer origin role');
  assert(/not authorized/i.test(reconcileArgs.options?.message || ''), 'the rejection reason is passed to reconcile');
  assert(pulls === 1, 'a pull-and-replace is triggered after reconcile');
  assert(
    errors.some((error) => error?.code === 'ctox_replication_push_rejected' && error.terminal === true),
    'a non-fatal terminal push-rejected signal is surfaced',
  );
  assert(state.pushCheckpointsByPeer.get('p1'), 'the push checkpoint advanced past the denied batch');
  await state.cancel();
}

// --- 3. a transient THROWN transport error still rejects (retry preserved) ---
{
  const state = await makeState('business_records');
  let reconcileCalls = 0;
  state.collection.storageCollection.getChangedDocumentsSince = async () => ({
    documents: [{ id: 'rec-2', updated_at_ms: 5 }], checkpoint: { lwt: 5, id: 'rec-2' }, scanned: 1, scanLimitReached: false,
  });
  state.collection.storageCollection.reconcileRejectedLocalWrites = async () => { reconcileCalls += 1; return []; };
  state.remoteProtocolForPeer = () => ({ peerSession: { sessionId: 'native-1', role: 'ctox_instance' } });
  state.shared.peer = { request: async () => { throw new Error('WebRTC peer p1 is not open'); } };

  let threw = false;
  try {
    await state.pushToPeer('p1');
  } catch {
    threw = true;
  }
  assert(threw, 'a transient transport error still rejects out of pushToPeer (retry preserved)');
  assert(reconcileCalls === 0, 'a transient error does NOT trigger the terminal-rejection rollback');
  await state.cancel();
}

// --- 4. storage rollback: journal + force master/base state, clear pushable --
{
  const conflicts = [];
  const reconciledMarks = [];
  const writes = [];
  const store = new Map();
  // record with a merge base (doc existed on master, local edited it)
  store.set('with-base', {
    id: 'with-base', replicationOriginRole: '',
    doc: { id: 'with-base', title: 'local edit', _meta: { lwt: 200 } },
    base: { id: 'with-base', title: 'master state', _meta: { lwt: 100 } },
  });
  // record with no base (created locally, never accepted by master)
  store.set('no-base', {
    id: 'no-base', replicationOriginRole: '',
    doc: { id: 'no-base', title: 'brand new local', _meta: { lwt: 50 } },
    base: null,
  });
  // already-origin record: nothing to reconcile
  store.set('already-synced', {
    id: 'already-synced', replicationOriginRole: 'ctox_instance',
    doc: { id: 'already-synced', _meta: { lwt: 10, ctoxReplicationOrigin: { role: 'ctox_instance' } } },
  });

  const fakeThis = {
    name: 'business_records',
    primaryPath: 'id',
    async initializeRecovery() {},
    async getStoredRecord(id) { return store.get(id) || null; },
    recoveryJournal: {
      async recordConflict(conflict) { conflicts.push(conflict); },
      async markReconciled(collection, ids) { reconciledMarks.push({ collection, ids }); },
    },
    async bulkWrite(rows, options) { writes.push({ rows, options }); },
  };

  const docs = [
    { id: 'with-base' },
    { id: 'no-base' },
    { id: 'already-synced' },
  ];
  const reconciled = await CtoxIndexedDbCollection.prototype.reconcileRejectedLocalWrites.call(
    fakeThis, docs, { origin: { role: 'ctox_instance', peerId: 'p1' }, code: 'authz_rejected', message: 'not authorized' },
  );

  assert(reconciled.length === 2, 'only the two pending local writes are reconciled (already-synced is skipped)');
  assert(reconciled.includes('with-base') && reconciled.includes('no-base'), 'both pending local ids reconciled');

  // journaled: both local versions, recoverable
  assert(conflicts.length === 2, 'each rejected local write is journaled as a conflict');
  const withBaseConflict = conflicts.find((c) => c.local?.id === 'with-base');
  assert(withBaseConflict?.conflictType === 'authz_rejected', 'journaled with conflictType authz_rejected');
  assert(withBaseConflict?.local?.title === 'local edit', 'the rejected LOCAL version is preserved for recovery');

  // rolled back: forced origin write of base/tombstone (clears pushable)
  assert(writes.length === 1, 'the rollback is a single forced write batch');
  const w = writes[0];
  assert(w.options?.force === true, 'rollback is a forced write (overrides the LWW gate)');
  assert(w.options?.replicationOrigin?.role === 'ctox_instance' && w.options?.replicationOrigin?.reconciled === true, 'rollback is an origin write (clears pushable) tagged reconciled');
  assert(w.options?.skipJournal === true, 'rollback bypasses the WAL (it is not a fresh local write)');
  const rolledBase = w.rows.find((r) => (r.document || r).id === 'with-base');
  assert((rolledBase.document || rolledBase).title === 'master state', 'with-base rolls back to the master merge base');
  const rolledTombstone = w.rows.find((r) => (r.document || r).id === 'no-base');
  assert((rolledTombstone.document || rolledTombstone)._deleted === true, 'no-base rolls back to a tombstone (never accepted by master)');

  assert(reconciledMarks.length === 1 && reconciledMarks[0].ids.length === 2, 'the WAL is told to drop the reconciled writes (stops re-push across restarts)');
}

console.log('ctox-rxdb authz/schema push-rejection rollback smoke OK');
process.exit(0);
