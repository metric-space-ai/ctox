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
  assert.match(controlSource, /startCollection\?\.\('workjet_projects', \{ pin: false, forceDirect: true \}\)/);
  assert.match(controlSource, /startCollection\?\.\('workjet_working_copies', \{ pin: false, forceDirect: true \}\)/);
  assert.equal((controlSource.match(/until: 'terminal'/g) || []).length, 5);
  assert.match(controlSource, /waitForProjectedWorkjetProject\(/);
  assert.match(controlSource, /rawProject\?\.name === expectedTitle/);
  assert.match(controlSource, /rawProject\?\.status === 'active'/);
  assert.match(controlSource, /waitForProjectedWorkjetWorkingCopy\(/);
  assert.match(controlSource, /return \{ action: 'project\.list', projects, count, truncated: false \}/);
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
            result: { ok: true, collection: 'workjet_projects',
              count: collections.workjet_projects.length, truncated: false } };
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

function projectConfigurationFixture(changeReceipt = () => {}) {
  const commands = [];
  const state = {
    session: { id: 'owner-1' },
    db: { collection: () => ({}) },
    sync: { async startCollection() { return {}; } },
    commandBus: {
      async dispatch(command) {
        commands.push(command);
        const project = {
          id: command.payload.project_id, name: command.payload.name,
          owner_user_id: 'owner-1', status: 'active', created_at_ms: 1_700_000_000_000,
        };
        for (const field of ['description', 'repo_url', 'public_url', 'info', 'jour_fixe']) {
          if (Object.hasOwn(command.payload, field) && command.payload[field] !== null) {
            project[field] = command.payload[field];
          }
        }
        const receipt = {
          command_id: command.id, target_record_id: command.payload.project_id,
          status: 'completed', ok: true,
          result: { ok: true, collection: 'workjet_projects', project },
        };
        changeReceipt(receipt, state);
        return receipt;
      },
    },
  };
  const context = { state, actorContext: (session) => ({ id: session.id }), URL };
  vm.runInNewContext(`${controlSource}\nglobalThis.invoke = workjetProjectControl;`, context);
  return {
    commands,
    invoke: async (request) => JSON.parse(JSON.stringify(await context.invoke(request))),
  };
}

function projectConfigurationRequest(extra = {}) {
  return {
    action: 'project.configure', commandId: 'project-config-1',
    projectId: 'project-1', title: 'CTOX', ...extra,
  };
}

test('project configuration forwards bounded metadata and returns native fields to Workjet', async () => {
  const fixture = projectConfigurationFixture();
  const result = await fixture.invoke(projectConfigurationRequest({
    repoUrl: 'https://github.com/metric-space-ai/ctox', publicUrl: 'https://ctox.dev',
    info: { description: 'Durable work', goal: 'All projects usable\nPersist after reopen', phase: 'delivery', status: 'active' },
    jourFixe: { weekday: 3, time: '09:30' },
  }));
  const command = JSON.parse(JSON.stringify(fixture.commands[0]));
  assert.equal(command.command_type, 'ctox.workjet.project.upsert');
  assert.equal(command.record_id, 'project-1');
  assert.equal(command.client_context.actor.id, 'owner-1');
  assert.deepEqual(command.payload, {
    project_id: 'project-1', name: 'CTOX',
    repo_url: 'https://github.com/metric-space-ai/ctox', public_url: 'https://ctox.dev',
    info: { description: 'Durable work', goal: 'All projects usable\nPersist after reopen', phase: 'delivery', status: 'active' },
    jour_fixe: { weekday: 3, time: '09:30', timezone: 'Europe/Berlin' },
  });
  assert.equal(result.action, 'project.configure');
  assert.equal(result.project.repoUrl, command.payload.repo_url);
  assert.equal(result.project.publicUrl, command.payload.public_url);
  assert.deepEqual(result.project.info, command.payload.info);
  assert.deepEqual(result.project.jourFixe, command.payload.jour_fixe);
  assert.equal('ownerUserId' in result.project, false);
});

test('configuration preserves omission versus explicit null at the command boundary', async () => {
  const omitted = projectConfigurationFixture();
  await omitted.invoke(projectConfigurationRequest());
  assert.deepEqual(Object.keys(omitted.commands[0].payload).sort(), ['name', 'project_id']);
  const cleared = projectConfigurationFixture();
  await cleared.invoke(projectConfigurationRequest({ repoUrl: null, publicUrl: null, info: null, jourFixe: null }));
  for (const key of ['repo_url', 'public_url', 'info', 'jour_fixe']) {
    assert.equal(cleared.commands[0].payload[key], null);
  }
});

test('project configuration rejects forged authority and invalid metadata before dispatch', async () => {
  for (const extra of [
    { ownerUserId: 'foreign' }, { owner_user_id: 'foreign' }, { archived: false },
    { repoUrl: 'https://user:password@example.org/project' }, { publicUrl: 'javascript:alert(1)' },
    { info: { administrator: true } }, { jourFixe: { weekday: 0, time: '09:30' } },
    { jourFixe: { weekday: 3, time: '24:00' } }, { jourFixe: { weekday: 3, time: '09:30', owner: 'foreign' } },
  ]) {
    const fixture = projectConfigurationFixture();
    await assert.rejects(fixture.invoke(projectConfigurationRequest(extra)));
    assert.equal(fixture.commands.length, 0);
  }
});

test('project configuration rejects wrong receipts, foreign projects and session replacement', async () => {
  for (const mutate of [
    (receipt) => { receipt.command_id = 'other-command'; },
    (receipt) => { receipt.target_record_id = 'other-project'; },
    (receipt) => { receipt.result.collection = 'other-collection'; },
    (receipt) => { receipt.result.project.owner_user_id = 'foreign'; },
    (receipt) => { receipt.result.project.id = 'other-project'; },
    (receipt) => { receipt.result.project.name = 'other-title'; },
    (receipt) => { receipt.status = 'failed'; },
    (receipt, state) => { state.session = { id: 'owner-1' }; },
  ]) {
    const fixture = projectConfigurationFixture(mutate);
    await assert.rejects(fixture.invoke(projectConfigurationRequest()), /uncorrelated|session changed/);
  }
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
            : rows[name].filter((row) => Object.entries(query.selector).every(([field, condition]) => (
              condition.$in ? condition.$in.includes(row[field]) : row[field] === condition.$eq
            ))).slice(query.skip || 0, (query.skip || 0) + query.limit); } };
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
      const nativeRows = rows.workjet_projects.filter((row) => row.owner_user_id === 'owner-1'
        && row.status === 'active' && row.is_deleted !== true && row._deleted !== true);
      const nativeCount = nativeRows.length;
      const receipt = { command_id: command.id, status: 'completed', ok: true,
        result: { ok: true, collection: 'workjet_projects',
          count: Math.min(nativeCount, 100), truncated: nativeCount > 100,
          project_ids: nativeRows.slice(0, 100).map((row) => row.id) } };
      return dispatch ? dispatch(receipt, state) : receipt;
    } },
  };
  let sequence = 0;
  const context = { state, actorContext: (session) => ({ id: session.id }),
    newId: () => `list-${++sequence}`, AbortController, URL, setTimeout, clearTimeout };
  vm.runInNewContext(`${controlSource}\nglobalThis.invoke = workjetProjectControl;`, context);
  return { state, context, starts, commands, reads, rows, peers,
    invoke: async (request = {}) => JSON.parse(JSON.stringify(await context.invoke({ action: 'project.list', ...request }))) };
}

