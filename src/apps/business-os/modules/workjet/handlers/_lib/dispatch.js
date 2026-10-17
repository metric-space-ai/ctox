// Origin: CTOX
// License: AGPL-3.0-only
// workjet.worker.dispatch – Absichts-Warteschlange als App-Daten (E54.3b, E17). Treuer Port von
// src/core/business_os/mcp_workjet_worker_dispatch.rs (Stand c59edf684); jede Funktion nennt ihre Rust-Quelle als
// `// ref: <zeilen>`. Kein Prozessstart, keine Capability: der Workjet-Rechner liest offene Absichten per api.query.
// Nur ECMAScript-Builtins (Handler-Laufzeit, §16.9). Fehler: throw {code, message} (invalid|forbidden|conflict|not_found).
import { sha256Hex, utf8Length, uuidFromHash } from './sha256.js';
import { validateWorkerOutcomeValue } from './outcome-contract.js';

export const CONTRACT = 'ctox.workjet.worker-dispatch.v1'; // ref: :12
export const SOURCES = 'workjet_worker_dispatch_sources';
export const INTENTS = 'workjet_worker_dispatch_intents';
export const OUTCOMES = 'workjet_worker_dispatch_outcomes';
const PROJECTS = 'workjet_projects';
const PROJECT_CHATS = 'workjet_project_chats';
const USERS = 'business_users';
export const PENDING_CAPACITY = 128; // ref: :687-690
const MAX_SAFE = 9_007_199_254_740_991;

export function fail(code, message) {
  const error = new Error(message);
  error.code = code;
  throw error;
}

const isObject = (v) => v !== null && typeof v === 'object' && !Array.isArray(v);
const live = (doc) => (doc && doc.is_deleted !== true && doc._deleted !== true ? doc : null);

// serde_json::Value-Gleichheit (Objektschlüssel ohne Reihenfolge).
export function canonical(value) {
  if (Array.isArray(value)) return `[${value.map(canonical).join(',')}]`;
  if (isObject(value)) return `{${Object.keys(value).sort().map((k) => `${JSON.stringify(k)}:${canonical(value[k])}`).join(',')}}`;
  return JSON.stringify(value);
}
const sameValue = (a, b) => canonical(a) === canonical(b);
const jsonBytes = (value) => utf8Length(JSON.stringify(value));
const clone = (value) => (value === undefined ? undefined : JSON.parse(JSON.stringify(value)));

// ref: :152-161 text()
function text(value, max, field = 'value') {
  if (typeof value !== 'string' || value === '' || utf8Length(value) > max || value !== value.trim() || value.includes('\0')) {
    fail('invalid', `invalid worker dispatch value: ${field}`);
  }
  return value;
}
const optText = (value, max, field) => (value === undefined || value === null ? undefined : text(value, max, field));
function revisionArg(value, field, optional = false) {
  if (optional && (value === undefined || value === null)) return undefined;
  if (!Number.isSafeInteger(value) || value < 1 || value > MAX_SAFE) fail('invalid', `invalid ${field}`);
  return value;
}

// ref: :80-118 enum Request (tag = action, deny_unknown_fields). v4-Ergänzung: dispatch/observe tragen die
// Supervisor-Bindung (project_id, source_supervisor_thread_id) im Payload, weil es keine eingeschränkte native
// Supervisor-Sitzung (trusted.workjet_supervisor_only + Lease) gibt; geprüft wird sie gegen die ctox-Daten.
const ACTION_FIELDS = {
  register_source: ['source_environment_id', 'source_supervisor_thread_id', 'project_id', 'expected_revision'],
  revoke_source: ['registration_id', 'revision'],
  poll: ['source_environment_id'],
  complete: ['registration_id', 'revision', 'intent_id', 'result'],
  report_outcome: ['registration_id', 'revision', 'intent_id', 'receipt'],
  dispatch: ['dispatch_key', 'task', 'title', 'computer_id', 'worker_profile_id', 'project_id', 'source_supervisor_thread_id'],
  observe: ['limit', 'project_id', 'source_supervisor_thread_id'],
};
const REQUIRED = {
  register_source: ['source_environment_id', 'source_supervisor_thread_id', 'project_id'],
  revoke_source: ['registration_id', 'revision'],
  poll: ['source_environment_id'],
  complete: ['registration_id', 'revision', 'intent_id', 'result'],
  report_outcome: ['registration_id', 'revision', 'intent_id', 'receipt'],
  dispatch: ['dispatch_key', 'task', 'project_id', 'source_supervisor_thread_id'],
  observe: ['project_id', 'source_supervisor_thread_id'],
};

