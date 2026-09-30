// SYNC-A F1: persisted first-pull readiness distinguishes an unsynced empty
// collection from a successfully synced collection that simply has zero rows.
// The marker is local retained-checkpoint metadata only: no wire fields or
// capability negotiation participate.

import { replicateWebRTC } from '../src/replication-webrtc.mjs';

const storage = new Map();
globalThis.localStorage = {
  getItem(key) { return storage.has(key) ? storage.get(key) : null; },
  setItem(key, value) { storage.set(key, String(value)); },
  removeItem(key) { storage.delete(key); },
};

const collectionName = 'first_pull_readiness';
const topic = 'room-first-pull-readiness-123456';
const baselineToken = mintToken({ uid: 'alice', role: 'user', epoch: 7 });
const changedPermissionToken = mintToken({ uid: 'alice', role: 'user', epoch: 8 });
const protocol = {
  storageGeneration: 'native-generation-A',
  checkpoint: { epoch: 'native-checkpoint-A' },
  peerSession: { sessionId: 'native-session-A', role: 'ctox_instance' },
  collection: { name: collectionName, schemaHash: `hash-${collectionName}` },
  capabilities: ['ctox-checkpoint-generation-v2'],
};

// --- 1. Fresh collections are never live before a pull drains ----------------
const first = await makeState(baselineToken);
assert(
  first.getTransportStatus().collectionReadinessState === 'offline-pending',
  'fresh collection without an open peer must report offline-pending',
);
first.remoteProtocolForPeer = () => protocol;
// Open-peer signal goes through the shared room peer (the production path);
// the replication state itself has no isPeerOpen.
first.shared.isPeerOpen = () => true;
first.shared.negotiated = { peerId: 'peer-1', remoteProtocol: protocol };
first.peerStates$.next(new Map([['peer-1', { peerId: 'peer-1', remoteProtocol: protocol }]]));
await first.resolveReadPermissionDigest();

assert(
  first.getTransportStatus().collectionReadinessState === 'never-synced',
  'fresh connected collection must start never-synced',
);
assert(first.firstPullCompletedAtMs === 0, 'fresh collection must not trust or invent a completion marker');

let releaseInitialPull;
const initialPullGate = new Promise((resolve) => { releaseInitialPull = resolve; });
first.shared.peer = {
  async request(_peerId, method) {
    assert(method === 'masterChangesSince', `unexpected pull method ${method}`);
    await initialPullGate;
    return { documents: [], checkpoint: null };
  },
};

const readinessEvents = [];
const readinessSubscription = first.transportStatus$.subscribe((status) => {
  if (status?.collectionReadinessState) readinessEvents.push(status.collectionReadinessState);
});
const pulling = first.pullFromRemotePeers();
await waitFor(() => first.pullInProgress === true);
assert(
  first.getTransportStatus().collectionReadinessState === 'catching-up',
  'an unfinished first pull must report catching-up, never live',
);
assert(first.firstPullCompletedAtMs === 0, 'marker must remain absent while the first pull is in flight');
releaseInitialPull();
await pulling;

// --- 2. The first successful empty pull is live and persists its marker -------
const completedAt = first.firstPullCompletedAtMs;
assert(completedAt > 0, 'successful initial pull must stamp firstPullCompletedAtMs');
assert(
  first.getTransportStatus().collectionReadinessState === 'live',
  'successfully synced empty collection must report live',
);
assert(readinessEvents.includes('catching-up'), 'diagnostics stream must publish catching-up');
assert(readinessEvents.includes('live'), 'diagnostics stream must publish live');

const retained = onlyRetainedRecord();
assert(
  retained.firstPullCompletedAtMs === completedAt,
  'persisted retained checkpoint must carry the first-pull marker',
);
assert(retained.validityKey, 'retained readiness marker must share checkpoint validity metadata');
first.pushCheckpointsByPeer.set('peer-1', { id: 'later-push', lwt: completedAt + 1 });
await first.persistCheckpointsForPeer('peer-1');
assert(
  onlyRetainedRecord().firstPullCompletedAtMs === completedAt,
  'later checkpoint persists must preserve the original first-pull marker',
);
assert(first.getTransportStatus().collectionFreshnessState === 'live', 'completed current pull confirms freshness');
const successfulPullAt = first.getTransportStatus().lastSuccessfulPullAtMs;
assert(successfulPullAt > 0, 'a successful current pull exposes its completion time');
let releaseRefresh;
const refreshGate = new Promise((resolve) => { releaseRefresh = resolve; });
first.shared.peer.request = async () => {
  await refreshGate;
  return { documents: [], checkpoint: { id: 'current-native-head', lwt: completedAt + 2 } };
};
const refreshing = first.pullFromRemotePeers();
await waitFor(() => first.pullInProgress);
assert(first.getTransportStatus().collectionReadinessState === 'live', 'historical cache readiness survives refresh');
assert(first.getTransportStatus().collectionFreshnessState === 'catching-up', 'historical readiness cannot confirm an unfinished current pull');
releaseRefresh();
await refreshing;
assert(first.getTransportStatus().collectionFreshnessState === 'live', 'the persisted empty response reconfirms current freshness');
const originalCheckpointStatus = first.collection.storageCollection.replicationCheckpointStatus;
first.collection.storageCollection.replicationCheckpointStatus = async () => { throw new Error('checkpoint storage unavailable'); };
await first.pullFromRemotePeers();
assert(first.getTransportStatus().collectionFreshnessState !== 'live', 'a storage failure after an empty response must not confirm freshness');
assert(first.firstPullCompletedAtMs === completedAt, 'failed refresh preserves the historical completion marker');
first.collection.storageCollection.replicationCheckpointStatus = originalCheckpointStatus;
// Peer removal invalidates the current confirmation without deleting the
// checkpoint or the saved first-pull marker. The next connection must drain.

