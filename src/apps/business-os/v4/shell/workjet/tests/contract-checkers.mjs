// Vertragsabgleich: Antwortformen aus Workjet packages/contracts/src/ctox.ts (Workjet main eb65947d) als JS-Prüfer,
// gleichwertig zu den effect/Schema-Pflichtfeldern. Ein Prüfer je Aktion; freigeschaltete Aktionen ohne Prüfer
// lassen contract.test.mjs scheitern. Prüfer liefern eine Fehlerliste (leer = gültig).
// ref: ctox.ts:629 CtoxProjectText, :1036 WorkingCopyProjection, :1044 ProjectMetadataProjection,
//      :1060 ProjectProjection, :1073 ProjectList, :1246-1265 Antwort project.list / project.worker.add|chat.create.

const CTRL = /[\u0000-\u001f\u007f]/u;
// TrimmedNonEmptyString + isMaxLength + NoAsciiControlCharacters
export const projectText = (max) => (v) => typeof v === 'string' && v.length > 0 && v.trim() === v && v.length <= max && !CTRL.test(v);
// ProjectId/CommandId = makeEntityId → TrimmedNonEmptyString (baseSchemas.ts:51-61), ohne Längengrenze.
const entityId = (v) => typeof v === 'string' && v.length > 0 && v.trim() === v;
// IsoDateTime = Schema.String (baseSchemas.ts:21).
const isoDateTime = (v) => typeof v === 'string';
const int = (v) => Number.isSafeInteger(v);

function struct(fields, value, path = '$', errors = []) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) { errors.push(`${path}: Objekt erwartet`); return errors; }
  for (const [key, [check, optional]] of Object.entries(fields)) {
    if (!Object.hasOwn(value, key)) { if (!optional) errors.push(`${path}.${key}: Pflichtfeld fehlt`); continue; }
    const r = check(value[key], `${path}.${key}`);
    if (r === false) errors.push(`${path}.${key}: ungültig (${JSON.stringify(value[key])?.slice(0, 80)})`);
    else if (Array.isArray(r)) errors.push(...r);
  }
  return errors;
}
const req = (c) => [c, false];
const opt = (c) => [c, true];

const workingCopy = (v, p) => struct({
  id: req(projectText(160)), computerId: req(projectText(256)), path: req(projectText(4096)),
  status: req((s) => s === 'active' || s === 'detached'),
}, v, p);

// CtoxWorkjetProjectConfiguration-Felder sind optional; geprüft werden nur Typen der vorhandenen Felder.
const projectProjection = (v, p) => {
  const errors = struct({
    id: req(entityId), title: req(projectText(256)), createdAt: opt(isoDateTime),
    workingCopies: req((list, lp) => (Array.isArray(list) && list.length <= 500 ? list.flatMap((c, i) => workingCopy(c, `${lp}[${i}]`)) : false)),
    exitModel: opt((e) => e === null || (e && typeof e === 'object')),
    description: opt((d) => d === null || typeof d === 'string'),
    repoUrl: opt((u) => u === null || projectText(2048)(u)),
    publicUrl: opt((u) => u === null || projectText(2048)(u)),
  }, v, p);
  if (v?.exitModel != null && v.exitModel.project_id !== v.id) errors.push(`${p}: Exit assessment belongs to another project.`);
  return errors;
};

