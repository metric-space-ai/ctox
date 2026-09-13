// REGRESSION: a WebRTC transport renewal is not a native authority change.
//
// Strict pin reads capture the collection query generation before waiting for
// multi-tab and demand-loader scheduling. Rebuilding the RTC connection or the
// negotiated protocol object for the SAME native authority must therefore keep
// that generation stable. Each invalidation case below changes exactly one
// native-authority input, so a token which ignores that input cannot pass.

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

function authorityProtocol(overrides = {}) {
  const sessionId = overrides.peerSessionId ?? 'native-session-1';
  const storageGeneration = overrides.storageGeneration ?? 'native-storage-1';
  const checkpointEpoch = overrides.checkpointEpoch ?? 'desktop-checkpoint-1';
  const schemaHash = overrides.schemaHash ?? 'desktop-schema-1';
  return {
    peerSession: { role: 'ctox_instance', sessionId },
    storageGeneration,
    collectionSchemas: {
      desktop_layout: { name: 'desktop_layout', schemaHash },
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

function connect({
  peerId = 'native-peer-1',
  protocol = authorityProtocol(),
  open = true,
} = {}) {
  // New object identities model a rebuilt RTC connection plus renegotiation.
  state.shared.negotiated = { peerId, remoteProtocol: protocol };
  state.shared.peer = { connections: new Map([[peerId, { rtc: Symbol('connection') }]]) };
  state.shared.isPeerOpen = () => open;
  state.activeRemotePeerId = peerId;
}

const token = () => state.collectionQueryGenerationToken();

connect();
const baseline = token();
assert.ok(baseline, 'a healthy native authority must have a strict generation');
assert.deepEqual(JSON.parse(baseline).authority, {
  peerSessionId: 'native-session-1',
  storageGeneration: 'native-storage-1',
  checkpointEpoch: 'desktop-checkpoint-1',
  schemaHash: 'desktop-schema-1',
});

// New negotiated/connection object identities with identical native authority
// are only a transport renewal.
connect();
assert.equal(token(), baseline, 'same native authority must keep the strict generation');

// Each remaining assertion changes exactly one required native-authority input.
for (const [label, overrides] of [
  ['checkpoint epoch', { checkpointEpoch: 'desktop-checkpoint-2' }],
  ['schema hash', { schemaHash: 'desktop-schema-2' }],
  ['storage generation', { storageGeneration: 'native-storage-2' }],
  ['peer session', { peerSessionId: 'native-session-2' }],
]) {
  connect({ protocol: authorityProtocol(overrides) });
  const replaced = token();
  assert.notEqual(replaced, baseline, `${label} must invalidate strict reads`);
  assert.ok(replaced, `${label} invalidation must be a new generation, not unavailability`);
}

// When any required stable authority input is absent, new browser objects must
// still receive different conservative object-identity tokens.
const missingStorage = authorityProtocol({ storageGeneration: '' });
connect({ protocol: missingStorage });
const missingStorageOne = token();
assert.ok(missingStorageOne, 'fallback fencing remains available');
assert.notEqual(JSON.parse(missingStorageOne).authority, {
  peerSessionId: 'native-session-1',
  storageGeneration: '',
  checkpointEpoch: 'desktop-checkpoint-1',
  schemaHash: 'desktop-schema-1',
});
connect({ protocol: missingStorage });
assert.notEqual(
  token(),
  missingStorageOne,
  'incomplete authority must retain object-identity fencing',
);

// Reopen the stable baseline, then prove both open-peer and peer identity are
// still part of the strict boundary.
connect();
assert.equal(token(), baseline);
connect({ open: false });
assert.equal(token(), '', 'a disconnected peer has no strict generation');
connect({ peerId: 'native-peer-2' });
assert.notEqual(token(), baseline, 'a different peer must not reuse the generation');
assert.ok(token(), 'a different healthy peer receives its own generation');

await state.cancel();
console.log('ctox-rxdb WebRTC authority generation smoke OK');
process.exit(0);
