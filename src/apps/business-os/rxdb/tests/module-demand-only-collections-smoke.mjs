import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { createSyncRuntime, __ctoxSyncTestHooks } from '../../shared/sync.js';
import { createMultiTabSyncCoordinator } from '../src/multi-tab-sync-coordinator.mjs';

const {
  DEMAND_ONLY_COLLECTION_START_ERROR,
  isDemandOnlyPullCollection,
  isModuleDemandOnlyCollection,
  moduleSyncCollections,
  createFollowerBridge,
  flushLeaderDirtyCollection,
  createPendingCollectionBridge,
  shouldReplaceCachedBridgeForStart,
  COMMAND_FOLLOWER_DIRECT_OPEN_TIMEOUT_MS,
  COMMAND_FOLLOWER_DIRECT_FLUSH_TIMEOUT_MS,
  COMMAND_FOLLOWER_BRIDGE_TIMEOUT_MS,
} = __ctoxSyncTestHooks;

{
  let resolveBridge;
  let stopCalls = 0;
  const ready = new Promise((resolve) => { resolveBridge = resolve; });
  const pending = createPendingCollectionBridge('outbound_lead_generation_adapters', ready);
  assert.equal(pending.mode, 'pending');
  assert.equal(pending.reason, 'startup-in-progress');
  assert.equal(pending.collection, 'outbound_lead_generation_adapters');
  assert.equal(pending.ready, ready, 'the bounded handle retains the authoritative bridge promise');
  const stop = pending.stop();
  resolveBridge({ stop: async () => { stopCalls += 1; } });
  assert.equal(await stop, true);
  assert.equal(stopCalls, 1, 'closing a pending module stops the bridge once it materializes');
}

assert(
  COMMAND_FOLLOWER_DIRECT_OPEN_TIMEOUT_MS < COMMAND_FOLLOWER_DIRECT_FLUSH_TIMEOUT_MS,
  'direct failover leaves time to push after the native peer opens',
);
assert(
  COMMAND_FOLLOWER_DIRECT_FLUSH_TIMEOUT_MS < COMMAND_FOLLOWER_BRIDGE_TIMEOUT_MS,
  'follower bridge deadline contains the complete direct failover budget',
);

{
  const follower = createFollowerBridge('document_blob_chunks', { role: 'follower' });
  assert.equal(
    shouldReplaceCachedBridgeForStart(follower, { forceDirect: true }),
    true,
    'a force-direct blob lease replaces a cached follower bridge',
  );
  assert.equal(
    shouldReplaceCachedBridgeForStart(follower, { forceDirect: false }),
    false,
    'ordinary collection starts keep the multi-tab follower bridge',
  );
  assert.equal(
    shouldReplaceCachedBridgeForStart({ mode: 'leader' }, { forceDirect: true }),
    false,
    'force-direct starts do not discard an existing direct bridge',
  );
}

{
  let directFallbackCalls = 0;
  let dirtyRequest = null;
  const follower = createFollowerBridge(
    'business_commands',
    { role: 'follower' },
    {
      async notifyDirtyAndWait(collection, ids) {
        dirtyRequest = { collection, ids };
        throw new Error('leader is frozen');
      },
    },
    async () => {
      directFallbackCalls += 1;
    },
  );
  assert.equal(follower.flushTimeoutMs, COMMAND_FOLLOWER_BRIDGE_TIMEOUT_MS);
  assert.deepEqual(
    await follower.flush([{ id: 'cmd-exact-follower-push' }]),
    { ok: true, mode: 'direct-fallback' },
  );
  assert.deepEqual(dirtyRequest, {
    collection: 'business_commands',
    ids: ['cmd-exact-follower-push'],
  });
  assert.equal(directFallbackCalls, 1, 'frozen leader falls back to a direct WebRTC bridge exactly once');
}

{
  const pushed = [];
  const raw = { id: 'cmd-shared-store', status: 'pending_sync', _rev: '1-a' };
  await flushLeaderDirtyCollection({
    state: {
      collection: {
        storageCollection: {
          async findDocumentsById(ids) {
            assert.deepEqual(ids, ['cmd-shared-store']);
            return { 'cmd-shared-store': raw };
          },
        },
      },
      async pushDocumentsToRemotePeers(documents) {
        pushed.push(...documents);
        return true;
      },
    },
  }, 'business_commands', ['cmd-shared-store']);
  assert.deepEqual(pushed, [raw], 'leader targets the exact command after the shared store exposes it');
}