first.shared.isPeerOpen = () => false;
first.shared.openSharedPeerIds = () => [];
assert(first.getTransportStatus().collectionFreshnessState === 'offline-pending', 'disconnected cached data must be visibly unconfirmed');
first.removePeer('peer-1');
assert(first.pullFresh === false, 'peer removal retires its freshness confirmation');
assert(first.firstPullCompletedAtMs === completedAt, 'peer removal keeps historical cache readiness intact');
readinessSubscription.unsubscribe?.();
await first.cancel();

// --- 3. Reload with the same validity inputs restores live before another pull
const reloaded = await makeState(baselineToken);
assert(
  reloaded.getTransportStatus().collectionReadinessState !== 'live',
  'persisted marker must remain untrusted before handshake validity is checked',
);
reloaded.remoteProtocolForPeer = () => protocol;
let readinessAtReloadPull = '';
reloaded.pullFromRemotePeers = async () => {
  readinessAtReloadPull = reloaded.getTransportStatus().collectionReadinessState;
};
reloaded.pushToRemotePeers = async () => {};
await reloaded.runPeerReady('peer-reload', protocol, false);
assert(readinessAtReloadPull === 'live', 'same validity key must restore live before the reload pull starts');
assert(reloaded.getTransportStatus().collectionFreshnessState !== 'live', 'a retained marker never substitutes for a pull in the new connection');
assert(
  reloaded.firstPullCompletedAtMs === completedAt,
  'reload must preserve the original first-pull timestamp',
);
await reloaded.cancel();

// --- 4. Permission/validity change invalidates readiness with checkpoints -----
const invalidated = await makeState(changedPermissionToken);
invalidated.remoteProtocolForPeer = () => protocol;
let readinessAfterInvalidation = '';
invalidated.pullFromRemotePeers = async () => {
  readinessAfterInvalidation = invalidated.getTransportStatus().collectionReadinessState;
};
invalidated.pushToRemotePeers = async () => {};
await invalidated.runPeerReady('peer-permission-change', protocol, false);
assert(readinessAfterInvalidation !== 'live', 'permission digest change must invalidate the live marker');
assert(invalidated.firstPullCompletedAtMs === 0, 'invalidated marker must be cleared in memory');
assert(invalidated.retainedCheckpoints === null, 'invalidated retained checkpoint must be discarded');
assert(storage.size === 0, 'invalidated retained checkpoint must be removed from persistence');
await invalidated.cancel();

console.log('ctox-rxdb first-pull readiness smoke OK');
process.exit(0);

async function makeState(token) {
  const state = await replicateWebRTC({
    collection: mockCollection(collectionName),
    topic,
    connectionHandlerCreator: {
      kind: 'ctox-native-webrtc',
      signalingServerUrl: 'wss://signaling.invalid/?token=t&token_iat=1&token_exp=2',
      config: {},
    },
    pull: { batchSize: 5 },
    push: { batchSize: 5 },
    retryTime: 60,
    ctox: { capabilityTokenProvider: async () => token },
  });
  state.initialReplication?.catch?.(() => {});
  return state;
}

function mockCollection(name) {
  return {
    name,
    schema: {
      version: 0,
      primaryPath: 'id',
      hash: async () => `hash-${name}`,
    },
    observe() { return { unsubscribe() {} }; },
    storageCollection: {
      replicationCheckpointStatus: async () => ({
        epoch: 'browser-checkpoint-A',
        schemaHash: `hash-${name}`,
        state: 'advertised',
      }),
      getChangedDocumentsSince: async () => ({ documents: [], checkpoint: null }),
      bulkWrite: async () => ({}),
    },
  };
}

function onlyRetainedRecord() {
  assert(storage.size === 1, `expected one retained checkpoint record, got ${storage.size}`);
  return JSON.parse(Array.from(storage.values())[0]);
}

function mintToken({ uid, role, epoch }) {
  const payload = Buffer.from(JSON.stringify({ uid, role, epoch, iat: 1, exp: 9_999_999_999 }))
    .toString('base64')
    .replace(/\+/g, '-')
    .replace(/\//g, '_')
    .replace(/=+$/g, '');
  return `${payload}.test-signature`;
}

async function waitFor(predicate, timeoutMs = 1000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    if (predicate()) return;
    await new Promise((resolve) => setTimeout(resolve, 0));
  }
  throw new Error('timed out waiting for test condition');
}

function assert(condition, message) {
  if (!condition) throw new Error(message);
}
