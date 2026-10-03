import assert from 'node:assert/strict';
import { webcrypto } from 'node:crypto';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import vm from 'node:vm';

const appSource = readFileSync(new URL('../app.js', import.meta.url), 'utf8');
const controlStart = appSource.indexOf('const WORKJET_PROJECT_CONTROL_MAX_RESULTS');
const controlEnd = appSource.indexOf('async function waitForSyncBridgeReady', controlStart);
const controlSource = appSource.slice(controlStart, controlEnd);
const syncWaitEnd = appSource.indexOf('\n}\n', controlEnd) + 3;
const projectRuntimeSource = controlSource + appSource.slice(controlEnd, syncWaitEnd);

test('project projection waits through the actual nested replication state', async () => {
  let pulled = false;
  const project = { id: 'project-nested', name: 'Nested', status: 'active', owner_user_id: 'owner-1' };
  const context = vm.createContext({
    state: { db: { collection: () => ({ findOne: () => ({ exec: async () => pulled ? project : null }) }) } },
    window: { setTimeout }, setTimeout, clearTimeout,
    Date,
  });
  vm.runInContext(projectRuntimeSource, context);
  context.bridge = { state: { async awaitInSync() { pulled = true; } } };
  const result = await vm.runInContext(
    "waitForProjectedWorkjetProject('project-nested', 'Nested', 'owner-1', bridge, 200)", context,
  );
  assert.equal(result.id, project.id);
  assert.equal(pulled, true);
});

test('working-copy projection waits through the actual nested replication state', async () => {
  let pulled = false;
  const copy = { id: 'copy-nested', project_id: 'project-nested', computer_id: 'computer-1', path: '/fixture/project', status: 'active', owner_user_id: 'owner-1' };
  const context = vm.createContext({
    state: { db: { collection: () => ({ find: () => ({ exec: async () => pulled ? [copy] : [] }) }) } },
    window: { setTimeout }, setTimeout, clearTimeout,
    Date,
  });
  vm.runInContext(projectRuntimeSource, context);
  context.bridge = { state: { async awaitInSync() { pulled = true; } } };
  const result = await vm.runInContext(
    "waitForProjectedWorkjetWorkingCopy('project-nested', {computerId:'computer-1',path:'/fixture/project'}, 'owner-1', bridge, 200)", context,
  );
  assert.equal(result.computerId, copy.computer_id);
  assert.equal(pulled, true);
});