export const CHECKERS = Object.freeze({
  'project.list': (r) => {
    const errors = struct({
      action: req((a) => a === 'project.list'),
      projects: req((list, p) => {
        if (!Array.isArray(list) || list.length > 100) return false;
        const errs = list.flatMap((project, i) => projectProjection(project, `${p}[${i}]`));
        if (new Set(list.map((x) => x?.id)).size !== list.length) errs.push(`${p}: Project ids must be unique.`);
        return errs;
      }),
      count: req((c) => int(c) && c >= 0 && c <= 100),
      truncated: req((t) => t === false),
    }, r);
    if (Array.isArray(r?.projects) && r.projects.length !== r.count) errors.push('$: Project list must match the confirmed native count.');
    return errors;
  },
  'project.worker.add': (r) => chatResponse(r, 'project.worker.add'),
  'project.chat.create': (r) => chatResponse(r, 'project.chat.create'),
  // Strang B (Prüfer unten im Block „Strang B“)
  'project.supervisor.bind': (r) => supervisorBindResponse(r),
  'project.supervisor.turn.submit': (r) => supervisorTurnResponse(r, 'project.supervisor.turn.submit'),
  'project.supervisor.turn.watch': (r) => supervisorTurnResponse(r, 'project.supervisor.turn.watch'),
  'project.supervisor.turn.cancel': (r) => supervisorTurnResponse(r, 'project.supervisor.turn.cancel'),
  'project.supervisor.turn.input': (r) => supervisorTurnResponse(r, 'project.supervisor.turn.input'),
  'project.supervisor.turn.capabilities': (r) => supervisorCapabilitiesResponse(r),
  'project.supervisor.route.read.v1': (r) => supervisorRouteResponse(r, 'project.supervisor.route.read.v1'),
  'project.supervisor.route.read.v2': (r) => supervisorRouteResponse(r, 'project.supervisor.route.read.v2'),
  'project.supervisor.route.capabilities.v1': (r) => supervisorRouteResponse(r, 'project.supervisor.route.capabilities.v1'),
  'project.supervisor.route.capabilities.v2': (r) => supervisorRouteResponse(r, 'project.supervisor.route.capabilities.v2'),
  'ctox.plan.step.retry': (r) => planStepRetryResponseB(r),
  // Ende Strang B
  // Strang C (Prüfer unten im Block „Strang C“)
  // Workjet #784 / Kern #875: Erfolg {action, commandId, ...Quittung} oder {action, commandId, error:'<code>'}
  'project.calendar.account.connect.start': (r) => calendarConnectResponse(r, 'project.calendar.account.connect.start'),
  'project.calendar.account.connect.complete': (r) => calendarConnectResponse(r, 'project.calendar.account.connect.complete'),
  'project.jour_fixe.meeting.read': (r) => meetingReadResponseC(r),
  ...Object.fromEntries(['meeting.start', 'meeting.end', 'transcript.append', 'todos.revise', 'todos.confirm', 'comment.add']
    .map((s) => [`project.jour_fixe.${s}`, (r) => meetingOwnerResponseC(r, `project.jour_fixe.${s}`)])),
  'project.jour_fixe.speech': (r) => jourFixeSpeechResponseC(r),
  'project.calendar.accounts.read': (r) => calendarResponseC(r, 'project.calendar.accounts.read'),
  'project.calendar.events.read': (r) => calendarResponseC(r, 'project.calendar.events.read'),
  'speech.dictation': (r) => dictationResponseC(r),
  ...Object.fromEntries(['read', 'configure', 'key', 'voices', 'check', 'check.transcription']
    .map((s) => [`speech.settings.${s}`, (r) => speechSettingsResponseC(r, `speech.settings.${s}`)])),
  'speech.settings.playback': (r) => struct({
    action: req((a) => a === 'speech.settings.playback'), commandId: req(entityId), rate: req(speechRateC),
  }, r),
  // Ende Strang C
  // Strang A (Prüfer unten im Block „Strang A“)
  'project.create': (r) => struct({ action: req((a) => a === 'project.create'), project: req(projectProjection) }, r),
  'project.configure': (r) => struct({
    action: req((a) => a === 'project.configure'), commandId: req(entityId), project: req(projectMetadataA),
  }, r),
  'project.kpis.read': (r) => kpisResponseA(r, 'project.kpis.read'),
  'project.kpis.configure': (r) => kpisResponseA(r, 'project.kpis.configure'),
  'project.kpis.refresh': (r) => kpisResponseA(r, 'project.kpis.refresh'),
  'project.exit_model.read': (r) => exitResponseA(r, 'project.exit_model.read'),
  'project.exit_model.refresh': (r) => exitResponseA(r, 'project.exit_model.refresh'),
  'project.exit_model.submit': (r) => exitResponseA(r, 'project.exit_model.submit'),
  // ref: ctox.ts:1085-1092 (CtoxWorkjetProjectInitStep), :1182-1188 (Antwort project.init)
  'project.init': (r) => struct({
    action: req((a) => a === 'project.init'), commandId: req(entityId), projectId: req(entityId),
    operationId: req(projectText(128)),
    steps: req((list, p) => (Array.isArray(list) && list.length <= 12 ? list.flatMap((step, i) => struct({
      id: req(projectText(64)), state: req(projectText(32)), detail: req((d) => typeof d === 'string' && d.length <= 256),
    }, step, `${p}[${i}]`)) : false)),
  }, r),
  // Ende Strang A
  // Strang D (Prüfer unten im Block „Strang D“)
  ...Object.fromEntries(['instance.grok.read', 'instance.grok.start', 'instance.grok.poll', 'instance.grok.cancel',
    'instance.grok.check', 'instance.grok.remove'].map((a) => [a, (r) => grokResponseD(r, a)])),
  ...Object.fromEntries(['instance.providers.read', 'instance.providers.adopt', 'instance.providers.observe',
    'instance.providers.models.select', 'instance.providers.models.exclude', 'instance.providers.account.enable',
    'instance.providers.account.remove'].map((a) => [a, (r) => providerResponseD(r, a)])),
  ...Object.fromEntries(['computer.list', 'computer.assign', 'computer.unassign', 'computer.endpoint.upsert',
    'computer.endpoint.disable', 'computer.ssh_key.ensure'].map((a) => [a, (r) => computerResponseD(r, a)])),
  ...Object.fromEntries(['session.list', 'session.create', 'session.transfer.start', 'session.transfer.status',
    'session.transfer.abort', 'session.transfer.pause_ack'].map((a) => [a, (r) => sessionResponseD(r, a)])),
  ...Object.fromEntries(['binding.list', 'binding.revoke', 'invite.create', 'invite.revoke']
    .map((a) => [a, (r) => deviceResponseD(r, a)])),
  // Ende Strang D
  // Strang R (Prüfer unten im Block „Strang R“)
  'project.presentation.read': (r) => presentationReadResponseR(r),
  'project.presentation.content.read': (r) => presentationContentResponseR(r),
  'project.presentation.canvas.save': (r) => presentationCanvasSaveResponseR(r),
  'project.jour_fixe.narration.read': (r) => narrationReadResponseR(r),
  // Ende Strang R
});

function chatResponse(r, action) {
  return struct({
    action: req((a) => a === action),
    commandId: req(entityId),
    projectId: req(entityId),
    workerProfileId: req(projectText(256)),
    chatId: req((c) => projectText(256)(c) && /^workjet_private_.+/.test(c)),
  }, r);
}

export function checkResponse(action, response) {
  const checker = CHECKERS[action];
  if (!checker) return [`kein Vertragsprüfer für ${action}`];
  return checker(JSON.parse(JSON.stringify(response)));
}

// Strang B – Supervisor-Antworten. ref: Workjet eb65947d packages/contracts/src/workjetSupervisor.ts:4-150
// (ThreadId, Goal, Binding, Turn, TurnCapabilitiesResponse, InputReceipt), workjetSupervisorRoute.ts:16-50 (Route-
// Antworten), ctox.ts:1113-1185 (bind/submit/watch/cancel), :873-877 (WorkjetPlanStepRetryResponse). Die Route-DTOs
// und die Ausführungsseite prüft der generierte Vertrag aus derselben Fixture-Quelle (src/core/rxdb/tests/fixtures),
// den auch Workjet generiert (workjetSupervisorRoute.generated.ts, workjetSupervisorExecution.ts).
import { validateSupervisorExecutionValue as validateExecutionB } from '../../../../shared/workjet-supervisor-execution-contract.generated.mjs';
import { validateSupervisorRouteDisplayValue as validateRouteV1B } from '../../../../shared/workjet-supervisor-route-display-contract.generated.mjs';
import { validateSupervisorRouteComputationValue as validateRouteV2B } from '../../../../shared/workjet-supervisor-route-computation-contract.generated.mjs';

