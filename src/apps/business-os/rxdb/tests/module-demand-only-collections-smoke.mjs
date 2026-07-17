import assert from 'node:assert/strict';
import { createSyncRuntime, __ctoxSyncTestHooks } from '../../shared/sync.js';

const {
  DEMAND_ONLY_COLLECTION_START_ERROR,
  isDemandOnlyPullCollection,
  isModuleDemandOnlyCollection,
  moduleSyncCollections,
} = __ctoxSyncTestHooks;

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

function createMockSyncRuntime({ emitProtocolCallback = true } = {}) {
  const starts = [];
  const cancels = [];
  const db = {
    mode: 'rxdb',
    raw: {
      desktop_file_chunks: { name: 'desktop_file_chunks' },
    },
    rxdb: {
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
  await runtime.restartCollection('desktop_file_chunks');
  assert.equal(starts.length, 2, 'restartCollection preserves the demand-only lease and restarts replication');
  assert.deepEqual(cancels, ['desktop_file_chunks']);
  await runtime.restartCollections(['desktop_file_chunks']);
  assert.equal(starts.length, 3, 'restartCollections preserves the demand-only lease and restarts replication');
  assert.deepEqual(cancels, ['desktop_file_chunks', 'desktop_file_chunks']);
  await runtime.suspendCollections(['desktop_file_chunks'], 'module-demand-only-suspend-smoke');
  assert.deepEqual(cancels, ['desktop_file_chunks', 'desktop_file_chunks', 'desktop_file_chunks']);
  await runtime.resumeCollections(['desktop_file_chunks']);
  assert.equal(starts.length, 4, 'resumeCollections preserves the demand-only lease after suspension');
  assert.equal(await lease.release(), true, 'lease release succeeds after restart/suspend/resume');
  assert.deepEqual(
    cancels,
    ['desktop_file_chunks', 'desktop_file_chunks', 'desktop_file_chunks', 'desktop_file_chunks'],
    'final release stops the resumed demand-only bridge',
  );
  await runtime.stop();
}

console.log('ctox-rxdb module demand-only collections smoke OK');
