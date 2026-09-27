// REGRESSION: once business_commands.insert() succeeds, a command submission
// is durable. A slow command push is therefore a trackable delivery state,
// not a submit failure that invites a retry with a new command_id.
//
// Runs the real createCommandBus with an in-memory reactive collection.

import assert from 'node:assert/strict';
import {
  createCommandBus,
  getBusinessOsCapabilityToken,
  resetBusinessOsCapabilityTokenCacheForTests,
} from '../../shared/command-bus.js';

resetBusinessOsCapabilityTokenCacheForTests();
globalThis.CTOX_BUSINESS_OS_SESSION = {
  capability_token: 'submit-atomicity-smoke-capability-token',
  capability_expires_at_ms: Date.now() + 60 * 60 * 1000,
};

function reactiveCollection() {
  const documents = new Map();
  const listeners = new Map();
  const emit = (id) => {
    const document = documents.get(id);
    const value = document ? { toJSON: () => ({ ...document }) } : null;
    for (const listener of listeners.get(id) || []) listener(value);
  };
  return {
    documents,
    async insert(document) {
      if (documents.has(document.id)) throw new Error('document already exists');
      documents.set(document.id, { ...document });
      emit(document.id);
    },
    findOne(id) {
      return {
        $: {
          subscribe(listener) {
            const current = listeners.get(id) || new Set();
            current.add(listener);
            listeners.set(id, current);
            emit(id);
            return { unsubscribe: () => current.delete(listener) };
          },
        },
        async exec() {
          const document = documents.get(id);
          return document ? { toJSON: () => ({ ...document }) } : null;
        },
      };
    },
    update(id, patch) {
      documents.set(id, { ...documents.get(id), ...patch });
      emit(id);
    },
  };
}

function makeDb(collection) {
  return {
    raw: {
      business_commands: collection,
      ctox_queue_tasks: collection,
    },
  };
}

function connectedState(pushDocumentsToRemotePeers) {
  return {
    demandStatus: { peerConnected: true },
    pushDocumentsToRemotePeers,
    async pullFromRemotePeers() {},
  };
}

function makeSync(state) {
  return {
    async startCollection(collection) {
      return { collection, state };
    },
  };
}

// 1. The 15-second command push flush expires after the local insert. The
// timeout is accelerated without changing the production timeout value.
{
  const collection = reactiveCollection();
  const state = connectedState(() => new Promise(() => {}));
  const bus = createCommandBus({ db: makeDb(collection), sync: makeSync(state) });
  const nativeSetTimeout = globalThis.setTimeout;
  globalThis.setTimeout = (callback, delay, ...args) => (
    nativeSetTimeout(callback, delay === 15_000 ? 25 : delay, ...args)
  );

  let receipt;
  try {
    receipt = await bus.submit({
      id: 'cmd-submit-push-timeout',
      module: 'ctox',
      command_type: 'business_os.test',
    });
  } finally {
    globalThis.setTimeout = nativeSetTimeout;
  }

  assert.equal(collection.documents.has('cmd-submit-push-timeout'), true, 'command was inserted locally');
  assert.equal(receipt.ok, true, 'local receipt remains a success');
  assert.equal(receipt.command_id, 'cmd-submit-push-timeout');
  assert.equal(receipt.status, 'local', 'existing local receipt status remains compatible');
  assert.equal(receipt.code, 'push_unconfirmed');
  assert.equal(receipt.transient, true);
  assert.equal(receipt.retryable, false, 'caller must track the existing command, not resubmit it');
  assert.equal(receipt.pushConfirmed, false);
  assert.equal(receipt.tracking.command_id, receipt.command_id);
  assert.equal(typeof receipt.resumeTracking, 'function');
  assert.equal(receipt.resumeTracking, receipt.tracking.resumeTracking);
  assert.equal(typeof receipt.tracking.subscribe, 'function');

  const observed = [];
  const subscription = receipt.tracking.subscribe((value) => {
    const document = value?.toJSON?.() || value;
    if (document) observed.push(document.status);
  });
  await subscription.ready;

  const accepted = receipt.resumeTracking({ until: 'accepted', timeoutMs: 1_500 });
  nativeSetTimeout(() => {
    collection.update(receipt.command_id, {
      status: 'accepted',
      replication_phase: 'native_observed',
      execution_task_id: 'queue:submit-atomicity',
    });
  }, 10);
  const tracked = await accepted;
  subscription.unsubscribe();

  assert.equal(tracked.command_id, receipt.command_id);
  assert.equal(tracked.task_id, 'queue:submit-atomicity');
  assert.ok(observed.includes('accepted'), 'subscription tracks the same durable command');
}

