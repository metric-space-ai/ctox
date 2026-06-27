// REGRESSION: replication recovery semantics that past regressions deleted.
//
// 1. Push re-run flag: a local write landing while a push is in flight must
//    trigger another push pass (trailing writes of a burst used to sit
//    unsynced until the NEXT local write).
// 2. Pull retry: a failed pull must re-arm via `retryTime` (pulls are
//    otherwise purely event-driven; a quiet collection stayed stale forever).
// 3. Checkpoint retention: pull/push checkpoints survive a peer drop and are
//    re-seeded on reconnect ONLY when the native storage generation
//    (epoch + peer session id) matches — otherwise a full resync is forced.
//
// The test drives the real CtoxWebRtcReplicationState through replicateWebRTC
// with a mock collection; network-level methods are stubbed per instance so
// the class logic under test runs unmodified.

import { replicateWebRTC, replicationWebRtcTestInternals } from '../src/replication-webrtc.mjs';

function mockCollection(name) {
  return {
    name,
    schema: { version: 0, hash: async () => `hash-${name}` },
    observe() { return { unsubscribe() {} }; },
    storageCollection: {
      replicationCheckpointStatus: async () => ({ epoch: 'checkpoint-epoch-1', state: 'ready' }),
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
  // cancel() rejects the initial-replication deferred; without a consumer the
  // rejection would crash node. (sync.js awaits it in production.)
  state.initialReplication?.catch?.(() => {});
  return state;
}

const delay = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

// --- 0. remote-origin-only changes must not trigger local push scans -------
{
  assert(
    replicationWebRtcTestInternals.changeEventHasOnlyReplicationOriginWrites({
      success: {
        a: { id: 'a', _meta: { ctoxReplicationOrigin: { role: 'ctox_instance' } } },
        b: { id: 'b', _meta: { ctoxReplicationOrigin: { role: 'ctox_instance' } } },
      },
    }),
    'remote-origin-only writes should not trigger a push scan',
  );
  assert(
    !replicationWebRtcTestInternals.changeEventHasOnlyReplicationOriginWrites({
      success: {
        a: { id: 'a', _meta: { ctoxReplicationOrigin: { role: 'ctox_instance' } } },
        local: { id: 'local', _meta: { lwt: 5 } },
      },
    }),
    'mixed remote/local writes must still trigger a push scan',
  );
}

// --- 1. push re-run flag ----------------------------------------------------
{
  const state = await makeState('push-rerun');
  let pushPasses = 0;
  let releaseFirstPush;
  const firstPushGate = new Promise((resolve) => { releaseFirstPush = resolve; });
  state.openPeerIds = () => ['p1'];
  state.pushToPeer = async () => {
    pushPasses += 1;
    if (pushPasses === 1) await firstPushGate;
  };
  const inFlight = state.pushToRemotePeers();
  await delay(10);
  // Local write lands while the first push is still in flight:
  state.pushToRemotePeers();
  releaseFirstPush();
  await inFlight;
  assert(pushPasses === 2, `push re-run: expected 2 push passes, got ${pushPasses}`);
  await state.cancel();
}

// --- 1b. local write bursts coalesce into one push scan ----------------------
{
  const state = await makeState('push-coalesce');
  let pushPasses = 0;
  state.pushToRemotePeers = async () => {
    pushPasses += 1;
  };
  state.scheduleLocalWritePush();
  state.scheduleLocalWritePush();
  state.scheduleLocalWritePush();
  assert(state.localPushTimer, 'local write push debounce timer must be armed');
  await delay(80);
  assert(pushPasses === 1, `local write burst should run one push pass, got ${pushPasses}`);
  assert(!state.localPushTimer, 'local write push debounce timer must clear after firing');
  await state.cancel();
}

// --- 2. push scan continues after empty scan-limit batches ------------------
{
  const state = await makeState('push-scan-limit');
  const reads = [
    {
      documents: [],
      checkpoint: { lwt: 100, id: 'remote-only' },
      scanned: 300,
      scanLimitReached: true,
    },
    {
      documents: [{ id: 'local-doc', _meta: { lwt: 101 } }],
      checkpoint: { lwt: 101, id: 'local-doc' },
      scanned: 1,
      scanLimitReached: false,
    },
    {
      documents: [],
      checkpoint: { lwt: 101, id: 'local-doc' },
      scanned: 0,
      scanLimitReached: false,
    },
  ];
  let readCalls = 0;
  let writeCalls = 0;
  state.collection.storageCollection.getChangedDocumentsSince = async () => reads[readCalls++] || reads.at(-1);
  state.shared.peer = {
    request: async (_peerId, method, params) => {
      assert(method === 'masterWrite', `expected masterWrite, got ${method}`);
      assert(params[0][0].newDocumentState.id === 'local-doc', 'local doc must be pushed after remote-only scan page');
      writeCalls += 1;
      return [];
    },
  };
  await state.pushToPeer('p1');
  assert(readCalls >= 2, `push scan must continue past empty scan-limit page (reads=${readCalls})`);
  assert(writeCalls === 1, `exactly one local batch should be pushed (writes=${writeCalls})`);
  assert(state.demandStatus.localPushChangedSinceCalls >= 2, 'local push changed-since reads must be counted');
  assert(
    state.demandStatus.localPushChangedSinceScannedRows === 301,
    `local push scanned rows mismatch: ${state.demandStatus.localPushChangedSinceScannedRows}`,
  );
  assert(
    state.demandStatus.localPushChangedSinceScanLimitHits === 1,
    `local push scan-limit hits mismatch: ${state.demandStatus.localPushChangedSinceScanLimitHits}`,
  );
  assert(
    state.demandStatus.localPushChangedSinceMaxScannedRows === 300,
    `local push max scanned rows mismatch: ${state.demandStatus.localPushChangedSinceMaxScannedRows}`,
  );
  await state.cancel();
}

// --- 3. pull retry via retryTime ---------------------------------------------
{
  const state = await makeState('pull-retry');
  let pullAttempts = 0;
  state.openPeerIds = () => ['p1'];
  state.reportPeerResults = () => {};
  state.pullFromPeer = async () => {
    pullAttempts += 1;
    if (pullAttempts === 1) throw new Error('transient pull failure');
  };
  await state.pullFromRemotePeers();
  assert(pullAttempts === 1, 'pull retry: first attempt ran');
  assert(state.pullRetryTimer, 'pull retry: retry timer armed after a failed pull');
  await delay(1200); // retry delay is clamped to >= 1000ms (anti-hammering floor)
  assert(pullAttempts >= 2, `pull retry: retry fired (attempts=${pullAttempts})`);
  await state.cancel();
  assert(!state.pullRetryTimer, 'pull retry: cancel clears the retry timer');
}

// --- 4. checkpoint retention across reconnects -------------------------------
{
  const state = await makeState('checkpoints');
  const protoSameGeneration = {
    checkpoint: { epoch: 'checkpoint-epoch-1' },
    peerSession: { sessionId: 'rxdb-rs-run-A', role: 'ctox_instance' },
    capabilities: [],
  };
  state.remoteProtocolForPeer = () => protoSameGeneration;
  state.pullFromRemotePeers = async () => {};
  state.pushToRemotePeers = async () => {};

  state.peerStates$.next(new Map([['peer-1', { peerId: 'peer-1' }]]));
  state.pullCheckpointsByPeer.set('peer-1', { lwt: 111 });
  state.pushCheckpointsByPeer.set('peer-1', { lwt: 222 });

  state.removePeer('peer-1', 'test-drop');
  assert(state.retainedCheckpoints, 'retention: checkpoints retained on peer drop');
  assert(!state.pullCheckpointsByPeer.has('peer-1'), 'retention: live map cleared');

  // Reconnect with the SAME storage generation: checkpoints are re-seeded.
  await state.runPeerReady('peer-2', protoSameGeneration, false);
  assert(
    state.pullCheckpointsByPeer.get('peer-2')?.lwt === 111,
    'retention: pull checkpoint re-seeded for the new peer id',
  );
  assert(
    state.pushCheckpointsByPeer.get('peer-2')?.lwt === 222,
    'retention: push checkpoint re-seeded for the new peer id',
  );

  // Drop again, then reconnect with a DIFFERENT daemon run: full resync.
  state.peerStates$.next(new Map([['peer-2', { peerId: 'peer-2' }]]));
  state.removePeer('peer-2', 'test-drop');
  const protoNewGeneration = {
    checkpoint: { epoch: 'checkpoint-epoch-1' },
    peerSession: { sessionId: 'rxdb-rs-run-B', role: 'ctox_instance' },
    capabilities: [],
  };
  await state.runPeerReady('peer-3', protoNewGeneration, false);
  assert(
    !state.pullCheckpointsByPeer.has('peer-3'),
    'retention: NO seeding across daemon runs (full resync is the safe path)',
  );
  assert(state.retainedCheckpoints === null, 'retention: stale checkpoints dropped');
  await state.cancel();
}

// --- 5. transient 'disconnected' keeps the replication peer state ----------
{
  const state = await makeState('disconnected-grace');
  const removed = [];
  state.removePeer = (peerId, reason) => removed.push({ peerId, reason });
  state.onSharedEvent('peer-state', { peerId: 'p1', state: 'disconnected' });
  assert(removed.length === 0, "transient 'disconnected' must NOT drop the replication peer state");
  state.onSharedEvent('peer-state', { peerId: 'p1', state: 'failed' });
  assert(removed.length === 1 && removed[0].reason === 'peer-failed', "terminal 'failed' drops the peer");
  await state.cancel();
}

// --- 5. cancel unregisters before slow sidecar cleanup ----------------------
{
  const state = await makeState('cancel-unregister-order');
  const events = [];
  let releaseClose;
  state.shared.unregister = (collection) => events.push(`unregister:${collection}`);
  state.demandLoader = {
    abortAllInFlight(reason) { events.push(`abort:${reason}`); },
  };
  state.demandSidecar = {
    stopEvictionScheduler() { events.push('stop-eviction'); },
    close() {
      events.push('close-start');
      return new Promise((resolve) => { releaseClose = resolve; });
    },
  };
  const cancelPromise = state.cancel();
  await delay(10);
  assert(
    events[0] === 'unregister:cancel-unregister-order',
    `cancel order: shared peer must unregister before cleanup starts, got ${events.join(',')}`,
  );
  assert(state.shared === null, 'cancel order: state.shared is cleared before slow cleanup finishes');
  releaseClose();
  await cancelPromise;
  assert(events.includes('close-start'), 'cancel order: sidecar close still runs');
}

console.log('ctox-rxdb replication recovery smoke OK');

function assert(condition, message) {
  if (!condition) throw new Error(message);
}
process.exit(0);
