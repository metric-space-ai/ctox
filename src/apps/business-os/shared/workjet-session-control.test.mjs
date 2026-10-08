import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import vm from 'node:vm';

const appSource = readFileSync(new URL('../app.js', import.meta.url), 'utf8');
const mobileSource = readFileSync(new URL('../mobile-host.js', import.meta.url), 'utf8');
const controlStart = appSource.indexOf('const WORKJET_SESSION_CONTROL_MAX_RESULTS');
const controlEnd = appSource.indexOf('function isStaleDataPlaneGeneration', controlStart);
const controlSource = appSource.slice(controlStart, controlEnd);

const publicSession = {
  id: 'workjet-session-1',
  projectId: 'project-1',
  workingCopyId: 'copy-1',
  computerId: 'computer-1',
  threadId: 'thread-1',
  codingSessionId: 'coding-1',
  runStatus: 'running',
  fenceEpoch: 0,
  activeTransferId: null,
  createdAtMs: 1_699_999_000_000,
  updatedAtMs: 1_700_000_000_000,
};

function projectedSession(overrides = {}) {
  return {
    id: 'workjet-session-1',
    project_id: 'project-1',
    working_copy_id: 'copy-1',
    computer_id: 'computer-1',
    thread_id: 'thread-1',
    coding_session_id: 'coding-1',
    run_status: 'running',
    fence_epoch: 0,
    owner_user_id: 'owner-1',
    updated_at_ms: 1_700_000_000_000,
    is_deleted: false,
    ...overrides,
  };
}