assert.equal(isDemandOnlyPullCollection('desktop_file_chunks'), true, 'desktop chunks are pull-demand-only');
assert.equal(isDemandOnlyPullCollection('document_blob_chunks'), true, 'document blob chunks are pull-demand-only');
assert.equal(isDemandOnlyPullCollection('spreadsheet_blob_chunks'), true, 'spreadsheet blob chunks are pull-demand-only');
assert.equal(isDemandOnlyPullCollection('user_threads'), true, 'thread headers hydrate through bounded demand queries');
assert.equal(isDemandOnlyPullCollection('user_thread_messages'), true, 'thread messages hydrate through bounded demand queries');
assert.equal(isDemandOnlyPullCollection('user_thread_links'), true, 'thread links hydrate through bounded demand queries');
assert.equal(isDemandOnlyPullCollection('user_notifications'), true, 'thread notifications hydrate through bounded demand queries');
assert.equal(isDemandOnlyPullCollection('ctox_task_approval_requests'), true, 'approval records hydrate through bounded demand queries');
assert.equal(isDemandOnlyPullCollection('business_commands'), true, 'command history hydrates by command id while new commands still push');
assert.equal(isDemandOnlyPullCollection('ctox_queue_tasks'), true, 'queue history hydrates by linked task id');
assert.equal(isDemandOnlyPullCollection('knowledge_tables'), true, 'knowledge table rows hydrate through bounded domain and chunk queries');
assert.equal(isDemandOnlyPullCollection('desktop_files'), false, 'desktop file metadata still pulls normally');

assert.equal(isModuleDemandOnlyCollection('desktop_file_chunks'), true, 'desktop chunks are module demand-only');
assert.equal(isModuleDemandOnlyCollection('document_blob_chunks'), true, 'document blob chunks are module demand-only');
assert.equal(isModuleDemandOnlyCollection('spreadsheet_blob_chunks'), true, 'spreadsheet chunks are module demand-only');
assert.equal(isModuleDemandOnlyCollection('desktop_files'), false, 'desktop file metadata stays module-startable');
assert.equal(isModuleDemandOnlyCollection('documents'), false, 'document metadata stays module-startable');
assert.equal(isModuleDemandOnlyCollection('spreadsheets'), false, 'spreadsheet metadata stays module-startable');
assert.equal(isModuleDemandOnlyCollection('user_threads'), false, 'thread bridges stay module-startable for demand queries');
assert.equal(isModuleDemandOnlyCollection('business_commands'), false, 'the command bridge stays module-startable for push');

assert.deepEqual(
  moduleSyncCollections([
    'business_commands',
    'documents',
    'document_versions',
    'document_blob_chunks',
    'spreadsheet_blob_chunks',
    'desktop_file_chunks',
  ]),
  ['business_commands', 'documents', 'document_versions'],
  'module sync startup skips only large chunk collections',
);

function inertObservable() {
  return {
    subscribe() {
      return { unsubscribe() {} };
    },
  };
}

function createMockReplicationState(collection = 'desktop_file_chunks') {
  const peerId = 'native-peer-1';
  const peerStates = new Map([
    [peerId, {
      remoteProtocol: {
        protocol: 'ctox-rxdb-protocol-v1',
        capabilities: ['ctox-peer-session-v1', 'ctox-checkpoint-epoch-v1'],
        peerSession: { role: 'ctox_instance', sessionId: peerId },
        checkpoint: { state: 'advertised', epoch: 'epoch-1', collection },
      },
    }],
  ]);
  return {
    peer: {
      connections: new Map([
        [peerId, {
          channel: { readyState: 'open' },
          peer: { connectionState: 'connected' },
        }],
      ]),
    },
    peerStates$: {
      getValue() {
        return peerStates;
      },
      subscribe(callback) {
        callback(peerStates);
        return { unsubscribe() {} };
      },
    },
    active$: inertObservable(),
    canceled$: inertObservable(),
    error$: inertObservable(),
    transportStatus$: inertObservable(),
    getTransportStatus() {
      return {};
    },
    async awaitInitialReplication() {
      return true;
    },
    async awaitInSync() {
      return true;
    },
    async cancel() {
      return true;
    },
  };
}