// Workjet dekodiert mit onExcessProperty: 'error' (CtoxGuestManager.ts:2474): Zusatzschlüssel → guest_failed.
const noExtraB = (value, fields, path, errors) => {
  if (value && typeof value === 'object' && !Array.isArray(value)) {
    for (const key of Object.keys(value)) if (!Object.hasOwn(fields, key)) errors.push(`${path}.${key}: Zusatzschlüssel (Workjet lehnt ab)`);
  }
  return errors;
};
const THREAD_UUID_B = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
const threadIdB = (v) => entityId(v) && THREAD_UUID_B.test(v) && v !== '00000000-0000-0000-0000-000000000000';
const identityB = projectText(256);
const goalB = (v) => entityId(v) && v.length <= 4096 && !v.includes('\0') && new TextEncoder().encode(v).byteLength <= 4096;
const BINDING_FIELDS_B = {
  contract: req((c) => c === 'ctox.workjet.supervisor_binding.v1'), projectId: req(entityId),
  threadId: req(threadIdB), threadKey: req(identityB),
};
const supervisorBindingB = (v, p) => {
  const errors = noExtraB(v, BINDING_FIELDS_B, p, struct(BINDING_FIELDS_B, v, p));
  if (v?.threadKey !== `business-os/threads/${v?.threadId}`) errors.push(`${p}: Supervisor thread key does not match its UUID.`);
  return errors;
};
const TURN_FIELDS_B = {
  commandId: req(identityB), taskId: req((t) => t === null || identityB(t)), threadId: req(threadIdB),
  threadKey: req(identityB), executionPhase: req(identityB), status: req(identityB),
  queueStatus: req((q) => q === null || identityB(q)), attempt: req((a) => int(a) && a >= 0),
  terminal: req((t) => typeof t === 'boolean'), result: opt(() => true), resultTruncated: req((t) => typeof t === 'boolean'),
  errorCode: req((e) => e === null || identityB(e)), errorMessage: req((e) => e === null || typeof e === 'string'),
};
const supervisorTurnB = (v, p) => {
  const errors = noExtraB(v, TURN_FIELDS_B, p, struct(TURN_FIELDS_B, v, p));
  // Schema.Unknown: Schlüssel muss nicht vorhanden sein (undefined wird beim JSON-Transport weggelassen).
  if (v?.threadKey !== `business-os/threads/${v?.threadId}` || v?.terminal !== (v?.executionPhase === 'terminal')) {
    errors.push(`${p}: Native supervisor turn has inconsistent thread or terminal facts.`);
  }
  return errors;
};
function planStepRetryResponseB(r) {
  const fields = { action: req((a) => a === 'ctox.plan.step.retry'), commandId: req(entityId), status: req((x) => x === 'pending') };
  return noExtraB(r, fields, '$', struct(fields, r));
}
function supervisorBindResponse(r) {
  const fields = {
    action: req((a) => a === 'project.supervisor.bind'), commandId: req(entityId), projectId: req(entityId),
    binding: req(supervisorBindingB),
  };
  const errors = noExtraB(r, fields, '$', struct(fields, r));
  if (r?.binding?.projectId !== r?.projectId) errors.push('$: Supervisor binding belongs to another project.');
  return errors;
}
function supervisorTurnResponse(r, action) {
  const inputting = action === 'project.supervisor.turn.input';
  const fields = {
    action: req((a) => a === action), commandId: req(entityId), projectId: req(entityId),
    contract: req((c) => c === (inputting ? 'ctox.workjet.supervisor_input.v1' : 'ctox.workjet.supervisor_turn.v1')),
    binding: req(supervisorBindingB), turn: req(supervisorTurnB),
  };
  if (action === 'project.supervisor.turn.submit') fields.messageId = req(identityB);
  if (action === 'project.supervisor.turn.watch') {
    fields.executionContract = opt((c) => c === 'ctox.workjet.supervisor_execution.v1');
    fields.executionPage = opt((page) => validateExecutionB('ExecutionPage', page).ok === true);
  }
  if (action === 'project.supervisor.turn.cancel') {
    fields.cancellation = req((c, p) => struct({
      commandId: req(identityB), sideEffectsMayHaveStarted: req((b) => typeof b === 'boolean'),
      workerInterruptAcknowledged: req((b) => b === false),
    }, c, p));
  }
  if (inputting) {
    fields.input = req((i, p) => struct({
      inputId: req(identityB), sequence: req((n) => int(n) && n >= 1), body: req(goalB), createdAt: req(isoDateTime),
    }, i, p));
    fields.delivery = req((d) => d === 'next_slice');
    fields.workerInterrupted = req((w) => w === false);
  }
  const errors = noExtraB(r, fields, '$', struct(fields, r));
  if (r?.projectId !== r?.binding?.projectId || r?.binding?.threadId !== r?.turn?.threadId) {
    errors.push('$: Supervisor turn belongs to another binding.');
  }
  if (action === 'project.supervisor.turn.watch') {
    if ((r?.executionContract === undefined) !== (r?.executionPage === undefined)) errors.push('$: executionContract/executionPage nur gemeinsam.');
    if (r?.executionPage && (r.executionPage.command_id !== r.turn?.commandId || r.executionPage.task_id !== r.turn?.taskId)) {
      errors.push('$: Supervisor turn belongs to another binding.');
    }
  }
  if (inputting && r?.turn?.taskId === null) errors.push('$: Supervisor input belongs to another native binding or has no task.');
  return errors;
}
function supervisorCapabilitiesResponse(r) {
  const fields = {
    action: req((a) => a === 'project.supervisor.turn.capabilities'), commandId: req(entityId), projectId: req(entityId),
    contract: req((c) => c === 'ctox.workjet.supervisor_turn_capabilities.v1'), binding: req(supervisorBindingB),
    turnKinds: req((k) => Array.isArray(k) && k.length === 2 && k[0] === 'work' && k[1] === 'conversation'),
    defaultTurnKind: req((d) => d === 'work'),
    inputContract: opt((c) => c === 'ctox.workjet.supervisor_input.v1'), inputDelivery: opt((d) => d === 'next_slice'),
    maxInputChars: opt((m) => m === 4096),
  };
  const errors = noExtraB(r, fields, '$', struct(fields, r));
  const present = ['inputContract', 'inputDelivery', 'maxInputChars'].filter((k) => r?.[k] !== undefined).length;
  if (present !== 0 && present !== 3) errors.push('$: Supervisor input support needs the complete native capability.');
  if (r?.projectId !== r?.binding?.projectId) errors.push('$: Supervisor capability belongs to another project.');
  return errors;
}
function supervisorRouteResponse(r, action) {
  const v2 = action.endsWith('.v2');
  const capabilities = action.includes('.capabilities.');
  const contract = `ctox.workjet.supervisor.${capabilities ? 'route-capabilities' : 'route-display'}.${v2 ? 'v2' : 'v1'}`;
  const field = capabilities ? 'capabilities' : 'route';
  const type = capabilities ? 'SupervisorRouteCapabilities' : 'SupervisorRouteDisplay';
  const validate = v2 ? validateRouteV2B : validateRouteV1B;
  const fields = {
    action: req((a) => a === action), commandId: req(entityId), projectId: req(entityId), threadId: req(threadIdB),
    contract: req((c) => c === contract), [field]: req((dto) => validate(type, dto).ok === true),
  };
  const errors = noExtraB(r, fields, '$', struct(fields, r));
  if (!capabilities && !v2 && r?.route?.actual !== null) errors.push('$: This route contract has no verified producer.');
  const dto = r?.[field];
  if (dto && (dto.project_id !== r.projectId || dto.supervisor_thread_id !== r.threadId)) errors.push('$: Route-DTO gehört zu einem anderen Projekt/Thread.');
  return errors;
}
// Ende Strang B

