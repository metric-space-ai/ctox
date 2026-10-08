import assert from 'node:assert/strict';
import { webcrypto } from 'node:crypto';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import vm from 'node:vm';
import { SUPERVISOR_EXECUTION_SCHEMA, validateSupervisorExecutionValue } from './workjet-supervisor-execution-contract.generated.mjs';
import { PROJECT_KPIS_SCHEMA, validateProjectKpiValue } from './workjet-project-kpis-contract.generated.mjs';
import { JOUR_FIXE_SCHEMA, validateJourFixeValue } from './workjet-jour-fixe-contract.generated.mjs';

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
  assert.equal((controlSource.match(/until: 'terminal'/g) || []).length, 9);
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

function projectConfigurationFixture(changeReceipt = () => {}, actor = 'owner-1') {
  const commands = [];
  const state = {
    session: { id: actor },
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
          result: { ok: true, collection: 'workjet_projects', owner_user_id: 'owner-1', project },
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

function nativeProjectDetailsFixture(changeReceipt = () => {}) {
  const commands = [];
  const state = {
    session: { id: 'owner-alias' }, db: { collection: () => ({}) },
    sync: { async startCollection(name) { assert.equal(name, 'business_commands'); return {}; } },
    commandBus: { async dispatch(command) {
      commands.push(command);
      const receipt = {
        command_id: command.id, target_record_id: command.payload.project_id,
        payload: structuredClone(command.payload), status: 'completed', ok: true,
        result: command.command_type === 'ctox.workjet.jour_fixe.meeting.read'
          ? { ok: true, meeting: null }
          : { ok: true, kpis: { project_id: command.payload.project_id,
            revision: command.payload.expected_revision === undefined ? 0 : command.payload.expected_revision + 1,
            items: (command.payload.prompts || []).map(prompt => ({
              prompt: { ...prompt, revision: 1 },
              result: { status: 'missing_source', reason_code: 'source_not_bound', message: 'No source.' },
            })) } },
      };
      changeReceipt(receipt, state);
      return receipt;
    } },
  };
  const context = { state, actorContext: session => ({ id: session.id }),
    PROJECT_KPIS_SCHEMA, validateProjectKpiValue, JOUR_FIXE_SCHEMA, validateJourFixeValue };
  vm.runInNewContext(`${controlSource}\nglobalThis.invoke = workjetProjectControl;`, context);
  return { commands, invoke: async request => JSON.parse(JSON.stringify(await context.invoke(request))) };
}

test('native KPI control reads, configures and clears prompts without a projection pull', async () => {
  const fixture = nativeProjectDetailsFixture();
  const base = { commandId: 'details-1', projectId: 'project-1' };
  const read = await fixture.invoke({ ...base, action: 'project.kpis.read' });
  assert.deepEqual(read.kpis, { project_id: 'project-1', revision: 0, items: [] });
  const configured = await fixture.invoke({ ...base, action: 'project.kpis.configure',
    operationId: 'operation-1', expectedRevision: 0, prompts: [{ prompt: 'Visitors per week', kpi_id: 'visitors' }] });
  assert.equal(configured.contract, PROJECT_KPIS_SCHEMA);
  assert.equal(configured.kpis.items[0].result.status, 'missing_source');
  assert.deepEqual(JSON.parse(JSON.stringify(fixture.commands[1].payload)), {
    project_id: 'project-1', operation_id: 'operation-1', expected_revision: 0,
    prompts: [{ kpi_id: 'visitors', prompt: 'Visitors per week' }],
  });
  const cleared = await fixture.invoke({ ...base, action: 'project.kpis.configure',
    operationId: 'operation-2', expectedRevision: 1, prompts: [] });
  assert.deepEqual(cleared.kpis, { project_id: 'project-1', revision: 2, items: [] });
  assert.equal(fixture.commands[0].command_type, 'ctox.workjet.project.kpis.read');
  assert.equal(fixture.commands[1].command_type, 'ctox.workjet.project.kpis.configure');
});

test('native project detail control rejects forged and invalid requests before dispatch', async () => {
  const configure = { action: 'project.kpis.configure', commandId: 'details', projectId: 'project-1',
    operationId: 'operation', expectedRevision: 0, prompts: [] };
  for (const change of [{ value: 3 }, { owner_user_id: 'foreign' }, { expectedRevision: -1 },
    { expectedRevision: Number.MAX_SAFE_INTEGER + 1 }, { prompts: [{ kpi_id: 'x', prompt: 'x', value: 4 }] },
    { prompts: [{ kpi_id: 'x', prompt: 'x', constructor: 'forged' }] },
    { prompts: Array.from({ length: 4 }, (_, i) => ({ kpi_id: String(i), prompt: 'x' })) }]) {
    const fixture = nativeProjectDetailsFixture();
    await assert.rejects(fixture.invoke({ ...configure, ...change }));
    assert.equal(fixture.commands.length, 0);
  }
  const fixture = nativeProjectDetailsFixture();
  await assert.rejects(fixture.invoke({ action: 'project.jour_fixe.meeting.read', commandId: 'details',
    projectId: 'project-1', meetingId: '', ownerUserId: 'foreign' }));
  assert.equal(fixture.commands.length, 0);
});

test('native KPI receipts reject foreign scope, changed intent and session replacement', async () => {
  const request = { action: 'project.kpis.configure', commandId: 'details', projectId: 'project-1',
    operationId: 'operation', expectedRevision: 0, prompts: [{ kpi_id: 'visitors', prompt: 'Visitors' }] };
  for (const mutate of [receipt => { receipt.command_id = 'foreign'; },
    receipt => { receipt.target_record_id = 'foreign'; }, receipt => { receipt.payload.operation_id = 'foreign'; },
    receipt => { receipt.payload.prompts[0].prompt = 'Different intent'; },
    receipt => { receipt.result.kpis.project_id = 'foreign'; },
    receipt => { receipt.result.kpis.items[0].result = { status: 'ready' }; },
    (receipt, state) => { state.session = { id: 'owner-alias' }; }]) {
    await assert.rejects(nativeProjectDetailsFixture(mutate).invoke(request));
  }
});

test('meeting read returns null or the authorized typed native meeting for a verified alias', async () => {
  const request = { action: 'project.jour_fixe.meeting.read', commandId: 'details', projectId: 'project-1' };
  const empty = await nativeProjectDetailsFixture().invoke(request);
  assert.equal(empty.meeting, null);
  assert.equal(empty.contract, JOUR_FIXE_SCHEMA);
  const corpus = JSON.parse(readFileSync(new URL('../../../core/rxdb/tests/fixtures/workjet-jour-fixe-v1.json', import.meta.url), 'utf8'));
  const meeting = corpus.valid_cases.find(item => item.type === 'Meeting').value;
  const fixture = nativeProjectDetailsFixture(receipt => {
    receipt.result.meeting = meeting; receipt.result.preparation_task_id = 'actual-preparation';
  });
  const result = await fixture.invoke({ ...request, meetingId: meeting.id });
  assert.deepEqual(result.meeting, meeting);
  assert.equal(result.preparationTaskId, 'actual-preparation');
  assert.equal(fixture.commands[0].command_type, 'ctox.workjet.jour_fixe.meeting.read');
});

test('meeting read rejects foreign, malformed and uncorrelated confirmations', async () => {
  const corpus = JSON.parse(readFileSync(new URL('../../../core/rxdb/tests/fixtures/workjet-jour-fixe-v1.json', import.meta.url), 'utf8'));
  for (const mutate of [receipt => { receipt.result.meeting.project_id = 'foreign'; },
    receipt => { receipt.result.meeting.id = 'foreign'; }, receipt => { delete receipt.result.meeting.supervisor; },
    receipt => { receipt.payload.meeting_id = 'foreign'; },
    receipt => { receipt.result.meeting = null; },
    receipt => { receipt.result.meeting = null; receipt.result.preparation_task_id = 'invented'; }]) {
    const fixture = nativeProjectDetailsFixture(receipt => {
      receipt.result.meeting = structuredClone(corpus.valid_cases.find(item => item.type === 'Meeting').value);
      mutate(receipt);
    });
    await assert.rejects(fixture.invoke({ action: 'project.jour_fixe.meeting.read', commandId: 'details',
      projectId: 'project-1', meetingId: 'meeting-1' }));
  }
});

function nativeMeetingOwnerFixture(changeReceipt = () => {}) {
  const fixtureCrypto = typeof webcrypto === 'undefined' ? globalThis.crypto : webcrypto;
  const commands = [];
  const state = {
    session: { id: 'owner-alias' }, db: { collection: () => ({}) },
    syncConfig: { instance_id: 'biz_fixture' },
    sync: { async startCollection(name) { assert.equal(name, 'business_commands'); return {}; } },
    commandBus: { async dispatch(command) {
      commands.push(command);
      // Match normalizeCommandDocument's actual transport metadata.
      const payload = { ...structuredClone(command.payload), inbound_channel: command.inbound_channel || 'ctox' };
      const append = command.command_type.endsWith('.transcript.append');
      const local = command.command_type.endsWith('.transcript.local_candidate');
      const revise = command.command_type.endsWith('.todos.revise');
      const narration = command.command_type.endsWith('.narration.local_publish');
      const comment = command.command_type.endsWith('.comment.add');
      const receipt = {
        command_id: command.id, target_record_id: command.record_id,
        payload, status: 'completed', ok: true,
        result: { ok: true, contract: JOUR_FIXE_SCHEMA, mutation: {
          operation_id: payload.operation_id, meeting_id: payload.meeting_id,
          project_id: command.record_id, revision: payload.expected_revision + 1,
          state: narration ? 'ready' : command.command_type.endsWith('.meeting.start') || append || local ? 'live' : 'review',
          ...(append ? { changed_id: payload.turn.id } : {}),
          ...(comment ? { changed_id: payload.comment_id } : {}),
          ...(narration ? { changed_id: payload.slide_id } : {}),
          ...(revise ? { todos_revision: payload.proposal_revision } : {}),
        } },
      };
      if (narration) {
        receipt.result.owner_user_id = 'native-owner';
        receipt.result.local_narration = { operation_id: payload.operation_id, instance_id: payload.instance_id,
          project_id: payload.project_id, meeting_id: payload.meeting_id, slide_id: payload.slide_id,
          deck_revision: payload.deck_revision, owner_user_id: 'native-owner', revision: payload.expected_revision + 1,
          persisted_at_ms: 1791450012000, provenance: 'authenticated_owner_local_audio', provider_verified: false,
          audio: { file_id: 'persisted-native-audio', generation_id: 'immutable-native-generation', sha256: payload.audio_sha256,
            narration_text_sha256: payload.narration_text_sha256, mime_type: 'audio/wav', duration_ms: 2000,
            source_run_id: payload.operation_id, model: 'owner-uploaded-local-audio', format: 'wav', synthesis_duration_ms: 0,
            provenance: 'authenticated_owner_local_audio' } };
      }
      if (local) {
        const hash = [...new Uint8Array(await fixtureCrypto.subtle.digest('SHA-256', new TextEncoder().encode(payload.text)))]
          .map(byte => byte.toString(16).padStart(2, '0')).join('');
        receipt.result.owner_user_id = 'owner-1';
        receipt.result.local_candidate = {
          operation_id: payload.operation_id, request_id: payload.request_id, instance_id: payload.instance_id,
          project_id: command.record_id, meeting_id: payload.meeting_id, deck_revision: payload.deck_revision,
          owner_user_id: 'owner-1', turn_id: 'native-local-turn', sequence: 1, revision: payload.expected_revision + 1,
          text_sha256: hash, persisted_at_ms: 1001, provenance: 'authenticated_owner_local_candidate', provider_verified: false,
        };
        receipt.result.mutation.changed_id = 'native-local-turn';
      }
      changeReceipt(receipt, state);
      return receipt;
    } },
  };
  const context = { state, crypto: fixtureCrypto, TextEncoder, actorContext: session => ({ id: session.id }), JOUR_FIXE_SCHEMA, validateJourFixeValue };
  vm.runInNewContext(`${controlSource}\nglobalThis.invoke = workjetProjectControl;`, context);
  return { commands, invoke: async request => JSON.parse(JSON.stringify(await context.invoke(request))) };
}

function meetingOwnerRequest(action = 'project.jour_fixe.meeting.start', extra = {}) {
  return { action, commandId: 'meeting-control', projectId: 'project-1', operationId: 'operation-1',
    meetingId: 'meeting-1', expectedRevision: 3,
    ...(action === 'project.jour_fixe.transcript.append' ? { turn: {
      id: 'owner-text', sequence: 1, speaker: 'owner', modality: 'text', text: 'Please verify persistence.',
      started_at_ms: 1000, ended_at_ms: 1001, meeting_id: 'meeting-1',
    } } : {}),
    ...(action === 'project.jour_fixe.narration.local_publish' ? { slideId: 'slide-1', fileId: 'owner-upload', generationId: 'upload-gen',
      deckRevision: 1, audioSha256: 'a'.repeat(64), narrationTextSha256: 'b'.repeat(64) } : {}),
    ...(action === 'project.jour_fixe.comment.add' ? { commentId: 'comment-1', slideId: 'slide-1',
      deckRevision: 1, x: 0.25, y: 0.75, text: 'Please prioritize persistence.' } : {}),
    ...(action === 'project.jour_fixe.todos.revise' ? { proposalRevision: 2, items: [{
      id: 'todo-1', title: 'Verify persistence', acceptance: 'Save survives reopen',
      priority: 'P1', evidence_ids: ['owner-text'],
    }] } : {}), ...extra };
}

test('meeting Owner controls preserve typed intent and compact native revision receipts', async () => {
  for (const suffix of ['meeting.start', 'meeting.end', 'transcript.append', 'todos.revise', 'comment.add']) {
    const action = `project.jour_fixe.${suffix}`;
    const fixture = nativeMeetingOwnerFixture();
    const request = meetingOwnerRequest(action);
    const result = await fixture.invoke(request);
    assert.equal(result.contract, JOUR_FIXE_SCHEMA);
    assert.equal(result.mutation.revision, 4);
    assert.equal(result.mutation.operation_id, request.operationId);
    assert.equal(fixture.commands[0].command_type, `ctox.workjet.jour_fixe.${suffix}`);
    assert.equal(fixture.commands[0].record_id, request.projectId);
    assert.equal('project_id' in fixture.commands[0].payload, false);
    assert.equal('meeting' in result, false);
    if (request.turn) assert.deepEqual(JSON.parse(JSON.stringify(fixture.commands[0].payload.turn)), request.turn);
    if (request.items) assert.deepEqual(JSON.parse(JSON.stringify(fixture.commands[0].payload.items)), request.items);
  }
});

test('meeting Owner controls reject caller authority and unsafe revisions before dispatch', async () => {
  for (const extra of [{ ownerUserId: 'foreign' }, { owner_user_id: 'foreign' },
    { expectedRevision: -1 }, { expectedRevision: Number.MAX_SAFE_INTEGER },
    { operationId: '' }, { meetingId: '' }, { proposalRevision: 2 }]) {
    const fixture = nativeMeetingOwnerFixture();
    await assert.rejects(fixture.invoke(meetingOwnerRequest(undefined, extra)));
    assert.equal(fixture.commands.length, 0);
  }
});

test('Owner text cannot manufacture supervisor or speech provenance', async () => {
  const request = meetingOwnerRequest('project.jour_fixe.transcript.append');
  for (const change of [{ speaker: 'supervisor' }, { modality: 'speech' }, { source_run_id: 'forged' },
    { stream_id: 'forged' }, { sentence_end_latency_ms: 20 }, { sequence: 0 },
    { meeting_id: 'foreign' }, { ended_at_ms: 999 }, { author_user_id: 'foreign' }]) {
    const fixture = nativeMeetingOwnerFixture();
    await assert.rejects(fixture.invoke({ ...request, turn: { ...request.turn, ...change } }));
    assert.equal(fixture.commands.length, 0);
  }
});

test('meeting mutation rejects unsuccessful, foreign, changed-intent and stale receipts', async () => {
  const request = meetingOwnerRequest('project.jour_fixe.transcript.append');
  for (const mutate of [r => { r.command_id = 'foreign'; }, r => { r.target_record_id = 'foreign'; },
    r => { r.status = 'failed'; }, r => { r.result.contract = 'foreign'; },
    r => { r.payload.turn.text = 'Different intent'; }, r => { r.payload.inbound_channel = 'foreign'; },
    r => { delete r.payload.inbound_channel; }, r => { r.payload.unexpected = true; },
    r => { r.result.mutation.project_id = 'foreign'; },
    r => { r.result.mutation.meeting_id = 'foreign'; }, r => { r.result.mutation.operation_id = 'foreign'; },
    r => { r.result.mutation.revision = 3; }, r => { r.result.mutation.changed_id = 'foreign'; },
    r => { r.result.mutation.state = 'confirmed'; }, r => { r.result.mutation.todos_revision = 5; },
    r => { r.result.mutation.owner_user_id = 'foreign'; },
    (r, state) => { state.session = { id: 'owner-alias' }; },
    (r, state) => { state.db = { collection: () => ({}) }; }]) {
    await assert.rejects(nativeMeetingOwnerFixture(mutate).invoke(request));
  }
  await assert.rejects(nativeMeetingOwnerFixture(r => { r.result.mutation.state = 'live'; })
    .invoke(meetingOwnerRequest('project.jour_fixe.meeting.end')));
  await assert.rejects(nativeMeetingOwnerFixture(r => { r.result.mutation.todos_revision = 1; })
    .invoke(meetingOwnerRequest('project.jour_fixe.todos.revise')));
});

test('meeting mutation snapshots nested intent across the native wait', async () => {
  const request = meetingOwnerRequest('project.jour_fixe.transcript.append');
  const fixture = nativeMeetingOwnerFixture(() => { request.turn.text = 'Changed while waiting'; });
  const result = await fixture.invoke(request);
  assert.equal(result.mutation.changed_id, 'owner-text');
  assert.equal(fixture.commands[0].payload.turn.text, 'Please verify persistence.');
});


function localCandidateRequest(extra = {}) {
  return meetingOwnerRequest('project.jour_fixe.transcript.local_candidate', {
    requestId: 'helper:final:1', deckRevision: 1, text: 'Lokaler Kandidat.', ...extra,
  });
}
test('local final uses the paired native instance and waits for exact unverified storage receipt', async () => {
  const fixture = nativeMeetingOwnerFixture();
  const result = await fixture.invoke(localCandidateRequest());
  assert.equal(fixture.commands.length, 1);
  assert.equal(fixture.commands[0].command_type, 'ctox.workjet.jour_fixe.transcript.local_candidate');
  assert.equal(fixture.commands[0].payload.instance_id, 'biz_fixture');
  assert.equal(fixture.commands[0].payload.project_id, 'project-1');
  assert.equal(result.localCandidate.owner_user_id, 'owner-1');
  assert.equal(result.localCandidate.provider_verified, false);
  assert.equal(result.localCandidate.provenance, 'authenticated_owner_local_candidate');
  assert.equal(result.localCandidate.turn_id, result.mutation.changed_id);
});
test('local final rejects caller provider authority, instance selection and invalid UTF-8 budgets', async () => {
  for (const extra of [{ nativeInstanceId: 'foreign' }, { instanceId: 'managed:foreign' },
    { provider_verified: true }, { speaker: 'supervisor' }, { turn: {} }, { text: ' ' },
    { text: 'é'.repeat(2049) }, { requestId: '' }, { deckRevision: 0 }]) {
    const fixture = nativeMeetingOwnerFixture();
    await assert.rejects(fixture.invoke(localCandidateRequest(extra)));
    assert.equal(fixture.commands.length, 0);
  }
});
test('local final rejects foreign, altered, verified-provider and stale receipt scopes', async () => {
  for (const mutate of [
    r => { r.result.local_candidate.instance_id = 'foreign'; },
    r => { r.result.local_candidate.project_id = 'foreign'; },
    r => { r.result.local_candidate.meeting_id = 'foreign'; },
    r => { r.result.local_candidate.deck_revision = 2; },
    r => { r.result.local_candidate.request_id = 'other-final'; },
    r => { r.result.local_candidate.operation_id = 'other-operation'; },
    r => { r.result.local_candidate.owner_user_id = 'foreign'; },
    r => { r.result.local_candidate.text_sha256 = 'a'.repeat(64); },
    r => { r.result.local_candidate.provider_verified = true; },
    r => { r.result.local_candidate.provenance = 'native_verified_gateway'; },
    r => { r.result.local_candidate.sequence = 0; },
    r => { r.result.local_candidate.revision = 3; },
    r => { r.result.mutation.state = 'review'; },
    r => { r.payload.text = 'Different final'; },
    (r, state) => { state.syncConfig.instance_id = 'biz_other'; },
    (r, state) => { state.session = { id: 'other-owner' }; },
  ]) await assert.rejects(nativeMeetingOwnerFixture(mutate).invoke(localCandidateRequest()));
});
test('local final snapshots exact UTF-8 text while awaiting native receipt', async () => {
  const request = localCandidateRequest({ text: 'é'.repeat(2048) });
  const fixture = nativeMeetingOwnerFixture(() => { request.text = 'Changed while waiting'; });
  const result = await fixture.invoke(request);
  assert.equal(result.localCandidate.provider_verified, false);
  assert.equal(fixture.commands[0].payload.text, 'é'.repeat(2048));
});

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

test('configuration accepts a native verified owner alias without changing its actor', async () => {
  const alias = 'owner@example.org';
  const fixture = projectConfigurationFixture(() => {}, alias);
  const result = await fixture.invoke(projectConfigurationRequest({ info: { summary: 'Saved via alias' } }));
  assert.equal(result.project.id, 'project-1');
  assert.equal(result.project.info.summary, 'Saved via alias');
  assert.equal(fixture.commands[0].client_context.actor.id, alias);
  assert.equal(Object.hasOwn(fixture.commands[0].payload, 'owner_user_id'), false);
});

test('configuration retains same-actor compatibility but does not infer aliases from old receipts', async () => {
  const legacy = (receipt) => { delete receipt.result.owner_user_id; };
  assert.equal((await projectConfigurationFixture(legacy).invoke(projectConfigurationRequest())).project.id, 'project-1');
  await assert.rejects(projectConfigurationFixture(legacy, 'owner@example.org')
    .invoke(projectConfigurationRequest()), /uncorrelated/);
});

test('project info summary follows the same configuration corpus as native upsert', async () => {
  const corpus = JSON.parse(readFileSync(new URL('../../../core/rxdb/tests/fixtures/workjet-project-configuration-v1.json', import.meta.url), 'utf8'));
  for (const info of corpus.valid) {
    const fixture = projectConfigurationFixture();
    const result = await fixture.invoke(projectConfigurationRequest({ info }));
    assert.deepEqual(JSON.parse(JSON.stringify(fixture.commands[0].payload.info)), info);
    assert.deepEqual(result.project.info, info);
  }
  for (const info of corpus.invalid) {
    const fixture = projectConfigurationFixture();
    await assert.rejects(fixture.invoke(projectConfigurationRequest({ info })));
    assert.equal(fixture.commands.length, 0);
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
    (receipt) => { receipt.result.owner_user_id = 'foreign'; },
    (receipt) => { receipt.result.owner_user_id = null; },
    (receipt) => { receipt.result.owner_user_id = ' owner-1'; },
    (receipt) => { receipt.result.owner_user_id = 'owner-1\n'; },
    (receipt) => { receipt.result.owner_user_id = 'x'.repeat(257); },
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

function nativeProjectListFixture({ start, dispatch, exec, ownerUserId = 'owner-1' } = {}) {
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
          assert.equal(query.selector.owner_user_id.$eq, ownerUserId);
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
      const nativeRows = rows.workjet_projects.filter((row) => row.owner_user_id === ownerUserId
        && row.status === 'active' && row.is_deleted !== true && row._deleted !== true);
      const nativeCount = nativeRows.length;
      const receipt = { command_id: command.id, status: 'completed', ok: true,
        result: { ok: true, collection: 'workjet_projects', owner_user_id: ownerUserId,
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

test('a verified alias lists twelve owner projects and repairs missing rows without changing its actor', async () => {
  const fixture = nativeProjectListFixture({ exec: (name, query) => {
    if (name !== 'workjet_projects') return fixture.rows[name];
    return query.selector.id
      ? fixture.rows[name].filter((row) => query.selector.id.$in.includes(row.id))
      : fixture.rows[name].slice(0, 10);
  } });
  fixture.state.session = { id: 'michael.welsch@metric-space.ai' };
  fixture.rows.workjet_projects = Array.from({ length: 16 }, (_, index) => ({
    id: `project-${index}`, name: `Project ${index}`,
    status: index < 12 ? 'active' : 'archived', owner_user_id: 'owner-1',
  }));
  fixture.rows.workjet_projects.push({
    id: 'foreign-active', name: 'Foreign', status: 'active', owner_user_id: 'owner-2',
  });
  fixture.rows.workjet_working_copies[0].project_id = 'project-0';
  fixture.rows.workjet_working_copies.push({
    id: 'foreign-copy', project_id: 'project-0', computer_id: 'foreign',
    path: 'guest://foreign', status: 'active', owner_user_id: 'owner-2',
  });
  const before = JSON.stringify(fixture.rows);
  const result = await fixture.invoke();
  assert.equal(result.count, 12);
  assert.deepEqual(result.projects.map(({ id }) => id).sort(),
    Array.from({ length: 12 }, (_, index) => `project-${index}`).sort());
  assert.deepEqual(result.projects.find(({ id }) => id === 'project-0').workingCopies.map(({ id }) => id),
    ['native-copy']);
  assert.ok(fixture.reads.every(({ query }) => query.selector.owner_user_id.$eq === 'owner-1'));
  const repair = fixture.reads.find(({ query }) => query.selector.id);
  assert.deepEqual(Array.from(repair.query.selector.id.$in), ['project-10', 'project-11']);
  assert.equal(fixture.commands[0].command.client_context.actor.id, 'michael.welsch@metric-space.ai');
  assert.equal(fixture.commands[0].command.record_id, 'michael.welsch@metric-space.ai');
  assert.equal(fixture.state.session.id, 'michael.welsch@metric-space.ai');
  assert.equal(JSON.stringify(fixture.rows), before, 'owners, archives and working copies are unchanged');
});

test('an unrelated actor receives zero projects even when projections contain another owner', async () => {
  const fixture = nativeProjectListFixture({ ownerUserId: 'foreign-user', exec: (name) => fixture.rows[name] });
  fixture.state.session = { id: 'foreign-user' };
  assert.deepEqual(await fixture.invoke(), { action: 'project.list', projects: [], count: 0, truncated: false });
  assert.ok(fixture.reads.every(({ query }) => query.selector.owner_user_id.$eq === 'foreign-user'));
});

test('invalid confirmed owners fail before reads and requests cannot choose an owner', async () => {
  for (const owner of [null, undefined, {}, [], 7, '', ' owner-1', 'owner-1 ', 'bad\u0000id', 'x'.repeat(257)]) {
    const fixture = nativeProjectListFixture({ dispatch: (receipt) => {
      receipt.result.owner_user_id = owner;
      return receipt;
    } });
    await assert.rejects(fixture.invoke(), (error) => error.code === 'WORKJET_PROJECT_LIST_UNCONFIRMED');
    assert.equal(fixture.reads.length, 0);
  }
  for (const request of [{ ownerUserId: 'owner-2' }, { owner_user_id: 'owner-2' }]) {
    const fixture = nativeProjectListFixture();
    await assert.rejects(fixture.invoke(request), /Unsupported Workjet project payload field/);
    assert.equal(fixture.commands.length, 0);
  }
});

test('a confirmed owner never replaces the original alias session fence during a repair read', async () => {
  const fixture = nativeProjectListFixture({ exec: (name, query) => {
    if (name !== 'workjet_projects' || !query.selector.id) return [];
    fixture.state.session.id = 'owner-1';
    return fixture.rows.workjet_projects;
  } });
  fixture.state.session = { id: 'michael.welsch@metric-space.ai' };
  await assert.rejects(fixture.invoke(), /session changed/);
  assert.ok(fixture.reads.every(({ query }) => query.signal.aborted));
});

test('legacy native receipts retain the authenticated actor scope without guessing an alias', async () => {
  const fixture = nativeProjectListFixture({ dispatch: (receipt) => {
    delete receipt.result.owner_user_id;
    return receipt;
  } });
  assert.equal((await fixture.invoke()).count, 1);
  assert.ok(fixture.reads.every(({ query }) => query.selector.owner_user_id.$eq === 'owner-1'));
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


const nativeTurnId = 'cmd_74c4a208-7b2d-4b5e-a83d-f2f012d4c5a9';
function supervisorTurnFixture(change = () => {}) {
  const commands = [];
  const state = {
    session: { id: 'owner-1' }, db: { collection: name => name === 'business_commands' ? {} : null },
    sync: { async startCollection(name) { assert.equal(name, 'business_commands'); } },
    commandBus: { async dispatch(command, options) {
      commands.push({ command, options });
      const cancelled = command.command_type.endsWith('.cancel');
      const receipt = {
        command_id: command.id, ok: true, status: 'completed', target_record_id: 'project-1',
        payload: command.payload,
        result: {
          ok: true, contract: 'ctox.workjet.supervisor_turn.v1',
          binding: { project_id: 'project-1', thread_id: supervisorThread, thread_key: `business-os/threads/${supervisorThread}` },
          message_id: 'workjet_supervisor_message_1',
          turn: {
            command_id: nativeTurnId, task_id: 'queue:system::supervisor-turn',
            thread_id: supervisorThread, thread_key: `business-os/threads/${supervisorThread}`,
            execution_phase: cancelled ? 'terminal' : 'queued', status: cancelled ? 'cancelled' : 'queued',
            queue_status: cancelled ? 'cancelled' : 'pending', attempt: 0,
            terminal: cancelled, result: {}, result_truncated: false, error_code: null, error_message: null,
          },
          cancellation: { command_id: 'workjet_project_cancel_1', side_effects_may_have_started: false,
            worker_interrupt_acknowledged: false },
        },
      };
      change(receipt, state);
      return receipt;
    } },
  };
  const context = { state, actorContext: session => ({ id: session.id }), URL, SUPERVISOR_EXECUTION_SCHEMA, validateSupervisorExecutionValue };
  vm.runInNewContext(`${controlSource}\nglobalThis.invoke = workjetProjectControl;`, context);
  return { commands, invoke: async request => JSON.parse(JSON.stringify(await context.invoke(request))) };
}
function supervisorTurnRequest(action, extra = {}) {
  return {
    action: `project.supervisor.turn.${action}`, commandId: `${action}-1`, projectId: 'project-1', threadId: supervisorThread,
    ...(action === 'submit' ? { goal: 'Prepare the project report' } : { targetCommandId: nativeTurnId }),
    ...(action === 'cancel' ? { reason: 'Cancelled in Workjet' } : {}), ...extra,
  };
}

test('supervisor submit watch cancel use the native control plane on the same CodeThread', async () => {
  for (const action of ['submit', 'watch', 'cancel']) {
    const fixture = supervisorTurnFixture();
    const result = await fixture.invoke(supervisorTurnRequest(action));
    const { command, options } = fixture.commands[0];
    assert.equal(command.command_type, `ctox.workjet.project.supervisor.turn.${action}`);
    assert.equal(command.client_context.actor.id, 'owner-1');
    assert.equal(options.until, 'terminal');
    assert.equal(options.sync_queue_tasks, false);
    assert.equal(result.binding.threadId, supervisorThread);
    assert.equal(result.binding.threadKey, `business-os/threads/${supervisorThread}`);
    assert.equal(result.turn.commandId, nativeTurnId);
    if (action === 'submit') {
      assert.equal(result.messageId, 'workjet_supervisor_message_1');
      assert.equal(command.payload.goal, 'Prepare the project report');
    } else assert.equal(command.payload.target_command_id, nativeTurnId);
    if (action === 'cancel') assert.equal(result.cancellation.workerInterruptAcknowledged, false);
  }
});

test('supervisor controls refuse forged owner routes and unsupported execution parameters', async () => {
  for (const extra of [
    { ownerUserId: 'foreign' }, { threadKey: 'invented-route' }, { computerId: 'foreign' },
    { externalExecutor: {} }, { riskClass: 'external' }, { threadId: '00000000-0000-0000-0000-000000000000' },
    { goal: 'x'.repeat(4097) }, { goal: ' ' },
  ]) {
    const fixture = supervisorTurnFixture();
    await assert.rejects(fixture.invoke(supervisorTurnRequest('submit', extra)));
    assert.equal(fixture.commands.length, 0);
  }
});

test('supervisor turn results reject foreign identities queue links and changed sessions', async () => {
  for (const change of [
    receipt => { receipt.command_id = 'foreign'; },
    receipt => { receipt.target_record_id = 'foreign'; },
    receipt => { receipt.result.binding.project_id = 'foreign'; },
    receipt => { receipt.result.turn.thread_key = 'foreign'; },
    receipt => { receipt.result.turn.command_id = 'foreign'; },
    receipt => { receipt.result.turn.task_id = ''; },
    receipt => { receipt.result.turn.attempt = -1; },
    receipt => { receipt.result.turn.terminal = true; },
    receipt => { receipt.result.contract = 'foreign'; },
    receipt => { receipt.payload = { ...receipt.payload, target_command_id: 'foreign' }; },
    (receipt, state) => { state.session = { id: 'foreign' }; },
  ]) await assert.rejects(supervisorTurnFixture(change).invoke(supervisorTurnRequest('watch')));
});

test('supervisor cancellation never pretends a running worker acknowledged interruption', async () => {
  for (const change of [
    receipt => { receipt.result.turn.status = 'completed'; },
    receipt => { receipt.result.cancellation.worker_interrupt_acknowledged = true; },
    receipt => { receipt.result.cancellation.side_effects_may_have_started = null; },
    receipt => { receipt.result.cancellation.command_id = ''; },
  ]) await assert.rejects(supervisorTurnFixture(change).invoke(supervisorTurnRequest('cancel')));
});


function nativeExecutionPage() {
  return {
    command_id: nativeTurnId, task_id: 'queue:system::supervisor-turn',
    attempt: { attempt_id: 'native-attempt', attempt_index: 47 },
    events: [{ id: 'event-actual', sequence: 12, kind: 'worker.tool_completed',
      title: 'Saved native tool result', created_at_ms: 1791410400000, tool_name: 'native.tool', success: true }],
    next_cursor: { after_sequence: 12, after_event_id: 'event-actual' }, has_more: false,
  };
}
function executionFixture(change = () => {}) {
  return supervisorTurnFixture((receipt, state) => {
    receipt.result.execution_contract = SUPERVISOR_EXECUTION_SCHEMA;
    receipt.result.execution_page = nativeExecutionPage();
    change(receipt, state);
  });
}

test('legacy supervisor watch has exactly the prior outer shape even if unsolicited facts arrive', async () => {
  const result = await executionFixture().invoke(supervisorTurnRequest('watch'));
  assert.deepEqual(Object.keys(result).sort(), ['action', 'binding', 'commandId', 'contract', 'projectId', 'turn']);
  assert.equal(result.turn.attempt, 0);
  assert.equal(result.executionPage, undefined);
});

test('opted-in supervisor watch forwards the bounded fixture contract and actual native facts', async () => {
  const fixture = executionFixture(receipt => {
    receipt.payload = { ...receipt.payload, execution_page: { limit: 1, attempt_id: 'native-attempt' } };
    receipt.result.execution_page.next_cursor = { after_event_id: 'event-actual', after_sequence: 12 };
  });
  const result = await fixture.invoke(supervisorTurnRequest('watch', { executionPage: { attempt_id: 'native-attempt', limit: 1 } }));
  assert.deepEqual(JSON.parse(JSON.stringify(fixture.commands[0].command.payload.execution_page)), { attempt_id: 'native-attempt', limit: 1 });
  assert.equal(result.executionContract, SUPERVISOR_EXECUTION_SCHEMA);
  assert.equal(result.executionPage.attempt.attempt_id, 'native-attempt');
  assert.equal(result.executionPage.attempt.attempt_index, 47);
  assert.equal(result.executionPage.attempt.run_id, undefined);
  assert.equal(result.executionPage.events[0].sequence, 12);
});

test('an opted-in queued supervisor turn preserves the absence of an actual attempt', async () => {
  const fixture = executionFixture(receipt => {
    receipt.result.execution_page = { command_id: nativeTurnId, task_id: 'queue:system::supervisor-turn', events: [], has_more: false };
  });
  const result = await fixture.invoke(supervisorTurnRequest('watch', { executionPage: {} }));
  assert.equal(result.executionPage.attempt, undefined);
  assert.deepEqual(result.executionPage.events, []);
});

test('execution page invalid bounds and extra caller authority never dispatch', async () => {
  for (const executionPage of [null, { limit: 0 }, { limit: 51 }, { attempt_id: ' ' },
    { attempt_id: '\u0085' }, { owner_user_id: 'foreign' },
    JSON.parse('{"__proto__":{}}'), { cursor: { after_sequence: 0, after_event_id: 'x' } }]) {
    const fixture = executionFixture();
    await assert.rejects(fixture.invoke(supervisorTurnRequest('watch', { executionPage })));
    assert.equal(fixture.commands.length, 0);
  }
  for (const action of ['submit', 'cancel']) {
    const fixture = executionFixture();
    await assert.rejects(fixture.invoke(supervisorTurnRequest(action, { executionPage: {} })));
    assert.equal(fixture.commands.length, 0);
  }
});

test('execution pages reject foreign native identities unsafe fields and receipt substitution', async () => {
  for (const change of [
    receipt => { receipt.result.execution_contract = 'foreign'; },
    receipt => { receipt.result.execution_page.command_id = 'foreign'; },
    receipt => { receipt.result.execution_page.task_id = 'foreign'; },
    receipt => { receipt.result.execution_page.attempt.attempt_id = 'foreign'; },
    receipt => { receipt.result.execution_page.events[0].arguments = { secret: 'private' }; },
    receipt => { receipt.result.execution_page.events[0].sequence = Number.MAX_SAFE_INTEGER + 1; },
    receipt => { receipt.payload = { ...receipt.payload, execution_page: { attempt_id: 'foreign' } }; },
    (receipt, state) => { state.session = { id: 'foreign' }; },
  ]) await assert.rejects(executionFixture(change).invoke(supervisorTurnRequest('watch', { executionPage: { attempt_id: 'native-attempt' } })));
});

test('execution page cursor must match the ordered safe native event page', async () => {
  for (const change of [
    receipt => { receipt.result.execution_page.next_cursor.after_event_id = 'foreign'; },
    receipt => { receipt.result.execution_page.events[0].sequence = 3; },
    receipt => { receipt.result.execution_page.events.push({ ...receipt.result.execution_page.events[0] }); },
    receipt => { receipt.result.execution_page.events = []; receipt.result.execution_page.has_more = true; },
    receipt => { delete receipt.result.execution_page.attempt; },
  ]) await assert.rejects(executionFixture(change).invoke(supervisorTurnRequest('watch', {
    executionPage: { attempt_id: 'native-attempt', cursor: { after_sequence: 5, after_event_id: 'prior-event' } },
  })));
});

test('slide comment rejects unsafe pins and claimed author before dispatch', async () => {
  for (const extra of [{ x: -0.1 }, { y: 1.1 }, { x: NaN }, { y: Infinity },
    { slideId: '' }, { commentId: '' }, { text: '' }, { author_user_id: 'owner' },
    { supervisor_event_id: 'forged' }]) {
    const fixture = nativeMeetingOwnerFixture();
    await assert.rejects(fixture.invoke(meetingOwnerRequest('project.jour_fixe.comment.add', extra)));
    assert.equal(fixture.commands.length, 0);
  }
});

test('slide comment receipt must confirm the same comment identity', async () => {
  const request = meetingOwnerRequest('project.jour_fixe.comment.add');
  for (const change of [receipt => { receipt.result.mutation.changed_id = 'foreign-comment'; },
    receipt => { delete receipt.result.mutation.changed_id; },
    receipt => { receipt.payload.slide_id = 'another-slide'; }]) {
    await assert.rejects(nativeMeetingOwnerFixture(change).invoke(request));
  }
});

test('local narration derives the native instance and exact persisted local custody', async () => {
 const request=meetingOwnerRequest('project.jour_fixe.narration.local_publish');
 const fixture=nativeMeetingOwnerFixture();const value=await fixture.invoke(request);
 assert.equal(fixture.commands[0].payload.instance_id,'biz_fixture');assert.equal(fixture.commands[0].payload.project_id,request.projectId);
 assert.equal(value.localNarration.audio.file_id,'persisted-native-audio');assert.equal(value.localNarration.provider_verified,false);
 assert.equal(value.mutation.state,'ready');assert.equal(value.localNarration.owner_user_id,'native-owner');
 for (const extra of [{instanceId:'caller-instance'},{ownerUserId:'foreign'},{model:'Apple'},{durationMs:1},{audioSha256:'A'.repeat(64)}]) {
  const bad=nativeMeetingOwnerFixture();await assert.rejects(bad.invoke({...request,...extra}));assert.equal(bad.commands.length,0);
 }
});
test('local narration rejects corrupted scope, bytes, provenance and replaced instance', async () => {
 const request=meetingOwnerRequest('project.jour_fixe.narration.local_publish');
 for (const mutate of [r=>{r.result.local_narration.instance_id='foreign';},r=>{r.result.local_narration.deck_revision=2;},
  r=>{r.result.local_narration.slide_id='foreign';},r=>{r.result.local_narration.owner_user_id='foreign';},
  r=>{r.result.local_narration.provider_verified=true;},r=>{r.result.local_narration.audio.sha256='c'.repeat(64);},
  r=>{r.result.local_narration.audio.file_id='owner-upload';},r=>{r.result.local_narration.audio.generation_id='';},
  r=>{r.result.local_narration.audio.model='gateway-provider';},r=>{r.result.local_narration.audio.duration_ms=0;},
  r=>{r.result.local_narration.audio.narration_text_sha256='c'.repeat(64);},r=>{r.result.local_narration.audio.provenance='native_gateway';},
  r=>{r.result.mutation.state='live';},(r,state)=>{state.syncConfig.instance_id='biz_other';}]) {
  await assert.rejects(nativeMeetingOwnerFixture(mutate).invoke(request));
 }
});
