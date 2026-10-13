import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import vm from 'node:vm';
import { commandRoundtripStagesFromMarks } from './command-bus.js';

const app = readFileSync(new URL('../app.js', import.meta.url), 'utf8');
const source = app.slice(app.indexOf('async function dispatchWorkjetProjectList('),
  app.indexOf('async function workjetProjectControl('));
const sample = {
  marks: { browser_dispatch_started: 100, browser_local_inserted: 160,
    browser_push_confirmed: 200, native_dispatch_entered: 170, native_handler_completed: 180,
    native_rxdb_projection_committed: 190, browser_terminal_observed: 210 },
  preinsert_marks: { capability_resolved: 110, database_resolved: 120, sync_ready: 130,
    authority_resolved: 140, local_write_started: 150 },
};

function harness(dispatch, samples = new Map()) {
  const context = { state: { commandBus: { dispatch } },
    loadCommandBusModule: async () => ({ commandRoundtripStagesFromMarks,
      consumeCommandRoundtripTiming: id => { const result = samples.get(id); samples.delete(id); return result; } }),
    awaitWorkjetProjectListStep: value => value, Date };
  vm.runInNewContext(source + '\nglobalThis.invoke = dispatchWorkjetProjectList;', context);
  return context.invoke;
}
const command = id => ({ id, client_context: {} });

test('reports actual preinsert and native roundtrip intervals without command data', async () => {
  const id = 'cmd_workjet_project_list_1';
  const samples = new Map([[id, sample]]);
  let observed;
  const invoke = harness(async cmd => { assert.equal(cmd.client_context.command_timing_probe, true); return 'receipt'; }, samples);
  assert.equal(await invoke(command(id), {}, 1000, { onCommandTiming: value => { observed = value; } }), 'receipt');
  assert.deepEqual(JSON.parse(JSON.stringify(observed)), { commandId: id, stagesMs: {
    browser_insert: 60, push: 40, local_to_native_intake: 10, native_processing: 10,
    projection_commit: 10, commit_to_browser_observed: 20, total: 110,
    initial_capability: 10, document_and_database: 10, sync_readiness: 10,
    fresh_peer_authority: 10, dependencies_and_revalidation: 10, local_persistence: 10,
  } });
  assert.equal(samples.size, 0);
});

test('timeout retains completed stages and the unfinished preinsert wait', async () => {
  const id = 'cmd_workjet_project_list_2';
  const failure = new Error('deadline');
  let observed;
  const invoke = harness(async () => { throw failure; }, new Map([[id, {
    marks: { browser_dispatch_started: Date.now() - 20 },
    preinsert_marks: { capability_resolved: Date.now() - 10 },
  }]]));
  await assert.rejects(invoke(command(id), {}, 1000, { onCommandTiming: value => { observed = value; } }), error => error === failure);
  assert.equal(observed.pendingStage, 'document_and_database');
  assert.ok(observed.pendingMs >= 10);
  assert.equal(Object.hasOwn(observed.stagesMs, 'sync_readiness'), false);
});

test('concurrent commands keep their own IDs and diagnostics cannot break dispatch', async () => {
  const ids = ['cmd_workjet_project_list_a', 'cmd_workjet_project_list_b'];
  const observed = [];
  const invoke = harness(async cmd => cmd.id, new Map(ids.map(id => [id, sample])));
  assert.deepEqual(await Promise.all(ids.map(id => invoke(command(id), {}, 1000, {
    onCommandTiming: value => { observed.push(value.commandId); throw new Error('observer'); },
  }))), ids);
  assert.deepEqual(observed.sort(), ids);
});

test('older callers do not request timing or load the diagnostic module', async () => {
  const cmd = command('cmd_workjet_project_list_legacy');
  assert.equal(await harness(async () => 'receipt')(cmd, {}, 1000), 'receipt');
  assert.equal(Object.hasOwn(cmd.client_context, 'command_timing_probe'), false);
});

test('native intervals survive an unconfirmed push without inventing a full roundtrip', async () => {
  const id = 'cmd_workjet_project_list_partial';
  const partial = { ...sample, marks: { ...sample.marks } };
  delete partial.marks.browser_push_confirmed;
  let observed;
  await harness(async () => 'receipt', new Map([[id, partial]]))(
    command(id), {}, 1000, { onCommandTiming: value => { observed = value; } },
  );
  assert.equal(observed.stagesMs.local_to_native_intake, 10);
  assert.equal(Object.hasOwn(observed.stagesMs, 'push'), false);
  assert.equal(Object.hasOwn(observed.stagesMs, 'total'), false);
});

test('an unavailable diagnostic module leaves dispatch behavior intact', async () => {
  const context = {
    state: { commandBus: { dispatch: async cmd => {
      assert.equal(Object.hasOwn(cmd.client_context, 'command_timing_probe'), false);
      return 'receipt';
    } } },
    loadCommandBusModule: async () => { throw new Error('diagnostic module'); },
    awaitWorkjetProjectListStep: value => value, Date,
  };
  vm.runInNewContext(source + '\nglobalThis.invoke = dispatchWorkjetProjectList;', context);
  assert.equal(await context.invoke(command('cmd_workjet_project_list_unavailable'), {}, 1000,
    { onCommandTiming: () => assert.fail('unavailable diagnostics') }), 'receipt');
});