// Strang C – Jour fixe, Kalender, Sprache. ref: Workjet eb65947d packages/contracts/src/
//   workjetJourFixeMeeting.ts:143-155 (ReadResponse), workjetJourFixeOwner.ts:92-129 (OwnerResponse),
//   workjetJourFixeSpeech.ts:39-75 (SpeechResponse), workjetCalendarNative.ts:26-58 (Responses),
//   workjetDictation.ts:28-37 (DictationResponse), workjetSpeechSettings.ts:4-13, 33-97 (Config, Settings/Playback).
// Verschachtelte Typen (Meeting, GoalRef, Kalenderseiten) prüft der generierte V2-Vertrag aus demselben Kern-Fixture.
import { validateJourFixeValue as validateJourFixeValueC } from '../../../../shared/workjet-jour-fixe-contract.generated.mjs';
import { validateCalendarValue as validateCalendarValueC } from '../../../../shared/workjet-calendar-contract.generated.mjs';
const UUID_C = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const uuidC = (v) => typeof v === 'string' && UUID_C.test(v);
const unsignedC = (v) => int(v) && v >= 0;
const positiveC = (v) => int(v) && v >= 1;
const maxStr = (max, min = 0) => (v) => typeof v === 'string' && v.length >= min && v.length <= max;
const generatedC = (validate, type) => (v) => validate(type, v).ok === true;
const speechRateC = (v) => typeof v === 'number' && v >= 0.8 && v <= 1.5;
const listC = (max, item) => (list, p) => (Array.isArray(list) && list.length <= max ? list.flatMap((x, i) => item(x, `${p}[${i}]`)) : false);
function meetingReadResponseC(r) {
  const errors = struct({
    action: req((a) => a === 'project.jour_fixe.meeting.read'), commandId: req(entityId), projectId: req(entityId),
    contract: req((c) => c === 'ctox.workjet.jour_fixe.v1'),
    meeting: req((m) => m === null || generatedC(validateJourFixeValueC, 'Meeting')(m)),
    preparationTaskId: opt(maxStr(256, 1)),
  }, r);
  if (r?.meeting && r.meeting.project_id !== r.projectId) errors.push('$: Meeting gehört zu einem anderen Projekt.');
  return errors;
}
const MEETING_STATES_C = new Set(['planned', 'preparing', 'ready', 'live', 'review', 'confirmed', 'cancelled', 'failed']);
function meetingOwnerResponseC(r, action) {
  const errors = struct({
    action: req((a) => a === action), commandId: req(entityId), projectId: req(entityId),
    contract: req((c) => c === 'ctox.workjet.jour_fixe.v1'),
    goal: opt(generatedC(validateJourFixeValueC, 'GoalRef')),
    mutation: req((m, p) => struct({
      operation_id: req(entityId), meeting_id: req(entityId), project_id: req(entityId), revision: req(unsignedC),
      state: req((s) => MEETING_STATES_C.has(s)),
      changed_id: opt((v) => v === null || entityId(v)), todos_revision: opt((v) => v === null || unsignedC(v)),
    }, m, p)),
  }, r);
  if (r?.mutation && r.projectId !== r.mutation.project_id) errors.push('$: Meeting receipt belongs to another project.');
  return errors;
}
const JF_ERRORS_C = new Set(['canceled', 'retired', 'timeout', 'invalid_sequence_or_audio', 'backpressure', 'invalid_finish',
  'invalid_response', 'commit_failed', 'gateway_failed', 'missing_final']);
function jourFixeSpeechResponseC(r) {
  const errors = struct({
    action: req((a) => a === 'project.jour_fixe.speech'), projectId: req(entityId), meetingId: req(maxStr(256, 1)),
    deckRevision: req(positiveC), op: req((o) => ['open', 'write', 'read', 'finish', 'cancel'].includes(o)),
    streamId: req(uuidC), state: req((s) => ['open', 'finishing', 'committed', 'canceled', 'failed'].includes(s)),
    requestId: opt(uuidC),
    events: req(listC(32, (e, p) => struct({ sequence: req(positiveC), producerSequence: req(positiveC), text: req(maxStr(8192)) }, e, p))),
    receipt: req((v, p) => (v === null ? [] : struct({ handle: req(uuidC), meetingRevision: req(positiveC) }, v, p))),
    error: req((e) => e === null || JF_ERRORS_C.has(e)),
  }, r);
  if (r?.state === 'committed' && (r.receipt === null || r.error !== null)) errors.push('$: Missing native speech terminal receipt.');
  if (r?.state === 'failed' && r.error === null) errors.push('$: Missing native speech terminal receipt.');
  return errors;
}
function calendarResponseC(r, action) {
  const events = action === 'project.calendar.events.read';
  const errors = struct({
    action: req((a) => a === action), commandId: req(entityId),
    ...(events ? { accountId: req(projectText(256)), startMs: req(int), endMs: req(int) } : {}),
    calendar: req(generatedC(validateCalendarValueC, events ? 'CalendarEventsPage' : 'CalendarAccountsPage')),
  }, r);
  if (!events && Array.isArray(r?.calendar?.accounts)) {
    const a = r.calendar.accounts;
    if (new Set(a.map((x) => x.id)).size !== a.length || new Set(a.map((x) => x.calendar_id)).size !== a.length) errors.push('$: Duplicate calendar accounts');
  }
  if (events) {
    if (!(r.endMs > r.startMs && r.endMs - r.startMs <= 400 * 86_400_000)) errors.push('$: Calendar range must be ordered and at most 400 days');
    if (Array.isArray(r?.calendar?.events) && (r.calendar.events.some((e) => e.kind !== 'synced' || e.account_id !== r.accountId)
      || new Set(r.calendar.events.map((e) => e.id)).size !== r.calendar.events.length)) errors.push('$: Calendar events belong to another account');
  }
  return errors;
}
function dictationResponseC(r) {
  return struct({
    action: req((a) => a === 'speech.dictation'), commandId: req(entityId), streamId: req(maxStr(256, 1)),
    op: req((o) => ['open', 'write', 'read', 'finish', 'cancel'].includes(o)),
    state: req((s) => ['open', 'finishing', 'finished', 'canceled', 'failed'].includes(s)),
    events: req(listC(32, (e, p) => struct({ sequence: req(unsignedC), text: req(maxStr(8192)) }, e, p))),
    text: req((t) => t === null || maxStr(32768)(t)),
    error: req((e) => e === null || maxStr(256)(e)),
  }, r);
}
const speechCheckC = (v, p, extra = {}) => (v === null ? [] : struct({
  state: req((s) => s === 'ok' || s === 'error'), checkedAt: req(maxStr(64)),
  latencyMs: req((x) => x === null || (typeof x === 'number' && x >= 0)), errorClass: req((x) => x === null || maxStr(64)(x)),
  ...extra,
}, v, p));
function speechSettingsResponseC(r, action) {
  const backend = (b) => ['runtime', 'mistral', 'computer'].includes(b);
  return struct({
    action: req((a) => a === action), commandId: req(entityId),
    status: req((s, p) => struct({
      config: req((c, cp) => struct({ synthesis: req(backend), transcription: req(backend),
        voice_id: req((v) => v === null || maxStr(256, 1)(v)), rate: req(speechRateC) }, c, cp)),
      mistral_credential_present: req((b) => typeof b === 'boolean'), mistral_voice_configured: req((b) => typeof b === 'boolean'),
      streaming_stt_selected: req((b) => typeof b === 'boolean'),
      stt: req((x) => ['available', 'unavailable', 'unknown'].includes(x)), tts: req((x) => ['available', 'unavailable', 'unknown'].includes(x)),
    }, s, p)),
    ttsCheck: req((v, p) => speechCheckC(v, p)),
    sttCheck: opt((v, p) => speechCheckC(v, p, {
      model: opt(maxStr(256, 1)), audioDurationMs: opt((x) => typeof x === 'number' && x >= 1 && x <= 10_000),
      partialBeforeAudioEnd: opt((x) => typeof x === 'boolean'), measurementBoundary: opt((x) => x === 'gateway_audio_end_to_final'),
    })),
    transcript: opt(maxStr(8192, 1)),
    voices: opt(listC(100, (v, p) => struct({ id: req(maxStr(256, 1)), name: req(maxStr(256)) }, v, p))),
    audioBase64: opt((x) => typeof x === 'string' && x.length <= 12 * 1024 * 1024 && /^[A-Za-z0-9+/]*={0,2}$/.test(x)),
  }, r);
}
// Ende Strang C

