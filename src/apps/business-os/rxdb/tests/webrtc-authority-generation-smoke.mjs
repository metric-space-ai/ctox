// REGRESSION: a WebRTC transport renewal is not a native authority change.
//
// Strict pin reads capture the collection query generation before waiting for
// multi-tab and demand-loader scheduling. Rebuilding the RTC connection or the
// negotiated protocol object for the SAME native peer session must therefore
// keep that generation stable; only a changed native authority may invalidate
// the read.

import assert from 'node:assert/strict';
import { replicateWebRTC } from '../src/replication-webrtc.mjs';

function mockCollection(name) {
  return {
    name,
    schema: { version: 3, hash: async () => `hash-${name}` },
    observe() { return { unsubscribe() {} }; },
    storageCollection: {
      databaseName: 'authority-generation-db',
      replicationCheckpointStatus: async () => ({ epoch: 'checkpoint-epoch-1', state: 'ready' }),
      getChangedDocumentsSince: async () => ({ documents: [], checkpoint: null }),
      bulkWrite: async () => ({}),
    },
  };
}

const state = await replicateWebRTC({
  collection: mockCollection('desktop_layout'),
  topic: 'room-authority-generation-123456',
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

function nativeProtocol(sessionId, checkpointEpoch = 'desktop-checkpoint-1') {
  return {
    peerSession: { role: 'ctox_instance', sessionId },
    storageGeneration: 'native-storage-1',
    collectionSchemas: {
      desktop_layout: { name: 'desktop_layout', schemaHash: 'desktop-schema-1' },
    },
    collectionCheckpoints: {
      desktop_layout: {
        collection: 'desktop_layout',
        epoch: checkpointEpoch,
        latestLwt: 500,
      },
    },
  };
}

function connect(negotiated) {
  // New object identities model a rebuilt RTC connection plus renegotiation.
  state.shared.negotiated = negotiated;
  state.shared.peer = { connections: new Map([['native-peer-1', { rtc: Symbol('connection') }]]) };
  state.shared.isPeerOpen = () => true;
  state.activeRemotePeerId = 'native-peer-1';
}

connect({ peerId: 'native-peer-1', remoteProtocol: nativeProtocol('native-session-1') });
const beforeTransportRenewal = state.collectionQueryGenerationToken();
assert.ok(beforeTransportRenewal, 'a healthy native authority must have a strict generation');

connect({ peerId: 'native-peer-1', remoteProtocol: nativeProtocol('native-session-1') });
assert.equal(
  state.collectionQueryGenerationToken(),
  beforeTransportRenewal,
  'same native authority across transport renewal must keep the strict generation',
);

connect({ peerId: 'native-peer-1', remoteProtocol: nativeProtocol('native-session-2') });
assert.notEqual(
  state.collectionQueryGenerationToken(),
  beforeTransportRenewal,
  'a new native peer session must invalidate strict reads',
);

connect({ peerId: 'native-peer-1', remoteProtocol: nativeProtocol('native-session-2', 'desktop-checkpoint-2') });
assert.notEqual(
  state.collectionQueryGenerationToken(),
  beforeTransportRenewal,
  'a new collection checkpoint must invalidate strict reads',
);

await state.cancel();
console.log('ctox-rxdb WebRTC authority generation smoke OK');
process.exit(0);
