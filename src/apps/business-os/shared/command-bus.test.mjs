import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import { createCommandBus, normalizeCommandClientContext } from './command-bus.js';

const source = readFileSync(resolve(dirname(fileURLToPath(import.meta.url)), 'command-bus.js'), 'utf8');

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

test('command bus actively pulls command projections while waiting', () => {
  assert.match(source, /waitForAuthoritativeQueueProjection\(\s*currentDb,\s*commandId,[\s\S]*syncPlan/);
  assert.match(source, /refreshProjectionBridges\(syncPlan\?\.afterCommand\)/);
  assert.match(source, /pullFromRemotePeers/);
  assert.match(source, /restartProjectionCollections\(syncPlan\?\.sync\)/);
  assert.match(source, /restartCollections\(\['business_commands', 'ctox_queue_tasks'\]\)/);
});

test('command bus returns direct control-command result after projection pull', async () => {
  let stored = null;
  const collection = {
    async insert(doc) {
      stored = { ...doc };
    },
    findOne(id) {
      return {
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
