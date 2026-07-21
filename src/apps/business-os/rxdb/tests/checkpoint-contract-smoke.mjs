// SYNC-03: cross-language pin of the checkpoint wire shape.
//
// The contract lives in src/core/rxdb/tests/fixtures/webrtc-checkpoint-contract.json
// and is consumed on the Rust side by unit tests in
// src/core/rxdb/src/storage/sqlite/instance.rs (checkpoint status object +
// epoch derivation) and src/core/rxdb/src/plugins/replication_webrtc/index_mod.rs
// (handshake key paths). This smoke is the JS consumer:
//
// 1. Structural pins: field list, capability names, handshake key paths and
//    the empty replication cursor must match the literals the browser runtime
//    depends on.
// 2. Algorithm pin: the fixture's worked example (latestIdHash, epoch) is
//    recomputed with node:crypto sha256 — a Rust-side change to the epoch
//    input format turns this red without touching any JS code.
// 3. Behavior pin: the v1/v2 checkpoint-validity keys are derived by the REAL
//    replication code (checkpointValidityKeyFromProtocol via
//    removePeer/runPeerReady on a live CtoxWebRtcReplicationState), not a
//    reimplementation — the retained-checkpoint validityKey must equal the
//    fixture's worked-example key byte for byte, checkpoints must be re-seeded
//    under the same v2 storage generation (even across native sessions), and
//    a different storage generation must force the full-resync reset.

import { createHash } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { replicateWebRTC } from '../src/replication-webrtc.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const fixturePath = resolve(
  here,
  '..',
  '..',
  '..',
  '..',
  'core',
  'rxdb',
  'tests',
  'fixtures',
  'webrtc-checkpoint-contract.json',
);
const fixture = JSON.parse(await readFile(fixturePath, 'utf8'));

const sha256 = (text) => createHash('sha256').update(text).digest('hex');

function assert(condition, message) {
  if (!condition) throw new Error(message);
}

// --- 1. structural pins ------------------------------------------------------
assert(fixture.contract === 'ctox-checkpoint-contract-v1', `unexpected contract name: ${fixture.contract}`);
assert(
  JSON.stringify(fixture.checkpointStatus.fields)
    === JSON.stringify(['source', 'state', 'collection', 'schemaHash', 'latestLwt', 'latestIdHash', 'epoch']),
  'checkpoint status field list drifted',
);
assert(fixture.checkpointStatus.source === 'rxdb-rs-sqlite', 'checkpoint source drifted');
assert(fixture.checkpointStatus.state === 'advertised', 'checkpoint state drifted');
assert(fixture.capabilities.peerSession === 'ctox-peer-session-v1', 'peer session capability drifted');
assert(fixture.capabilities.checkpointEpoch === 'ctox-checkpoint-epoch-v1', 'checkpoint epoch capability drifted');
assert(
  fixture.capabilities.checkpointGeneration === 'ctox-checkpoint-generation-v2',
  'checkpoint generation capability drifted',
);
assert(
  JSON.stringify(fixture.replicationCursor.empty) === JSON.stringify({ id: '', lwt: 0 }),
  'empty replication cursor drifted',
);
for (const path of ['collection.checkpoint', 'collectionCheckpoints', 'storageGeneration', 'peerSession.sessionId', 'nativeTimeMs']) {
  assert(
    fixture.handshake.checkpointKeyPaths.includes(path),
    `handshake checkpoint key path missing from fixture: ${path}`,
  );
}

// --- 2. worked example recomputed with node:crypto ---------------------------
const { example } = fixture;
assert(
  sha256(example.latestId) === example.latestIdHash,
  `latestIdHash mismatch: sha256(${example.latestId}) !== ${example.latestIdHash}`,
);
const epochInput = fixture.checkpointStatus.epochInputTemplate
  .replace('{databaseName}', example.databaseName)
  .replace('{collectionName}', example.collectionName)
  .replace('{schemaHash}', example.schemaHash)
  // Rust renders the f64 lwt via Display: integer values carry no decimal
  // point, so String(Number) matches byte for byte.
  .replace('{latestLwt}', String(example.latestLwt))
  .replace('{latestId}', example.latestId);
assert(epochInput === example.epochInput, `epochInput mismatch:\nexpected: ${JSON.stringify(example.epochInput)}\nactual:   ${JSON.stringify(epochInput)}`);
assert(sha256(epochInput) === example.epoch, `epoch mismatch: sha256 gave ${sha256(epochInput)}, fixture says ${example.epoch}`);

// Validity-key templates render to the worked-example keys.
const v1 = fixture.validityKeys.v1;
const v2 = fixture.validityKeys.v2;
const renderedV2 = v2.template
  .replace('{storageGeneration}', v2.example.storageGeneration)
  .replace('{collectionName}', v2.example.collectionName)
  .replace('{schemaHash}', v2.example.schemaHash)
  .replace('{epoch}', v2.example.epoch);
assert(renderedV2 === v2.example.key, `v2 validity key template mismatch: ${renderedV2}`);
assert(v2.example.epoch === example.epoch, 'v2 example epoch must be the worked-example epoch');
const renderedV1 = v1.template
  .replace('{epoch}', v1.example.epoch)
  .replace('{sessionId}', v1.example.sessionId)
  .replace('{schemaHash}', v1.example.schemaHash);
assert(renderedV1 === v1.example.key, `v1 validity key template mismatch: ${renderedV1}`);
assert(v1.example.epoch === example.epoch, 'v1 example epoch must be the worked-example epoch');