export function parseRequest(payload) {
  if (!isObject(payload)) fail('invalid', 'payload must be an object');
  const args = { ...payload };
  delete args._context; // ref: :329-334 Gateway-Metadaten sind kein Argument
  const action = args.action;
  if (typeof action !== 'string' || !Object.hasOwn(ACTION_FIELDS, action)) fail('invalid', 'unknown worker dispatch action');
  for (const key of Object.keys(args)) {
    if (key !== 'action' && !ACTION_FIELDS[action].includes(key)) fail('invalid', `unknown field ${key}`);
  }
  for (const key of REQUIRED[action]) if (args[key] === undefined || args[key] === null) fail('invalid', `missing field ${key}`);
  return args;
}

// ---------------------------------------------------------------- Autorität / Bindung

// ref: mcp_remote_worker.rs:444-461 current_actor – persistierter, aktiver chef/admin; die Rolle des Befehls muss der
// persistierten entsprechen (ref: :178-182 „dispatch role changed“). capability_epoch gibt es in v4-business_users nicht:
// Epoche = capability_epoch, falls die Projektion es trägt, sonst 0 (offener Punkt im Bericht).
export async function currentActor(tx, actor) {
  const actorId = typeof actor?.id === 'string' ? actor.id : '';
  if (!actorId) fail('forbidden', 'worker source actor is not a persisted native user');
  const rows = (await tx.read(USERS, { selector: { user_id: actorId } })).map(live).filter(Boolean);
  const row = rows[0] ?? live(await tx.get(USERS, actorId));
  if (!row) fail('forbidden', 'worker source actor is not a persisted native user');
  if (row.active !== true || !['chef', 'admin'].includes(row.role)) fail('forbidden', 'worker source actor authority is no longer current');
  if (actor.role !== undefined && actor.role !== row.role) fail('forbidden', 'dispatch role changed');
  const epoch = Number.isSafeInteger(row.capability_epoch) ? row.capability_epoch : 0;
  return { actorId, role: row.role, epoch };
}

// ref: :163-249 current_project / current_project_with_team_binding. owned_project (project_chats.rs:344-362) aus
// workjet_projects; supervisor_binding::for_thread + Thread-Provenienz (Nutzer-Thread des Projekts) aus workjet_project_chats (Thread ↔ Projekt
// des Owners). Policy-Rechte CtoxTaskCreate/IntegrationsManage prüft der Kern über command_rights vor dem Handler.
export async function currentProject(tx, actor, project, thread) {
  const { actorId, epoch } = await currentActor(tx, actor);
  text(project, 128, 'project_id');
  const record = live(await tx.get(PROJECTS, project));
  if (!record) fail('forbidden', 'project is unavailable in this instance');
  if (record.owner_user_id !== actorId) fail('forbidden', 'project is unavailable to this user');
  if (record.status !== 'active') fail('forbidden', 'project is archived');
  const chats = (await tx.read(PROJECT_CHATS, { selector: { thread_id: thread } })).map(live).filter(Boolean)
    .filter((chat) => chat.owner_user_id === actorId);
  if (!chats.length) fail('forbidden', 'source supervisor is not registered natively');
  if (!chats.some((chat) => chat.project_id === project)) fail('forbidden', 'source supervisor belongs to another project');
  return epoch;
}

// v4 hat keinen verwalteten MCP-Kontext (managed_source_instance, workspace): Herkunft = Instanz/Arbeitsbereich aus ctx,
// sonst feste Kennung „local“.
export function sourceScope(ctx) {
  const instance = typeof ctx?.instance_id === 'string' && ctx.instance_id ? ctx.instance_id : 'local';
  const workspace = typeof ctx?.workspace_id === 'string' && ctx.workspace_id ? ctx.workspace_id : 'local';
  return { instance, workspace };
}

