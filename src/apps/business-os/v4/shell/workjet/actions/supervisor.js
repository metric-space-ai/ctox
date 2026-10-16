// Shell v4 – Workjet-Brücke, Aktionsgruppe B (Supervisor bind/turn/route, Planschritt wiederholen). Eigentum Strang B.
//
// Muster wie actions/project.js: jede Aktion ist `async (env, request, diagnostics) => response`; der Rumpf bleibt
// wörtlich V2 (ref-Anker), nur `state.` → `env.state.`, `actorContext` → `env.actorContext`.
// V2-Quellen: app.js = origin/main e35a116ae (/Volumes/tmp/app-main.js); ctox.plan.step.retry = V2-PR #874
// (Branch shell/plan-step-retry f0c836569, app.js:13918-13950); requestSupervisorRoute =
// shared/workjet-supervisor-route-native.mjs (e35a116ae).
//
// Alle Aktionen laufen über den Befehlspfad (commandBus.dispatch → v2-compat → api.command); kein requestNative.
// Fehlt eine Kernroute, antwortet der Kern unknown_command und v2-compat wirft {code:'unsupported'} mit der
// Nachricht „Unsupported Workjet project control action: <befehl>“ – Workjet blendet den Knopf dann aus.
import {
  WORKJET_PROJECT_CONTROL_TIMEOUT_MS, boundedWorkjetProjectText, assertWorkjetProjectPayloadKeys,
} from '../v2-helpers.js';
import { SUPERVISOR_EXECUTION_SCHEMA, validateSupervisorExecutionValue } from '../../../../shared/workjet-supervisor-execution-contract.generated.mjs';
import { validateSupervisorRouteDisplayValue } from '../../../../shared/workjet-supervisor-route-display-contract.generated.mjs';
import { validateSupervisorRouteComputationValue } from '../../../../shared/workjet-supervisor-route-computation-contract.generated.mjs';

// ---- requestSupervisorRoute (wörtlich) ---------------------------------------------------------------------------
// ref: shared/workjet-supervisor-route-native.mjs:7-73
const ROUTE_READ = 'project.supervisor.route.read.v1';
const ROUTE_CAPABILITIES = 'project.supervisor.route.capabilities.v1';
const ROUTE_SCHEMA = 'ctox.workjet.supervisor.route-display.v1';
const CAPABILITIES_SCHEMA = 'ctox.workjet.supervisor.route-capabilities.v1';
const ROUTE_ACTIONS = [ROUTE_READ, ROUTE_CAPABILITIES, 'project.supervisor.route.read.v2', 'project.supervisor.route.capabilities.v2'];

function routeScopeText(value, label) {
  if (typeof value !== 'string' || !value || value !== value.trim()
    || value.length > 256 || /[\u0000-\u001f]/u.test(value)) {
    throw new TypeError('Invalid Supervisor route ' + label + '.');
  }
  return value;
}