test('project list preserves the strict legacy shape until configuration is explicitly requested', async () => {
  const fixture = nativeProjectListFixture();
  Object.assign(fixture.rows.workjet_projects[0], {
    created_at_ms: 1_700_000_000_000,
    description: 'Native description', repo_url: 'https://example.test/repo',
    public_url: 'https://example.test', info: { goal: 'Saved native goal' },
    jour_fixe: { weekday: 1, time: '09:00', timezone: 'Europe/Berlin' },
  });
  const legacy = await fixture.invoke();
  assert.deepEqual(Object.keys(legacy.projects[0]).sort(), ['createdAt', 'id', 'title', 'workingCopies']);
  assert.deepEqual(await fixture.invoke({ includeConfiguration: false }), legacy);
  const enhanced = await fixture.invoke({ includeConfiguration: true });
  assert.equal(enhanced.projects[0].repoUrl, 'https://example.test/repo');
  assert.equal(enhanced.projects[0].publicUrl, 'https://example.test');
  assert.deepEqual(enhanced.projects[0].info, { goal: 'Saved native goal' });
  assert.deepEqual(enhanced.projects[0].jourFixe, { weekday: 1, time: '09:00', timezone: 'Europe/Berlin' });
  assert.deepEqual(enhanced.projects[0].workingCopies, legacy.projects[0].workingCopies);
  assert.ok(fixture.commands.every(({ command }) => !Object.hasOwn(command.payload, 'includeConfiguration')));
});