// ---------------------------------------------------------------- Speicher (ref: :251-300 load/check_source/save)

const sourceDocId = (owner, project) => uuidFromHash(`ctox.workjet.worker-dispatch.source|${owner}|${project}`);

async function sourceFor(tx, owner, project) {
  const doc = live(await tx.get(SOURCES, sourceDocId(owner, project)));
  return doc && doc.owner_user_id === owner && doc.project_id === project ? doc.record : null;
}

// ref: :251-266 load
async function load(tx, owner, id) {
  text(id, 128, 'registration_id');
  const docs = (await tx.read(SOURCES, { selector: { registration_id: id } })).map(live).filter(Boolean);
  const doc = docs.find((d) => d.owner_user_id === owner);
  if (!doc) fail('not_found', 'worker source registration unavailable');
  const record = doc.record;
  if (!isObject(record) || record.contract !== CONTRACT || record.ownerUserId !== owner || record.registrationId !== id || !(record.revision > 0)) {
    fail('conflict', 'invalid worker source registration');
  }
  return record;
}

// ref: :267-291 check_source
async function checkSource(tx, actor, ctx, record, revision) {
  const scope = sourceScope(ctx);
  if (record.state !== 'active' || record.revision !== revision || record.sourceInstanceId !== scope.instance
    || record.sourceWorkspaceId !== scope.workspace) fail('conflict', 'worker source is stale or revoked');
  const epoch = await currentProject(tx, actor, record.projectId, record.sourceSupervisorThreadId);
  if (epoch !== record.authorityEpoch) fail('conflict', 'worker source authority changed; explicit registration required');
}

// ref: :292-299 save (ON CONFLICT(owner_user_id,project_id) DO UPDATE)
async function save(tx, record, now) {
  await tx.put(SOURCES, {
    id: sourceDocId(record.ownerUserId, record.projectId),
    owner_user_id: record.ownerUserId,
    project_id: record.projectId,
    registration_id: record.registrationId,
    record: clone(record),
    state: record.state,
    revision: record.revision,
    source_environment_id: record.sourceEnvironmentId,
    updated_at_ms: now,
  });
}

// ---------------------------------------------------------------- Aktionen

// ref: :386-460 RegisterSource
async function registerSource(tx, ctx, actor, args, now) {
  const env = text(args.source_environment_id, 256, 'source_environment_id');
  const thread = text(args.source_supervisor_thread_id, 36, 'source_supervisor_thread_id');
  const project = text(args.project_id, 128, 'project_id');
  const expected = revisionArg(args.expected_revision, 'expected_revision', true);
  const epoch = await currentProject(tx, actor, project, thread);
  const scope = sourceScope(ctx);
  const existing = await sourceFor(tx, actor.id, project);
  let record;
  if (existing) record = clone(existing);
  else {
    if (expected !== undefined) fail('conflict', 'source registration does not exist');
    record = {
      contract: CONTRACT,
      registrationId: uuidFromHash(`ctox.workjet.worker-dispatch.registration|${actor.id}|${project}`),
      revision: 1,
      sourceEnvironmentId: env,
      sourceSupervisorThreadId: thread,
      sourceInstanceId: scope.instance,
      sourceWorkspaceId: scope.workspace,
      projectId: project,
      ownerUserId: actor.id,
      authorityEpoch: epoch,
      state: 'active',
    };
  }
  const unchanged = record.state === 'active' && record.sourceEnvironmentId === env && record.sourceSupervisorThreadId === thread
    && record.sourceInstanceId === scope.instance && record.sourceWorkspaceId === scope.workspace && record.authorityEpoch === epoch;
  if (unchanged) {
    if (!(expected === undefined || expected === record.revision || expected + 1 === record.revision)) fail('conflict', 'source registration revision stale');
  } else {
    if (expected !== record.revision) fail('conflict', 'source replacement needs exact revision');
    if (record.revision + 1 > MAX_SAFE) fail('conflict', 'source revision exhausted');
    Object.assign(record, {
      revision: record.revision + 1,
      sourceEnvironmentId: env,
      sourceSupervisorThreadId: thread,
      sourceInstanceId: scope.instance,
      sourceWorkspaceId: scope.workspace,
      authorityEpoch: epoch,
      state: 'active',
    });
  }
  await save(tx, record, now);
  return record;
}

