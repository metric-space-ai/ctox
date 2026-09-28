// REGRESSION: replication recovery semantics that past regressions deleted.
//
// 1. Push re-run flag: a local write landing while a push is in flight must
//    trigger another push pass (trailing writes of a burst used to sit
//    unsynced until the NEXT local write).
// 2. Pull retry: a failed pull must re-arm via `retryTime` (pulls are
//    otherwise purely event-driven; a quiet collection stayed stale forever).
// 3. Checkpoint retention: pull/push checkpoints survive a peer drop and are
//    re-seeded on reconnect ONLY when both the native storage generation and
//    the local browser collection checkpoint match. A cleared browser store
//    must force a full pull even when the daemon generation is unchanged.
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

// --- 0b. awaitInSync must not certify an offline stale projection ----------
{
  const state = await makeState('await-in-sync-reconnect');
  // The initial replication completed earlier in the session; the native
  // peer then went away for a controlled runtime-schema reconfiguration.
  state.initialReplicationDeferred.resolve(true);
  let releasePeer;
  const peerGate = new Promise((resolve) => { releasePeer = resolve; });
  let pulls = 0;
  let pushes = 0;
  state.waitForOpenPeerId = async () => {
    await peerGate;
    return 'native-peer-reopened';
  };
  state.pullFromRemotePeers = async () => { pulls += 1; };
  state.pushToRemotePeers = async () => { pushes += 1; };

  let settled = false;
  const inSync = state.awaitInSync().then(() => { settled = true; });
  await delay(10);
  assert(!settled, 'awaitInSync must remain pending while the native peer is offline');
  assert(pulls === 0 && pushes === 0, 'offline awaitInSync must not certify an empty pull/push pass');

  releasePeer();
  await inSync;
  assert(pulls === 1, `reconnected awaitInSync must pull exactly once, got ${pulls}`);
  assert(pushes === 1, `reconnected awaitInSync must push exactly once, got ${pushes}`);
  await state.cancel();
}

// --- 0a. critical command collections have checkpoint catch-up timers -----
{
  const commands = await makeState('business_commands');
  let masterChanges = 0;
  const masterChangeSubscription = commands.masterChange$.subscribe(() => {
    masterChanges += 1;
  });
  commands.onMasterChange();
  assert(masterChanges === 1, 'business_commands must expose native master-change hints');
  masterChangeSubscription.unsubscribe();
  assert(commands.periodicPullTimer, 'business_commands must periodically catch up missed master-change frames');
  await commands.cancel();
  assert(!commands.periodicPullTimer, 'cancel must clear the command catch-up timer');

  const ordinary = await makeState('ordinary_collection');
  assert(!ordinary.periodicPullTimer, 'ordinary collections stay event-driven');
  await ordinary.cancel();
}

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

// --- 1c. urgent command writes bypass an unrelated collection backlog -------
{
  const state = await makeState('business_commands');
  const pushed = [];
  state.collection.schema.primaryPath = 'id';
  state.openPeerIds = () => ['p1'];
  state.shared.peer = {
    async request(peerId, method, [rows], _timeoutMs, collection) {
      assert(peerId === 'p1', `unexpected peer ${peerId}`);
      assert(method === 'masterWrite', `expected masterWrite, got ${method}`);
      assert(collection === 'business_commands', `unexpected collection ${collection}`);
      pushed.push(...rows.map((row) => row.newDocumentState));
      return [];
    },
  };
  state.demandSidecar = { markDirty: async () => new Promise(() => {}) };
  state.pushInProgressPromise = new Promise(() => {});

  const confirmed = await state.pushDocumentsToRemotePeers([{ id: 'cmd-urgent', status: 'pending_sync' }]);

  assert(pushed.length === 1, `targeted push must write one command, got ${pushed.length}`);
  assert(pushed[0].id === 'cmd-urgent', 'targeted push wrote the wrong command');
  assert(confirmed === true, 'targeted push must confirm an answered native masterWrite');
  let retryScheduled = false;
  state.openPeerIds = () => [];
  state.schedulePushRetry = () => { retryScheduled = true; };
  const unconfirmed = await state.pushDocumentsToRemotePeers([{ id: 'cmd-offline' }]);
  assert(unconfirmed === false, 'targeted push without an open peer must stay unconfirmed');
  assert(retryScheduled, 'targeted push without an open peer must schedule retry');
  await state.cancel();
}

// --- 1d. targeted writes wait for a late native collection handler ----------
{
  const state = await makeState('document_blob_chunks');
  let requests = 0;
  state.collection.schema.primaryPath = 'id';
  state.openPeerIds = () => ['p1'];
  state.shared.peer = {
    async request() {
      requests += 1;
      if (requests < 3) {
        return {
          type: 'ctoxError',
          scope: 'replication',
          code: 'RC_WEBRTC_PEER',
          message: 'no master handler registered for document_blob_chunks',
        };
      }
      return [];
    },
  };

  await state.pushDocumentsToRemotePeers([{
    id: 'blob_1_0',
    blob_id: 'blob_1',
    idx: 0,
    total: 1,
  }]);

  assert(requests === 3, `targeted push should retry the late handler twice, got ${requests}`);
  await state.cancel();
}