test('sixteen projected owner rows with four archived return exactly twelve active projects', async () => {
  const fixture = nativeProjectListFixture({ exec: (name) => fixture.rows[name] });
  fixture.rows.workjet_projects = Array.from({ length: 16 }, (_, index) => ({
    id: `project-${index}`, name: `Project ${index}`,
    status: index < 12 ? 'active' : 'archived', owner_user_id: 'owner-1',
  }));
  fixture.rows.workjet_projects.push({
    id: 'foreign-active', name: 'Foreign', status: 'active', owner_user_id: 'owner-2',
  });
  const result = await fixture.invoke();
  assert.equal(result.count, 12);
  assert.equal(result.projects.length, 12);
  assert.deepEqual(new Set(result.projects.map((project) => project.id)),
    new Set(Array.from({ length: 12 }, (_, index) => `project-${index}`)));
  assert.equal(fixture.rows.workjet_projects.length, 17, 'archives and foreign rows stay intact');
  assert.equal(fixture.reads.find(({ name }) => name === 'workjet_projects').query.selector.status.$eq, 'active');
});

test('a two-project replication gap reloads only the missing active native identities', async () => {
  const missingIds = ['71462c13-b395-402f-b6c8-788b405783e7', 'a93f348f-miltonticket'];
  const fixture = nativeProjectListFixture({ exec: (name, query) => {
    if (name !== 'workjet_projects') return [];
    return query.selector.id
      ? fixture.rows.workjet_projects.filter((row) => query.selector.id.$in.includes(row.id))
      : fixture.rows.workjet_projects.slice(0, 10);
  } });
  fixture.rows.workjet_projects = Array.from({ length: 16 }, (_, index) => ({
    id: index < 10 ? `project-${index}` : missingIds[index - 10] || `archived-${index}`,
    name: index === 10 ? 'greppy.xyz' : index === 11 ? 'miltonticket.app' : `Project ${index}`,
    status: index < 12 ? 'active' : 'archived', owner_user_id: 'owner-1',
  }));
  const result = await fixture.invoke();
  assert.equal(result.count, 12);
  assert.equal(result.projects.length, 12);
  assert.ok(missingIds.every((id) => result.projects.some((project) => project.id === id)));
  const projectReads = fixture.reads.filter(({ name }) => name === 'workjet_projects');
  assert.equal(projectReads.length, 2);
  assert.deepEqual(Array.from(projectReads[1].query.selector.id.$in), missingIds);
  assert.equal(projectReads[1].query.selector.status.$eq, 'active');
  assert.equal(projectReads[1].query.limit, 2);
  assert.notEqual(projectReads[0].query.requireRevision, projectReads[1].query.requireRevision);
  assert.ok(fixture.reads.every(({ query }) => query.signal.aborted));
});

test('malformed native identity windows cannot trigger projection or repair reads', async () => {
  for (const ids of [null, {}, [], ['native-project', 'native-project'], [7], ['bad\u0000id']]) {
    const fixture = nativeProjectListFixture({ dispatch: (receipt) => {
      receipt.result.project_ids = ids;
      return receipt;
    } });
    await assert.rejects(fixture.invoke(), (error) => error.code === 'WORKJET_PROJECT_LIST_UNCONFIRMED');
    assert.equal(fixture.reads.length, 0);
  }
});