function createMockSyncRuntime({ emitProtocolCallback = true, coordinator = null } = {}) {
  const browserToken = 'browser-role-token';
  const starts = [];
  const cancels = [];
  const db = {
    mode: 'rxdb',
    raw: {
      desktop_file_chunks: { name: 'desktop_file_chunks' },
    },
    rxdb: {
      ...(coordinator ? { getMultiTabSyncCoordinator: () => coordinator } : {}),
      getConnectionHandlerSimplePeer() {
        return {};
      },
      async replicateWebRTC(options) {
        starts.push({
          collection: options.collection?.name || '',
          pull: options.pull ?? null,
          push: options.push ?? null,
        });
        if (emitProtocolCallback) {
          options.ctox?.onPeerProtocol?.({
            protocol: 'ctox-rxdb-protocol-v1',
            capabilities: ['ctox-peer-session-v1', 'ctox-checkpoint-epoch-v1'],
            peerSession: { role: 'ctox_instance', sessionId: 'native-peer-1' },
            checkpoint: { state: 'advertised', epoch: 'epoch-1', collection: options.collection?.name },
          });
        }
        const state = createMockReplicationState(options.collection?.name);
        const cancel = state.cancel;
        state.cancel = async () => {
          cancels.push(options.collection?.name || '');
          return cancel();
        };
        return state;
      },
    },
  };
  const runtime = createSyncRuntime({
    db,
    config: {
      transport: 'webrtc',
      sync_room: 'ctox-business-os:test',
      signaling_urls: ['ws://127.0.0.1/signaling'],
      signaling_auth_version: 'ctox-role-bound-v1',
      signaling_browser_token: browserToken,
      signaling_browser_token_hash: createHash('sha256').update(browserToken).digest('hex'),
      signaling_native_token_hash: createHash('sha256').update('distinct-native-token').digest('hex'),
    },
  });
  return { runtime, starts, cancels };
}

{
  const { runtime } = createMockSyncRuntime({ emitProtocolCallback: false });
  const lease = await runtime.leaseCollection('desktop_file_chunks', 'peer-state-protocol-backfill-smoke');
  const diagnostics = runtime.diagnostics.collections.desktop_file_chunks;
  assert.deepEqual(
    diagnostics.remotePeerSession,
    { role: 'ctox_instance', sessionId: 'native-peer-1' },
    'live peer state backfills a protocol callback missed during bridge startup',
  );
  assert.equal(diagnostics.remoteCheckpoint?.epoch, 'epoch-1');
  assert.equal(diagnostics.peerGeneration, 1);
  await lease.release();
  await runtime.stop();
}

{
  const { runtime, starts } = createMockSyncRuntime();
  await assert.rejects(
    () => runtime.startCollection('desktop_file_chunks'),
    (error) => error?.code === DEMAND_ONLY_COLLECTION_START_ERROR,
    'direct demand-only collection start requires an explicit scoped lease',
  );
  assert.equal(starts.length, 0, 'direct demand-only start must fail before WebRTC replication starts');
  await runtime.stop();
}

{
  const { runtime, starts, cancels } = createMockSyncRuntime();
  const lease = await runtime.leaseCollection('desktop_file_chunks', 'module-demand-only-smoke');
  assert.equal(starts.length, 1, 'lease starts the demand-only collection exactly once');
  assert.equal(starts[0].collection, 'desktop_file_chunks');
  assert.equal(starts[0].pull, null, 'demand-only chunk collection keeps normal pull replication disabled');
  assert.equal(await lease.release(), true, 'lease release succeeds');
  assert.deepEqual(cancels, ['desktop_file_chunks'], 'releasing the final lease stops the demand-only bridge');
  assert.equal(runtime.diagnostics.collections.desktop_file_chunks.status, 'skipped');
  assert.equal(runtime.diagnostics.collections.desktop_file_chunks.connectionStatus, 'demand-only');
  assert.equal(runtime.diagnostics.collections.desktop_file_chunks.reason, 'demand-only-lease-released');
  assert.equal(runtime.diagnostics.collections.desktop_file_chunks.active, false);
  assert.equal(runtime.diagnostics.collections.desktop_file_chunks.frameTransport, null);
  await runtime.stop();
}