test('Workjet project control is installed and uses the RxDB command plane', () => {
  assert.match(appSource, /globalThis\.workjetProjectControl = workjetProjectControl/);
  assert.ok(controlStart >= 0 && controlEnd > controlStart, 'project control implementation exists');
  assert.match(controlSource, /action === 'project\.list'/);
  assert.match(controlSource, /action === 'project\.create'/);
  assert.match(controlSource, /command_type: 'ctox\.workjet\.project\.list'/);
  assert.match(controlSource, /command_type: 'ctox\.workjet\.project\.upsert'/);
  assert.match(controlSource, /command_type: 'ctox\.workjet\.working_copy\.upsert'/);
  assert.match(controlSource, /startCollection\?\.\('business_commands'\)/);
  assert.match(controlSource, /startCollection\?\.\('workjet_projects', \{ pin: false, forceDirect: true \}\)/);
  assert.match(controlSource, /startCollection\?\.\('workjet_working_copies', \{ pin: false, forceDirect: true \}\)/);
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
    demandLoader: {},
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
      async startCollection(name) {
        return { state: {
          collection: collection(name),
          async awaitQueryReady() {},
          collectionQueryGenerationToken: () => 'generation-1',
          async awaitInSync() { throw new Error('Historical pull must not gate project control'); },
        } };
      },
    },
    commandBus: {
      async dispatch(command) {
        dispatched.push(command);
        if (command.command_type === 'ctox.workjet.project.list') {
          return { command_id: command.id, status: 'completed', ok: true,
            result: { ok: true, collection: 'workjet_projects' } };
        }
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
    AbortController,
    TextEncoder,
    window: { setTimeout },
    setTimeout,
    clearTimeout,
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

function nativeProjectListFixture({ start, dispatch, exec } = {}) {
  const starts = [];
  const commands = [];
  const reads = [];
  const rows = {
    workjet_projects: [{ id: 'native-project', name: 'Native project', status: 'active',
      owner_user_id: 'owner-1', created_at_ms: 1_700_000_000_000 }],
    workjet_working_copies: [{ id: 'native-copy', project_id: 'native-project',
      computer_id: 'computer-a', path: 'guest://native', status: 'active', owner_user_id: 'owner-1' }],
  };
  const peers = Object.fromEntries(Object.keys(rows).map((name) => {
    const peer = {
      generation: 'generation-1',
      async awaitQueryReady(budget) { assert.ok(budget > 0 && budget <= 29_000); },
      async awaitInSync() { assert.fail('Historical replication must not gate a project list'); },
      collectionQueryGenerationToken() { return this.generation; },
      collection: {
        demandLoader: {},
        find(query) {
          assert.equal(query.selector.owner_user_id.$eq, 'owner-1');
          assert.match(query.requireRevision, /^cmd_workjet_project_list_/);
          assert.ok(query.signal instanceof AbortSignal);
          reads.push({ name, query });
          return { async exec() { return exec ? exec(name, query, peer)
            : rows[name].slice(query.skip || 0, (query.skip || 0) + query.limit); } };
        },
      },
    };
    return [name, peer];
  }));
  const state = {
    session: { id: 'owner-1' },
    db: { collection: () => ({ find() { assert.fail('Cached rows cannot confirm the native project list'); } }) },
    sync: { async startCollection(name, options) {
      starts.push(name);
      if (name !== 'business_commands') assert.deepEqual(JSON.parse(JSON.stringify(options)), { pin: false, forceDirect: true });
      const bridge = { state: peers[name] || { async awaitInSync() { assert.fail('Command history'); } } };
      return start ? start(name, bridge) : bridge;
    } },
    commandBus: { async dispatch(command, options) {
      commands.push({ command, options });
      assert.equal(options.until, 'terminal');
      assert.equal(options.sync_queue_tasks, false);
      assert.ok(options.timeoutMs > 0 && options.timeoutMs <= 29_000);
      const receipt = { command_id: command.id, status: 'completed', ok: true,
        result: { ok: true, collection: 'workjet_projects' } };
      return dispatch ? dispatch(receipt, state) : receipt;
    } },
  };
  let sequence = 0;
  const context = { state, actorContext: (session) => ({ id: session.id }),
    newId: () => `list-${++sequence}`, AbortController, setTimeout, clearTimeout };
  vm.runInNewContext(`${controlSource}\nglobalThis.invoke = workjetProjectControl;`, context);
  return { state, context, starts, commands, reads, rows, peers,
    invoke: async () => JSON.parse(JSON.stringify(await context.invoke({ action: 'project.list' }))) };
}

test('project list starts all bridges concurrently and skips historical command replication', async () => {
  const waiting = [];
  const fixture = nativeProjectListFixture({
    start: (name, bridge) => new Promise((resolve) => waiting.push(() => resolve(bridge))),
  });
  const pending = fixture.invoke();
  assert.deepEqual(fixture.starts, ['business_commands', 'workjet_projects', 'workjet_working_copies']);
  waiting.forEach((resolve) => resolve());
  const result = await pending;
  assert.equal(result.projects[0].id, 'native-project');
  assert.equal(result.projects[0].workingCopies[0].id, 'native-copy');
  assert.equal(fixture.reads.length, 2);
  assert.ok(fixture.reads.every(({ query }) => query.signal.aborted));
});

test('each project list requires new native authority and accepts a confirmed empty result', async () => {
  const fixture = nativeProjectListFixture();
  await fixture.invoke();
  fixture.rows.workjet_projects.length = 0;
  fixture.rows.workjet_working_copies.length = 0;
  assert.deepEqual((await fixture.invoke()).projects, []);
  assert.notEqual(fixture.reads[0].query.requireRevision, fixture.reads[2].query.requireRevision);
});

test('native working-copy reads page through 200-row windows up to the declared 500-row cap', async () => {
  const fixture = nativeProjectListFixture();
  fixture.rows.workjet_working_copies = Array.from({ length: 520 }, (_, index) => ({
    id: `copy-${String(index).padStart(3, '0')}`, project_id: 'native-project',
    computer_id: `computer-${index}`, path: `guest://native/${index}`,
    status: 'active', owner_user_id: 'owner-1',
  }));
  assert.equal((await fixture.invoke()).projects[0].workingCopies.length, 500);
  const pages = fixture.reads.filter(({ name }) => name === 'workjet_working_copies');
  assert.deepEqual(pages.map(({ query }) => query.limit), [200, 200, 100]);
  assert.deepEqual(pages.map(({ query }) => query.skip), [0, 200, 400]);
});

test('a replaced generation or duplicated page boundary cannot deliver a partial native copy list', async () => {
  for (const mode of ['generation', 'duplicate']) {
    const fixture = nativeProjectListFixture({ exec: (name, query, peer) => {
      if (name === 'workjet_projects') return fixture.rows.workjet_projects;
      if (query.skip && mode === 'generation') peer.generation = 'generation-2';
      const offset = query.skip && mode === 'duplicate' ? 0 : query.skip;
      return fixture.rows.workjet_working_copies.slice(offset, offset + query.limit);
    } });
    fixture.rows.workjet_working_copies = Array.from({ length: 220 }, (_, index) => ({
      id: `copy-${String(index).padStart(3, '0')}`, project_id: 'native-project',
      computer_id: `computer-${index}`, path: `guest://native/${index}`,
      status: 'active', owner_user_id: 'owner-1',
    }));
    await assert.rejects(fixture.invoke(), /generation changed|repeated identity/);
    assert.ok(fixture.reads.every(({ query }) => query.signal.aborted));
  }
});

test('missing, rejected or replaced native project authority cannot return cached data', async () => {
  for (const mode of ['missing', 'rejected', 'replaced']) {
    const fixture = nativeProjectListFixture({ exec: async (name, query, peer) => {
      if (mode === 'replaced') peer.generation = 'generation-2';
      return [{ id: 'stale', name: 'Cached', status: 'active' }];
    } });
    if (mode === 'missing') fixture.peers.workjet_projects.collection.demandLoader = null;
    if (mode === 'rejected') fixture.peers.workjet_projects.awaitQueryReady = async () => { throw new Error('Peer denied'); };
    await assert.rejects(fixture.invoke(), /authority is unavailable|Peer denied|generation changed/);
    assert.ok(fixture.reads.every(({ query }) => query.signal.aborted));
  }
});

test('project list rejects wrong command receipt or a changed actor before querying', async () => {
  for (const mode of ['receipt', 'actor']) {
    const fixture = nativeProjectListFixture({ dispatch: (receipt, state) => {
      if (mode === 'actor') state.session = { id: 'owner-2' };
      else receipt.command_id = 'other-command';
      return receipt;
    } });
    await assert.rejects(fixture.invoke(), /uncorrelated|session changed/);
    assert.equal(fixture.reads.length, 0);
  }
});

test('collection and command phases consume one list deadline rather than restarting it', async () => {
  for (const phase of ['collections', 'command']) {
    let now = 10_000;
    const fixture = nativeProjectListFixture({
      start: (name, bridge) => { if (phase === 'collections') now = 39_000; return bridge; },
      dispatch: (receipt) => { if (phase === 'command') now = 39_000; return receipt; },
    });
    fixture.context.Date = class extends Date { static now() { return now; } };
    await assert.rejects(fixture.invoke(), (error) => error.code === 'WORKJET_PROJECT_TIMEOUT');
    assert.equal(fixture.reads.length, 0);
    if (phase === 'collections') assert.equal(fixture.commands.length, 0);
  }
});

test('a query deadline aborts both native projection streams', async () => {
  let now = 10_000;
  const fixture = nativeProjectListFixture({
    dispatch: (receipt) => { now = 38_995; return receipt; },
    exec: (name, query) => new Promise((resolve, reject) => {
      query.signal.addEventListener('abort', () => reject(new Error('Query aborted')), { once: true });
    }),
  });
  fixture.context.Date = class extends Date { static now() { return now; } };
  await assert.rejects(fixture.invoke(), (error) => error.code === 'WORKJET_PROJECT_TIMEOUT');
  assert.equal(fixture.reads.length, 2);
  assert.ok(fixture.reads.every(({ query }) => query.signal.aborted));
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
