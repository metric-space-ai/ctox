// REGRESSION: a native "peer cannot change native-owned document fields"
// rejection is TERMINAL and reconciled, not retried forever.
//
// The native masterWrite validator rejects a whole batch when one row changes
// native-owned fields. Its result carries the reason only in the nested
// `errors[].parameters.message` (top-level code RC_PUSH, no message), so the
// classifier treated it as transient: 157 lead writes in one browser were
// re-pushed and re-denied forever and the leads collection never finished
// replicating (thesen 09.10.2026).

import { replicateWebRTC, replicationWebRtcTestInternals } from '../src/replication-webrtc.mjs';

const { terminalPushRejection } = replicationWebRtcTestInternals;
const assert = (condition, message) => {
  if (!condition) throw new Error(message);
};

// Exact shape observed from the thesen native peer.
const nativeOwned = {
  type: 'ctoxError', scope: 'replication', rxdb: true,
  code: 'RC_PUSH', phase: 'replication-push', direction: 'push', rowCount: 1,
  errors: [{
    rxdb: true,
    code: 'RC_WEBRTC_PEER',
    name: 'RxError (RC_WEBRTC_PEER)',
    message: '\n\n        RxDB Error-Code: RC_WEBRTC_PEER.\n        Hint: Error messages are not included in RxDB core.\n',
    parameters: {
      collection: 'outbound_lead_generation_leads',
      message: 'peer cannot change native-owned document fields',
    },
  }],
};

const rejection = terminalPushRejection(nativeOwned);
assert(rejection, 'the nested native-owned rejection is terminal');
assert(/native-owned/.test(rejection.message), `the nested reason is surfaced, got ${rejection.message}`);
assert(rejection.collection === 'outbound_lead_generation_leads', 'the nested collection is surfaced');
assert(
  terminalPushRejection({ ...nativeOwned, errors: [{ parameters: { message: 'no master handler registered for x' } }] }) === null,
  'a nested transient reason stays retryable',
);

const collection = {
  name: 'outbound_lead_generation_leads',
  schema: { version: 0, primaryPath: 'id', hash: async () => 'hash-leads' },
  observe() { return { unsubscribe() {} }; },
  storageCollection: {
    conflictStrategy: 'lww',
    replicationCheckpointStatus: async () => ({ epoch: 'e1', state: 'ready' }),
    getChangedDocumentsSince: async () => ({ documents: [], checkpoint: null }),
    bulkWrite: async () => ({}),
  },
};
const state = await replicateWebRTC({
  collection,
  topic: 'room-native-owned-abcdef',
  connectionHandlerCreator: {
    kind: 'ctox-native-webrtc',
    signalingServerUrl: 'wss://signaling.invalid/?token=t&token_iat=1&token_exp=2',
    config: {},
  },
  pull: { batchSize: 100 },
  push: { batchSize: 100 },
  retryTime: 60,
});
state.initialReplication?.catch?.(() => {});

const stale = { id: 'lead_1', research_status: 'running', updated_at_ms: 100 };
let reads = 0;
let reconciled = null;
const writes = [];
state.collection.storageCollection.getChangedDocumentsSince = async () => {
  reads += 1;
  if (reads === 1) return { documents: [stale], checkpoint: { lwt: 100, id: 'lead_1' }, scanned: 1, scanLimitReached: false };
  return { documents: [], checkpoint: { lwt: 100, id: 'lead_1' }, scanned: 0, scanLimitReached: false };
};
state.collection.storageCollection.reconcileRejectedLocalWrites = async (documents, options) => {
  reconciled = { documents, options };
  return documents.map((doc) => doc.id);
};
state.remoteProtocolForPeer = () => ({ peerSession: { sessionId: 'native-1', role: 'ctox_instance' } });
state.pullFromRemotePeers = async () => {};
state.shared.peer = {
  request: async (_peerId, method, params) => {
    assert(method === 'masterWrite', `expected masterWrite, got ${method}`);
    writes.push(params[0]);
    return nativeOwned;
  },
};

// Must not throw: throwing re-arms the endless retry this regression is about.
await state.pushToPeer('p1');

assert(writes.length === 1, `the denied write is not retried (got ${writes.length} masterWrite calls)`);
assert(reconciled?.documents?.[0]?.id === 'lead_1', 'the denied local write is reconciled to master');
assert(/native-owned/.test(reconciled?.options?.message || ''), 'the reconcile carries the native reason');
assert(state.pushCheckpointsByPeer.get('p1')?.id === 'lead_1', 'the checkpoint advances past the denied write');
await state.cancel();

console.log('native-owned-rejection-smoke ok');
process.exit(0);