/** Dedicated versioned bridge. No route/model is derived from local thread settings. */
export async function requestSupervisorRoute(dispatch, request, actor, assertCurrent) {
  if (!request || typeof request !== 'object' || Array.isArray(request)
    || Object.keys(request).some(key => !['action', 'commandId', 'projectId', 'threadId'].includes(key))
    || !ROUTE_ACTIONS.includes(request.action)) {
    throw new TypeError('Invalid Supervisor route request.');
  }
  const commandId = routeScopeText(request.commandId, 'commandId');
  const projectId = routeScopeText(request.projectId, 'projectId');
  const threadId = routeScopeText(request.threadId, 'threadId');
  routeScopeText(actor?.id, 'authenticated Owner');
  const version = request.action.endsWith('.v2') ? 'v2' : 'v1';
  const capabilities = request.action.includes('.capabilities.');
  const commandType = 'ctox.workjet.' + request.action;
  const routeSchema = version === 'v2' ? 'ctox.workjet.supervisor.route-display.v2' : ROUTE_SCHEMA;
  const capabilitiesSchema = version === 'v2' ? 'ctox.workjet.supervisor.route-capabilities.v2' : CAPABILITIES_SCHEMA;
  const validate = version === 'v2' ? validateSupervisorRouteComputationValue : validateSupervisorRouteDisplayValue;
  const payload = { project_id: projectId, thread_id: threadId };
  assertCurrent();
  const receipt = await dispatch({
    id: commandId, command_id: commandId, module: 'ctox', record_id: projectId,
    command_type: commandType, payload,
    client_context: { source: 'workjet-project-control', actor },
  }, { until: 'terminal', sync_queue_tasks: false, timeoutMs: 30_000 }); // v4-allow: debt L-42
  assertCurrent();
  const transported = receipt?.result;
  if (!transported || transported.status !== 'completed' || transported.task_status !== 'completed') {
    throw new Error('Supervisor route returned incomplete native result status.');
  }
  // command_plane decorates terminal DTOs. Validate these two known fields before decoding
  // the domain contract; every other extra property is still rejected by its strict validator.
  const { status: _status, task_status: _taskStatus, ...result } = transported;
  if (receipt?.command_id !== commandId || receipt.ok !== true || receipt.status !== 'completed'
    || receipt.target_record_id !== projectId
    || receipt.payload?.project_id !== projectId || receipt.payload?.thread_id !== threadId
    || result?.project_id !== projectId || result?.supervisor_thread_id !== threadId) {
    throw new Error('Supervisor route returned an unmatched native receipt.');
  }
  const type = capabilities ? 'SupervisorRouteCapabilities' : 'SupervisorRouteDisplay';
  if (!validate(type, result).ok) {
    throw new Error('Invalid native Supervisor route DTO.');
  }
  if (result.schema !== (capabilities ? capabilitiesSchema : routeSchema)
    || (capabilities && (result.read_schema !== routeSchema
      || result.read_command !== 'ctox.workjet.project.supervisor.route.read.' + version))
    || (!capabilities && version === 'v1' && result.actual !== null)) {
    throw new Error('Supervisor route returned an unsupported or unproved producer contract.');
  }
  return {
    action: request.action, commandId, projectId, threadId, contract: result.schema,
    ...(capabilities ? { capabilities: result } : { route: result }),
  };
}

// ---- gemeinsame V2-Teile -----------------------------------------------------------------------------------------

// ref: app.js:13838-13849
function boundedWorkjetExecutionRequest(value) {
  validateSupervisorExecutionValue('ExecutionPageRequest', value);
  return {
    ...(value.attempt_id == null ? {} : { attempt_id: value.attempt_id }),
    ...(value.cursor == null ? {} : { cursor: {
      after_sequence: value.cursor.after_sequence, after_event_id: value.cursor.after_event_id,
    } }),
    ...(value.limit == null ? {} : { limit: value.limit }),
    ...(value.include_public_text == null ? {} : { include_public_text: value.include_public_text }),
    ...(value.include_native_message_text == null ? {} : { include_native_message_text: value.include_native_message_text }),
  };
}

// ref: app.js:15002-15008
async function requireWorkjetSupervisorDataPlane(env) {
  const { state } = env;
  if (!state.commandBus?.dispatch || !state.db?.collection?.('business_commands')) { // v4-allow: debt L-42
    throw new Error('Workjet supervisor control is not ready.'); // v4-allow: code (V2-Fehlertext, Workjet vergleicht wörtlich)
  }
  await state.sync?.startCollection?.('business_commands'); // v4-allow: debt L-42
  return {};
}

// Gemeinsamer Vorspann aus workjetProjectControl (ref: app.js:13906-13917).
function scope(env) {
  const { state } = env;
  return {
    ownerUserId: boundedWorkjetProjectText(env.actorContext(state.session).id, 'owner_user_id', 256),
    requestSession: state.session,
    requestDb: state.db,
  };
}