// Strang A – Projekt-Metadaten, KPI, Exit. ref: Workjet eb65947d packages/contracts/src/ctox.ts:715-723
//   (CtoxWorkjetProjectConfiguration), :800-812 (ProjectKpis), :1044-1057 (ProjectMetadataProjection),
//   :1204-1240 (Antwort kpis.read|configure|refresh), :1266-1274 (Antwort create/configure);
//   workjetExitModel.ts:31-38 (Status), :141-214 (Assessment, Pflichtfelder oberste Ebene), :243-258 (ExitModelResponse).
// KPI-Einträge prüft der generierte V2-Vertrag aus demselben Kern-Fixture (validateProjectKpiValue('ProjectKpis')).
// Exit: geprüft werden der Schalenteil der Antwort und die Pflichtfelder der obersten Ebene des Assessments; die
// inneren Strukturen (result, scenarios, sources …) reicht die Shell unverändert aus dem Kern durch.
import { validateProjectKpiValue as validateProjectKpiValueA } from '../../../../shared/workjet-project-kpis-contract.generated.mjs';
const urlA = (u) => u === null || projectText(2048)(u);
const objOrNullA = (v) => v === null || (v && typeof v === 'object' && !Array.isArray(v));
function projectMetadataA(v, p) {
  const errors = struct({
    id: req(entityId), title: req(projectText(256)), createdAt: opt(isoDateTime),
    exitModel: opt(objOrNullA),
    description: opt((d) => d === null || (typeof d === 'string' && d.length <= 4096)),
    repoUrl: opt(urlA), publicUrl: opt(urlA),
    supervisorLumaId: opt((x) => x === null || projectText(160)(x)),
    info: opt(objOrNullA), jourFixe: opt(objOrNullA), feedSettings: opt(objOrNullA),
  }, v, p);
  if (v?.exitModel != null && v.exitModel.project_id !== v.id) errors.push(`${p}: Exit assessment belongs to another project.`);
  return errors;
}
const KPI_REFRESH_STATUS_A = new Set(['fresh', 'stale', 'blocked', 'missing']);
function kpisResponseA(r, action) {
  const fields = {
    action: req((a) => a === action), commandId: req(entityId), projectId: req(entityId),
    contract: req((c) => c === 'ctox.workjet.project_kpis.v1'),
    kpis: req((k) => validateProjectKpiValueA('ProjectKpis', k).ok === true && Array.isArray(k.items) && k.items.length <= 3),
  };
  if (action === 'project.kpis.refresh') {
    fields.refresh = req((x, p) => struct({
      completed: req((c) => typeof c === 'boolean'),
      reasonCode: opt(projectText(128)),
      items: req((list, lp) => (Array.isArray(list) && list.length <= 3 ? list.flatMap((item, i) => struct({
        kpiId: req(projectText(128)), status: req((st) => KPI_REFRESH_STATUS_A.has(st)), reasonCode: opt(projectText(128)),
      }, item, `${lp}[${i}]`)) : false)),
    }, x, p));
  }
  const errors = struct(fields, r);
  if (r?.kpis && r.kpis.project_id !== r.projectId) errors.push('$: KPI result belongs to another project.');
  return errors;
}
const EXIT_STATUS_A = new Set(['not_started', 'researching', 'blocked', 'provisional', 'ready', 'failed']);
const isoDateOrNullA = (d) => d === null || (typeof d === 'string' && /^\d{4}-\d{2}-\d{2}$/.test(d));
const arrayMaxA = (max) => (a) => Array.isArray(a) && a.length <= max;
function exitResponseA(r, action) {
  const errors = struct({
    action: req((a) => a === action), commandId: req(entityId), projectId: req(entityId),
    assessment: req((a, p) => struct({
      contract: req((c) => c === 'ctox.workjet.exit_model.v1'), project_id: req(entityId),
      run_id: req((x) => x === null || projectText(160)(x)),
      as_of: req(isoDateOrNullA), exit_date: req(isoDateOrNullA), refresh_due: req(isoDateOrNullA),
      status: req((st) => EXIT_STATUS_A.has(st)),
      missing_inputs: req(arrayMaxA(200)), findings: req(arrayMaxA(200)), sources: req(arrayMaxA(400)),
      plan_summary: req(objOrNullA), result: req(objOrNullA), scenarios: req(arrayMaxA(100)), history: req(arrayMaxA(120)),
    }, a, p)),
  }, r);
  if (r?.assessment && r.assessment.project_id !== r.projectId) errors.push('$: Exit assessment belongs to another project.');
  return errors;
}
// Ende Strang A

