// Origin: CTOX
// License: AGPL-3.0-only
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import vm from 'node:vm';
import { requestSupervisorRoute } from './workjet-supervisor-route-native.mjs';
import { validateSupervisorRouteDisplayValue } from './workjet-supervisor-route-display-contract.generated.mjs';

const fixture = JSON.parse(readFileSync(new URL('../../../core/rxdb/tests/fixtures/workjet-supervisor-route-display-v1.json', import.meta.url)));
const route = fixture.valid_cases.find(c => c.type === 'SupervisorRouteDisplay' && c.value.configured).value;
const caps = fixture.valid_cases.find(c => c.type === 'SupervisorRouteCapabilities').value;
const computed = JSON.parse(readFileSync(new URL('../../../core/rxdb/tests/fixtures/workjet-supervisor-route-computation-v2.json', import.meta.url)));
const computedRoute = computed.valid_cases.find(c=>c.type==='SupervisorRouteDisplay' && c.value.actual).value;
const computedCaps = computed.valid_cases.find(c=>c.type==='SupervisorRouteCapabilities').value;
const actor = { id: 'owner', role: 'chef' };
const request = { action: 'project.supervisor.route.read.v1', commandId: 'read-route',
  projectId: route.project_id, threadId: route.supervisor_thread_id };

function transport(value = route, mutate = () => {}) {
  const commands = [];
  return { commands, dispatch: async (command, options) => {
    commands.push({ command, options });
    const receipt = { command_id: command.id, ok: true, status: 'completed',
      target_record_id: command.record_id, payload: structuredClone(command.payload),
      result: { ...structuredClone(value), status: 'completed', task_status: 'completed' } };
    mutate(receipt);
    return receipt;
  } };
}

for (const [action, value, field] of [
  ['project.supervisor.route.read.v1', route, 'route'],
  ['project.supervisor.route.capabilities.v1', caps, 'capabilities'],
  ['project.supervisor.route.read.v2', computedRoute, 'route'],
  ['project.supervisor.route.capabilities.v2', computedCaps, 'capabilities'],
]) {
  test(action + ' requires an exact native project/thread receipt', async () => {
    const t = transport(value);
    const result = await requestSupervisorRoute(t.dispatch, { ...request, action }, actor, () => {});
    assert.deepEqual(result[field], value);
    assert.equal(result.contract, value.schema);
    assert.equal(t.commands.length, 1);
    assert.equal(t.commands[0].command.command_type, action.replace(/^project/, 'ctox.workjet.project'));
    assert.deepEqual(t.commands[0].command.payload, { project_id: request.projectId, thread_id: request.threadId });
    assert.deepEqual(t.commands[0].command.client_context.actor, actor);
    assert.deepEqual(t.commands[0].options, { until: 'terminal', sync_queue_tasks: false, timeoutMs: 30_000 });
  });
}

for (const [label, change] of [
  ['forged actor', { actor: { id: 'foreign' } }],
  ['model override', { model: route.configured.model }],
  ['foreign capability action', { action: 'project.supervisor.turn.capabilities' }],
  ['leading scope whitespace', { projectId: ' project' }],
  ['missing bound thread', { threadId: undefined }],
  ['control character', { commandId: 'read\nroute' }],
]) {
  test('rejects ' + label + ' before native dispatch', async () => {
    const t = transport();
    await assert.rejects(requestSupervisorRoute(t.dispatch, { ...request, ...change }, actor, () => {}));
    assert.equal(t.commands.length, 0);
  });
}

test('cannot replace receipt identity or promote a failed/native-incomplete response', async () => {
  for (const change of [
    r => { r.command_id = 'another-operation'; },
    r => { r.target_record_id = 'another-project'; },
    r => { r.payload.thread_id = 'another-thread'; },
    r => { r.result.project_id = 'another-project'; },
    r => { r.result.supervisor_thread_id = 'another-thread'; },
    r => { r.status = 'running'; },
    r => { r.ok = false; },
  ]) {
    const t = transport(route, change);
    await assert.rejects(requestSupervisorRoute(t.dispatch, request, actor, () => {}), /unmatched native receipt/);
  }
});

