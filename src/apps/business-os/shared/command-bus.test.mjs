import test from 'node:test';
import { CollectionSyncRegistry } from './sync-collection-registry.js';
import assert from 'node:assert/strict';
import { webcrypto } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import {
  consumeCommandRoundtripTiming,
  createCommandBus,
  getBusinessOsCapabilityToken,
  normalizeCommandClientContext,
  peekCommandRoundtripTiming,
  resetBusinessOsCapabilityTokenCacheForTests,
} from './command-bus.js';

globalThis.crypto ??= webcrypto;

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

test('dispatch receipt lifecycle reports elapsed time from the original dispatch', async (t) => {
  let now = Date.now();
  const startedAt = now;
  t.mock.method(Date, 'now', () => now);
  const events = [];
  t.mock.method(console, 'info', (tag, payload) => {
    if (tag === '[command-bus]') events.push(JSON.parse(payload));
  });
  let stored;
  const commands = {
    async insert(document) { stored = { ...document }; now = startedAt + 125; },
    findOne() {
      return {
        $: { subscribe(listener) {
          listener({ toJSON: () => ({ ...stored }) });
          return { unsubscribe() {} };
        } },
        async exec() { return stored ? { toJSON: () => ({ ...stored }) } : null; },
      };
    },
  };
  const bus = createCommandBus({
    db: { raw: { business_commands: commands } },
    sync: { async startCollection() {
      return { state: { async pushDocumentsToRemotePeers() {
        now = startedAt + 900;
        stored = { ...stored, status: 'queued', replication_phase: 'native_observed' };
        return true;
      } } };
    } },
  });
  const receipt = await bus.dispatch({
    id: 'cmd-lifecycle-elapsed', command_type: 'business_os.smoke', sync_queue_tasks: false,
  }, { until: 'accepted' });
  assert.equal(receipt.status, 'queued');
  for (const phase of ['local_receipt', 'accepted']) {
    const event = events.find((entry) => entry.phase === phase);
    assert.ok(event, `missing ${phase} lifecycle event`);
    assert.equal(event.command_id, 'cmd-lifecycle-elapsed');
    assert.equal(event.elapsed_ms, 900, `${phase} must retain the dispatch start time`);
  }
});

test('sync_queue_tasks:false as a dispatch option skips the queue-task collection', async () => {
  let stored;
  const started = [];
  const commands = {
    async insert(document) { stored = { ...document }; },
    findOne() {
      return {
        $: { subscribe(listener) {
          listener({ toJSON: () => ({ ...stored }) });
          return { unsubscribe() {} };
        } },
        async exec() { return stored ? { toJSON: () => ({ ...stored }) } : null; },
      };
    },
  };
  const bus = createCommandBus({
    db: { raw: { business_commands: commands } },
    sync: { async startCollection(name) {
      started.push(name);
      return { state: { async pushDocumentsToRemotePeers() {
        stored = { ...stored, status: 'queued', replication_phase: 'native_observed' };
        return true;
      } } };
    } },
  });
  await bus.dispatch({ id: 'cmd-option-no-queue', command_type: 'ctox.secret.list' }, {
    until: 'accepted',
    sync_queue_tasks: false,
  });
  assert.deepEqual(started.filter((name) => name === 'ctox_queue_tasks'), []);
  assert.ok(started.includes('business_commands'));
});

function leaseTestState(connected) {
  const listeners = new Set();
  const peers = connected ? new Map([['native', {}]]) : new Map();
  return {
    collection: { name: 'business_commands' },
    peerStates$: {
      getValue: () => peers,
      subscribe(fn) { listeners.add(fn); fn(peers); return { unsubscribe: () => listeners.delete(fn) }; },
    },
    listenerCount: () => listeners.size,
    async pushDocumentsToRemotePeers() { return true; },
  };
}