// ref: :461-490 RevokeSource
async function revokeSource(tx, ctx, actor, args, now) {
  await currentActor(tx, actor); // ref: :383 remote_worker::current_actor vor jeder Aktion
  const revision = revisionArg(args.revision, 'revision');
  const record = clone(await load(tx, actor.id, args.registration_id));
  const scope = sourceScope(ctx);
  if (record.sourceInstanceId !== scope.instance || record.sourceWorkspaceId !== scope.workspace) fail('conflict', 'source instance differs');
  if (record.state === 'revoked') {
    if (!(revision === record.revision || revision + 1 === record.revision)) fail('conflict', 'source revision stale');
  } else {
    if (revision !== record.revision) fail('conflict', 'source revocation needs exact revision');
    record.revision = revision + 1;
    record.state = 'revoked';
  }
  await save(tx, record, now);
  return record;
}

// rowid-Reihenfolge (ref: :509) ≈ created_at_ms, Gleichstand nach id.
const byQueueOrder = (a, b) => (a.created_at_ms - b.created_at_ms) || String(a.id).localeCompare(String(b.id));

// ref: :491-535 Poll – eine quellweite Warteschlange; ersetzte/widerrufene/alte Registrierungen halten den Kopf nicht.
async function poll(tx, ctx, actor, args) {
  const env = text(args.source_environment_id, 256, 'source_environment_id');
  const { epoch } = await currentActor(tx, actor);
  const scope = sourceScope(ctx);
  const sources = (await tx.read(SOURCES, { selector: { owner_user_id: actor.id, source_environment_id: env } }))
    .map(live).filter(Boolean).map((doc) => doc.record)
    .filter((r) => r.sourceInstanceId === scope.instance && r.state === 'active' && r.authorityEpoch === epoch
      && r.sourceWorkspaceId === scope.workspace);
  let head = null;
  for (const registration of sources) {
    const rows = (await tx.read(INTENTS, { selector: { registration_id: registration.registrationId, status: 'pending' } }))
      .map(live).filter(Boolean).filter((row) => row.registration_revision === registration.revision && row.result === null);
    for (const row of rows) if (!head || byQueueOrder(row, head.row) < 0) head = { row, registration };
  }
  if (!head) return { contract: CONTRACT, intents: [], registration: null };
  await checkSource(tx, actor, ctx, head.registration, head.row.registration_revision);
  return { contract: CONTRACT, intents: [clone(head.row.intent)], registration: clone(head.registration) };
}

async function intentRow(tx, intentId, registrationId, revision, missing) {
  const row = live(await tx.get(INTENTS, intentId));
  if (!row || row.registration_id !== registrationId || row.registration_revision !== revision) fail('not_found', missing);
  return row;
}

// ref: :536-566 Complete – Ergebnis genau einmal; Wiederholung muss gleich sein.
async function complete(tx, ctx, actor, args, now) {
  await currentActor(tx, actor);
  const revision = revisionArg(args.revision, 'revision');
  const intentId = text(args.intent_id, 128, 'intent_id');
  const record = await load(tx, actor.id, args.registration_id);
  await requireLeaseSubstitute(tx, actor, record);
  await checkSource(tx, actor, ctx, record, revision);
  const row = await intentRow(tx, intentId, args.registration_id, revision, 'worker dispatch intent unavailable');
  validateResult(row.intent, args.result);
  if (row.result !== null && row.result !== undefined) {
    if (!sameValue(row.result, args.result)) fail('conflict', 'dispatch completion differs from prior result');
  } else {
    await tx.patch(INTENTS, intentId, { result: clone(args.result), status: 'completed', updated_at_ms: now });
  }
  return { contract: CONTRACT, intentId, registrationId: args.registration_id, revision, result: clone(args.result) };
}