// Strang D – Grok, Provider, Rechner, Sitzung, Gerät. ref: Workjet eb65947d packages/contracts/src/
//   workjetInstanceGrok.ts:30-75 (WorkjetInstanceGrokResponse), workjetNativeProviders.ts:212-273 (Registry/Response),
//   ctox.ts:1586-1633 (Rechner), ctox.ts:1665-1828 (Sitzung, Ereignis), ctox.ts:564-606 (Gerät).
// Nur Funktionsdeklarationen (CHECKERS oben verweist vor diesem Block auf sie).
function uuidD(v) { return typeof v === 'string' && /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(v); }
function textD(max, min = 1) { return (v) => typeof v === 'string' && v.length >= min && v.length <= max; }
function trimmedD(max) { return (v) => entityId(v) && v.length <= max; }
function counterD(v) { return int(v) && v >= 0 && v <= Number.MAX_SAFE_INTEGER - 1; }
function revisionD(v) { return counterD(v) && v >= 1; }
function nonNegD(v) { return int(v) && v >= 0; }
function listD(max, item) { return (list, p) => (Array.isArray(list) && list.length <= max ? list.flatMap((x, i) => { const r = item(x, `${p}[${i}]`); return r === false ? [`${p}[${i}]: ungültig`] : (r === true ? [] : r); }) : false); }
function nullOrD(check) { return (v, p) => (v === null ? true : check(v, p)); }
function oneOfD(values) { const set = new Set(values); return (v) => set.has(v); }

const GROK_ACTIONS_D = ['instance.grok.read', 'instance.grok.start', 'instance.grok.poll', 'instance.grok.cancel', 'instance.grok.check', 'instance.grok.remove'];
function grokResponseD(r, action) {
  const upstream = (v, p) => (v === null ? [] : struct({ status: req((s) => int(s) && s >= 300 && s <= 599), stage: req(oneOfD(['catalog', 'inference'])) }, v, p));
  return struct({
    version: req((v) => v === 1), operationId: req(uuidD),
    action: req((a) => a === action && GROK_ACTIONS_D.includes(a)),
    installed: req((v) => typeof v === 'boolean'), accountLabel: req(textD(256)),
    login: req((v, p) => (v === null ? [] : struct({
      loginId: req(uuidD), phase: req(oneOfD(['pending', 'accepted', 'cancelled', 'failed', 'expired'])),
      verificationUri: req(textD(2048)), userCode: req(textD(256)), expiresAt: req(nonNegD),
    }, v, p))),
    models: req(listD(200, textD(256))),
    check: req((v, p) => (v === null ? [] : struct({
      modelId: req(textD(256)), status: req(oneOfD(['ok', 'error'])), checkedAt: req(nonNegD),
      latencyMs: req((x) => x === null || (typeof x === 'number' && x >= 0)),
      errorCode: req((x) => x === null || ['timeout', 'missing_credential', 'model_unavailable', 'invalid_response', 'request_failed'].includes(x)),
      upstream: opt(upstream),
    }, v, p))),
    catalogError: opt((v, p) => (v === null ? [] : struct({ errorCode: req(oneOfD(['timeout', 'request_failed'])), upstream: opt(upstream) }, v, p))),
  }, r);
}

function providerAccountD(v, p) {
  const id = trimmedD(256);
  const models = (max) => listD(max, id);
  const attempt = (a, ap) => (a === null ? [] : struct({
    checkedAtMs: req(revisionD), httpStatus: req((x) => x === null || (int(x) && x >= 100 && x <= 599)),
    elapsedMs: req(counterD), retryAfterSeconds: req((x) => x === null || counterD(x)),
    failure: req((x) => x === null || (typeof x === 'string' && /^[a-z][a-z0-9_]{0,63}$/.test(x))),
    success: req((x) => typeof x === 'boolean'),
  }, a, ap));
  const errors = struct({
    id: req(id), holder: req((h, hp) => struct({ kind: req((k) => k === 'ctox_instance'), id: req(id) }, h, hp)),
    provider: req(id), enabled: req((x) => typeof x === 'boolean'),
    controls: opt((c, cp) => struct({ canEnable: req((x) => typeof x === 'boolean'), canRemove: req((x) => typeof x === 'boolean') }, c, cp)),
    reportedQuota: opt((q) => !!q && typeof q === 'object'),
    credentialReady: req((x) => typeof x === 'boolean'), revision: req(revisionD), observedAtMs: req(revisionD),
    nativeAccountReference: req((n, np) => struct({ accountId: req(id), holderInstanceId: req(id), accountRevision: req((x) => int(x) && x >= 1) }, n, np)),
    modelCatalogObserved: req((x) => typeof x === 'boolean'),
    modelCatalog: req((c, cp) => struct({
      observed: req((x) => typeof x === 'boolean'), fresh: req((x) => typeof x === 'boolean'), models: req(models(1024)),
      lastSuccessAtMs: req((x) => x === null || revisionD(x)), lastAttempt: req(attempt),
    }, c, cp)),
    excludedModels: req(models(256)), effectiveModels: req(models(1024)), inferenceVerified: req((x) => x === false),
    modelChecks: opt((x) => Array.isArray(x) && x.length <= 256),
  }, v, p);
  if (!errors.length) {
    const ref = v.nativeAccountReference;
    if (v.id !== ref.accountId || v.holder.id !== ref.holderInstanceId || v.revision !== ref.accountRevision
      || v.modelCatalogObserved !== v.modelCatalog.observed
      || (v.modelCatalog.fresh && !(v.modelCatalog.observed && v.modelCatalog.lastSuccessAtMs !== null && v.modelCatalog.lastAttempt?.success === true))) {
      errors.push(`${p}: Native account reference or catalog metadata is inconsistent.`);
    }
  }
  return errors;
}
function providerResponseD(r, action) {
  return struct({
    version: req((v) => v === 1), operationId: req(uuidD), action: req((a) => a === action),
    registry: req((g, gp) => struct({
      ok: req((x) => x === true), schema: req((x) => x === 'ctox.provider-federation-registry.v1'), revision: req(counterD),
      accounts: req(listD(256, providerAccountD)),
      providers: req(listD(256, (x, xp) => struct({ provider: req(trimmedD(256)), selection: req((s, sp) => (s === null ? [] : listD(256, trimmedD(256))(s, sp))) }, x, xp))),
    }, g, gp)),
  }, r);
}

