import assert from 'node:assert/strict';
import { webcrypto } from 'node:crypto';
import { createAppActions } from './app-actions.js';

globalThis.crypto ||= webcrypto;
const dispatched = [];
const commandBus = {
  dispatch: async (command, options) => {
    dispatched.push({ command, options });
    return { command_id: command.id, status: options.until };
  },
  getStatus: async (id) => ({ id, status: 'completed' }),
  subscribe: (id, listener) => ({
    id,
    listener,
    ready: Promise.resolve(id),
    unsubscribe() { this.unsubscribed = true; },
  }),
};
let readinessChecks = 0;
const actions = createAppActions({
  module: { id: 'record-workbench' },
  commandBus,
  ensureRuntimeReady: async () => { readinessChecks += 1; },
});
const first = await actions.run('save', { title: 'One' }, { idempotencyKey: 'stable-1' });
const second = await actions.run('save', { title: 'One' }, { idempotencyKey: 'stable-1' });
assert.equal(first.command_id, second.command_id);
assert.equal(dispatched[0].command.command_type, 'ctox.app.action.run');
assert.equal(dispatched[0].command.module, 'record-workbench');
assert.equal(readinessChecks, 2);
assert.deepEqual(dispatched[0].command.payload.input, { title: 'One' });
assert.equal((await actions.getStatus(first.command_id)).status, 'completed');
const unsubscribe = actions.subscribe(first.command_id, () => {});
assert.equal(typeof unsubscribe, 'function');
assert.equal(await unsubscribe.ready, first.command_id);
unsubscribe();
const denied = createAppActions({
  module: { id: 'record-workbench' },
  commandBus: {
    ...commandBus,
    dispatch: async () => ({
      status: 'failed',
      error_code: 'app_action_permission_denied',
      error_message: 'not granted',
    }),
  },
});
await assert.rejects(
  denied.run('save', {}),
  (error) => error?.name === 'CtoxAppActionError'
    && error?.code === 'app_action_permission_denied'
    && error?.status === 'failed',
);
console.log('app actions SDK tests passed');