test('repair reads preserve the first query generation and shared deadline', async () => {
  for (const mode of ['generation', 'actor', 'deadline', 'foreign', 'archived']) {
    let now = 10_000;
    const fixture = nativeProjectListFixture({ exec: (name, query, peer) => {
      if (name !== 'workjet_projects' || !query.selector.id) return [];
      if (mode === 'generation') peer.generation = 'generation-2';
      if (mode === 'actor') fixture.state.session = { id: 'owner-2' };
      if (mode === 'deadline') now = 39_000;
      return [{ ...fixture.rows.workjet_projects[0],
        ...(mode === 'foreign' ? { owner_user_id: 'owner-2' } : {}),
        ...(mode === 'archived' ? { status: 'archived' } : {}),
      }];
    } });
    fixture.context.Date = class extends Date { static now() { return now; } };
    await assert.rejects(fixture.invoke(), (error) => (
      /generation changed|session changed/.test(error.message)
      || error.code === 'WORKJET_PROJECT_TIMEOUT'
      || error.code === 'WORKJET_PROJECT_LIST_INCOMPLETE'
    ));
    assert.ok(fixture.reads.every(({ query }) => query.signal.aborted));
  }
});

test('project list rejects malformed configuration negotiation before native dispatch', async () => {
  const fixture = nativeProjectListFixture();
  for (const value of ['true', 1, null, {}]) {
    await assert.rejects(fixture.invoke({ includeConfiguration: value }), /includeConfiguration/);
  }
  assert.equal(fixture.commands.length, 0);
});

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
  const empty = await fixture.invoke();
  assert.deepEqual(empty.projects, []);
  assert.equal(empty.count, 0);
  assert.equal(empty.truncated, false);
  assert.notEqual(fixture.reads[0].query.requireRevision, fixture.reads[2].query.requireRevision);
});

test('nonempty native project counts cannot confirm empty or partial projections', async () => {
  for (const projectedCount of [0, 8]) {
    const fixture = nativeProjectListFixture({ exec: (name, query) => name === 'workjet_projects'
      ? (query.selector.id ? [] : fixture.rows.workjet_projects.slice(0, projectedCount)) : [] });
    fixture.rows.workjet_projects = Array.from({ length: 16 }, (_, index) => ({
      id: `project-${index}`, name: `Project ${index}`, status: 'active', owner_user_id: 'owner-1',
    }));
    await assert.rejects(fixture.invoke(), (error) =>
      error.code === 'WORKJET_PROJECT_LIST_INCOMPLETE' && error.retryable === true);
    assert.ok(fixture.reads.every(({ query }) => query.signal.aborted));
  }
});

test('missing or malformed native completeness metadata rejects before any projection read', async () => {
  for (const count of [undefined, -1, 0.5, '1', 101, NaN, Infinity]) {
    const fixture = nativeProjectListFixture({ dispatch: (receipt) => {
      receipt.result.count = count;
      return receipt;
    } });
    await assert.rejects(fixture.invoke(), (error) =>
      error.code === 'WORKJET_PROJECT_LIST_UNCONFIRMED' && error.retryable === false);
    assert.equal(fixture.reads.length, 0);
  }
  const fixture = nativeProjectListFixture({ dispatch: (receipt) => {
    delete receipt.result.truncated;
    return receipt;
  } });
  await assert.rejects(fixture.invoke(), (error) => error.code === 'WORKJET_PROJECT_LIST_UNCONFIRMED');
  assert.equal(fixture.reads.length, 0);
});

test('a truncated native window cannot be delivered as a complete project list', async () => {
  const fixture = nativeProjectListFixture({ dispatch: (receipt) => {
    receipt.result.count = 100;
    receipt.result.truncated = true;
    return receipt;
  } });
  await assert.rejects(fixture.invoke(), (error) =>
    error.code === 'WORKJET_PROJECT_LIST_INCOMPLETE' && error.retryable === false);
  assert.equal(fixture.reads.length, 0);
});

test('a native zero count cannot confirm extra or discarded projection rows', async () => {
  const stale = nativeProjectListFixture({ dispatch: (receipt) => {
    receipt.result.count = 0;
    receipt.result.project_ids = [];
    return receipt;
  } });
  await assert.rejects(stale.invoke(), (error) => error.code === 'WORKJET_PROJECT_LIST_INCOMPLETE');
  const deleted = nativeProjectListFixture({ exec: (name) => name === 'workjet_projects'
    ? [{ id: 'native-project', name: 'Deleted projection', is_deleted: true }] : [] });
  await assert.rejects(deleted.invoke(), (error) => error.code === 'WORKJET_PROJECT_LIST_INCOMPLETE');
});