test('submission follows a retained lease replacement without inserting on the cancelled peer', async () => {
  const registry = new CollectionSyncRegistry();
  const previous = leaseTestState(false);
  registry.set('business_commands', Promise.resolve({ state: previous }));
  let inserts = 0;
  const commands = {
    async insert(doc) { inserts++; assert.equal(doc.id, 'cmd-replaced-lease'); },
    findOne() { return { async exec() { return null; } }; },
  };
  const bus = createCommandBus({
    db: { raw: { business_commands: commands } },
    sync: { async leaseCollection(name) { return registry.acquire(name, 'command', async () => {}); } },
  });
  const pending = bus.submit({ id: 'cmd-replaced-lease', command_type: 'business_os.chat.task', sync_queue_tasks: false });
  await new Promise(resolve => setTimeout(resolve, 20));
  assert.equal(inserts, 0);
  previous.cancelled = true;
  registry.delete('business_commands');
  const replacement = leaseTestState(true);
  registry.set('business_commands', Promise.resolve({ state: replacement }));
  const receipt = await pending;
  assert.equal(receipt.pushConfirmed, true);
  assert.equal(inserts, 1);
  assert.equal(previous.listenerCount(), 0);
  assert.equal(replacement.listenerCount(), 0);
  assert.equal(registry.leaseCount('business_commands'), 0);
});

test('repeated lease replacement cannot reset the command deadline or bypass peer readiness', async () => {
  const registry = new CollectionSyncRegistry();
  registry.set('business_commands', Promise.resolve({ state: leaseTestState(false) }));
  let inserts = 0;
  const bus = createCommandBus({
    db: { raw: { business_commands: { async insert() { inserts++; } } } },
    sync: { async leaseCollection(name) { return registry.acquire(name, 'command', async () => {}); } },
  });
  const replacement = setInterval(() => {
    registry.delete('business_commands');
    registry.set('business_commands', Promise.resolve({ state: leaseTestState(false) }));
  }, 10);
  try {
    await assert.rejects(bus.submit({
      id: 'cmd-lease-deadline', command_type: 'business_os.chat.task',
      sync_queue_tasks: false, sync_ready_timeout_ms: 80,
    }), /no authenticated WebRTC peer after 80 ms/);
    assert.equal(inserts, 0);
    assert.equal(registry.leaseCount('business_commands'), 0);
  } finally { clearInterval(replacement); registry.revokeAllLeases(); }
});


test('terminal tracking moves its master-change subscription to the replacement bridge', async () => {
  const registry = new CollectionSyncRegistry();
  const createState = () => {
    const state = leaseTestState(true);
    const listeners = new Set();
    state.masterChange$ = { subscribe(fn) { listeners.add(fn); return { unsubscribe: () => listeners.delete(fn) }; } };
    state.emit = command => { for (const fn of [...listeners]) fn({ documents: [command] }); };
    state.masterListeners = () => listeners.size;
    return state;
  };
  const old = createState();
  registry.set('business_commands', Promise.resolve({ state: old }));
  const id = 'cmd-master-replacement';
  const commands = {
    findOne() { return {
      $: { subscribe() { return { unsubscribe() {} }; } },
      async exec() { return { id, status: 'queued' }; },
    }; },
  };
  const bus = createCommandBus({
    db: { raw: { business_commands: commands } },
    sync: { async leaseCollection(name) { return registry.acquire(name, 'watch', async () => {}); } },
  });
  const pending = bus.waitForTerminal(id, { timeoutMs: 1000, sync_queue_tasks: false });
  await new Promise(resolve => setTimeout(resolve, 20));
  assert.equal(old.masterListeners(), 1);
  const replacement = createState();
  registry.delete('business_commands');
  registry.set('business_commands', Promise.resolve({ state: replacement }));
  await Promise.resolve();
  assert.equal(old.masterListeners(), 0);
  assert.equal(replacement.masterListeners(), 1);
  replacement.emit({ id, command_id: id, status: 'completed', result: { outcome: { ok: true } } });
  const result = await pending;
  assert.equal(result.ok, true);
  assert.equal(replacement.masterListeners(), 0);
  assert.equal(registry.leaseCount('business_commands'), 0);
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
      command_type: 'ctox.coding.turn',
      client_context: {
        actor: callerActor,
        source_module: 'coding-agents',
        target: 'external-agent',
      },
    },
    moduleId: 'coding-agents',
    commandType: 'ctox.coding.turn',
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
  assert.equal(context.scope.command.type, 'ctox.coding.turn');
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
  assert.match(source, /Die Rückmeldung steht noch aus/);
  assert.doesNotMatch(source, /noch keinen echten Queue-Task/);
});

