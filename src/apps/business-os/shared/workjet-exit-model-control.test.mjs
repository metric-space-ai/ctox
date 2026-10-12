import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import vm from 'node:vm';

const app = readFileSync(new URL('../app.js', import.meta.url), 'utf8');
const source = app.slice(app.indexOf('const WORKJET_PROJECT_CONTROL_MAX_RESULTS'), app.indexOf('async function waitForSyncBridgeReady', app.indexOf('const WORKJET_PROJECT_CONTROL_MAX_RESULTS')));
function fixture(change = () => {}) {
  const commands = [];
  const state = {
    session: { id: 'owner-alias' }, db: { collection: () => ({}) },
    sync: { async startCollection(name) { assert.equal(name, 'business_commands'); return {}; } },
    commandBus: { async dispatch(command) {
      commands.push(command);
      const receipt = { ok: true, status: 'completed', command_id: command.id,
        target_record_id: command.payload.project_id,
        result: { ok: true, assessment: { contract: 'ctox.workjet.exit_model.v1', project_id: command.payload.project_id,
          status: 'blocked', result: null, missing_inputs: ['confirmed_resource_plan'] } } };
      change(receipt, state); return receipt;
    } },
  };
  const context = { state, actorContext: session => ({ id: session.id }) };
  vm.runInNewContext(`${source}\nglobalThis.invoke = workjetProjectControl;`, context);
  return { commands, invoke: request => context.invoke(request) };
}
test('exit refresh carries explicit resource proposal through existing native command plane', async () => {
  const f = fixture();
  const result = await f.invoke({ action: 'project.exit_model.refresh', commandId: 'refresh-1', projectId: 'p1', asOf: '2026-10-08',
    resources: { hoursPerWeek: 20, monthlyBudgetEur: 300, comparisonMode: 'equal_resources' } });
  assert.equal(result.assessment.status, 'blocked'); assert.equal(result.assessment.result, null);
  assert.equal(result.commandId, 'refresh-1'); assert.equal(result.projectId, 'p1');
  assert.equal(f.commands[0].command_type, 'ctox.workjet.exit_model.refresh');
  assert.deepEqual(JSON.parse(JSON.stringify(f.commands[0].payload)), { project_id: 'p1', as_of: '2026-10-08',
    resources: { hours_per_week: 20, monthly_budget_eur: 300, comparison_mode: 'equal_resources' } });
});
test('exit reads reject source writes and receipt/session identity drift', async () => {
  const request = { action: 'project.exit_model.read', commandId: 'r1', projectId: 'p1' };
  await assert.rejects(fixture().invoke({ ...request, inputs: {} }), /identity only/);
  await assert.rejects(fixture(receipt => { receipt.result.assessment.project_id = 'foreign'; }).invoke(request), /uncorrelated/);
  await assert.rejects(fixture(receipt => { receipt.command_id = 'wrong'; }).invoke(request), /uncorrelated/);
  await assert.rejects(fixture((_, state) => { state.session = { id: 'foreign' }; }).invoke(request), /uncorrelated/);
});
test('exit resource proposal has no implicit defaults or hidden allocations', async () => {
  const request = { action: 'project.exit_model.refresh', commandId: 'r1', projectId: 'p1' };
  const f=fixture(); await f.invoke(request); assert.deepEqual(JSON.parse(JSON.stringify(f.commands[0].payload)), { project_id: 'p1' });
  for (const resources of [{hoursPerWeek:200,monthlyBudgetEur:1,comparisonMode:'equal_resources'}, {hoursPerWeek:10,monthlyBudgetEur:-1,comparisonMode:'equal_resources'}, {hoursPerWeek:10,monthlyBudgetEur:1,comparisonMode:'other'}]) {
    await assert.rejects(fixture().invoke({ ...request, resources }), /Invalid exit resource/);
  }
});