test('terminal transport metadata cannot promote an incomplete domain result', async () => {
  for (const change of [
    r => { delete r.result.status; },
    r => { r.result.status = 'failed'; },
    r => { r.result.task_status = 'running'; },
    r => { r.result.unexpected_field = 'private'; },
  ]) {
    const t = transport(route, change);
    await assert.rejects(requestSupervisorRoute(t.dispatch, request, actor, () => {}));
  }
});

test('strict wire decoding never exports account or private selector fields', async () => {
  for (const key of ['native_account', 'private_local_account_id', 'gatewayAccountId']) {
    const t = transport(route, r => { r.result.configured[key] = 'private'; });
    await assert.rejects(requestSupervisorRoute(t.dispatch, request, actor, () => {}));
  }
});

test('even a well-shaped reserved producer object is not execution evidence', async () => {
  const claimed = { ...structuredClone(route), actual: {
    ...route.configured, run_id: 'run', turn_id: 'turn', receipt_id: 'receipt',
  } };
  assert.equal(validateSupervisorRouteDisplayValue('SupervisorRouteDisplay', claimed).ok, true);
  const t = transport(claimed);
  await assert.rejects(requestSupervisorRoute(t.dispatch, request, actor, () => {}), /unproved producer/);
});

test('checks current authority before dispatch and after the native wait', async () => {
  const before = transport();
  await assert.rejects(requestSupervisorRoute(before.dispatch, request, actor, () => {
    throw new Error('replaced before');
  }), /replaced before/);
  assert.equal(before.commands.length, 0);
  const after = transport();
  let count = 0;
  await assert.rejects(requestSupervisorRoute(after.dispatch, request, actor, () => {
    if (++count === 2) throw new Error('replaced after');
  }), /replaced after/);
  assert.equal(after.commands.length, 1);
});

test('the separate capability cannot advertise a different read command or include actual', async () => {
  for (const change of [
    r => { r.result.read_command = 'ctox.workjet.project.supervisor.turn.submit'; },
    r => { r.result.actual = null; },
  ]) {
    const t = transport(caps, change);
    await assert.rejects(requestSupervisorRoute(t.dispatch,
      { ...request, action: 'project.supervisor.route.capabilities.v1' }, actor, () => {}));
  }
});

test('actual Shell control uses Supervisor admission and fences instance changes during collection readiness', async () => {
  const app = readFileSync(new URL('../app.js', import.meta.url), 'utf8');
  const source = app.slice(app.indexOf('const WORKJET_PROJECT_CONTROL_MAX_RESULTS'),
    app.indexOf('async function waitForSyncBridgeReady', app.indexOf('const WORKJET_PROJECT_CONTROL_MAX_RESULTS')));
  for (const [action,value,field] of [['project.supervisor.route.capabilities.v1',caps,'capabilities'],
    ['project.supervisor.route.capabilities.v2',computedCaps,'capabilities'], ['project.supervisor.route.read.v2',computedRoute,'route']]) {
  for (const changedDuringReadiness of [false, true]) {
    const t = transport(value);
    const state = { session: actor, db: {}, sync: {}, syncConfig: { instance_id: 'native' },
      commandBus: { dispatch: t.dispatch } };
    let admitted = 0;
    const context = vm.createContext({ state, requestSupervisorRoute, actorContext: value => value,
      admittedSupervisor: async () => {
        admitted += 1;
        if (changedDuringReadiness) state.syncConfig.instance_id = 'replaced';
        return {};
      } });
    vm.runInContext(source + '\nrequireWorkjetSupervisorDataPlane = admittedSupervisor;\n'
      + 'requireWorkjetProjectDataPlane = () => { throw new Error("wrong admission"); };\n'
      + 'globalThis.control = workjetProjectControl;', context);
    const call = context.control({ ...request, action });
    if (changedDuringReadiness) {
      await assert.rejects(call, /instance or authority changed/);
      assert.equal(t.commands.length, 0);
    } else {
      assert.deepEqual((await call)[field], value);
      assert.equal(t.commands.length, 1);
    }
    assert.equal(admitted, 1);
  }
  }
});

