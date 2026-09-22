import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import vm from 'node:vm';

const source = readFileSync(new URL('../../app.js', import.meta.url), 'utf8');
const start = source.indexOf('function createContextActionsFacade(');
const end = source.indexOf('\nfunction createLiveBusinessChatFacade(', start);
assert.ok(start >= 0 && end > start, 'test must execute the shell context action facade');
const module = { id: 'tickets', title: 'Tickets' };
const context = { module: 'tickets', record_id: 'ticket-42', context_v2: { record_id: 'ticket-42' } };
const actor = { id: 'requester', role: 'user' };
const scope = { allowed: ['data.read'], record_id: 'ticket-42' };

for (const [action, type, target] of [
  ['ask', 'business_os.context.ask', 'read'],
  ['data', 'business_os.data.modify', 'data'],
  ['app', 'ctox.business_os.app.modify', 'app'],
]) {
  const direct = [];
  const chat = [];
  let resolveAccepted;
  let presented = 0;
  const accepted = new Promise((resolve) => { resolveAccepted = resolve; });
  const facadeFactory = vm.runInNewContext(`(${source.slice(start, end)})`, {
    state: { commandBus: { dispatch: (...args) => { direct.push(args); return Promise.resolve({ status: 'pending_sync' }); } } },
    crypto: { randomUUID: () => 'generated-command' },
    submitBusinessChatTask: (owner, options) => {
      chat.push({ owner, options });
      options.onPresented?.();
      return accepted;
    },
  });
  const facade = facadeFactory(module);
  const options = {
    context, prompt: 'Inspect the selected record', title: 'Selection',
    command_id: `command-${action}`, payload: { crew_member_id: 'crew-one' },
    actor, visible_scope: scope,
    client_context: { source: 'business-os-global-context' },
    crew_identity: { name: 'Ada', shape: 'round', color: 'blue' },
  };
  let settled = false;
  const request = facade.dispatch(action, { ...options, openChat: true, onPresented: () => { presented += 1; } });
  request.then(() => { settled = true; });
  assert.equal(chat.length, 1, 'presentation must start before receipt completion');
  assert.equal(presented, 1);
  assert.equal(direct.length, 0, 'the shell must not submit a second command beside the chat');
  assert.equal(settled, false, 'presentation does not acknowledge backend acceptance');
  const command = chat[0].options;
  assert.equal(command.id, options.command_id);
  assert.equal(command.command_type, type);
  assert.equal(command.record_id, action === 'app' ? module.id : context.record_id);
  assert.equal(command.payload.target, target);
  assert.equal(command.payload.mode, action);
  assert.equal(command.payload.instruction, options.prompt);
  assert.equal(command.payload.crew_member_id, 'crew-one');
  assert.equal(command.payload.context, context.context_v2);
  assert.equal(command.client_context.actor, actor);
  assert.equal(command.client_context.visible_scope, scope);
  assert.equal(command.crew_identity, options.crew_identity);
  resolveAccepted({ status: 'queued', command_id: options.command_id, task_id: 'native-task' });
  assert.equal((await request).task_id, 'native-task');

  await facade.dispatch(action, options);
  assert.equal(direct.length, 1, 'non-presentational context actions keep their direct contract');
  assert.equal(direct[0][1].until, 'local');
  assert.equal(direct[0][0].payload.instruction, command.payload.instruction);
  await assert.rejects(facade.dispatch('invalid', options), /Unsupported context action/);
  assert.equal(direct.length, 1);
}
console.log('context chat submission smoke OK: single command, pending presentation, native receipt and actor/scope preserved');