// E54.3c (Kern, eng statt offen): Ersatz für die native Supervisor-Lease (current_lease :37), bis der Kern
// workjet_supervisor_leases projiziert. dispatch, complete und report_outcome nur, wenn der Akteur Rolle admin hat,
// Projekt-Owner ist, die Bindung über workjet_project_chats passt und seine capability_epoch der Epoche der Quelle
// entspricht. Alles andere → forbidden (keine Öffnung für chef).
export async function requireLeaseSubstitute(tx, actor, source) {
  const { role, epoch } = await currentActor(tx, actor);
  if (role !== 'admin') fail('forbidden', 'worker dispatch requires the supervisor lease (admin until projected, E54.3c)');
  const bound = await currentProject(tx, actor, source.projectId, source.sourceSupervisorThreadId);
  if (bound !== epoch || epoch !== source.authorityEpoch) fail('forbidden', 'supervisor authority epoch differs from the source');
}

// ref: :591-700 dispatch_in_native_scope
async function dispatch(tx, ctx, actor, args, now) {
  const dispatchKey = text(args.dispatch_key, 128, 'dispatch_key');
  const task = text(args.task, 16 * 1024, 'task');
  const title = optText(args.title, 200, 'title');
  const computerId = optText(args.computer_id, 256, 'computer_id');
  const workerProfileId = optText(args.worker_profile_id, 256, 'worker_profile_id');
  const project = text(args.project_id, 128, 'project_id');
  const thread = text(args.source_supervisor_thread_id, 36, 'source_supervisor_thread_id');
  // ref: :624-627 execution_key = Supervisor-Befehl bzw. bestätigter Plan. v4: die gebundene Supervisor-Sitzung
  // (Projekt + Thread) ist der Idempotenzraum des dispatch_key.
  const executionKey = `workjet-supervisor:${project}:${thread}`;
  const epoch = await currentProject(tx, actor, project, thread);
  const record = await sourceFor(tx, actor.id, project);
  if (!record) fail('not_found', 'no Workjet source registered for this supervisor');
  await requireLeaseSubstitute(tx, actor, record);
  if (record.state !== 'active' || record.authorityEpoch !== epoch || record.sourceSupervisorThreadId !== thread) {
    fail('conflict', 'Workjet source binding stale');
  }
  let intent = {
    intentId: uuidFromHash(`ctox.workjet.worker-dispatch.intent|${actor.id}|${executionKey}|${dispatchKey}`),
    registrationId: record.registrationId,
    registrationRevision: record.revision,
    sourceEnvironmentId: record.sourceEnvironmentId,
    sourceSupervisorThreadId: thread,
    projectId: project,
    task,
    ...(title === undefined ? {} : { title }),
    ...(computerId === undefined ? {} : { computerId }),
    ...(workerProfileId === undefined ? {} : { workerProfileId }),
  };
  // ref: :661-666 Digest über (Registrierung, Revision, Auftrag, Titel, Rechner, Profil); serde_json-Map sortiert Schlüssel.
  const digest = sha256Hex(canonical({
    registrationId: intent.registrationId, revision: intent.registrationRevision, task: intent.task,
    title: title ?? null, computerId: computerId ?? null, workerProfileId: workerProfileId ?? null,
  }));
  const prior = live(await tx.get(INTENTS, intent.intentId));
  let result = null;
  if (prior) {
    if (prior.command_id !== executionKey || prior.dispatch_key !== dispatchKey) fail('conflict', 'dispatch intent identity differs');
    if (prior.request_digest !== digest) fail('conflict', 'dispatch_key reused with different scope or task');
    intent = clone(prior.intent);
    result = prior.result ?? null;
  } else {
    // ref: :680-690 offene Absichten aktueller Registrierungen des Owners (aktiv, gleiche Revision, gleiche Epoche)
    const pending = (await tx.read(INTENTS, { selector: { owner_user_id: actor.id, status: 'pending' } })).map(live).filter(Boolean);
    const current = new Map();
    let count = 0;
    for (const row of pending) {
      if (row.result !== null && row.result !== undefined) continue;
      if (!current.has(row.registration_id)) {
        const docs = (await tx.read(SOURCES, { selector: { registration_id: row.registration_id } })).map(live).filter(Boolean)
          .filter((d) => d.owner_user_id === actor.id);
        current.set(row.registration_id, docs[0]?.record ?? null);
      }
      const src = current.get(row.registration_id);
      if (src && src.state === 'active' && src.revision === row.registration_revision && src.authorityEpoch === epoch) count += 1;
    }
    if (count >= PENDING_CAPACITY) fail('conflict', 'worker dispatch pending-intent capacity exceeded');
    await tx.put(INTENTS, {
      id: intent.intentId,
      intent_id: intent.intentId,
      registration_id: intent.registrationId,
      registration_revision: intent.registrationRevision,
      command_id: executionKey,
      dispatch_key: dispatchKey,
      request_digest: digest,
      intent: clone(intent),
      result: null,
      owner_user_id: actor.id,
      project_id: project,
      source_environment_id: intent.sourceEnvironmentId,
      status: 'pending',
      created_at_ms: now,
      updated_at_ms: now,
    });
  }
  return { contract: CONTRACT, state: result !== null ? 'completed' : 'pending', intent, result };
}