// --- 1e. large confirmed writes stay below the framed-transfer ceiling ------
{
  const state = await makeState('document_blob_chunks');
  const batches = [];
  state.collection.schema.primaryPath = 'id';
  state.openPeerIds = () => ['p1'];
  state.shared.peer = {
    async request(_peerId, method, [rows], _timeoutMs, collection) {
      assert(method === 'masterWrite', `expected masterWrite, got ${method}`);
      assert(collection === 'document_blob_chunks', `unexpected collection ${collection}`);
      batches.push(rows);
      return [];
    },
  };
  const largeDocuments = Array.from({ length: 6 }, (_, index) => ({
    id: `blob_large_${index}`,
    blob_id: 'blob_large',
    idx: index,
    total: 6,
    data: 'x'.repeat(700_000),
  }));

  await state.pushDocumentsToRemotePeers(largeDocuments);

  assert(batches.length === 3, `large direct push should be split into 3 batches, got ${batches.length}`);
  assert(
    batches.every((batch) => batch.length === 2),
    `large direct push should preserve two rows per bounded batch, got ${batches.map((batch) => batch.length)}`,
  );
  assert(
    batches.flat().map((row) => row.newDocumentState.id).join(',') === largeDocuments.map((row) => row.id).join(','),
    'bounded direct push must preserve document order and content',
  );
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

// --- 2b. a malformed masterWrite reply cannot acknowledge a local write ----
for (const [label, reply] of [['missing', undefined], ['null', null], ['object', {}]]) {
  const state = await makeState(`masterwrite-${label}`);
  const pending = { id: `lead-${label}`, _meta: { lwt: 101 } };
  state.collection.storageCollection.getChangedDocumentsSince = async () => ({
    documents: [pending],
    checkpoint: { lwt: 101, id: pending.id },
    scanned: 1,
    scanLimitReached: false,
  });
  state.shared.peer = { request: async () => reply };
  for (const push of [() => state.pushToPeer('p1'), () => state.writeDocumentsToPeer('p1', [pending])]) {
    let rejected = null;
    try { await push(); } catch (error) { rejected = error; }
    assert(rejected?.code === 'ctox_replication_invalid_master_write_result',
      `${label}: malformed masterWrite must fail both push paths`);
    assert(!state.pushCheckpointsByPeer.has('p1'),
      `${label}: a local write must remain behind the push checkpoint`);
  }
  await state.cancel();
}

// --- 2c. a malformed pull reply cannot mark the collection synchronized ---
for (const [label, reply] of [['missing', undefined], ['null', null], ['array', []], ['object', {}]]) {
  const state = await makeState(`masterchanges-${label}`);
  state.shared.peer = { request: async () => reply };
  let rejected = null;
  try { await state.pullFromPeer('p1'); } catch (error) { rejected = error; }
  assert(rejected?.code === 'ctox_replication_invalid_master_changes_result',
    `${label}: malformed masterChangesSince must fail the pull`);
  assert(!state.pullCheckpointsByPeer.has('p1'),
    `${label}: a malformed pull must not advance the checkpoint`);
  assert(!state.firstPullCompletedAtMs,
    `${label}: a malformed pull must not mark an empty collection live`);
  await state.cancel();
}

// --- 2d. stale pending business commands absorb authoritative master ------
{
  const state = await makeState('business_commands');
  const localPending = {
    id: 'cmd-device-code',
    command_id: 'cmd-device-code',
    command_type: 'ctox.subscription_auth.start',
    status: 'pending_sync',
    updated_at_ms: 100,
  };
  const masterCompleted = {
    ...localPending,
    status: 'completed',
    result: { status: 'device_code', user_code: 'T5IZ-W9AFF' },
    updated_at_ms: 200,
  };
  let readCalls = 0;
  let writeCalls = 0;
  let absorbed = null;
  state.collection.storageCollection.getChangedDocumentsSince = async () => {
    readCalls += 1;
    if (readCalls === 1) {
      return {
        documents: [localPending],
        checkpoint: { lwt: 100, id: 'cmd-device-code' },
        scanned: 1,
        scanLimitReached: false,
      };
    }
    return {
      documents: [],
      checkpoint: { lwt: 100, id: 'cmd-device-code' },
      scanned: 0,
      scanLimitReached: false,
    };
  };
  state.collection.storageCollection.bulkWrite = async (docs, options = {}) => {
    writeCalls += 1;
    absorbed = { docs, options };
    return { success: { 'cmd-device-code': docs[0] }, error: [] };
  };
  state.remoteProtocolForPeer = () => ({
    peerSession: { sessionId: 'native-business-commands', role: 'ctox_instance' },
  });
  state.shared.peer = {
    request: async (_peerId, method, params) => {
      assert(method === 'masterWrite', `expected masterWrite, got ${method}`);
      assert(params[0][0].newDocumentState.status === 'pending_sync', 'local pending command is pushed');
      return [masterCompleted];
    },
  };
  await state.pushToPeer('p1');
  assert(writeCalls === 1, `authoritative command conflict should be absorbed once, got ${writeCalls}`);
  assert(absorbed?.docs?.[0]?.status === 'completed', 'completed master command must be stored locally');
  assert(
    absorbed?.docs?.[0]?.result?.user_code === 'T5IZ-W9AFF',
    'device code from master command must survive local absorption',
  );
  assert(
    absorbed?.options?.replicationOrigin?.role,
    'absorbed master command must be written as replication-origin state',
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

// --- 3b. large knowledge-table pulls drain every byte-bounded response -----
{
  const state = await makeState('knowledge_tables');
  const docs = [];
  let expectedRows = 0;
  for (let chunkIndex = 0; chunkIndex < 57; chunkIndex += 1) {
    const rowCount = chunkIndex < 31 ? 86 : 85;
    expectedRows += rowCount;
    docs.push({
      id: `measured_load_points:${chunkIndex}`,
      chunk_index: chunkIndex,
      chunk_count: 57,
      row_count: rowCount,
      rows: Array.from({ length: rowCount }, (_, rowIndex) => ({
        row_id: chunkIndex * 100 + rowIndex,
        value: 'x'.repeat(4500),
      })),
    });
  }
  assert(expectedRows === 4876, `large knowledge fixture row count mismatch: ${expectedRows}`);
  let stored = [];
  state.collection.storageCollection.bulkWrite = async (batch) => {
    stored = stored.concat(batch);
    return {};
  };
  let requestCount = 0;
  state.shared.peer = {
    request: async (_peerId, method, params) => {
      assert(method === 'masterChangesSince', `expected masterChangesSince, got ${method}`);
      requestCount += 1;
      const checkpoint = params[0];
      const nextIndex = checkpoint?.id ? Number(String(checkpoint.id).split(':').at(-1)) + 1 : 0;
      if (nextIndex >= docs.length) return { documents: [], checkpoint };
      // Mirrors the native byte limiter: a short non-empty response advances
      // only to the last document actually returned.
      return {
        documents: [docs[nextIndex]],
        checkpoint: { id: docs[nextIndex].id, lwt: nextIndex + 1 },
      };
    },
  };
  await state.pullFromPeer('p1');
  const receivedRows = stored.reduce((sum, doc) => sum + doc.rows.length, 0);
  assert(stored.length === 57, `large knowledge pull lost chunks: ${stored.length}/57`);
  assert(receivedRows === 4876, `large knowledge pull lost rows: ${receivedRows}/4876`);
  assert(requestCount === 58, `large knowledge pull must finish with an empty probe: ${requestCount}`);
  assert(
    state.pullCheckpointsByPeer.get('p1')?.id === 'measured_load_points:56',
    'large knowledge pull must persist the last received checkpoint',
  );
  await state.cancel();
}

// --- 4. checkpoint retention across reconnects -------------------------------
{
  const state = await makeState('checkpoints');
  const protoSameGeneration = {
    checkpoint: { epoch: 'checkpoint-epoch-1' },
    peerSession: { sessionId: 'rxdb-rs-run-A', role: 'ctox_instance' },
    collection: { schemaHash: 'schema-hash-A' },
    capabilities: [],
  };
  state.remoteProtocolForPeer = () => protoSameGeneration;
  state.pullFromRemotePeers = async () => {};
  state.pushToRemotePeers = async () => {};

  state.peerStates$.next(new Map([['peer-1', { peerId: 'peer-1' }]]));
  state.pullCheckpointsByPeer.set('peer-1', { lwt: 111 });
  state.pushCheckpointsByPeer.set('peer-1', { lwt: 222 });
  state.localCheckpointValidityKey = 'checkpoint-epoch-1|';

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

  // A primary IndexedDB reset clears the local collection but not localStorage.
  // The old persistent pull checkpoint must not hide all master documents.
  state.peerStates$.next(new Map([['peer-2', { peerId: 'peer-2' }]]));
  state.removePeer('peer-2', 'test-browser-reset');
  state.collection.storageCollection.replicationCheckpointStatus = async () => ({
    epoch: 'browser:checkpoints:empty',
    schemaHash: 'hash-checkpoints',
    state: 'advertised',
  });
  await state.runPeerReady('peer-browser-reset', protoSameGeneration, false);
  assert(
    !state.pullCheckpointsByPeer.has('peer-browser-reset'),
    'retention: browser primary reset must discard the stale pull checkpoint',
  );
  assert(state.retainedCheckpoints === null, 'retention: browser reset clears persistent checkpoints');

  // Drop again, then reconnect with a DIFFERENT daemon run: full resync.
  state.peerStates$.next(new Map([['peer-browser-reset', { peerId: 'peer-browser-reset' }]]));
  state.pullCheckpointsByPeer.set('peer-browser-reset', { lwt: 333 });
  state.removePeer('peer-browser-reset', 'test-drop');
  const protoNewGeneration = {
    checkpoint: { epoch: 'checkpoint-epoch-1' },
    peerSession: { sessionId: 'rxdb-rs-run-B', role: 'ctox_instance' },
    collection: { schemaHash: 'schema-hash-A' },
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
