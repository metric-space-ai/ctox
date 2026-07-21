import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import {
  createCommandBus,
  getBusinessOsCapabilityToken,
  normalizeCommandClientContext,
  resetBusinessOsCapabilityTokenCacheForTests,
} from './command-bus.js';

const source = readFileSync(resolve(dirname(fileURLToPath(import.meta.url)), 'command-bus.js'), 'utf8');

test.beforeEach(() => {
  resetBusinessOsCapabilityTokenCacheForTests();
  globalThis.CTOX_BUSINESS_OS_SESSION = {
    capability_token: 'test-capability-token',
    capability_expires_at_ms: Date.now() + 60 * 60 * 1000,
  };
});

test.afterEach(() => {
  delete globalThis.CTOX_BUSINESS_OS_SESSION;
  resetBusinessOsCapabilityTokenCacheForTests();
});

test('command client context normalizer preserves visible app scope and canonical aliases', () => {
  const actor = {
    id: 'team_member',
    display_name: 'Team Member',
    role: 'user',
    is_admin: false,
  };
  const visibleScope = {
    app: {
      module_id: 'inventory',
      module_title: 'Inventory',
      version: 'v1.0.0',
      visibility: 'team',
      can_modify: false,
    },
    data: {
      summary: 'Freigegeben: Inventory Items (inventory_items)',
      granted_collections: ['inventory_items'],
    },
    external_actions: {
      mode: 'none',
      label: 'In diesem Schritt aus',
    },
    selection: {
      module_id: 'inventory',
      column: 'right',
      record_type: 'account',
      record_id: 'acc_1',
      label: 'Account A',
    },
  };

  const context = normalizeCommandClientContext({
    command: {
      command_type: 'business_os.chat.task',
      record_id: 'acc_1',
      payload: {
        mode: 'data',
        target: 'data',
        context: {
          module: 'inventory',
          record_type: 'account',
          record_id: 'acc_1',
          label: 'Account A',
        },
      },
      client_context: {
        module_id: 'inventory',
        action: 'context-chat',
        visible_scope: visibleScope,
      },
    },
    moduleId: 'inventory',
    commandType: 'business_os.chat.task',
    recordId: 'acc_1',
    inboundChannel: 'business_os_chat',
    actor,
  });

  assert.equal(context.module, 'inventory');
  assert.equal(context.module_id, 'inventory');
  assert.equal(context.app_id, 'inventory');
  assert.equal(context.source_module, 'inventory');
  assert.equal(context.command_type, 'business_os.chat.task');
  assert.equal(context.action, 'context-chat');
  assert.equal(context.mode, 'data');
  assert.equal(context.target, 'data');
  assert.equal(context.record_id, 'acc_1');
  assert.equal(context.inbound_channel, 'business_os_chat');
  assert.equal(context.dispatch_transport, 'rxdb-command-bus');
  assert.deepEqual(context.actor, actor);
  assert.equal(context.visible_scope, visibleScope);
  assert.equal(context.scope.visible_scope, visibleScope);
  assert.equal(context.scope.app.module_id, 'inventory');
  assert.equal(context.scope.data.granted_collections[0], 'inventory_items');
  assert.equal(context.scope.external_actions.label, 'In diesem Schritt aus');
  assert.equal(context.scope.selection.record_id, 'acc_1');
});

test('command client context normalizer does not overwrite caller actor', () => {
  const callerActor = { id: 'service_agent', role: 'agent' };
  const sessionActor = { id: 'human_user', role: 'user' };
  const context = normalizeCommandClientContext({
    command: {
      module: 'coding-agents',
      command_type: 'ctox.coding_agent.session.prompt',
      client_context: {
        actor: callerActor,
        source_module: 'coding-agents',
        target: 'external-agent',
      },
    },
    moduleId: 'coding-agents',
    commandType: 'ctox.coding_agent.session.prompt',
    recordId: 'cmd_1',
    inboundChannel: 'business_os.coding_agents',
    actor: sessionActor,
  });

  assert.deepEqual(context.actor, callerActor);
  assert.equal(context.module, 'coding-agents');
  assert.equal(context.module_id, 'coding-agents');
  assert.equal(context.app_id, 'coding-agents');
  assert.equal(context.target, 'external-agent');
  assert.equal(context.scope.app.module_id, 'coding-agents');
  assert.equal(context.scope.command.type, 'ctox.coding_agent.session.prompt');
});