// ref: :703-827 observe
async function observe(tx, actor, args) {
  const limit = args.limit === undefined || args.limit === null ? 16 : args.limit;
  if (!Number.isInteger(limit) || limit < 1 || limit > 32) fail('invalid', 'observe limit must be 1..32');
  const project = text(args.project_id, 128, 'project_id');
  const thread = text(args.source_supervisor_thread_id, 36, 'source_supervisor_thread_id');
  const epoch = await currentProject(tx, actor, project, thread);
  const response = { contract: CONTRACT, projectId: project, supervisorThreadId: thread, observations: [], truncated: false };
  const sources = new Map();
  for (const doc of (await tx.read(SOURCES, { selector: { owner_user_id: actor.id, project_id: project } })).map(live).filter(Boolean)) {
    sources.set(doc.registration_id, doc.record);
  }
  const rows = (await tx.read(INTENTS, { selector: { owner_user_id: actor.id, project_id: project } })).map(live).filter(Boolean)
    .filter((row) => sources.has(row.registration_id) && row.intent?.sourceSupervisorThreadId === thread)
    .sort((a, b) => -byQueueOrder(a, b))
    .slice(0, limit + 1);
  const observations = [];
  for (const row of rows) {
    if (observations.length === limit) { response.truncated = true; break; }
    const { intent } = row;
    const source = sources.get(row.registration_id);
    if (source.contract !== CONTRACT || source.ownerUserId !== actor.id || source.projectId !== project || intent.projectId !== project
      || intent.sourceSupervisorThreadId !== thread || intent.registrationId !== source.registrationId) {
      fail('conflict', 'persisted dispatch observation identity differs');
    }
    let acknowledgement = null;
    if (row.result !== null && row.result !== undefined) {
      validateResult(intent, row.result);
      acknowledgement = {};
      for (const key of ['status', 'reason', 'workerThreadId', 'environmentId', 'computerId', 'branch']) {
        if (row.result[key] !== undefined) acknowledgement[key] = clone(row.result[key]);
      }
    }
    observations.push({
      intentId: intent.intentId, executionKey: row.command_id, dispatchKey: row.dispatch_key,
      registrationRevision: intent.registrationRevision,
      registrationCurrent: source.state === 'active' && source.revision === intent.registrationRevision
        && source.authorityEpoch === epoch && source.sourceSupervisorThreadId === thread,
      title: intent.title ?? null, taskPreview: [...intent.task].slice(0, 256).join(''),
      computerId: intent.computerId ?? null, workerProfileId: intent.workerProfileId ?? null,
      acknowledgement, execution: null,
      reportedOutcome: await readOutcome(tx, actor.id, project, thread, intent.intentId),
    });
    if (jsonBytes(observations) > 60 * 1024) { observations.pop(); response.truncated = true; break; }
  }
  response.observations = observations;
  return response;
}