// 2. Before the insert boundary, missing storage is still a submit error.
{
  const bus = createCommandBus({ db: { raw: {} } });
  await assert.rejects(
    bus.submit({
      id: 'cmd-submit-no-collection',
      module: 'ctox',
      command_type: 'business_os.test',
    }),
    (error) => error?.command_id === 'cmd-submit-no-collection'
      && /business_commands collection is required/.test(error.message),
  );
}

// A collection that exists but has no authenticated peer also fails before
// insert, so there is still no durable command receipt to return.
{
  const collection = reactiveCollection();
  let inserts = 0;
  const originalInsert = collection.insert;
  collection.insert = async (document) => {
    inserts += 1;
    return originalInsert(document);
  };
  const state = {
    demandStatus: { peerConnected: false },
    getTransportStatus() {
      return { activePeerCount: 0, connectionCount: 0 };
    },
    async pushDocumentsToRemotePeers() {},
  };
  const bus = createCommandBus({ db: makeDb(collection), sync: makeSync(state) });
  await assert.rejects(
    bus.submit({
      id: 'cmd-submit-offline-before-insert',
      module: 'ctox',
      command_type: 'business_os.test',
      sync_ready_timeout_ms: 25,
    }),
    (error) => error?.code === 'native_unavailable' && error?.retryable === true,
  );
  assert.equal(inserts, 0, 'offline readiness failure happens before local insert');
}

// 3. The established successful path keeps the original receipt contract and
// additionally reports a positive push confirmation.
{
  const collection = reactiveCollection();
  let pushedId = '';
  const state = connectedState(async (documents) => {
    pushedId = documents[0]?.id || '';
    return true;
  });
  const bus = createCommandBus({ db: makeDb(collection), sync: makeSync(state) });
  const receipt = await bus.submit({
    id: 'cmd-submit-push-confirmed',
    module: 'ctox',
    command_type: 'business_os.test',
  });

  assert.equal(receipt.ok, true);
  assert.equal(receipt.command_id, 'cmd-submit-push-confirmed');
  assert.equal(receipt.status, 'local');
  assert.equal(receipt.transport, 'rxdb-command-bus');
  assert.equal(receipt.code, 'push_confirmed');
  assert.equal(receipt.transient, false);
  assert.equal(receipt.pushConfirmed, true);
  assert.equal(pushedId, receipt.command_id);
}

// Reusing an ID tracks the original durable command. A renewed token must not
// replace its stored authority or create a distinct explicit wire submission.
for (const terminal of [false, true]) {
  resetBusinessOsCapabilityTokenCacheForTests();
  globalThis.CTOX_BUSINESS_OS_SESSION.capability_token = 'original-authority';
  const collection = reactiveCollection();
  const pushed = [];
  const state = connectedState(async (documents) => {
    pushed.push(...structuredClone(documents));
    return true;
  });
  const bus = createCommandBus({ db: makeDb(collection), sync: makeSync(state) });
  const command = { id: 'cmd-submit-existing-id', module: 'ctox', command_type: 'business_os.test' };
  await bus.submit(command);
  if (terminal) collection.update(command.id, { status: 'completed', terminal_status: 'completed' });
  const original = structuredClone(collection.documents.get(command.id));
  globalThis.CTOX_BUSINESS_OS_SESSION.capability_token = 'renewed-authority';
  await getBusinessOsCapabilityToken({ refresh: true });
  const reused = await bus.submit(command);
  assert.deepEqual(collection.documents.get(command.id), original, 'existing document stays immutable');
  assert.equal(pushed.length, 1, 'existing ID returns tracking without another explicit push');
  assert.equal(pushed[0].client_context.capability_token, 'original-authority');
  assert.equal(reused.tracking.command_id, command.id);
  assert.equal(reused.retryable, false);
  if (terminal) {
    const tracked = await reused.resumeTracking({ until: 'terminal', timeoutMs: 1000 });
    assert.equal(tracked.status, 'completed');
  }
  await assert.rejects(bus.submit({ ...command, payload: { changed: true } }),
    (error) => error.code === 'idempotency_conflict' && error.retryable === false);
  assert.equal(pushed.length, 1, 'different payload cannot push over the existing ID');
  assert.deepEqual(collection.documents.get(command.id), original);
}

console.log('ctox-rxdb command submit atomicity smoke OK');
process.exit(0);