test('command bus revalidates exact command ids without restarting the shared room', () => {
  assert.match(source, /waitForCommandState\(\{[\s\S]*until/);
  assert.match(source, /refreshProjectionBridges\(syncPlan\?\.afterCommand\)/);
  assert.match(source, /pullFromRemotePeers/);
  assert.match(source, /COMMAND_TERMINAL_REVALIDATE_DELAYS_MS/);
  assert.match(source, /25, 50, 100, 200, 400, 800, 1600, 3000, 5000/);
  assert.match(source, /masterChange\$\?\.subscribe/);
  assert.match(source, /requireRevision/);
  assert.match(source, /bind\(\{ authoritative: true \}\)[\s\S]*scheduleTerminalRevalidation\(index \+ 1\)/);
  assert.doesNotMatch(
    source,
    /refreshProjectionBridges\(syncPlan\?\.afterCommand\)[\s\S]{0,160}bind\(\{ authoritative: true \}\)[\s\S]{0,160}scheduleTerminalRevalidation\(index \+ 1\)/,
  );
  assert.match(source, /scheduleTerminalRevalidation\(index \+ 1\)/);
  assert.match(source, /evaluateCommandDataPlaneProgress/);
  assert.match(source, /repairCommandDataPlaneStall/);
  assert.doesNotMatch(source, /restartProjectionCollections/);
  assert.doesNotMatch(source, /restartCollections\(\['business_commands', 'ctox_queue_tasks'\]\)/);
  assert.match(source, /async submit\(command\)/);
  assert.match(source, /async waitForAccepted\(commandId/);
  assert.match(source, /async waitForTerminal\(commandId/);
  assert.match(source, /subscribe\(commandId, observer\)/);
});

test('command bus exposes bounded read-only status lookup by record id', async () => {
  const seenQueries = [];
  const rows = [
    { id: 'cmd-1', record_id: 'lead-1', command_type: 'web_stack.person_research' },
    { id: 'cmd-2', record_id: 'lead-2', command_type: 'web_stack.person_research' },
  ];
  const commands = {
    find(query) {
      seenQueries.push(query);
      return {
        async exec() {
          return rows.map((row) => ({ toJSON: () => ({ ...row }) }));
        },
      };
    },
  };
  const bus = createCommandBus({ db: { raw: { business_commands: commands } } });
  const result = await bus.getStatusesByRecordIds(['lead-1', 'lead-2', 'lead-1'], {
    commandType: 'web_stack.person_research',
  });
  assert.equal(seenQueries.length, 1);
  assert.deepEqual(seenQueries[0].selector, {
    command_type: { $eq: 'web_stack.person_research' },
  });
  assert.equal(seenQueries[0].limit, 512);
  assert.match(seenQueries[0].requireRevision, /^command-record-status:/);
  assert.deepEqual(result, rows);
});

test('command wait initializes its progress timer before a synchronous terminal emission can settle', () => {
  const waitStart = source.indexOf('async function waitForCommandState');
  const timerDeclaration = source.indexOf('let progressTimer = null;', waitStart);
  const settleDeclaration = source.indexOf('const settle =', waitStart);
  const bindCall = source.indexOf('bind();', settleDeclaration);
  assert.ok(waitStart >= 0);
  assert.ok(timerDeclaration > waitStart && timerDeclaration < settleDeclaration);
  assert.ok(bindCall > settleDeclaration);
  assert.doesNotMatch(source.slice(waitStart, bindCall), /const progressTimer = setInterval/);
});

test('native master-change consumes an authoritative command payload and retains legacy fallback', async () => {
  const commandId = 'cmd-master-change-revalidate';
  let stored = {
    id: commandId,
    command_id: commandId,
    status: 'pending_sync',
  };
  const masterChangeListeners = new Set();
  const authoritativeQueries = [];
  const commands = {
    findOne(idOrQuery) {
      const requireRevision = idOrQuery?.requireRevision || '';
      const id = typeof idOrQuery === 'string'
        ? idOrQuery
        : idOrQuery?.selector?.id;
      if (requireRevision) authoritativeQueries.push(requireRevision);
      return {
        $: { subscribe() { return { unsubscribe() {} }; } },
        async exec() {
          return stored?.id === id ? { toJSON: () => ({ ...stored }) } : null;
        },
      };
    },
  };
  const state = {
    collection: { name: 'business_commands' },
    demandStatus: { peerConnected: true },
    masterChange$: {
      subscribe(listener) {
        masterChangeListeners.add(listener);
        return { unsubscribe: () => masterChangeListeners.delete(listener) };
      },
    },
    async pullFromRemotePeers() {},
  };
  const bus = createCommandBus({
    db: { raw: { business_commands: commands } },
    sync: { async startCollection() { return { state }; } },
  });

  const waiting = bus.waitForTerminal(commandId, {
    timeoutMs: 1000,
    sync_queue_tasks: false,
  });
  await new Promise((resolve) => setTimeout(resolve, 0));
  stored = {
    ...stored,
    status: 'completed',
    execution_phase: 'terminal',
    terminal_status: 'completed',
  };
  masterChangeListeners.forEach((listener) => listener({
    result: { documents: [{ ...stored }] },
  }));

  const receipt = await waiting;
  assert.equal(receipt.status, 'completed');
  assert.equal(authoritativeQueries.length, 0);
  assert.equal(masterChangeListeners.size, 0);

  const legacyCommandId = 'cmd-master-change-legacy';
  stored = {
    id: legacyCommandId,
    command_id: legacyCommandId,
    status: 'pending_sync',
  };
  const legacyWaiting = bus.waitForTerminal(legacyCommandId, {
    timeoutMs: 1000,
    sync_queue_tasks: false,
  });
  await new Promise((resolve) => setTimeout(resolve, 0));
  stored = {
    ...stored,
    status: 'completed',
    execution_phase: 'terminal',
    terminal_status: 'completed',
  };
  masterChangeListeners.forEach((listener) => listener(Date.now()));
  const legacyReceipt = await legacyWaiting;
  assert.equal(legacyReceipt.status, 'completed');
  assert.equal(authoritativeQueries.length, 1);
  assert.match(
    authoritativeQueries[0],
    new RegExp(`^command-terminal:${legacyCommandId}:1$`),
  );
  assert.equal(masterChangeListeners.size, 0);
});

test('finite exact-id retries observe a two-second native command without a master-change hint', async () => {
  const commandId = 'cmd-delayed-terminal-without-hint';
  let stored = {
    id: commandId,
    command_id: commandId,
    status: 'pending_sync',
  };
  const commands = {
    findOne(idOrQuery) {
      const id = typeof idOrQuery === 'string'
        ? idOrQuery
        : idOrQuery?.selector?.id;
      return {
        $: { subscribe() { return { unsubscribe() {} }; } },
        async exec() {
          return stored?.id === id ? { toJSON: () => ({ ...stored }) } : null;
        },
      };
    },
  };
  const state = {
    collection: { name: 'business_commands' },
    demandStatus: { peerConnected: true },
    masterChange$: {
      subscribe() { return { unsubscribe() {} }; },
    },
    async pullFromRemotePeers() {},
  };
  const bus = createCommandBus({
    db: { raw: { business_commands: commands } },
    sync: { async startCollection() { return { state }; } },
  });

  setTimeout(() => {
    stored = {
      ...stored,
      status: 'completed',
      execution_phase: 'terminal',
      terminal_status: 'completed',
      result: { ok: true },
    };
  }, 1100);

  const receipt = await bus.waitForTerminal(commandId, {
    timeoutMs: 3000,
    sync_queue_tasks: false,
  });
  assert.equal(receipt.status, 'completed');
  assert.deepEqual(receipt.result, { ok: true });
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

test('command bus returns direct control-command result after exact-id revalidation', async () => {
  let stored = null;
  const collection = {
    async insert(doc) {
      stored = { ...doc };
    },
    findOne(idOrQuery) {
      const id = typeof idOrQuery === 'string'
        ? idOrQuery
        : idOrQuery?.selector?.id;
      const requireRevision = idOrQuery?.requireRevision || '';
      return {
        $: { subscribe() { return { unsubscribe() {} }; } },
        async exec() {
          if (!stored || stored.id !== id) return null;
          if (requireRevision) {
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
  assert.equal(pullCount, 0);
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
                return true;
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
  assert.equal(receipt.code, 'push_confirmed');
});

test('submit keeps the local command pending when no native peer acknowledges it', async () => {
  let stored = null;
  const collection = {
    async insert(doc) { stored = { ...doc }; },
    findOne() { return { async exec() { return null; } }; },
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
              async pushDocumentsToRemotePeers() { return false; },
            },
          },
        };
      },
    },
  });

  const receipt = await bus.submit({
    id: 'cmd-no-native-ack',
    module: 'notes',
    command_type: 'business_os.context.ask',
    record_id: 'note_2',
    payload: { prompt: 'read only' },
  });

  assert.equal(stored.id, 'cmd-no-native-ack');
  assert.equal(receipt.code, 'push_unconfirmed');
  assert.equal(receipt.pushConfirmed, false);
  assert.equal(receipt.transient, true);
});

test('import command is inserted when immutable dependency flush misses its acknowledgement', async () => {
  let stored = null;
  let commandPushCount = 0;
  const collection = {
    async insert(doc) { stored = { ...doc }; },
    findOne() { return { async exec() { return null; } }; },
  };
  const sync = {
    async leaseCollection(collectionName) {
      return {
        collection: collectionName,
        state: collectionName === 'business_commands'
          ? {
            demandStatus: { peerConnected: true },
            async pushDocumentsToRemotePeers(documents) {
              commandPushCount += 1;
              assert.equal(documents[0].id, 'cmd-import-delivery-lag');
              return true;
            },
          }
          : {
            demandStatus: { peerConnected: true },
            async pushToRemotePeers() { await new Promise(() => {}); },
          },
        async release() {},
      };
    },
  };
  const bus = createCommandBus({
    db: { raw: { business_commands: collection, ctox_queue_tasks: collection } },
    sync,
  });

  const receipt = await bus.submit({
    id: 'cmd-import-delivery-lag',
    module: 'importer',
    command_type: 'ctox.business_os.app.create',
    dependencies: [{ collection: 'desktop_files', record_id: 'file-1', required: true }],
    sync_collections: ['desktop_files', 'desktop_file_chunks'],
    sync_flush_timeout_ms: 100,
    allow_dependency_delivery_lag: true,
  });

  assert.equal(stored.id, 'cmd-import-delivery-lag');
  assert.equal(commandPushCount, 1);
  assert.equal(receipt.code, 'push_confirmed');
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

test('authorized local-first intake persists before the native peer is ready', async () => {
  let stored = null;
  let pushCount = 0;
  const collection = {
    async insert(doc) { stored = { ...doc }; },
    findOne() { return { async exec() { return null; } }; },
  };
  const bus = createCommandBus({
    db: { raw: { business_commands: collection, ctox_queue_tasks: collection } },
    sync: {
      async startCollection() {
        return {
          state: {
            getTransportStatus() {
              return { activePeerCount: 0, connectionCount: 0 };
            },
            async pushToRemotePeers() { pushCount += 1; },
          },
        };
      },
    },
  });

  const receipt = await bus.submit({
    id: 'cmd-local-first-report',
    module: 'ctox',
    command_type: 'ctox.report.bug',
    allow_local_intent_without_peer: true,
    sync_queue_tasks: false,
  });

  assert.equal(receipt.command_id, 'cmd-local-first-report');
  assert.equal(receipt.status, 'local');
  assert.equal(receipt.pushConfirmed, false);
  assert.equal(stored.id, 'cmd-local-first-report');
  assert.equal(stored.status, 'pending_sync');
  assert.equal(pushCount, 0);
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

test('terminal tracking falls back to the local store when demand queries are overloaded', async () => {
  const commandId = 'cmd-native-completed-query-unsupported';
  const completed = {
    id: commandId,
    command_id: commandId,
    status: 'completed',
    result: { outcome: { ok: true } },
  };
  let demandReads = 0;
  let localReads = 0;
  const commands = {
    storageCollection: {
      async findDocumentsById(ids) {
        localReads += 1;
        // The first local receipt probe precedes native completion. After
        // the overloaded demand query, tracking must read the fresh local row.
        const document = localReads === 1 ? { ...completed, status: 'queued' } : completed;
        return ids.includes(commandId) ? { [commandId]: document } : {};
      },
    },
    findOne(id) {
      assert.equal(id, commandId);
      return {
        $: { subscribe() { return { unsubscribe() {} }; } },
        async exec() {
          demandReads += 1;
          const error = new Error('QUERY_QUEUE_LIMIT: queued demand requests exceed the browser budget');
          error.code = 'QUERY_QUEUE_LIMIT';
          throw error;
        },
      };
    },
  };
  const bus = createCommandBus({ db: { raw: { business_commands: commands } } });

  const receipt = await bus.waitForTerminal(commandId, {
    timeoutMs: 1000,
    sync_queue_tasks: false,
  });

  assert.equal(receipt.ok, true);
  assert.equal(receipt.status, 'completed');
  assert.equal(demandReads, 1);
  assert.equal(localReads, 2);
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

test('concurrent cold capability lookups share one native request', async (context) => {
  delete globalThis.CTOX_BUSINESS_OS_SESSION;
  resetBusinessOsCapabilityTokenCacheForTests();
  const originalFetch = globalThis.fetch;
  let calls = 0;
  let resolveFetch;
  globalThis.fetch = () => {
    calls += 1;
    return new Promise((resolve) => { resolveFetch = resolve; });
  };
  context.after(() => {
    globalThis.fetch = originalFetch;
    resetBusinessOsCapabilityTokenCacheForTests();
  });

  const first = getBusinessOsCapabilityToken({ timeoutMs: 1000 });
  const second = getBusinessOsCapabilityToken({ timeoutMs: 1000 });
  await Promise.resolve();
  assert.equal(calls, 1);
  resolveFetch({
    ok: true,
    async json() {
      return {
        capability_token: 'cold-start-capability',
        expires_at_ms: Date.now() + 60 * 60 * 1000,
      };
    },
  });
  assert.deepEqual(await Promise.all([first, second]), [
    'cold-start-capability',
    'cold-start-capability',
  ]);
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

test('command timing probe records seven correlated marks only when requested', async () => {
  const listeners = new Set();
  let stored = null;
  const collection = {
    async insert(document) {
      stored = { ...document };
    },
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
  const metrics = [];
  const bus = createCommandBus({
    db: { raw: { business_commands: collection } },
    sync: {
      recordCommandMetric(metric) { metrics.push(metric); },
      async startCollection() {
        return {
          state: {
            async pushDocumentsToRemotePeers() {
              stored = {
                ...stored,
                status: 'completed',
                execution_phase: 'terminal',
                terminal_status: 'completed',
                updated_at_ms: Date.now(),
                result: {
                  ok: true,
                  command_timing: {
                    native_dispatch_entered: Date.now(),
                    native_handler_completed: Date.now(),
                    native_rxdb_projection_committed: Date.now(),
                  },
                },
              };
              listeners.forEach((listener) => listener({ toJSON: () => ({ ...stored }) }));
              return true;
            },
          },
        };
      },
    },
  });

  const quiet = await bus.dispatch({
    id: 'cmd-timing-quiet',
    command_type: 'ctox.provider_subscription.status',
    until: 'terminal',
    sync_queue_tasks: false,
  });
  assert.equal(quiet.status, 'completed');
  assert.equal(peekCommandRoundtripTiming('cmd-timing-quiet'), null);

  const receipt = await bus.dispatch({
    id: 'cmd-timing-probe',
    command_type: 'ctox.provider_subscription.status',
    until: 'terminal',
    sync_queue_tasks: false,
    client_context: { command_timing_probe: true },
  });
  assert.equal(receipt.status, 'completed');
  const sample = consumeCommandRoundtripTiming('cmd-timing-probe');
  assert.equal(sample.command_id, 'cmd-timing-probe');
  assert.deepEqual(Object.keys(sample.marks).sort(), [
    'browser_dispatch_started',
    'browser_local_inserted',
    'browser_push_confirmed',
    'browser_terminal_observed',
    'native_dispatch_entered',
    'native_handler_completed',
    'native_rxdb_projection_committed',
  ]);
  const marks = sample.marks;
  assert.ok(marks.browser_local_inserted >= marks.browser_dispatch_started);
  assert.ok(marks.browser_push_confirmed >= marks.browser_local_inserted);
  assert.ok(marks.native_handler_completed >= marks.native_dispatch_entered);
  assert.ok(marks.native_rxdb_projection_committed >= marks.native_handler_completed);
  assert.ok(marks.browser_terminal_observed >= marks.browser_push_confirmed);
  assert.ok(metrics.some((metric) => metric.name === 'roundtrip_total'));
  assert.ok(!JSON.stringify(sample).includes('capability_token'));
});