// ---- project.supervisor.route.* ---------------------------------------------------------------------------------
// ref: app.js:13914-13917 (routeAuthority vor der Datenebene), 14049-14061 (Supervisor-Zulassung), 14063-14075
async function supervisorRoute(env, request) {
  const { state, actorContext } = env;
  const { ownerUserId, requestSession, requestDb } = scope(env);
  const routeAuthority = { sync: state.sync, instance: boundedWorkjetProjectText(
    state.syncConfig?.instance_id || state.sync?.config?.instance_id, 'native instanceId', 256,
  ) };
  await requireWorkjetSupervisorDataPlane(env);
  const requestSync = routeAuthority.sync;
  const instance = routeAuthority.instance;
  return requestSupervisorRoute(
    (command, options) => state.commandBus.dispatch(command, options), // v4-allow: debt L-42
    request, actorContext(requestSession), () => {
      if (state.session !== requestSession || state.db !== requestDb || state.sync !== requestSync
        || actorContext(state.session).id !== ownerUserId
        || (state.syncConfig?.instance_id || state.sync?.config?.instance_id) !== instance) {
        throw new Error('Supervisor route instance or authority changed.');
      }
    },
  );
}

// ---- project.supervisor.turn.capabilities|submit|watch|cancel|input ---------------------------------------------
// ref: app.js:14049-14061 (Zulassung), 14430-14609
async function supervisorTurn(env, request) {
  const { state, actorContext } = env;
  const action = request.action;
  const { ownerUserId, requestSession, requestDb } = scope(env);
  await requireWorkjetSupervisorDataPlane(env);
  const submitting = action === 'project.supervisor.turn.submit';
  const cancelling = action === 'project.supervisor.turn.cancel';
  const inputting = action === 'project.supervisor.turn.input';
  const capabilities = action === 'project.supervisor.turn.capabilities';
  const allowedKeys = new Set(['action', 'commandId', 'projectId', 'threadId']);
  if (submitting) { allowedKeys.add('goal'); allowedKeys.add('turnKind'); }
  else if (!capabilities) allowedKeys.add('targetCommandId');
  if (cancelling) allowedKeys.add('reason');
  if (inputting) allowedKeys.add('body');
  if (capabilities) allowedKeys.add('includeInput');
  const observing = action === 'project.supervisor.turn.watch' && request.executionPage !== undefined;
  if (action === 'project.supervisor.turn.watch') allowedKeys.add('executionPage');
  assertWorkjetProjectPayloadKeys(request, allowedKeys);
  const commandId = boundedWorkjetProjectText(request.commandId, 'commandId', 128);
  const projectId = boundedWorkjetProjectText(request.projectId, 'projectId', 128);
  const threadId = boundedWorkjetProjectText(request.threadId, 'threadId', 36);
  if (!/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(threadId)
    || threadId === '00000000-0000-0000-0000-000000000000') {
    throw new TypeError('Workjet supervisor threadId must be its existing lowercase CodeThread UUID.');
  }
  const payload = { project_id: projectId, thread_id: threadId };
  if (capabilities && Object.hasOwn(request, 'includeInput')) {
    if (typeof request.includeInput !== 'boolean') throw new TypeError('includeInput must be boolean.');
    payload.include_input = request.includeInput;
  }
  if (submitting) {
    payload.goal = boundedWorkjetProjectText(request.goal, 'goal', 4096);
    if (Object.hasOwn(request, 'turnKind')) {
      if (!['work', 'conversation'].includes(request.turnKind)) {
        throw new TypeError('Workjet supervisor turnKind must be work or conversation.');
      }
      payload.turn_kind = request.turnKind;
    }
  } else if (!capabilities) {
    payload.target_command_id = boundedWorkjetProjectText(request.targetCommandId, 'targetCommandId', 256);
  }
  if (cancelling) payload.reason = boundedWorkjetProjectText(request.reason, 'reason', 512);
  if (inputting) payload.body = boundedWorkjetProjectText(request.body, 'body', 4096);
  if (observing) payload.execution_page = boundedWorkjetExecutionRequest(request.executionPage);
  const assertCurrentIdentity = () => {
    if (state.session !== requestSession || state.db !== requestDb
      || actorContext(state.session).id !== ownerUserId) {
      throw new Error('Workjet project session changed before the supervisor turn was delivered.');
    }
  };
  assertCurrentIdentity();
  const receipt = await state.commandBus.dispatch({ // v4-allow: debt L-42
    id: commandId, command_id: commandId, module: 'ctox', record_id: projectId,
    command_type: {
      'project.supervisor.turn.capabilities': 'ctox.workjet.project.supervisor.turn.capabilities',
      'project.supervisor.turn.submit': 'ctox.workjet.project.supervisor.turn.submit',
      'project.supervisor.turn.watch': 'ctox.workjet.project.supervisor.turn.watch',
      'project.supervisor.turn.cancel': 'ctox.workjet.project.supervisor.turn.cancel',
      'project.supervisor.turn.input': 'ctox.workjet.project.supervisor.turn.input',
    }[action], payload,
    client_context: { source: 'workjet-project-control', actor: actorContext(requestSession) },
  }, { until: 'terminal', sync_queue_tasks: false, timeoutMs: WORKJET_PROJECT_CONTROL_TIMEOUT_MS }); // v4-allow: debt L-42
  assertCurrentIdentity();
  const contract = inputting ? 'ctox.workjet.supervisor_input.v1' : 'ctox.workjet.supervisor_turn.v1';
  const binding = receipt?.result?.binding;
  const turn = receipt?.result?.turn;
  const threadKey = `business-os/threads/${threadId}`;
  if (capabilities) {
    const capabilityContract = 'ctox.workjet.supervisor_turn_capabilities.v1';
    const kinds = receipt?.result?.turn_kinds;
    if (receipt?.command_id !== commandId || receipt.ok !== true || receipt.status !== 'completed'
      || receipt.target_record_id !== projectId || receipt.result?.ok !== true
      || receipt.result?.contract !== capabilityContract
      || Object.entries(payload).some(([key, value]) => receipt.payload?.[key] !== value)
      || binding?.project_id !== projectId || binding?.thread_id !== threadId
      || binding?.thread_key !== threadKey || !Array.isArray(kinds) || kinds.length !== 2
      || kinds[0] !== 'work' || kinds[1] !== 'conversation' || receipt.result.default_turn_kind !== 'work') {
      throw new Error('Workjet supervisor capabilities returned an invalid or unmatched confirmation.');
    }
    const result = {
      action, commandId, projectId, contract: capabilityContract,
      binding: { contract: 'ctox.workjet.supervisor_binding.v1', projectId,
        threadId, threadKey },
      turnKinds: kinds, defaultTurnKind: receipt.result.default_turn_kind,
    };
    if (request.includeInput === true) {
      if (receipt.result.input_contract !== 'ctox.workjet.supervisor_input.v1'
        || receipt.result.input_delivery !== 'next_slice' || receipt.result.max_input_chars !== 4096) {
        throw new Error('The connected instance does not support same-task Supervisor input.');
      }
      result.inputContract = receipt.result.input_contract;
      result.inputDelivery = 'next_slice';
      result.maxInputChars = 4096;
    }
    return result;
  }
  if (receipt?.command_id !== commandId || receipt.ok !== true || receipt.status !== 'completed'
    || receipt.target_record_id !== projectId || receipt.result?.ok !== true
    || receipt.result?.contract !== contract
    || Object.entries(payload).some(([key, value]) => key === 'execution_page'
      ? JSON.stringify(boundedWorkjetExecutionRequest(receipt.payload?.[key])) !== JSON.stringify(value)
      : receipt.payload?.[key] !== value)
    || binding?.project_id !== projectId || binding?.thread_id !== threadId
    || binding?.thread_key !== threadKey || turn?.thread_id !== threadId
    || turn?.thread_key !== threadKey || typeof turn?.command_id !== 'string'
    || !turn.command_id || turn.command_id.length > 256
    || (!submitting && turn.command_id !== payload.target_command_id)
    || typeof turn.task_id !== 'string' || !turn.task_id || turn.task_id.length > 256
    || typeof turn.execution_phase !== 'string' || !turn.execution_phase
    || typeof turn.status !== 'string' || !turn.status || typeof turn.queue_status !== 'string'
    || !Number.isSafeInteger(turn.attempt) || turn.attempt < 0
    || typeof turn.terminal !== 'boolean' || turn.terminal !== (turn.execution_phase === 'terminal')
    || typeof turn.result_truncated !== 'boolean') {
    throw new Error('Workjet supervisor turn returned an invalid or unmatched confirmation.');
  }
  const result = {
    action, commandId, projectId, contract,
    binding: { contract: 'ctox.workjet.supervisor_binding.v1', projectId, threadId, threadKey },
    turn: {
      commandId: turn.command_id, taskId: turn.task_id, threadId, threadKey,
      executionPhase: turn.execution_phase, status: turn.status, queueStatus: turn.queue_status,
      attempt: turn.attempt, terminal: turn.terminal, result: turn.result,
      resultTruncated: turn.result_truncated, errorCode: turn.error_code, errorMessage: turn.error_message,
    },
  };
  if (observing) {
    const page = receipt.result.execution_page;
    validateSupervisorExecutionValue('ExecutionPage', page);
    if (receipt.result.execution_contract !== SUPERVISOR_EXECUTION_SCHEMA
      || page.command_id !== turn.command_id || page.task_id !== turn.task_id
      || page.events.length > (payload.execution_page.limit ?? 25)
      || (payload.execution_page.attempt_id !== undefined
        && page.attempt?.attempt_id !== payload.execution_page.attempt_id)) {
      throw new Error('Workjet execution page does not match the authorized supervisor turn.');
    }
    let sequence = payload.execution_page.cursor?.after_sequence ?? 0;
    for (const event of page.events) {
      if (event.sequence <= sequence) throw new Error('Die Ereignisse zur Ausführung sind nicht in der erwarteten Reihenfolge eingetroffen.');
      sequence = event.sequence;
    }
    const last = page.events.at(-1);
    const expectedCursor = last
      ? { after_sequence: last.sequence, after_event_id: last.id } : payload.execution_page.cursor;
    const cursorMatches = expectedCursor
      ? page.next_cursor?.after_sequence === expectedCursor.after_sequence
        && page.next_cursor?.after_event_id === expectedCursor.after_event_id
      : page.next_cursor == null;
    if (!cursorMatches || (page.has_more && !last) || (last && !page.attempt)) {
      throw new Error('Workjet execution page has an unmatched continuation cursor.');
    }
    result.executionContract = SUPERVISOR_EXECUTION_SCHEMA;
    result.executionPage = JSON.parse(JSON.stringify(page));
  }
  if (inputting) {
    const input = receipt.result.input;
    if (receipt.result.delivery !== 'next_slice' || receipt.result.worker_interrupted !== false
      || !Number.isSafeInteger(input?.sequence) || input.sequence < 1
      || input.body !== payload.body || typeof input.created_at !== 'string'
      || !Number.isFinite(Date.parse(input.created_at))) {
      throw new Error('Workjet supervisor follow-up has no matching durable input receipt.');
    }
    result.input = {
      inputId: boundedWorkjetProjectText(input.input_id, 'native inputId', 256),
      sequence: input.sequence, body: input.body, createdAt: input.created_at,
    };
    result.delivery = 'next_slice';
    result.workerInterrupted = false;
  }
  if (submitting) result.messageId = boundedWorkjetProjectText(receipt.result.message_id, 'native messageId', 256);
  if (cancelling) {
    const cancellation = receipt.result.cancellation;
    if (turn.status !== 'cancelled' || typeof cancellation?.side_effects_may_have_started !== 'boolean'
      || cancellation.worker_interrupt_acknowledged !== false) {
      throw new Error('Workjet supervisor cancellation has no matching confirmation.');
    }
    result.cancellation = {
      commandId: boundedWorkjetProjectText(cancellation.command_id, 'native cancellation commandId', 256),
      sideEffectsMayHaveStarted: cancellation.side_effects_may_have_started,
      workerInterruptAcknowledged: false,
    };
  }
  return result;
}