test('command bus scopes demand-only desktop chunk dependencies with leases', () => {
  assert.match(source, /const DEMAND_ONLY_SYNC_COLLECTIONS = new Set/);
  assert.match(source, /'desktop_file_chunks'/);
  assert.match(source, /sync\.leaseCollection\(collection,\s*reason\)/);
  assert.match(source, /releaseSyncPlan\(syncPlan\)/);
  assert.match(source, /cleanContextText\(payload\.source_kind\) === 'zip'/);
});

test('command bus reports missing queue projection as transient tracking state', () => {
  assert.match(source, /status:\s*'projection_pending'/);
  assert.match(source, /transient:\s*true/);
  assert.match(source, /wartet noch auf die Rueckmeldung/);
  assert.doesNotMatch(source, /noch keinen echten Queue-Task/);
});

test('command bus pulls projections without restarting the shared room', () => {
  assert.match(source, /waitForCommandState\(\{[\s\S]*until/);
  assert.match(source, /refreshProjectionBridges\(syncPlan\?\.afterCommand\)/);
  assert.match(source, /pullFromRemotePeers/);
  assert.doesNotMatch(source, /restartProjectionCollections/);
  assert.doesNotMatch(source, /restartCollections\(\['business_commands', 'ctox_queue_tasks'\]\)/);
  assert.match(source, /async submit\(command\)/);
  assert.match(source, /async waitForAccepted\(commandId/);
  assert.match(source, /async waitForTerminal\(commandId/);
  assert.match(source, /subscribe\(commandId, observer\)/);
});

test('command bus rejects conflicting legacy and canonical command types', async () => {
  const bus = createCommandBus({ db: { raw: {} } });
  await assert.rejects(
    bus.submit({
      id: 'cmd-conflicting-type',
      module: 'test',
      type: 'business_os.command',
      command_type: 'business_os.chat.task',
    }),
    (error) => error?.code === 'invalid_command_contract' && error?.retryable === false,
  );
});

test('command bus rejects an unsynchronizable command before inserting it', async () => {
  let inserted = false;
  const bus = createCommandBus({
    db: {
      raw: {
        business_commands: {
          async insert() { inserted = true; },
        },
      },
    },
  });

  await assert.rejects(
    bus.submit({
      id: 'cmd-oversized',
      module: 'research',
      command_type: 'research.systematic.run',
      client_context: { embedded_rows: 'x'.repeat(6 * 1024 * 1024) },
    }),
    (error) => error?.code === 'command_payload_too_large'
      && error?.retryable === false
      && error?.size_bytes > error?.max_bytes,
  );
  assert.equal(inserted, false);
});

test('command bus returns direct control-command result after projection pull', async () => {
  let stored = null;
  const collection = {
    async insert(doc) {
      stored = { ...doc };
    },
    findOne(id) {
      return {
        $: { subscribe() { return { unsubscribe() {} }; } },
        async exec() {
          if (!stored || stored.id !== id) return null;
          return { toJSON: () => ({ ...stored }) };
        },
      };
    },
  };
  let pullCount = 0;
  const bus = createCommandBus({
    db: { raw: { business_commands: collection, ctox_queue_tasks: collection } },
    sync: {
      async startCollection(collectionName) {
        return {
          bridge: {
            state: {
              async awaitInSync() {},
              async pushToRemotePeers() {},
              async pullFromRemotePeers() {
                pullCount += 1;
                if (collectionName === 'business_commands' && stored) {
                  stored = {
                    ...stored,
                    status: 'completed',
                    task_id: '',
                    result: {
                      status: 'device_code',
                      user_code: 'T123-ABCDE',
                      verification_url: 'https://auth.openai.com/codex/device',
                    },
                  };
                }
              },
            },
          },
        };
      },
    },
  });

  const result = await bus.dispatch({
    command_type: 'ctox.subscription_auth.start',
    payload: { provider: 'openai', auth_mode: 'chatgpt_subscription', flow: 'device_code' },
    wait_timeout_ms: 2500,
  });

  assert.equal(result.status, 'completed');
  assert.equal(result.task_id, '');
  assert.equal(result.result.user_code, 'T123-ABCDE');
  assert.ok(pullCount > 0);
});

test('submit writes an immutable lifecycle-v2 shadow envelope and returns locally', async () => {
  let stored = null;
  const metrics = [];
  const collection = {
    async insert(doc) {
      stored = { ...doc };
    },
    findOne() {
      return { async exec() { return null; } };
    },
  };
  const bus = createCommandBus({
    db: { raw: { business_commands: collection, ctox_queue_tasks: collection } },
    sync: { recordCommandMetric(metric) { metrics.push(metric); } },
  });

  const receipt = await bus.submit({
    id: 'cmd-v2-shadow',
    command_type: 'business_os.chat.task',
    module: 'ctox',
    payload: { instruction: 'Run once' },
  });

  assert.equal(receipt.status, 'local');
  assert.equal(stored.contract_version, 2);
  assert.equal(stored.idempotency_key, 'cmd-v2-shadow');
  assert.match(stored.payload_hash, /^sha256:[0-9a-f]{64}$/);
  assert.equal(stored.status, 'pending_sync');
  assert.equal(stored.execution_phase, undefined);
  assert.deepEqual(metrics.map((metric) => metric.name), ['local_submit', 'submit_receipt']);
  assert.ok(metrics.every((metric) => metric.commandId === 'cmd-v2-shadow'));
});

test('submit can push a new command before historical command pull is complete', async () => {
  let stored = null;
  let pushCount = 0;
  let targetedPushCount = 0;
  let initialReplicationAwaited = false;
  const collection = {
    async insert(doc) {
      stored = { ...doc };
    },
    findOne() {
      return { async exec() { return null; } };
    },
  };
  const bus = createCommandBus({
    db: { raw: { business_commands: collection, ctox_queue_tasks: collection } },
    sync: {
      async startCollection() {
        return {
          bridge: {
            state: {
              getTransportStatus() {
                return { demandLoading: { peerConnected: true } };
              },
              async awaitInSync() {
                initialReplicationAwaited = true;
                await new Promise(() => {});
              },
              async pushToRemotePeers() {
                pushCount += 1;
              },
              async pushDocumentsToRemotePeers(documents) {
                targetedPushCount += 1;
                assert.equal(documents.length, 1);
                assert.equal(documents[0].id, 'cmd_cold_history_push');
              },
            },
          },
        };
      },
    },
  });

  const receipt = await bus.submit({
    id: 'cmd_cold_history_push',
    module: 'notes',
    command_type: 'business_os.context.ask',
    record_id: 'note_1',
    payload: { prompt: 'read only' },
  });

  assert.equal(receipt.command_id, 'cmd_cold_history_push');
  assert.equal(stored.id, 'cmd_cold_history_push');
  assert.equal(initialReplicationAwaited, false);
  assert.equal(targetedPushCount, 1);
  assert.equal(pushCount, 0);
});

test('submit waits for the negotiated collection peer before inserting the command', async () => {
  let stored = null;
  let peerStates = new Map();
  const listeners = new Set();
  const peerStates$ = {
    getValue: () => peerStates,
    subscribe(listener) {
      listeners.add(listener);
      listener(peerStates);
      return { unsubscribe: () => listeners.delete(listener) };
    },
  };
  const collection = {
    async insert(doc) {
      assert.equal(peerStates.size, 1);
      stored = { ...doc };
    },
    findOne() {
      return { async exec() { return null; } };
    },
  };
  const state = {
    peerStates$,
    getTransportStatus() {
      return { activePeerCount: peerStates.size };
    },
    async pushToRemotePeers() {},
  };
  const bus = createCommandBus({
    db: { raw: { business_commands: collection, ctox_queue_tasks: collection } },
    sync: {
      async startCollection() {
        return { state };
      },
    },
  });

  const submission = bus.submit({
    id: 'cmd-waits-for-collection-peer',
    command_type: 'business_os.chat.task',
  });
  await new Promise((resolve) => setTimeout(resolve, 20));
  assert.equal(stored, null);
  peerStates = new Map([['native-peer', {}]]);
  listeners.forEach((listener) => listener(peerStates));

  const receipt = await submission;
  assert.equal(receipt.command_id, 'cmd-waits-for-collection-peer');
  assert.equal(stored.id, 'cmd-waits-for-collection-peer');
  assert.equal(listeners.size, 0);
});

test('submit recognizes the transport channelState emitted by CTOX Sync Engine', async () => {
  let inserted = false;
  const collection = {
    async insert() { inserted = true; },
    findOne() { return { async exec() { return null; } }; },
  };
  const bus = createCommandBus({
    db: { raw: { business_commands: collection, ctox_queue_tasks: collection } },
    sync: {
      async startCollection() {
        return {
          state: {
            getTransportStatus() {
              return {
                connectionStates: [{ channelState: 'open', peerConnectionState: 'connected' }],
              };
            },
            async pushToRemotePeers() {},
          },
        };
      },
    },
  });

  await bus.submit({
    id: 'cmd-channel-state-ready',
    command_type: 'business_os.chat.task',
  });
  assert.equal(inserted, true);
});

test('submit does not block on the native queue projection peer', async () => {
  let inserted = false;
  const collection = {
    async insert() { inserted = true; },
    findOne() { return { async exec() { return null; } }; },
  };
  const commandState = {
    getTransportStatus() {
      return {
        connectionStates: [{ channelState: 'open', peerConnectionState: 'connected' }],
      };
    },
    async pushToRemotePeers() {},
  };
  const queueState = {
    getTransportStatus() {
      return { activePeerCount: 1, connectionCount: 1 };
    },
    async pushToRemotePeers() {
      assert.fail('the browser must not push the native queue projection during submit');
    },
  };
  const bus = createCommandBus({
    db: { raw: { business_commands: collection, ctox_queue_tasks: collection } },
    sync: {
      async startCollection(collectionName) {
        return {
          collection: collectionName,
          state: collectionName === 'business_commands' ? commandState : queueState,
        };
      },
    },
  });

  const receipt = await bus.submit({
    id: 'cmd-queue-projection-not-ready',
    command_type: 'business_os.chat.task',
    sync_ready_timeout_ms: 25,
  });

  assert.equal(receipt.command_id, 'cmd-queue-projection-not-ready');
  assert.equal(inserted, true);
});

test('submit reports the blocked collection and observed peer state precisely', async () => {
  const collection = {
    async insert() { assert.fail('command must not be inserted without a collection peer'); },
    findOne() { return { async exec() { return null; } }; },
  };
  const bus = createCommandBus({
    db: { raw: { business_commands: collection, ctox_queue_tasks: collection } },
    sync: {
      async startCollection(collectionName) {
        return {
          collection: collectionName,
          state: {
            getTransportStatus() {
              return { activePeerCount: 0, connectionCount: 0 };
            },
            async pushToRemotePeers() {},
          },
        };
      },
    },
  });

  await assert.rejects(
    bus.submit({
      id: 'cmd-no-collection-peer',
      command_type: 'business_os.chat.task',
      sync_ready_timeout_ms: 25,
    }),
    (error) => error?.code === 'native_unavailable'
      && error?.retryable === true
      && /business_commands/.test(error.message)
      && /active peers: 0/.test(error.message),
  );
});

test('dispatch returns native command and queue task ids after acceptance', async () => {
  let stored = null;
  const listeners = new Set();
  const collection = {
    async insert(doc) { stored = { ...doc }; },
    findOne(id) {
      return {
        $: {
          subscribe(listener) {
            listeners.add(listener);
            if (stored?.id === id) listener({ toJSON: () => ({ ...stored }) });
            return { unsubscribe: () => listeners.delete(listener) };
          },
        },
        async exec() {
          return stored?.id === id ? { toJSON: () => ({ ...stored }) } : null;
        },
      };
    },
  };
  const state = {
    demandStatus: { peerConnected: true },
    async pushToRemotePeers() {
      if (!stored || stored.status === 'accepted') return;
      stored = {
        ...stored,
        status: 'accepted',
        replication_phase: 'native_observed',
        execution_task_id: 'queue-real-7',
      };
      listeners.forEach((listener) => listener({ toJSON: () => ({ ...stored }) }));
    },
    async pullFromRemotePeers() {},
  };
  const bus = createCommandBus({
    db: { raw: { business_commands: collection, ctox_queue_tasks: collection } },
    sync: { async startCollection() { return { state }; } },
  });

  const receipt = await bus.dispatch({
    id: 'cmd-native-accepted',
    command_type: 'business_os.chat.task',
  });

  assert.equal(receipt.command_id, 'cmd-native-accepted');
  assert.equal(receipt.task_id, 'queue-real-7');
  assert.equal(receipt.execution_task_id, 'queue-real-7');
  assert.equal(receipt.transport, 'rxdb-command-bus');
});

test('control command can skip the unrelated queue projection bridge', async () => {
  const startedCollections = [];
  const pushedCollections = [];
  const collection = {
    async insert() {},
    findOne() {
      return { async exec() { return null; } };
    },
  };
  const bus = createCommandBus({
    db: { raw: { business_commands: collection, ctox_queue_tasks: collection } },
    sync: {
      async startCollection(collectionName) {
        startedCollections.push(collectionName);
        return {
          state: {
            getTransportStatus() {
              return { demandLoading: { peerConnected: true } };
            },
            async pushToRemotePeers() {
              pushedCollections.push(collectionName);
            },
          },
        };
      },
    },
  });

  await bus.submit({
    id: 'cmd-control-without-queue',
    command_type: 'outbound.research_source.auth_assist',
    sync_queue_tasks: false,
  });

  assert.deepEqual(startedCollections, ['business_commands']);
  assert.deepEqual(pushedCollections, ['business_commands']);
});

test('duplicate command id rejects a changed immutable payload without regressing state', async () => {
  let stored = null;
  const collection = {
    async insert(doc) {
      if (stored) throw new Error('RxDB Error-Code: CONFLICT');
      stored = { ...doc };
    },
    findOne() {
      return { async exec() { return stored ? { toJSON: () => ({ ...stored }) } : null; } };
    },
  };
  const bus = createCommandBus({
    db: { raw: { business_commands: collection, ctox_queue_tasks: collection } },
  });
  await bus.submit({
    id: 'cmd-idempotency',
    command_type: 'business_os.chat.task',
    payload: { instruction: 'Original' },
  });
  stored = { ...stored, status: 'completed', result: { ok: true } };

  await assert.rejects(
    bus.submit({
      id: 'cmd-idempotency',
      command_type: 'business_os.chat.task',
      payload: { instruction: 'Changed' },
    }),
    (error) => error.code === 'idempotency_conflict',
  );
  assert.equal(stored.status, 'completed');
});

test('completed control command treats legacy task_id as a target rather than an execution task', async () => {
  let stored = null;
  const commands = {
    async insert(doc) {
      stored = {
        ...doc,
        status: 'completed',
        task_id: 'workspace-branding',
        result: { outcome: { ok: true } },
      };
    },
    findOne() {
      return {
        $: { subscribe() { return { unsubscribe() {} }; } },
        async exec() { return stored; },
      };
    },
  };
  const queue = {
    findOne() {
      return { async exec() { return null; } };
    },
  };
  const bus = createCommandBus({ db: { raw: { business_commands: commands, ctox_queue_tasks: queue } } });
  const result = await bus.dispatch({
    id: 'cmd-branding-target',
    command_type: 'ctox.business_os.branding.update',
  });
  assert.equal(result.status, 'completed');
  assert.equal(result.execution_task_id, '');
  assert.equal(result.target_task_id, 'workspace-branding');
});

test('sync push errors remain typed instead of becoming a command timeout', async () => {
  const collection = {
    async insert() {},
    findOne() {
      return { async exec() { return null; } };
    },
  };
  const bus = createCommandBus({
    db: { raw: { business_commands: collection, ctox_queue_tasks: collection } },
    sync: {
      async startCollection() {
        return {
          state: {
            async awaitInSync() {},
            async pushToRemotePeers() {
              const error = new Error('schema hash mismatch');
              error.code = 'ctox_rxdb_schema_hash_mismatch';
              throw error;
            },
          },
        };
      },
    },
  });

  await assert.rejects(
    bus.submit({ id: 'cmd-sync-error', command_type: 'business_os.chat.task' }),
    /schema hash mismatch/,
  );
});

test('capability lookup aborts a hanging request and negatively caches the outage', async (context) => {
  delete globalThis.CTOX_BUSINESS_OS_SESSION;
  resetBusinessOsCapabilityTokenCacheForTests();
  const originalFetch = globalThis.fetch;
  let calls = 0;
  globalThis.fetch = (_url, options = {}) => {
    calls += 1;
    return new Promise((_, reject) => {
      options.signal?.addEventListener('abort', () => reject(new Error('aborted')), { once: true });
    });
  };
  context.after(() => {
    globalThis.fetch = originalFetch;
    resetBusinessOsCapabilityTokenCacheForTests();
  });

  assert.equal(await getBusinessOsCapabilityToken({ timeoutMs: 20 }), null);
  assert.equal(await getBusinessOsCapabilityToken({ timeoutMs: 20 }), null);
  assert.equal(calls, 1);
});

test('command mutation fails before local insertion when authorization is unavailable', async (context) => {
  delete globalThis.CTOX_BUSINESS_OS_SESSION;
  resetBusinessOsCapabilityTokenCacheForTests();
  const originalFetch = globalThis.fetch;
  globalThis.fetch = async () => { throw new Error('offline'); };
  context.after(() => { globalThis.fetch = originalFetch; });
  let inserts = 0;
  const collection = {
    async insert() { inserts += 1; },
    findOne() { return { async exec() { return null; } }; },
  };
  const bus = createCommandBus({
    db: { raw: { business_commands: collection, ctox_queue_tasks: collection } },
  });
  await assert.rejects(
    bus.submit({ id: 'cmd-auth-required', command_type: 'business_os.chat.task' }),
    (error) => error.code === 'auth_required' && error.retryable === true,
  );
  assert.equal(inserts, 0);
});

test('command subscriptions are bounded and release capacity on unsubscribe', async () => {
  const collection = {
    findOne() {
      return {
        $: {
          subscribe() {
            return { unsubscribe() {} };
          },
        },
      };
    },
  };
  const bus = createCommandBus({
    db: { raw: { business_commands: collection, ctox_queue_tasks: collection } },
  });
  const subscriptions = Array.from({ length: 128 }, (_, index) => (
    bus.subscribe(`cmd-watcher-${index}`, () => {})
  ));
  await Promise.all(subscriptions.map((subscription) => subscription.ready));
  assert.throws(
    () => bus.subscribe('cmd-watcher-overflow', () => {}),
    (error) => error.code === 'projection_delayed' && error.retryable === true,
  );
  subscriptions[0].unsubscribe();
  const replacement = bus.subscribe('cmd-watcher-replacement', () => {});
  await replacement.ready;
  replacement.unsubscribe();
  subscriptions.slice(1).forEach((subscription) => subscription.unsubscribe());
});
