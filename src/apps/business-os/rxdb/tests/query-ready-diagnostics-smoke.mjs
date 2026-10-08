import assert from 'node:assert/strict';
import { replicateWebRTC } from '../src/replication-webrtc.mjs';

// Exercise the actual replication waiter. Diagnostics must not change readiness,
// cancellation, capability checks, or subscription ownership.
const state = await replicateWebRTC({
  collection: {
    name: 'query_ready_diagnostics',
    schema: { version: 0, primaryPath: 'id', hash: async () => 'test-schema' },
    observe() { return { unsubscribe() {} }; },
    storageCollection: {
      replicationCheckpointStatus: async () => ({ epoch: 'test-epoch', schemaHash: 'test-schema', state: 'advertised' }),
      getChangedDocumentsSince: async () => ({ documents: [], checkpoint: null }),
      bulkWrite: async () => ({}),
    },
  },
  topic: 'query-ready-diagnostics-fixture',
  connectionHandlerCreator: {
    kind: 'ctox-native-webrtc',
    signalingServerUrl: 'wss://signaling.invalid/?token=t&token_iat=1&token_exp=2',
    config: {},
  },
});
state.initialReplication.catch(() => {});
const subjects = [state.queryReady$, state.peerStates$, state.error$];
const counts = subjects.map((subject) => subject.listeners.size);
const assertClean = () => assert.deepEqual(subjects.map((subject) => subject.listeners.size), counts);
try {
  state.shared.negotiated = null;
  await assert.rejects(state.awaitQueryReady(250), { message: 'Native query readiness exceeded 250ms' });
  assertClean();

  state.error$.next(new Error('previous peer failure'));
  const timeout = state.awaitQueryReady(250);
  state.error$.next(new Error('peer is not authorized for collection'));
  await assert.rejects(timeout, { message: 'Native query readiness exceeded 250ms; last error: peer is not authorized for collection' });
  assertClean();

  state.error$.next(new Error('retained handshake failure'));
  await assert.rejects(state.awaitQueryReady(250), { message: 'Native query readiness exceeded 250ms; last error: retained handshake failure' });
  assertClean();

  state.shared.negotiated = { peerId: 'fixture-peer', queryFetchCapable: true };
  state.shared.isPeerOpen = () => true;
  const generation = state.collectionQueryGenerationToken('fixture-peer');
  assert.ok(generation);
  const ready = state.awaitQueryReady(250);
  state.error$.next(new Error('transient failure before recovery'));
  state.demandStatus.queryDemandReadyGeneration = generation;
  state.queryReady$.next(generation);
  assert.equal(await ready, generation, 'a recovered peer resolves despite an earlier diagnostic error');
  assertClean();
  assert.equal(await state.awaitQueryReady(250), generation, 'already-ready state resolves synchronously');
  assertClean();

  state.shared.negotiated.queryFetchCapable = false;
  await assert.rejects(state.awaitQueryReady(250), /Native WebRTC peer lacks/);
  assertClean();

  state.shared.negotiated = null;
  const cancelled = state.awaitQueryReady(250);
  state.cancelled = true;
  state.peerStates$.next(new Map());
  await assert.rejects(cancelled, { message: 'WebRTC replication cancelled' });
  assertClean();
  state.cancelled = false;
} finally {
  await state.cancel();
}
console.log('query readiness diagnostics and waiter cleanup OK');