// ---- project.supervisor.bind ------------------------------------------------------------------------------------
// ref: app.js:14049-14061 (Zulassung), 14611-14644
async function supervisorBind(env, request) {
  const { state, actorContext } = env;
  const { ownerUserId, requestSession, requestDb } = scope(env);
  await requireWorkjetSupervisorDataPlane(env);
  const action = request.action;
  assertWorkjetProjectPayloadKeys(request, new Set(['action', 'commandId', 'projectId', 'threadId']));
  const commandId = boundedWorkjetProjectText(request.commandId, 'commandId', 128);
  const projectId = boundedWorkjetProjectText(request.projectId, 'projectId', 128);
  const threadId = boundedWorkjetProjectText(request.threadId, 'threadId', 36);
  if (!/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(threadId)
    || threadId === '00000000-0000-0000-0000-000000000000') {
    throw new TypeError('Workjet supervisor threadId must be its existing lowercase CodeThread UUID.');
  }
  const assertCurrentIdentity = () => {
    if (state.session !== requestSession || state.db !== requestDb
      || actorContext(state.session).id !== ownerUserId) {
      throw new Error('Workjet project session changed before the supervisor binding was delivered.');
    }
  };
  assertCurrentIdentity();
  const receipt = await state.commandBus.dispatch({ // v4-allow: debt L-42
    id: commandId, command_id: commandId, module: 'ctox',
    command_type: 'ctox.workjet.project.supervisor.bind', record_id: projectId,
    payload: { project_id: projectId, thread_id: threadId },
    client_context: { source: 'workjet-project-control', actor: actorContext(requestSession) },
  }, { until: 'terminal', sync_queue_tasks: false, timeoutMs: WORKJET_PROJECT_CONTROL_TIMEOUT_MS }); // v4-allow: debt L-42
  assertCurrentIdentity();
  const contract = 'ctox.workjet.supervisor_binding.v1';
  const binding = receipt?.result?.binding;
  const threadKey = `business-os/threads/${threadId}`;
  if (receipt?.command_id !== commandId || receipt.ok !== true || receipt.status !== 'completed'
    || receipt.target_record_id !== projectId || receipt.payload?.project_id !== projectId
    || receipt.payload?.thread_id !== threadId || receipt.result?.ok !== true
    || receipt.result?.contract !== contract || binding?.project_id !== projectId
    || binding?.thread_id !== threadId || binding?.thread_key !== threadKey) {
    throw new Error('Workjet supervisor binding returned an uncorrelated or unsuccessful receipt.');
  }
  return { action, commandId, projectId, binding: { contract, projectId, threadId, threadKey } };
}

