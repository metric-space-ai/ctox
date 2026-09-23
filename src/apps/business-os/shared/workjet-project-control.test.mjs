import assert from 'node:assert/strict';
import { webcrypto } from 'node:crypto';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import vm from 'node:vm';

const appSource = readFileSync(new URL('../app.js', import.meta.url), 'utf8');
const controlStart = appSource.indexOf('const WORKJET_PROJECT_CONTROL_MAX_RESULTS');
const controlEnd = appSource.indexOf('async function waitForSyncBridgeReady', controlStart);
const controlSource = appSource.slice(controlStart, controlEnd);

test('Workjet project control is installed and uses the RxDB command plane', () => {
  assert.match(appSource, /globalThis\.workjetProjectControl = workjetProjectControl/);
  assert.ok(controlStart >= 0 && controlEnd > controlStart, 'project control implementation exists');
  assert.match(controlSource, /action === 'project\.list'/);
  assert.match(controlSource, /action === 'project\.create'/);
  assert.match(controlSource, /command_type: 'ctox\.workjet\.project\.list'/);
  assert.match(controlSource, /command_type: 'ctox\.workjet\.project\.upsert'/);
  assert.match(controlSource, /command_type: 'ctox\.workjet\.working_copy\.upsert'/);
  assert.match(controlSource, /startCollection\?\.\('business_commands'\)/);
  assert.match(controlSource, /startCollection\?\.\('workjet_projects'\)/);
  assert.match(controlSource, /startCollection\?\.\('workjet_working_copies'\)/);
  assert.equal((controlSource.match(/until: 'terminal'/g) || []).length, 4);
  assert.match(controlSource, /waitForProjectedWorkjetProject\(/);
  assert.match(controlSource, /rawProject\?\.name === expectedTitle/);
  assert.match(controlSource, /rawProject\?\.status === 'active'/);
  assert.match(controlSource, /waitForProjectedWorkjetWorkingCopy\(/);
  assert.match(controlSource, /return \{ action: 'project\.list', projects \}/);
  assert.match(controlSource, /action: 'project\.create',\s+project:/);
});

test('Workjet project control supports logical projects with optional opaque working copies', () => {
  assert.match(controlSource, /WORKJET_PROJECT_CONTROL_MAX_RESULTS = 100/);
  assert.match(controlSource, /boundedWorkjetProjectText\(request\.commandId, 'commandId', 128\)/);
  assert.match(controlSource, /boundedWorkjetProjectText\(request\.projectId, 'projectId', 128\)/);
  assert.match(controlSource, /boundedWorkjetProjectText\(request\.title, 'title', 256\)/);
  assert.match(controlSource, /id: commandId,\s+command_id: commandId/);
  assert.match(controlSource, /if \(requestedWorkingCopy\) \{/);
  assert.match(controlSource, /workjetProjectChildCommandId\(commandId, 'working-copy'\)/);
  assert.match(controlSource, /computer_id: requestedWorkingCopy\.computerId/);
  assert.match(controlSource, /path: requestedWorkingCopy\.path/);
  assert.match(controlSource, /active: true/);
  assert.match(controlSource, /workingCopies: Object\.freeze/);
  assert.match(controlSource, /!\['active', 'detached'\]\.includes\(value\.status\)/);
  assert.match(controlSource, /status: value\.status/);
  assert.match(controlSource, /payload: \{\s+project_id: projectId,\s+name: title,\s+\}/);
  assert.doesNotMatch(
    controlSource,
    /payload: \{\s+project_id: projectId,\s+name: title,\s+workspaceRoot:/,
  );
  assert.doesNotMatch(controlSource, /\.\.\.\(workspaceRoot/);
  assert.doesNotMatch(controlSource, /fetch\s*\(/);
  assert.doesNotMatch(controlSource, /XMLHttpRequest|\/api\/|https?:\/\//);
  assert.doesNotMatch(controlSource, /canonical/i);
  assert.doesNotMatch(controlSource, /_rev\s*:/);
});

test('Workjet project create/list is idempotent across optional copies and computers', async () => {
  const collections = {
    workjet_projects: [],
    workjet_working_copies: [],
  };
  const dispatched = [];
  const completedCommandIds = new Set();
  const collection = (name) => ({
    find({ selector = {}, limit = Number.MAX_SAFE_INTEGER } = {}) {
      return {
        async exec() {
          return collections[name]
            .filter((doc) => Object.entries(selector).every(([field, condition]) => (
              doc[field] === condition?.$eq
            )))
            .slice(0, limit);
        },
      };
    },
    findOne(id) {
      return { async exec() { return collections[name].find((doc) => doc.id === id) || null; } };
    },
  });
  const state = {
    session: { id: 'owner-1' },
    db: { collection },
    sync: {
      async startCollection() {
        return { async awaitInSync() {} };
      },
    },
    commandBus: {
      async dispatch(command) {
        dispatched.push(command);
        if (completedCommandIds.has(command.id)) return { status: 'completed' };
        completedCommandIds.add(command.id);
        if (command.command_type === 'ctox.workjet.project.upsert') {
          const previous = collections.workjet_projects.find((doc) => doc.id === command.payload.project_id);
          const next = {
            id: command.payload.project_id,
            name: command.payload.name,
            status: 'active',
            owner_user_id: 'owner-1',
            created_at_ms: previous?.created_at_ms || 1_700_000_000_000,
            updated_at_ms: 1_700_000_000_000,
          };
          collections.workjet_projects = collections.workjet_projects
            .filter((doc) => doc.id !== next.id).concat(next);
        }
        if (command.command_type === 'ctox.workjet.working_copy.upsert') {
          const id = `wc-${command.payload.project_id}-${command.payload.computer_id}`;
          const next = {
            id,
            project_id: command.payload.project_id,
            computer_id: command.payload.computer_id,
            path: command.payload.path,
            status: 'active',
            owner_user_id: 'owner-1',
          };
          collections.workjet_working_copies = collections.workjet_working_copies
            .filter((doc) => doc.id !== id).concat(next);
        }
        return { status: 'completed' };
      },
    },
  };
  const context = {
    state,
    actorContext: (session) => ({ id: session.id }),
    newId: () => 'list-id',
    waitForSyncBridgeReady: async () => {},
    crypto: webcrypto,
    TextEncoder,
    window: { setTimeout },
    setTimeout,
  };
  vm.runInNewContext(`${controlSource}\nglobalThis.__workjetProjectControl = workjetProjectControl;`, context);
  const invoke = async (request) => JSON.parse(JSON.stringify(
    await context.__workjetProjectControl(request),
  ));

  const withoutCopy = await invoke({
    action: 'project.create',
    commandId: 'create-empty',
    projectId: 'project-empty',
    title: 'Empty project',
    createdAt: '2026-08-28T10:00:00.000Z',
  });
  assert.deepEqual(withoutCopy.project.workingCopies, []);
  assert.equal('workspaceRoot' in withoutCopy.project, false);

  await assert.rejects(invoke({
    action: 'project.create',
    commandId: 'legacy-root',
    projectId: 'legacy-root-project',
    title: 'Legacy root must fail',
    workspaceRoot: 'guest://legacy/not-project-identity',
    createdAt: '2026-08-28T10:00:00.000Z',
  }), /Unsupported Workjet project payload field: workspaceRoot/);

  await assert.rejects(invoke({
    action: 'project.create',
    commandId: 'invalid-copy',
    projectId: 'invalid-copy-project',
    title: 'Invalid copy',
    createdAt: '2026-08-28T10:00:00.000Z',
    workingCopy: { computerId: 'computer-a', path: 'guest://a/project', label: 'extra' },
  }), /Unsupported Workjet project payload field: label/);

  const firstRequest = {
    action: 'project.create',
    commandId: 'create-with-copy',
    projectId: 'project-copy',
    title: 'Copied project',
    createdAt: '2026-08-28T10:00:00.000Z',
    workingCopy: { computerId: 'computer-a', path: 'guest://a/project' },
  };
  const first = await invoke(firstRequest);
  const retry = await invoke(firstRequest);
  assert.deepEqual(first, retry);
  assert.deepEqual(first.project.workingCopies, [{
    id: 'wc-project-copy-computer-a',
    computerId: 'computer-a',
    path: 'guest://a/project',
    status: 'active',
  }]);

  const secondComputer = await invoke({
    ...firstRequest,
    commandId: 'create-second-computer',
    workingCopy: { computerId: 'computer-b', path: 'guest://b/project' },
  });
  assert.equal(secondComputer.project.workingCopies.length, 2);
  assert.equal(new Set(secondComputer.project.workingCopies.map((copy) => copy.id)).size, 2);

  collections.workjet_working_copies.find((copy) => copy.computer_id === 'computer-a').status = 'detached';
  const listed = await invoke({ action: 'project.list' });
  assert.equal(listed.projects.length, 2);
  const listedCopies = listed.projects.find((project) => project.id === 'project-copy').workingCopies;
  assert.equal(listedCopies.length, 2);
  assert.equal(listedCopies.find((copy) => copy.computerId === 'computer-a').status, 'detached');
  assert.equal(collections.workjet_working_copies.length, 2);
  assert.equal(
    dispatched.filter((command) => command.command_type === 'ctox.workjet.working_copy.upsert').length,
    3,
    'retry reuses the same child command id instead of creating another logical copy',
  );
});

function projectChatFixture(dispatch, onBridge = async () => {}) {
  const commands = [];
  const state = {
    session: { id: 'owner-1' },
    db: { collection: () => ({}) },
    sync: { startCollection: onBridge },
    commandBus: {
      async dispatch(command, options) {
        commands.push({ command, options });
        return dispatch(command, state);
      },
    },
  };
  const context = {
    state,
    actorContext: (session) => ({ id: session?.id }),
    waitForSyncBridgeReady: async () => {},
    newId: () => { throw new Error('Chat mutations must retain their supplied command id.'); },
  };
  vm.runInNewContext(`${controlSource}\nglobalThis.invoke = workjetProjectControl;`, context);
  return { state, commands, invoke: async (request) => JSON.parse(JSON.stringify(await context.invoke(request))) };
}

function projectChatRequest(action = 'project.worker.add') {
  return {
    action,
    commandId: 'native-command-1',
    projectId: 'project-1',
    workerProfileId: 'worker-1',
    createdAt: '2026-09-12T12:00:00.000Z',
    ...(action === 'project.chat.create' ? { title: 'Second conversation' } : {}),
  };
}

function nativeChatReceipt(command) {
  return {
    ok: true,
    status: 'completed',
    command_id: command.id,
    target_record_id: command.record_id,
    payload: { ...command.payload },
    result: {
      ok: true,
      contract: 'workjet-project-chats.v1',
      ...(command.command_type.endsWith('worker.add')
        ? { first_chat_id: 'workjet_private_native_first' }
        : { chat_id: 'workjet_private_native_second' }),
    },
  };
}

for (const action of ['project.worker.add', 'project.chat.create']) {
  test(`Workjet ${action} forwards native mutation and preserves receipt identity on retry`, async () => {
    const { commands, invoke } = projectChatFixture(nativeChatReceipt);
    const request = projectChatRequest(action);
    const response = await invoke(request);
    assert.deepEqual(response, {
      action,
      commandId: request.commandId,
      projectId: request.projectId,
      workerProfileId: request.workerProfileId,
      chatId: action.endsWith('worker.add') ? 'workjet_private_native_first' : 'workjet_private_native_second',
    });
    assert.deepEqual(await invoke(request), response);
    assert.equal(commands.length, 2);
    for (const { command, options } of commands) {
      assert.equal(command.id, request.commandId);
      assert.equal(command.command_id, request.commandId);
      assert.equal(command.command_type, `ctox.workjet.${action}`);
      assert.equal(command.record_id, request.projectId);
      assert.deepEqual(JSON.parse(JSON.stringify(command.payload)), {
        project_id: request.projectId,
        worker_profile_id: request.workerProfileId,
        ...(request.title ? { title: request.title } : {}),
      });
      assert.equal(options.until, 'terminal');
      assert.equal(command.client_context.actor.id, 'owner-1');
    }
  });
}

const invalidChatReceipts = {
  'different command': (receipt) => { receipt.command_id = 'other-command'; },
  'different project': (receipt) => { receipt.payload.project_id = 'other-project'; },
  'different worker': (receipt) => { receipt.payload.worker_profile_id = 'other-worker'; },
  'different record': (receipt) => { receipt.target_record_id = 'other-project'; },
  'pending command': (receipt) => { receipt.status = 'running'; },
  'cancelled command': (receipt) => { receipt.status = 'cancelled'; },
  'failed command': (receipt) => { receipt.ok = false; },
  'failed domain mutation': (receipt) => { receipt.result.ok = false; },
  'wrong contract': (receipt) => { receipt.result.contract = 'unknown'; },
  'missing native id': (receipt) => { delete receipt.result.first_chat_id; },
  'group id': (receipt) => { receipt.result.first_chat_id = 'workjet_group_native'; },
};
for (const [reason, mutate] of Object.entries(invalidChatReceipts)) {
  test(`Workjet private chat rejects ${reason} without dispatching a replacement`, async () => {
    const { invoke, commands } = projectChatFixture((command) => {
      const receipt = nativeChatReceipt(command);
      mutate(receipt);
      return receipt;
    });
    await assert.rejects(invoke(projectChatRequest()), /receipt|private chat id/);
    assert.equal(commands.length, 1);
  });
}

test('Workjet chat creation rejects a receipt for another title', async () => {
  const { invoke } = projectChatFixture((command) => {
    const receipt = nativeChatReceipt(command);
    receipt.payload.title = 'Different conversation';
    return receipt;
  });
  await assert.rejects(invoke(projectChatRequest('project.chat.create')), /receipt/);
});

test('Workjet chat mutation propagates native policy rejection without retry', async () => {
  const { invoke, commands } = projectChatFixture(() => { throw new Error('native policy denied'); });
  await assert.rejects(invoke(projectChatRequest()), /native policy denied/);
  assert.equal(commands.length, 1);
});

for (const changed of ['session', 'database']) {
  test(`Workjet discards a pending chat receipt after ${changed} replacement`, async () => {
    const { invoke, commands } = projectChatFixture(async (command, state) => {
      await Promise.resolve();
      if (changed === 'session') state.session = { id: 'owner-2' };
      else state.db = { collection: () => ({}) };
      return nativeChatReceipt(command);
    });
    await assert.rejects(invoke(projectChatRequest()), /session changed/);
    assert.equal(commands.length, 1);
  });
}

test('Workjet validates chat request fields before dispatch', async () => {
  const { invoke, commands } = projectChatFixture(nativeChatReceipt);
  for (const request of [
    { ...projectChatRequest(), commandId: '' },
    { ...projectChatRequest(), workerProfileId: '' },
    { ...projectChatRequest(), ownerUserId: 'other-owner' },
    { ...projectChatRequest(), createdAt: 'not-a-date' },
    { ...projectChatRequest('project.chat.create'), title: '' },
  ]) await assert.rejects(invoke(request));
  assert.equal(commands.length, 0);
});

test('Workjet does not dispatch after session replacement during data-plane readiness', async () => {
  let fixture;
  fixture = projectChatFixture(nativeChatReceipt, async () => {
    fixture.state.session = { id: 'owner-2' };
    return {};
  });
  await assert.rejects(fixture.invoke(projectChatRequest()), /session changed/);
  assert.equal(fixture.commands.length, 0);
});
