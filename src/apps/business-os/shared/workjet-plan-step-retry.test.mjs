import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import vm from 'node:vm';

// Workjet „Planschritt wiederholen“ (ctox#872, Workjet #783): workjetProjectControl leitet
// { action: 'ctox.plan.step.retry', commandId, payload: { step_id } } als Business-Command weiter
// und antwortet { action, commandId, status: 'pending' }, sobald der Befehl angenommen ist.
const app = readFileSync(new URL('../app.js', import.meta.url), 'utf8');
const source = app.slice(app.indexOf('const WORKJET_PROJECT_CONTROL_MAX_RESULTS'), app.indexOf('async function waitForSyncBridgeReady', app.indexOf('const WORKJET_PROJECT_CONTROL_MAX_RESULTS')));

function fixture(change = () => {}) {
  const commands = [];
  const options = [];
  const state = {
    session: { id: 'owner' }, db: { collection: () => ({}) },
    sync: {},
    commandBus: { async dispatch(command, opts) {
      commands.push(command); options.push(opts);
      const receipt = { ok: true, status: 'accepted', command_id: command.id };
      change(receipt, state);
      return receipt;
    } },
  };
  const context = { state, actorContext: (session) => ({ id: session.id }) };
  vm.runInNewContext(`${source}\nglobalThis.invoke = workjetProjectControl;`, context);
  return { commands, options, invoke: (request) => context.invoke(request) };
}

test('retry forwards exactly { step_id } as ctox.plan.step.retry and answers pending', async () => {
  const f = fixture();
  const result = await f.invoke({ action: 'ctox.plan.step.retry', commandId: 'retry-1', payload: { step_id: 'step-7' } });
  assert.deepEqual(JSON.parse(JSON.stringify(result)), { action: 'ctox.plan.step.retry', commandId: 'retry-1', status: 'pending' });
  assert.equal(f.commands.length, 1);
  assert.equal(f.commands[0].command_type, 'ctox.plan.step.retry');
  assert.equal(f.commands[0].module, 'ctox');
  assert.equal(f.commands[0].id, 'retry-1');
  assert.equal(f.commands[0].record_id, 'step-7');
  assert.deepEqual(JSON.parse(JSON.stringify(f.commands[0].payload)), { step_id: 'step-7' });
  assert.equal(f.options[0].until, 'accepted');
});

test('optional thread_id is accepted but not forwarded', async () => {
  const f = fixture();
  await f.invoke({ action: 'ctox.plan.step.retry', commandId: 'retry-2', payload: { step_id: 's', thread_id: 't' } });
  assert.deepEqual(JSON.parse(JSON.stringify(f.commands[0].payload)), { step_id: 's' });
});

test('retry rejects missing step, unknown fields and rejected or uncorrelated receipts', async () => {
  await assert.rejects(fixture().invoke({ action: 'ctox.plan.step.retry', commandId: 'r', payload: {} }));
  await assert.rejects(fixture().invoke({ action: 'ctox.plan.step.retry', commandId: 'r' }), /payload object/);
  await assert.rejects(fixture().invoke({ action: 'ctox.plan.step.retry', commandId: 'r', payload: { step_id: 's', actor: 'x' } }), /Unsupported/);
  await assert.rejects(fixture().invoke({ action: 'ctox.plan.step.retry', commandId: 'r', payload: { step_id: 's' }, projectId: 'p' }), /Unsupported/);
  await assert.rejects(fixture((receipt) => { receipt.ok = false; receipt.status = 'rejected'; }).invoke({ action: 'ctox.plan.step.retry', commandId: 'r', payload: { step_id: 's' } }));
  await assert.rejects(fixture((receipt) => { receipt.command_id = 'other'; }).invoke({ action: 'ctox.plan.step.retry', commandId: 'r', payload: { step_id: 's' } }), /uncorrelated/);
});