// ---- ctox.plan.step.retry ---------------------------------------------------------------------------------------
// ref: app.js:13918-13950 (V2-PR #874, Branch shell/plan-step-retry f0c836569). Steht in V2 vor der Datenebene.
// v4-Abweichung (gehört nach v2-compat): v2-compat liefert für until 'accepted' die Quittung mit ok:false, solange
// der Kern 'accepted'/'running' meldet (ok = status done). V2 (command-bus.js:1653-1664 commandReceipt) setzt dort
// ok:true und wirft bei failed. Ohne Angleichung meldete der V2-Rumpf jeden angenommenen Retry als „rejected“
// (falsches „fehlgeschlagen“). acceptedReceipt gleicht nur diesen Fall an, alles andere bleibt unverändert.
function acceptedReceipt(receipt) {
  if (receipt && receipt.ok === false && (receipt.status === 'accepted' || receipt.status === 'running')
    && receipt.error === undefined) {
    return { ...receipt, ok: true };
  }
  return receipt;
}

async function planStepRetry(env, request) {
  const { state, actorContext } = env;
  const action = request.action;
  const { ownerUserId, requestSession, requestDb } = scope(env);
  // Workjet-Planschritt wiederholen (ctox#872): leitet genau { step_id } als Business-Command weiter.
  // Antwort sobald der Befehl angenommen ist; den Ausgang meldet die Plan-Projektion.
  assertWorkjetProjectPayloadKeys(request, new Set(['action', 'commandId', 'payload']));
  const commandId = boundedWorkjetProjectText(request.commandId, 'commandId', 128);
  const payloadIn = request.payload;
  if (!payloadIn || typeof payloadIn !== 'object' || Array.isArray(payloadIn)) {
    throw new TypeError('Plan step retry requires a payload object.');
  }
  assertWorkjetProjectPayloadKeys(payloadIn, new Set(['step_id', 'thread_id']));
  const stepId = boundedWorkjetProjectText(payloadIn.step_id, 'step_id', 256);
  if (state.session !== requestSession || state.db !== requestDb
    || actorContext(state.session).id !== ownerUserId) {
    throw new Error('Workjet project session changed before the retry was sent.');
  }
  // v4-allow: code (V2-Fehlertext wörtlich aus PR #874; Code-Text, kein Anzeigezustand – Workjet zeigt ihn nicht an)
  if (typeof state.commandBus?.dispatch !== 'function') throw new Error('Command transport is unavailable.'); // v4-allow: debt L-42
  const receipt = acceptedReceipt(await state.commandBus.dispatch({ // v4-allow: debt L-42
    id: commandId,
    command_id: commandId,
    module: 'ctox',
    command_type: 'ctox.plan.step.retry',
    record_id: stepId,
    payload: { step_id: stepId },
    client_context: { source: 'workjet-project-control', actor: actorContext(requestSession) },
  }, { until: 'accepted', timeoutMs: WORKJET_PROJECT_CONTROL_TIMEOUT_MS })); // v4-allow: debt L-42
  if (receipt?.command_id && receipt.command_id !== commandId) {
    throw new Error('Plan step retry returned an uncorrelated receipt.');
  }
  if (receipt?.ok === false || receipt?.status === 'failed' || receipt?.status === 'rejected') {
    throw new Error(String(receipt?.error?.message || receipt?.error || 'Plan step retry was rejected.'));
  }
  return { action, commandId, status: 'pending' };
}

// Freigeschaltete Aktionen dieser Gruppe (Orakel in tests/oracle.test.mjs, Block „Strang B“).
export const SUPERVISOR_ACTIONS = Object.freeze({
  'project.supervisor.bind': supervisorBind,
  'project.supervisor.turn.submit': supervisorTurn,
  'project.supervisor.turn.input': supervisorTurn,
  'project.supervisor.turn.cancel': supervisorTurn,
  'project.supervisor.turn.watch': supervisorTurn,
  'project.supervisor.turn.capabilities': supervisorTurn,
  'project.supervisor.route.read.v1': supervisorRoute,
  'project.supervisor.route.read.v2': supervisorRoute,
  'project.supervisor.route.capabilities.v1': supervisorRoute,
  'project.supervisor.route.capabilities.v2': supervisorRoute,
  'ctox.plan.step.retry': planStepRetry,
});