{
  const { runtime, starts } = createMockSyncRuntime();
  const restarted = await runtime.restartCollections(['document_blob_chunks']);
  assert.deepEqual(restarted, [], 'batch repair skips an unleased demand-only collection');
  assert.equal(starts.length, 0, 'batch repair must not start demand-only replication without a lease');
  assert.equal(runtime.diagnostics.collections.document_blob_chunks.connectionStatus, 'demand-only');
  await runtime.stop();
}

{
  const { runtime, starts, cancels } = createMockSyncRuntime();
  const lease = await runtime.leaseCollection('desktop_file_chunks', 'module-demand-only-restart-smoke');
  const originalBridge = lease.bridge;
  const replacementBridge = await runtime.restartCollection('desktop_file_chunks');
  assert.notEqual(replacementBridge, originalBridge, 'runtime creates a replacement bridge');
  assert.equal(lease.bridge === replacementBridge, true, 'retained lease must expose the current authoritative bridge after restart');
  assert.equal(starts.length, 2, 'restartCollection preserves the demand-only lease and restarts replication');
  assert.deepEqual(cancels, ['desktop_file_chunks']);
  await runtime.restartCollections(['desktop_file_chunks']);
  assert.notEqual(lease.bridge, replacementBridge, 'the retained lease follows a batch restart too');
  assert.equal(starts.length, 3, 'restartCollections preserves the demand-only lease and restarts replication');
  assert.deepEqual(cancels, ['desktop_file_chunks', 'desktop_file_chunks']);
  await runtime.suspendCollections(['desktop_file_chunks'], 'module-demand-only-suspend-smoke');
  assert.equal(lease.bridge.state, null, 'suspend cannot expose the retired replication state');
  assert.deepEqual(cancels, ['desktop_file_chunks', 'desktop_file_chunks', 'desktop_file_chunks']);
  await runtime.resumeCollections(['desktop_file_chunks']);
  assert.equal(starts.length, 4, 'resumeCollections preserves the demand-only lease after suspension');
  assert.ok(lease.bridge.state, 'resume publishes the new state through the same lease');
  assert.equal(await lease.release(), true, 'lease release succeeds after restart/suspend/resume');
  assert.equal(lease.bridge.mode, 'released', 'a released lease never returns a usable state');
  assert.deepEqual(
    cancels,
    ['desktop_file_chunks', 'desktop_file_chunks', 'desktop_file_chunks', 'desktop_file_chunks'],
    'final release stops the resumed demand-only bridge',
  );
  await runtime.stop();
}

{
  const room = `direct-acquisition-${process.pid}-${Date.now()}`;
  const leader = createMultiTabSyncCoordinator({ databaseName: 'direct-acquisition-test', room, tabId: 'tab-a' });
  const follower = createMultiTabSyncCoordinator({ databaseName: 'direct-acquisition-test', room, tabId: 'tab-b' });
  await leader.start();
  await follower.start();
  await new Promise((resolve) => setTimeout(resolve, 80));
  const { runtime, starts, cancels } = createMockSyncRuntime({ coordinator: follower });
  let lease;
  try {
    assert.equal(follower.isLeader(), false);
    lease = await runtime.leaseCollection('desktop_file_chunks', 'active-file-transfer');
    const direct = await runtime.startCollection('desktop_file_chunks', { forceDirect: true });
    assert.equal(lease.bridge, direct, 'follower promotion updates the existing lease without app-side assignment');
    const acquired = await runtime.startCollection('desktop_file_chunks', { pin: false });
    assert.equal(acquired === direct, true, 'ordinary acquisition must retain the direct bridge serving an active transfer');
    assert.equal(await runtime.startCollection('desktop_file_chunks', { forceDirect: true }), direct);
    assert.equal(starts.length, 1, 'reacquisition must not replace the native registration');
    assert.equal(cancels.length, 0, 'reacquisition must not cancel an in-flight file transfer');
  } finally {
    await lease?.release();
    await runtime.stop();
    await follower.close();
    await leader.close();
  }
}

console.log('ctox-rxdb module demand-only collections smoke OK');