function computerProjectionD(v, p) {
  const text = projectText;
  return struct({
    id: req(text(160)), displayName: req(text(256)), hostingMode: req(oneOfD(['workstation', 'self_hosted'])),
    status: req(oneOfD(['assigned', 'unassigned'])), capabilities: req(listD(32, text(80))),
    capabilityConfig: opt((c) => Array.isArray(c) && c.length <= 3 && new Set(c.map((x) => x?.kind)).size === c.length
      && c.every((x) => ['build', 'storage', 'gpu'].includes(x?.kind))),
    roles: opt((x) => Array.isArray(x)), active: opt((x) => typeof x === 'boolean'), detectedFacts: opt((x) => !!x && typeof x === 'object'),
    hostKeyFingerprint: opt((x) => x === null || text(256)(x)), agentless: opt((x) => typeof x === 'boolean'),
    selfHostedColocation: req((x) => typeof x === 'boolean'),
  }, v, p);
}
function computerResponseD(r, action) {
  const text = projectText;
  const secretRef = (v, p) => struct({ scope: req(text(128)), name: req(text(128)) }, v, p);
  if (action === 'computer.list') {
    const errors = struct({ action: req((a) => a === action), computers: req(listD(100, computerProjectionD)) }, r);
    if (Array.isArray(r?.computers) && (r.computers.some((c) => c?.status !== 'assigned') || new Set(r.computers.map((c) => c?.id)).size !== r.computers.length)) {
      errors.push('$.computers: nur zugewiesene, eindeutige Rechner');
    }
    return errors;
  }
  if (action === 'computer.assign' || action === 'computer.unassign') {
    return struct({ action: req((a) => a === action), computer: req(computerProjectionD) }, r);
  }
  if (action === 'computer.ssh_key.ensure') {
    return struct({
      action: req((a) => a === action), contract: req((c) => c === 'ctox.workjet.computer-ssh-key.v1'), computerId: req(text(160)),
      privateKey: req(secretRef), publicKey: req(text(1024)), publicKeySha256: req(text(80)),
    }, r);
  }
  return struct({ action: req((a) => a === action), endpointRef: req(text(128)), computerId: req(text(160)), enabled: req((x) => typeof x === 'boolean') }, r);
}

const RUN_STATUS_D = ['running', 'pausing', 'paused', 'transferring', 'resuming', 'transfer_failed'];
function sessionProjectionD(v, p) {
  const id = projectText(160);
  return struct({
    id: req(id), projectId: req(id), workingCopyId: req(id), computerId: req(projectText(256)),
    threadId: req((x) => x === null || id(x)), codingSessionId: req((x) => x === null || id(x)),
    runStatus: req(oneOfD(RUN_STATUS_D)), fenceEpoch: req(nonNegD), activeTransferId: req((x) => x === null || id(x)),
    updatedAtMs: req(nonNegD), createdAtMs: opt(nonNegD),
  }, v, p);
}
function transferOutcomeD(v, p) {
  const outcomeText = (x) => typeof x === 'string' && x.length <= 512 && !CTRL.test(x);
  return struct({
    ok: req((x) => typeof x === 'boolean'), transferId: opt(projectText(160)), state: opt(outcomeText),
    errorCode: opt(outcomeText), retryable: opt((x) => typeof x === 'boolean'), message: opt(outcomeText),
    session: opt(sessionProjectionD),
    transfer: opt((t) => !!t && typeof t === 'object' && !Array.isArray(t) && new TextEncoder().encode(JSON.stringify(t)).byteLength <= 64 * 1024),
  }, v, p);
}
function sessionResponseD(r, action) {
  if (action === 'session.list') return struct({ action: req((a) => a === action), sessions: req(listD(10_000, sessionProjectionD)) }, r);
  if (action === 'session.create') return struct({ action: req((a) => a === action), session: req(sessionProjectionD) }, r);
  return struct({ action: req((a) => a === action), outcome: req(transferOutcomeD) }, r);
}
// Ereignis an workjetHostBridge.postSessionTransferEvent (ctox.ts:1665-1689).
export function sessionTransferEventD(v) {
  const t = projectText(256);
  return struct({
    type: req((x) => x === 'workjet.session.transfer'), transferId: req(t), sessionId: req(t),
    state: req(oneOfD(['pause_requested', 'packing', 'packed', 'shipping', 'applying', 'applied', 'switching', 'resuming', 'completed', 'aborting', 'rolled_back', 'failed'])),
    fenceEpoch: req(nonNegD), sourceComputerId: req(t), targetComputerId: req(t), deadlineAtMs: req(nonNegD), updatedAtMs: req(nonNegD),
  }, v);
}

// Gerät: Workjet reicht die Antwort durch buildGuestDeviceControlExpression (nur diese Felder) und prüft dann
// WorkjetDeviceWebRtcResponseV1. invite ist CtoxBusinessOsInviteV1 (hier nur als Objekt geprüft).
function deviceResponseD(r, action) {
  const pairText = (max) => (x) => typeof x === 'string' && x.length > 0 && x.length <= max;
  if (action === 'binding.list') {
    return struct({
      schema: req((x) => x === 'ctox.workjet-device-bindings.v1'),
      bindings: req(listD(1000, (b, bp) => struct({
        id: req(pairText(256)), deviceId: req(pairText(256)), displayName: req(pairText(256)),
        createdAtMs: req(nonNegD), pairedAtMs: req((x) => x === null || nonNegD(x)),
      }, b, bp))),
    }, r);
  }
  if (action === 'invite.create') {
    return struct({
      businessOsInstanceId: req(pairText(256)), deviceId: req((x) => x === null || pairText(256)(x)),
      proofKeyThumbprint: req((x) => x === null || (typeof x === 'string' && /^[A-Za-z0-9_-]{43}$/.test(x))),
      grantId: req(pairText(256)), inviteId: req(pairText(256)), invite: req((x) => !!x && typeof x === 'object'),
      expiresAt: req(pairText(64)),
    }, r);
  }
  return struct({ revoked: req((x) => x === true) }, r);
}
// Ende Strang D

// Workjet #784 / Kern #875 – „Google Kalender verbinden“. Workjet erwartet action/commandId zurück und bei Fehlern
// einen Code im Feld error statt eines geworfenen Fehlers (geworfen = guest_failed).
function calendarConnectResponse(r, action) {
  const errors = [];
  if (!r || typeof r !== 'object' || Array.isArray(r)) return ['Antwort ist kein Objekt'];
  if (r.action !== action) errors.push('action');
  if (typeof r.commandId !== 'string' || !r.commandId) errors.push('commandId');
  if ('error' in r && (typeof r.error !== 'string' || !/^[a-z][a-z0-9_]{0,63}$/.test(r.error))) errors.push('error ist kein Code');
  return errors;
}

// Strang R – Präsentation und Narration (E26 / K-WJ-2). ref: Workjet eb65947d packages/contracts/src/
//   workjetPresentation.ts:14-122 (Manifest, Mutation, Read/ContentRead/CanvasSave-Antwort),
//   workjetJourFixeNarration.ts:5-54 (NarrationReadResponse), workjetJourFixeMeeting.ts:7-48 (AudioRef).
// Workjet dekodiert mit onExcessProperty: "error" → zusätzlich exakte Schlüsselmenge je Struktur.
function exactR(fields, value, path = '$') {
  const errors = struct(fields, value, path);
  if (value && typeof value === 'object' && !Array.isArray(value)) {
    for (const key of Object.keys(value)) if (!Object.hasOwn(fields, key)) errors.push(`${path}.${key}: überzähliges Feld`);
  }
  return errors;
}
const textR = (min, max) => (v) => typeof v === 'string' && v.length >= min && v.length <= max;
const idR = (v) => entityId(v) && v.length <= 128;
const uintR = (v) => int(v) && v >= 0;
const betweenR = (min, max) => (v) => int(v) && v >= min && v <= max;
const hashR = (v) => typeof v === 'string' && /^[a-f0-9]{64}$/.test(v);
const b64R = (max) => (v) => typeof v === 'string' && v.length <= max
  && /^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/.test(v);