// ref: :830-940 retain_outcome
async function reportOutcome(tx, ctx, actor, args, now) {
  await currentActor(tx, actor);
  const registrationId = text(args.registration_id, 128, 'registration_id');
  const intentId = text(args.intent_id, 128, 'intent_id');
  const revision = revisionArg(args.revision, 'revision');
  const receipt = args.receipt;
  const checked = validateWorkerOutcomeValue('WorkerTerminalReceipt', receipt);
  if (!checked.ok) fail('invalid', checked.error);
  const source = await load(tx, actor.id, registrationId);
  await requireLeaseSubstitute(tx, actor, source);
  await checkSource(tx, actor, ctx, source, revision);
  const row = await intentRow(tx, intentId, registrationId, revision, 'worker outcome intent unavailable');
  const { intent } = row;
  if (intent.projectId !== source.projectId || intent.sourceSupervisorThreadId !== source.sourceSupervisorThreadId
    || intent.sourceEnvironmentId !== source.sourceEnvironmentId) fail('conflict', 'worker outcome source scope differs');
  if (row.result === null || row.result === undefined) fail('conflict', 'worker has no startup acknowledgement');
  const startup = row.result;
  validateResult(intent, startup);
  if (!(startup.status === 'dispatched' && receipt.worker_thread_id === intent.intentId && startup.workerThreadId === receipt.worker_thread_id
    && startup.environmentId === receipt.environment_id && startup.computerId === receipt.computer_id && startup.branch === receipt.branch
    && receipt.branch === `workjet/worker/${receipt.worker_thread_id}` && receipt.execution_stopped === true)) {
    fail('conflict', 'worker outcome does not match a stopped, acknowledged isolated worker');
  }
  if (!validPullRequest(receipt.pull_request)) fail('invalid', 'worker pull request identity or head is invalid');
  if (jsonBytes(receipt) > 16 * 1024) fail('invalid', 'worker outcome exceeds bound');
  const prior = live(await tx.get(OUTCOMES, intentId));
  if (prior) {
    if (!sameValue(prior.receipt, receipt)) fail('conflict', 'worker terminal outcome differs from its immutable prior receipt');
  } else {
    await tx.put(OUTCOMES, {
      id: intentId, intent_id: intentId, owner_user_id: actor.id, project_id: source.projectId,
      supervisor_thread_id: source.sourceSupervisorThreadId, registration_revision: revision,
      receipt: clone(receipt), accepted_at_ms: now,
    });
  }
  return readOutcome(tx, actor.id, source.projectId, source.sourceSupervisorThreadId, intentId);
}