test('identity remains fenced through final project serialization', async () => {
  let reads = 0;
  const fixture = nativeProjectListFixture({ exec: (name) => name === 'workjet_projects'
    ? [{ toJSON() {
      if (++reads === 2) fixture.state.session = { id: 'owner-2' };
      return { id: 'native-project', name: 'Native project', owner_user_id: 'owner-1', status: 'active' };
    } }] : [] });
  await assert.rejects(fixture.invoke(), /session changed/);
  assert.ok(fixture.reads.every(({ query }) => query.signal.aborted));
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

const supervisorThread = 'cc6cfe73-2824-4360-9daf-3b3efb079931';
function supervisorBindingFixture(change = () => {}) {
  const commands = [];
  const startedCollections = [];
  const state = {
    session: { id: 'owner-1' }, db: { collection: name => name === 'business_commands' ? {} : null },
    sync: { async startCollection(name) { startedCollections.push(name); return {}; } },
    commandBus: { async dispatch(command, options) {
      commands.push({ command, options });
      const receipt = {
        command_id: command.id, status: 'completed', ok: true,
        target_record_id: command.record_id, payload: command.payload,
        result: { ok: true, contract: 'ctox.workjet.supervisor_binding.v1', binding: {
          project_id: command.payload.project_id, thread_id: command.payload.thread_id,
          thread_key: `business-os/threads/${command.payload.thread_id}`,
        } },
      };
      change(receipt, state);
      return receipt;
    } },
  };
  const context = { state, actorContext: session => ({ id: session.id }), URL };
  vm.runInNewContext(`${controlSource}\nglobalThis.invoke = workjetProjectControl;`, context);
  return { commands, startedCollections, invoke: async request => JSON.parse(JSON.stringify(await context.invoke(request))) };
}
function supervisorBindingRequest(extra = {}) {
  return { action: 'project.supervisor.bind', commandId: 'bind-1', projectId: 'project-1', threadId: supervisorThread, ...extra };
}
test('supervisor registration uses authenticated command plane and returns the existing UUID', async () => {
  const fixture = supervisorBindingFixture();
  const result = await fixture.invoke(supervisorBindingRequest());
  const { command, options } = JSON.parse(JSON.stringify(fixture.commands[0]));
  assert.equal(command.command_type, 'ctox.workjet.project.supervisor.bind');
  assert.equal(command.client_context.actor.id, 'owner-1');
  assert.deepEqual(command.payload, { project_id: 'project-1', thread_id: supervisorThread });
  assert.equal(options.until, 'terminal');
  assert.deepEqual(fixture.startedCollections, ['business_commands']);
  assert.deepEqual(result.binding, {
    contract: 'ctox.workjet.supervisor_binding.v1', projectId: 'project-1',
    threadId: supervisorThread, threadKey: `business-os/threads/${supervisorThread}`,
  });
});
test('supervisor registration rejects fabricated input and uncorrelated native results', async () => {
  for (const extra of [{ ownerUserId: 'foreign' }, { threadId: 'fake-session' }, { threadId: '00000000-0000-0000-0000-000000000000' }]) {
    const fixture = supervisorBindingFixture();
    await assert.rejects(fixture.invoke(supervisorBindingRequest(extra)));
    assert.equal(fixture.commands.length, 0);
  }
  for (const mutate of [
    receipt => { receipt.command_id = 'foreign'; },
    receipt => { receipt.result.binding.project_id = 'foreign'; },
    receipt => { receipt.result.binding.thread_key = 'made-up'; },
    receipt => { receipt.payload = { ...receipt.payload, thread_id: 'foreign' }; },
    receipt => { receipt.status = 'pending'; },
    (receipt, state) => { state.session = { id: 'foreign' }; },
  ]) {
    await assert.rejects(supervisorBindingFixture(mutate).invoke(supervisorBindingRequest()));
  }
});