// --- 3. validity keys derived by the REAL replication code -------------------
// Mirrors replication-recovery-smoke.mjs: replicateWebRTC with a mock
// collection; the class logic under test (checkpointValidityKeyFromProtocol
// via removePeer/runPeerReady) runs unmodified.
function mockCollection(name) {
  return {
    name,
    schema: { version: 0, hash: async () => `hash-${name}` },
    observe() { return { unsubscribe() {} }; },
    storageCollection: {
      replicationCheckpointStatus: async () => ({ epoch: 'local-epoch-1', state: 'advertised' }),
      getChangedDocumentsSince: async () => ({ documents: [], checkpoint: null }),
      bulkWrite: async () => ({}),
    },
  };
}

async function makeState(name) {
  const state = await replicateWebRTC({
    collection: mockCollection(name),
    topic: `room-${name}-123456`,
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
  state.pullFromRemotePeers = async () => {};
  state.pushToRemotePeers = async () => {};
  return state;
}

// 3a. v2 path: storageGeneration|collectionName|schemaHash, gated on the
// generation capability, independent of the native per-run session id.
{
  const state = await makeState(v2.example.collectionName);
  const protoRunA = {
    capabilities: [v2.capability],
    storageGeneration: v2.example.storageGeneration,
    checkpoint: { epoch: example.epoch },
    peerSession: { sessionId: 'rxdb-rs-run-a', role: 'ctox_instance' },
    collection: { name: v2.example.collectionName, schemaHash: v2.example.schemaHash },
  };
  await state.runPeerReady('peer-1', protoRunA, false);
  state.pullCheckpointsByPeer.set('peer-1', { lwt: 111 });
  state.pushCheckpointsByPeer.set('peer-1', { lwt: 222 });
  state.removePeer('peer-1', 'test-drop');
  assert(state.retainedCheckpoints, 'v2: checkpoints must be retained on peer drop');
  assert(
    state.retainedCheckpoints.validityKey === v2.example.key,
    `v2 validity key drifted from the fixture:\nexpected: ${v2.example.key}\nactual:   ${state.retainedCheckpoints.validityKey}`,
  );

  // Reconnect under the SAME storage generation but a DIFFERENT native
  // session: v2 keys ignore the session id, so checkpoints are re-seeded.
  const protoRunB = { ...protoRunA, peerSession: { sessionId: 'rxdb-rs-run-b', role: 'ctox_instance' } };
  await state.runPeerReady('peer-2', protoRunB, false);
  assert(
    state.pullCheckpointsByPeer.get('peer-2')?.lwt === 111,
    'v2: pull checkpoint must be re-seeded under the same storage generation',
  );
  assert(
    state.pushCheckpointsByPeer.get('peer-2')?.lwt === 222,
    'v2: push checkpoint must be re-seeded under the same storage generation',
  );

  // A DIFFERENT storage generation invalidates retained checkpoints: full
  // resync (no seeding, retention dropped).
  state.removePeer('peer-2', 'test-drop');
  assert(state.retainedCheckpoints, 'v2: checkpoints retained again before the generation change');
  const protoGen2 = { ...protoRunB, storageGeneration: `${v2.example.storageGeneration}-next` };
  await state.runPeerReady('peer-3', protoGen2, false);
  assert(
    !state.pullCheckpointsByPeer.has('peer-3'),
    'v2: NO checkpoint seeding across storage generations (full resync is the safe path)',
  );
  assert(state.retainedCheckpoints === null, 'v2: stale retained checkpoints must be dropped');
  await state.cancel();
}

// 3b. v1 fallback path (no generation capability): epoch|sessionId|schemaHash.
{
  const state = await makeState(v2.example.collectionName);
  const protoV1 = {
    capabilities: [fixture.capabilities.peerSession, fixture.capabilities.checkpointEpoch],
    checkpoint: { epoch: v1.example.epoch },
    peerSession: { sessionId: v1.example.sessionId, role: 'ctox_instance' },
    collection: { name: v2.example.collectionName, schemaHash: v1.example.schemaHash },
  };
  await state.runPeerReady('peer-1', protoV1, false);
  state.pullCheckpointsByPeer.set('peer-1', { lwt: 333 });
  state.removePeer('peer-1', 'test-drop');
  assert(state.retainedCheckpoints, 'v1: checkpoints must be retained on peer drop');
  assert(
    state.retainedCheckpoints.validityKey === v1.example.key,
    `v1 validity key drifted from the fixture:\nexpected: ${v1.example.key}\nactual:   ${state.retainedCheckpoints.validityKey}`,
  );

  // Same epoch + session: re-seeded.
  await state.runPeerReady('peer-2', protoV1, false);
  assert(
    state.pullCheckpointsByPeer.get('peer-2')?.lwt === 333,
    'v1: pull checkpoint must be re-seeded for the same epoch and session',
  );

  // A new native session id under v1 invalidates the retained checkpoints.
  state.removePeer('peer-2', 'test-drop');
  const protoNewSession = { ...protoV1, peerSession: { sessionId: `${v1.example.sessionId}-next`, role: 'ctox_instance' } };
  await state.runPeerReady('peer-3', protoNewSession, false);
  assert(
    !state.pullCheckpointsByPeer.has('peer-3'),
    'v1: NO checkpoint seeding across native sessions',
  );
  assert(state.retainedCheckpoints === null, 'v1: stale retained checkpoints must be dropped');
  await state.cancel();
}

console.log('ctox-rxdb checkpoint contract smoke OK');
process.exit(0);