// ref: :883-912 Pull-Request-URL: https://github.com/<owner>/<repo>/pull/<number>, kein Nutzer/Port/Query/Fragment.
function validPullRequest(pr) {
  if (!isObject(pr) || typeof pr.url !== 'string') return false;
  const m = /^https:\/\/github\.com\/([^/?#@:]+)\/([^/?#@:]+)\/pull\/([^/?#]+)$/.exec(pr.url);
  if (!m) return false;
  const part = /^[A-Za-z0-9\-_.]+$/;
  return part.test(m[1]) && part.test(m[2]) && m[3] === String(pr.number)
    && (pr.head_oid.length === 40 || pr.head_oid.length === 64) && /^[0-9a-f]+$/.test(pr.head_oid);
}

// ref: :942-985 read_outcome
async function readOutcome(tx, owner, project, supervisor, intentId) {
  const row = live(await tx.get(OUTCOMES, intentId));
  if (!row || row.owner_user_id !== owner || row.project_id !== project || row.supervisor_thread_id !== supervisor) return null;
  if (jsonBytes(row.receipt) > 16 * 1024) fail('conflict', 'persisted worker outcome exceeds bound');
  const checked = validateWorkerOutcomeValue('WorkerTerminalReceipt', row.receipt);
  if (!checked.ok) fail('conflict', checked.error);
  if (row.receipt.worker_thread_id !== intentId) fail('conflict', 'persisted worker outcome identity differs');
  return { provenance: 'authenticated_source_report', accepted_at_ms: row.accepted_at_ms, registration_revision: row.registration_revision, receipt: clone(row.receipt) };
}

const FAILURE_REASONS = new Set([
  'role-not-authorized', 'parent-unavailable', 'parent-not-orchestrator', 'duplicate-capabilities', 'capability-escalation',
  'computer-unavailable', 'worker-profile-unavailable', 'remote-dispatch-unavailable', 'remote-dispatch-failed',
  'worktree-failed', 'create-failed', 'turn-start-failed', 'rollback-failed',
]);
const DISPATCHED_FIELDS = new Set(['schemaVersion', 'status', 'environmentId', 'workerThreadId', 'computerId', 'branch', 'worktreePath',
  'parent', 'modelSelection', 'enabledCapabilityIds']);

// ref: :1037-1176 validate_result
export function validateResult(intent, value) {
  const bad = (message) => fail('invalid', message);
  if (!isObject(value)) bad('dispatch result must be an object');
  if (jsonBytes(value) > 32 * 1024 || value.schemaVersion !== 1) bad('invalid dispatch result schema/bound');
  if (value.status === 'dispatched') {
    if (!Object.keys(value).every((k) => DISPATCHED_FIELDS.has(k))) bad('unknown dispatch result field');
    for (const field of ['environmentId', 'workerThreadId', 'computerId', 'branch', 'worktreePath']) {
      if (typeof value[field] !== 'string') bad('dispatch result field missing');
      text(value[field], 2048, field);
    }
    if (!(value.workerThreadId === intent.intentId && value.parent?.environmentId === intent.sourceEnvironmentId
      && value.parent?.threadId === intent.sourceSupervisorThreadId)) bad('dispatch result identity differs from intent');
    if (intent.computerId !== undefined && intent.computerId !== null && value.computerId !== intent.computerId) bad('dispatch computer differs');
    if (!isObject(value.parent)) bad('dispatch parent missing');
    if (Object.keys(value.parent).length !== 2) bad('unknown dispatch parent field');
    const model = value.modelSelection;
    if (!isObject(model)) bad('dispatch model missing');
    if (!Object.keys(model).every((k) => ['instanceId', 'model', 'options'].includes(k))) bad('unknown model selection field');
    if (typeof model.instanceId !== 'string') bad('model instance missing');
    text(model.instanceId, 256, 'modelSelection.instanceId');
    if (typeof model.model !== 'string') bad('model missing');
    text(model.model, 256, 'modelSelection.model');
    if (model.options !== undefined) {
      if (!Array.isArray(model.options)) bad('model options must be a typed array');
      if (model.options.length > 64) bad('too many model options');
      const ids = new Set();
      for (const option of model.options) {
        if (!isObject(option)) bad('invalid model option');
        if (!(Object.keys(option).length === 2 && Object.hasOwn(option, 'id') && Object.hasOwn(option, 'value'))) bad('invalid model option fields');
        if (typeof option.id !== 'string') bad('model option id missing');
        text(option.id, 256, 'option.id');
        if (ids.has(option.id)) bad('duplicate model option');
        ids.add(option.id);
        if (typeof option.value === 'string') text(option.value, 2048, 'option.value');
        else if (typeof option.value !== 'boolean') bad('invalid model option value');
      }
    }
    const caps = value.enabledCapabilityIds;
    if (!Array.isArray(caps)) bad('dispatch capabilities missing');
    if (caps.length > 64) bad('too many dispatch capabilities');
    caps.forEach((cap, i) => {
      if (typeof cap !== 'string') bad('invalid dispatch capability');
      text(cap, 256, 'capability');
      if (caps.slice(0, i).includes(cap)) bad('duplicate dispatch capability');
    });
  } else if (value.status === 'failed') {
    if (!(Object.keys(value).length === 3 && FAILURE_REASONS.has(value.reason))) bad('invalid terminal dispatch failure');
  } else {
    bad('pending or unsupported dispatch result is not terminal');
  }
}

// ref: :323-589 execute – eine Transaktion je Befehl (Rust: Core- + Policy-Transaktion IMMEDIATE).
export async function handleDispatch(command, ctx) {
  const args = parseRequest(command?.payload);
  const actor = { id: command?.actor?.id, role: command?.actor?.role };
  const now = Number(typeof ctx?.now === 'function' ? ctx.now() : Date.now());
  const run = (tx) => {
    switch (args.action) {
      case 'register_source': return registerSource(tx, ctx, actor, args, now);
      case 'revoke_source': return revokeSource(tx, ctx, actor, args, now);
      case 'poll': return poll(tx, ctx, actor, args);
      case 'complete': return complete(tx, ctx, actor, args, now);
      case 'report_outcome': return reportOutcome(tx, ctx, actor, args, now);
      case 'dispatch': return dispatch(tx, ctx, actor, args, now);
      case 'observe': return observe(tx, actor, args);
      default: return fail('invalid', 'unknown worker dispatch action');
    }
  };
  return typeof ctx?.transaction === 'function' ? ctx.transaction(run) : run(ctx);
}