const PRESENTATION_CONTRACT_R = 'ctox.workjet.presentation.v1';
const PRESENTATION_RANGE_R = 128 * 1024;
const PRESENTATION_MAX_R = 8 * 1024 * 1024;
const revisionR = betweenR(1, Number.MAX_SAFE_INTEGER - 1);
const documentBytesR = betweenR(2, PRESENTATION_MAX_R);
const slideIdsR = (v, p) => (Array.isArray(v) && v.length >= 1 && v.length <= 160
  ? v.flatMap((x, i) => (textR(1, 128)(x) ? [] : [`${p}[${i}]: ungültig`])) : false);
const presentationScopeR = (action) => ({ action: req((a) => a === action), commandId: req(entityId), projectId: req(entityId), meetingId: req(idR) });
const manifestR = (v, p) => exactR({
  presentation_id: req(idR), project_id: req(idR), meeting_id: req(idR), owner_user_id: req(textR(1, 256)),
  title: req(textR(1, 180)), revision: req(revisionR), document_schema: req(textR(1, 64)), document_file_id: req(idR),
  document_generation_id: req(idR), document_sha256: req(hashR), document_bytes: req(documentBytesR), slide_ids: req(slideIdsR),
  source: req((x) => x === 'agent' || x === 'owner'), updated_by: req(textR(1, 256)), updated_at_ms: req(uintR),
}, v, p);
function presentationReadResponseR(r) {
  return exactR({ ...presentationScopeR('project.presentation.read'), contract: req((c) => c === PRESENTATION_CONTRACT_R),
    presentation: req((m, p) => (m === null ? true : manifestR(m, p))) }, r);
}
function presentationContentResponseR(r) {
  return exactR({ ...presentationScopeR('project.presentation.content.read'),
    range: req((v, p) => exactR({
      presentation_id: req(idR), revision: req(revisionR), offset: req(betweenR(0, PRESENTATION_MAX_R)),
      length: req(betweenR(1, PRESENTATION_RANGE_R)), total_bytes: req(documentBytesR), document_sha256: req(hashR),
      data_base64: req(b64R(4 * Math.ceil(PRESENTATION_RANGE_R / 3))),
    }, v, p)),
    rangeSha256: req(hashR) }, r);
}
function presentationCanvasSaveResponseR(r) {
  return exactR({ ...presentationScopeR('project.presentation.canvas.save'), contract: req((c) => c === PRESENTATION_CONTRACT_R),
    mutation: req((v, p) => exactR({
      operation_id: req(idR), presentation_id: req(idR), project_id: req(idR), meeting_id: req(idR), revision: req(revisionR),
      document_sha256: req(hashR), document_bytes: req(documentBytesR), slide_ids: req(slideIdsR),
    }, v, p)),
    presentation: req(manifestR) }, r);
}
const NARRATION_RANGE_R = 256 * 1024;
const NARRATION_MAX_R = 8 * 1024 * 1024;
function narrationReadResponseR(r) {
  const textN = (n) => textR(1, n);
  return exactR({
    action: req((a) => a === 'project.jour_fixe.narration.read'), commandId: req(entityId), projectId: req(entityId),
    meetingId: req(idR), slideId: req(idR), deckRevision: req(betweenR(1, Number.MAX_SAFE_INTEGER)),
    meetingRevision: req(uintR),
    audio: req((v, p) => exactR({
      file_id: req(textN(128)), sha256: req(hashR), mime_type: req((m) => m === 'audio/wav'),
      duration_ms: req(betweenR(1, 300_000)), narration_text_sha256: req(hashR), source_run_id: req(textN(128)),
      model: req(textN(128)), format: req((f) => f === 'wav'), synthesis_duration_ms: req(uintR),
      provenance: req((x) => x === 'native_gateway' || x === 'authenticated_owner_local_audio'), generation_id: req(idR),
    }, v, p)),
    totalBytes: req(betweenR(1, NARRATION_MAX_R)), offset: req(uintR), length: req(betweenR(1, NARRATION_RANGE_R)),
    bytesBase64: req(b64R(4 * Math.ceil(NARRATION_RANGE_R / 3))), rangeSha256: req(hashR),
  }, r);
}
/** Korrelation Anfrage ↔ Antwort wie isWorkjet…ReceiptForRequest (workjetPresentation.ts:152-205,
 *  workjetJourFixeNarration.ts:62-79). Liefert eine Fehlerliste (leer = Workjet akzeptiert die Antwort). */
export function correlateR(request, response) {
  const errors = checkResponse(request.action, response);
  const same = (keys) => { for (const k of keys) if (response?.[k] !== request[k]) errors.push(`$.${k}: gehört nicht zur Anfrage`); };
  same(['action', 'commandId', 'projectId', 'meetingId']);
  if (request.action === 'project.presentation.read' && response?.presentation) {
    if (response.presentation.project_id !== request.projectId || response.presentation.meeting_id !== request.meetingId) errors.push('$.presentation: anderer Umfang');
  }
  if (request.action === 'project.presentation.content.read') {
    const g = response?.range || {};
    if (g.presentation_id !== request.presentationId || g.revision !== request.revision || g.offset !== request.offset
      || !(g.offset < g.total_bytes) || g.length !== Math.min(request.length, g.total_bytes - g.offset)) errors.push('$.range: passt nicht zur Anfrage');
  }
  if (request.action === 'project.presentation.canvas.save') {
    const m = response?.mutation || {}; const pr = response?.presentation || {};
    if (m.operation_id !== request.operationId || m.presentation_id !== request.presentationId || m.project_id !== request.projectId
      || m.meeting_id !== request.meetingId || m.revision !== request.expectedRevision + 1 || pr.presentation_id !== request.presentationId
      || pr.revision !== m.revision || pr.document_sha256 !== m.document_sha256) errors.push('$.mutation: passt nicht zur Anfrage');
  }
  if (request.action === 'project.jour_fixe.narration.read') {
    same(['slideId', 'deckRevision', 'offset']);
    if (!(response?.offset < response?.totalBytes) || response?.length !== Math.min(request.length, response.totalBytes - response.offset)) errors.push('$.length: passt nicht zur Anfrage');
  }
  return errors;
}
// Ende Strang R