test('Workjet session control is installed and stays on the projected RxDB command plane', () => {
  assert.match(appSource, /globalThis\.workjetSessionControl = workjetSessionControl/);
  assert.ok(controlStart >= 0 && controlEnd > controlStart, 'session control implementation exists');
  for (const action of [
    'session.list',
    'session.create',
    'session.transfer.start',
    'session.transfer.pause_ack',
    'session.transfer.status',
    'session.transfer.abort',
  ]) {
    assert.match(controlSource, new RegExp(`action === '${action.replaceAll('.', '\\.')}'`));
  }
  for (const commandType of [
    'ctox.workjet.session.list',
    'ctox.workjet.session.create',
    'ctox.workjet.session.transfer.start',
    'ctox.workjet.session.transfer.pause_ack',
    'ctox.workjet.session.transfer.status',
    'ctox.workjet.session.transfer.abort',
  ]) {
    assert.match(controlSource, new RegExp(commandType.replaceAll('.', '\\.')));
  }
  assert.match(controlSource, /startCollection\?\.\('business_commands'\)/);
  assert.match(controlSource, /startCollection\?\.\('workjet_sessions'\)/);
  assert.match(controlSource, /\{ until: 'terminal', timeoutMs: WORKJET_SESSION_CONTROL_TIMEOUT_MS \}/);
  assert.match(controlSource, /state\.db\?\.collection\?\.\('business_commands'\)/);
  assert.match(controlSource, /readTerminalCommandOutcome\(commandId\)/);
  assert.match(controlSource, /owner_user_id: \{ \$eq: ownerUserId \}/);
  assert.match(controlSource, /project_id: \{ \$eq: expected\.projectId \}/);
  assert.match(controlSource, /working_copy_id: \{ \$eq: expected\.workingCopyId \}/);
  assert.doesNotMatch(controlSource, /fetch\s*\(|XMLHttpRequest|\/api\/|https?:\/\//);
});

test('Workjet session create/list is idempotent and transfer outcomes pass through terminal documents', async () => {
  const collections = {
    business_commands: [],
    workjet_sessions: [],
  };
  const dispatched = [];
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
      return {
        async exec() {
          return collections[name].find((doc) => doc.id === id) || null;
        },
      };
    },
  });
  const writeCommand = (command, result) => {
    const doc = {
      ...command,
      status: 'completed',
      terminal_status: 'completed',
      result,
      updated_at_ms: 1_700_000_000_000,
    };
    collections.business_commands = collections.business_commands
      .filter((entry) => entry.id !== command.id).concat(doc);
  };
  const state = {
    session: { user: { id: 'owner-1' } },
    db: { collection },
    sync: {
      async startCollection() {
        return { async awaitInSync() {} };
      },
    },
    commandBus: {
      async dispatch(command, options) {
        dispatched.push({ command, options });
        const previous = collections.business_commands.find((entry) => entry.id === command.id);
        if (previous) return previous;
        if (command.command_type === 'ctox.workjet.session.create') {
          const session = projectedSession({
            id: command.payload.session_id || 'workjet-session-1',
            project_id: command.payload.project_id,
            working_copy_id: command.payload.working_copy_id,
            thread_id: command.payload.thread_id,
            coding_session_id: command.payload.coding_session_id,
          });
          collections.workjet_sessions = collections.workjet_sessions
            .filter((entry) => entry.id !== session.id).concat(session);
          writeCommand(command, { ok: true, collection: 'workjet_sessions', session });
        } else if (command.command_type === 'ctox.workjet.session.list') {
          writeCommand(command, { ok: true, sessions: collections.workjet_sessions });
        } else if (command.command_type === 'ctox.workjet.session.transfer.start') {
          writeCommand(command, {
            ok: false,
            transfer_id: 'workjet-transfer-1',
            state: null,
            error_code: 'session_not_running',
            retryable: false,
            message: 'Workjet session is not running',
          });
        } else if (command.command_type === 'ctox.workjet.session.transfer.pause_ack') {
          writeCommand(command, {
            ok: true,
            transfer_id: command.payload.transfer_id,
            state: 'packing',
            retryable: false,
          });
        } else {
          writeCommand(command, { ok: true });
        }
        return collections.business_commands.find((entry) => entry.id === command.id);
      },
    },
  };
  const context = {
    state,
    actorContext: (session) => ({ id: session.user.id }),
    newId: () => 'list-1',
    waitForSyncBridgeReady: async () => {},
    window: { setTimeout },
    setTimeout,
  };
  vm.runInNewContext(`${controlSource}\nglobalThis.__workjetSessionControl = workjetSessionControl;`, context);
  const invoke = async (request) => JSON.parse(JSON.stringify(
    await context.__workjetSessionControl(request),
  ));

  const createRequest = {
    action: 'session.create',
    commandId: 'create-session-1',
    projectId: 'project-1',
    workingCopyId: 'copy-1',
    threadId: 'thread-1',
    codingSessionId: 'coding-1',
  };
  const first = await invoke(createRequest);
  const retry = await invoke(createRequest);
  assert.deepEqual(first, { action: 'session.create', session: publicSession });
  assert.deepEqual(retry, first);
  assert.equal(collections.workjet_sessions.length, 1);

  const listed = await invoke({ action: 'session.list' });
  assert.deepEqual(listed, { action: 'session.list', sessions: [publicSession] });
  const listCommand = dispatched.find(({ command }) => command.command_type === 'ctox.workjet.session.list').command;
  assert.equal(listCommand.record_id, 'owner-1');
  assert.equal(listCommand.payload.limit, 100);
  assert.deepEqual(Object.keys(listCommand.payload), ['limit']);

  const transferOutcome = await invoke({
    action: 'session.transfer.start',
    commandId: 'transfer-start-1',
    sessionId: 'workjet-session-1',
    targetComputerId: 'computer-2',
    targetPath: '/work/project-1',
    idempotencyKey: 'transfer-key-1',
  });
  assert.deepEqual(transferOutcome, {
    action: 'session.transfer.start',
    outcome: {
      ok: false,
      retryable: false,
      transferId: 'workjet-transfer-1',
      errorCode: 'session_not_running',
      message: 'Workjet session is not running',
    },
  });

  const pauseAckRequest = {
    action: 'session.transfer.pause_ack',
    commandId: 'pause-ack-1',
    transferId: 'workjet-transfer-1',
    computerId: 'computer-1',
    fenceEpoch: 1,
    lastTerminalTurnId: 'turn-42',
    gitRepository: true,
    idempotencyKey: 'pause-key-1',
  };
  assert.deepEqual(await invoke(pauseAckRequest), {
    action: 'session.transfer.pause_ack',
    outcome: {
      ok: true,
      retryable: false,
      transferId: 'workjet-transfer-1',
      state: 'packing',
    },
  });
  const pauseAckCommand = dispatched.find(({ command }) => (
    command.command_type === 'ctox.workjet.session.transfer.pause_ack'
  )).command;
  assert.equal(pauseAckCommand.record_id, 'workjet-transfer-1');
  assert.deepEqual(JSON.parse(JSON.stringify(pauseAckCommand.payload)), {
    transfer_id: 'workjet-transfer-1',
    computer_id: 'computer-1',
    fence_epoch: 1,
    last_terminal_turn_id: 'turn-42',
    git_repository: true,
    idempotency_key: 'pause-key-1',
  });
  assert.ok(dispatched.every(({ options }) => options.until === 'terminal'));

  await assert.rejects(invoke({ action: 'session.unknown' }), /Unsupported Workjet session control action/);
  await assert.rejects(invoke({
    ...createRequest,
    commandId: 'create-session-extra',
    extra: true,
  }), /Unsupported Workjet session payload field: extra/);
  await assert.rejects(invoke({
    action: 'session.transfer.status',
    commandId: 'status-invalid',
    transferId: 'workjet-transfer-1',
    sessionId: 'workjet-session-1',
  }), /exactly one of transferId or sessionId/);
  await assert.rejects(invoke({ ...pauseAckRequest, extra: true }), (error) => (
    error?.name === 'TypeError' && /Unsupported Workjet session payload field: extra/.test(error.message)
  ));
  for (const invalidRequest of [
    { ...pauseAckRequest, transferId: 't'.repeat(161) },
    { ...pauseAckRequest, computerId: 'c'.repeat(257) },
    { ...pauseAckRequest, lastTerminalTurnId: 't'.repeat(161) },
    { ...pauseAckRequest, idempotencyKey: 'k'.repeat(161) },
    { ...pauseAckRequest, fenceEpoch: -1 },
    { ...pauseAckRequest, fenceEpoch: 1.5 },
    { ...pauseAckRequest, fenceEpoch: '1' },
    { ...pauseAckRequest, gitRepository: 'true' },
  ]) {
    await assert.rejects(invoke(invalidRequest), (error) => error?.name === 'TypeError');
  }
});

test('Mobile host bridges session.control with bounded result errors', () => {
  const bridgeStart = mobileSource.indexOf("if (command.type === 'session.control'");
  const bridgeEnd = mobileSource.indexOf('\n    }', bridgeStart);
  const bridgeSource = mobileSource.slice(bridgeStart, bridgeEnd + 6);
  assert.ok(bridgeStart >= 0 && bridgeEnd > bridgeStart, 'session mobile bridge exists');
  assert.match(bridgeSource, /allowedKeys\(command, \['protocol', 'type', 'requestId', 'request'\]\)/);
  assert.match(bridgeSource, /SAFE_ID\.test\(command\.requestId\)/);
  assert.match(bridgeSource, /globalThis\.workjetSessionControl\(command\.request\)/);
  assert.match(bridgeSource, /type: 'session\.control\.result'/);
  assert.match(bridgeSource, /'session-control-failed'/);
  assert.match(bridgeSource, /\.slice\(0, 128\)/);
  assert.match(bridgeSource, /\.slice\(0, 512\)/);
});