test('v2 keeps observed SDK/native IDs distinct and rejects private selectors or version substitution', async () => {
  const wanted={...request,action:'project.supervisor.route.read.v2'};
  const response=await requestSupervisorRoute(transport(computedRoute).dispatch,wanted,actor,()=>{});
  assert.deepEqual(response.route.actual,computedRoute.actual);
  assert.equal(response.route.actual.run_id,undefined);
  assert.equal(response.route.actual.turn_id,undefined);
  for (const mutate of [
    r=>{r.result.actual.private_local_account_id='private';},
    r=>{r.result.actual.turn_id='sdk-as-native';},
    r=>{r.result.actual.model_operation_id='';},
    r=>{r.result.schema='ctox.workjet.supervisor.route-display.v1';},
    r=>{r.payload.thread_id='foreign';},
  ]) {
    const t=transport(computedRoute,mutate);
    await assert.rejects(requestSupervisorRoute(t.dispatch,wanted,actor,()=>{}));
  }
  await assert.rejects(requestSupervisorRoute(transport(route).dispatch,wanted,actor,()=>{}));
  const nullRoute=computed.valid_cases.find(c=>c.type==='SupervisorRouteDisplay' && c.value.configured && !c.value.actual).value;
  const pending=await requestSupervisorRoute(transport(nullRoute).dispatch,wanted,actor,()=>{});
  assert.equal(pending.route.actual,null);
});

for (const [action, template, field] of [
  ['project.supervisor.route.read.v1', route, 'route'],
  ['project.supervisor.route.capabilities.v1', caps, 'capabilities'],
  ['project.supervisor.route.read.v2', { ...computedRoute, actual: null }, 'route'],
  ['project.supervisor.route.capabilities.v2', computedCaps, 'capabilities'],
]) {
  test(action + ' admits exact Workjet command and lowercase Owner/project/thread UUIDs', async () => {
    const projectId = 'f791215c-e416-4205-8619-bbf82b999799';
    const threadId = 'cc6cfe73-2824-4360-9daf-3b3efb079931';
    const owner = { id: '196a89ba-ee86-4413-885c-04ca60e6f291', role: 'chef' };
    const commandId = 'supervisor-route-d843c721-6db9-4f5b-b921-81fd5a716d80';
    const value = { ...structuredClone(template), project_id: projectId, supervisor_thread_id: threadId };
    const t = transport(value);
    const result = await requestSupervisorRoute(t.dispatch, { action, commandId, projectId, threadId }, owner, () => {});
    assert.equal(result.commandId, commandId);
    assert.equal(result.projectId, projectId);
    assert.equal(result.threadId, threadId);
    assert.deepEqual(result[field], value);
    assert.equal(t.commands[0].command.id, commandId);
    assert.deepEqual(t.commands[0].command.client_context.actor, owner);
  });
}

test('every C0 control byte in a public scope is rejected before dispatch', async () => {
  for (let byte = 0; byte < 32; byte += 1) {
    for (const scope of ['commandId', 'projectId', 'threadId', 'owner']) {
      const t = transport();
      const changed = { ...request };
      const owner = { ...actor };
      if (scope === 'owner') owner.id = 'owner' + String.fromCharCode(byte) + 'suffix';
      else changed[scope] = changed[scope] + String.fromCharCode(byte) + 'suffix';
      await assert.rejects(requestSupervisorRoute(t.dispatch, changed, owner, () => {}), /Invalid Supervisor route/);
      assert.equal(t.commands.length, 0, scope + ':' + byte);
    }
  }
});
