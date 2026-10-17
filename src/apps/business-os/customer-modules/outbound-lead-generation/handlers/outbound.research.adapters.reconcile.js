// outbound.research.adapters.reconcile — FACHLOGIK (Antwortprojektion) + cap.agent.task
// ref: store_outbound_commands.rs:2085-2600 apply_outbound_adapter_reconciliation_reply
//      :2602-2703 mark_outbound_adapter_reconciliation_failed
//      :2705-2774 safe identifier / script path / revision / target key / secret-key scan
//      store.rs:10369-10404 (reply → apply, on error mark_failed then fail), :15080 (queue
//      failure → mark_failed), :27739 suggested skill universal-scraping
//
// Two phases:
//   1. command without `agent_reply`: queue ONE agent task through the flat §16.30 primitive
//      ctx.cap.agent.task.enqueue({title, instruction, skill, priority, idempotency_key, payload:{command},
//      on_complete, on_failed}) – Rückruf = eigener Befehl outbound.research.adapters.reconcile.done.
//   2. invoked by that callback handler with `agent_reply = {task_id, text}` (worker reply) or
//      `{task_id, error}` (queue failure/deadline): validate the typed result
//      all-or-nothing, then project sources / adapters / policy.
// RUST-EIGENHEIT: a few checks (adapter target key / adapter id) only run inside
// the projection loop, so a bad entry can fail after earlier writes; kept as is.
import { fail, firstString, has, isObject, requiredString, str, utf8Len } from './_lib/olg.js';
import { isSafeRxdbCollectionName, putI64, putString, runtimeAppStarterCollectionName } from './_lib/research-source-adapter.js';

const SCHEMA = 'ctox.outbound.adapter_reconciliation.v1';
export const RECONCILE_DONE = 'outbound.research.adapters.reconcile.done';
const ADAPTER_STATUSES = ['ready', 'auth_required', 'failed', 'disabled', 'needs_attention'];

const clone = (value) => (value === undefined ? undefined : JSON.parse(JSON.stringify(value)));
const get = (value, key) => (has(value, key) ? value[key] : undefined);

/** ref: :2705 — ASCII alnum . _ -, 1..=max bytes. */
export function safeIdentifier(value, maxLen) {
  return value !== '' && utf8Len(value) <= maxLen && /^[A-Za-z0-9._-]+$/.test(value);
}

/** ref: :2727 — as identifier plus ':' , ≤ 180. */
export function safeRevision(value) {
  return value !== '' && utf8Len(value) <= 180 && /^[A-Za-z0-9._:-]+$/.test(value);
}

/**
 * ref: :2713 — relative, only Normal components, extension js|mjs.
 * Emulates std::path::Path::components on Unix: empty segments vanish, a
 * leading "." is CurDir (rejected), inner "." vanish, ".." is ParentDir.
 */
export function safeRelativeScriptPath(value) {
  if (value === '' || utf8Len(value) > 512 || value.startsWith('/')) return false;
  const segments = value.split('/');
  const normal = [];
  for (let i = 0; i < segments.length; i += 1) {
    const segment = segments[i];
    if (segment === '') continue;
    if (segment === '.') {
      if (normal.length === 0 && i === 0) return false;
      continue;
    }
    if (segment === '..') return false;
    normal.push(segment);
  }
  if (normal.length === 0) return false;
  const name = normal[normal.length - 1];
  const dot = name.lastIndexOf('.');
  if (dot <= 0) return false;
  const extension = name.slice(dot + 1);
  return extension === 'js' || extension === 'mjs';
}

/** ref: :2735 — bytes: ASCII alnum lowercased, everything else '-', trim '-'. */
export function reconciliationTargetKey(sourceId) {
  let out = '';
  for (const byte of new TextEncoder().encode(sourceId)) {
    const ch = String.fromCharCode(byte);
    out += byte < 0x80 && /[A-Za-z0-9]/.test(ch) ? ch.toLowerCase() : '-';
  }
  return out.replace(/^-+|-+$/g, '');
}

/** ref: :2750 */
export function containsSecretKey(value) {
  if (Array.isArray(value)) return value.some(containsSecretKey);
  if (!isObject(value)) return false;
  return Object.entries(value).some(([rawKey, child]) => {
    const key = rawKey.trim().replace(/[A-Z]/g, (c) => c.toLowerCase()).replace(/-/g, '_');
    return ['password', 'passphrase', 'secret', 'credential_value', 'api_key', 'access_token', 'refresh_token', 'private_key'].includes(key)
      || key.endsWith('_password') || key.endsWith('_secret') || key.endsWith('_token')
      || containsSecretKey(child);
  });
}

function httpUrl(url) {
  let parsed;
  try {
    parsed = new URL(url);
  } catch {
    fail('invalid', 'discovered source URL is invalid');
  }
  if (!((parsed.protocol === 'http:' || parsed.protocol === 'https:') && parsed.hostname !== '')) {
    fail('invalid', 'discovered source URL must be HTTP(S)');
  }
}

/** v4 commands carry no client_context; ABWEICHUNG: app_id stands in for command.module. */
function sourceModuleOf(command) {
  const cc = isObject(command.client_context) ? command.client_context : {};
  return firstString([str(cc, ['source_module']), typeof command.app_id === 'string' ? command.app_id : null]);
}

function modulePrefixOf(sourceModule) {
  return runtimeAppStarterCollectionName(sourceModule).replace(/(_records)+$/, '');
}

/** ref: :2093 apply_outbound_adapter_reconciliation_reply */
export async function applyReconciliationReply(ctx, command, taskId, replyText) {
  if (command.type !== 'outbound.research.adapters.reconcile') fail('invalid', 'not an outbound adapter reconciliation command');
  const commandId = command.id;
  const text = String(replyText ?? '').trim();
  if (text === '') fail('invalid', 'adapter reconciliation reply was empty');
  let result;
  try {
    result = JSON.parse(text);
  } catch {
    // anyhow `.context()` Display shows only the outer message.
    fail('invalid', 'adapter reconciliation reply must be one strict JSON object');
  }
  if (get(result, 'schema') !== SCHEMA) fail('invalid', 'adapter reconciliation reply has the wrong schema');
  if (containsSecretKey(result)) fail('invalid', 'adapter reconciliation reply must not contain credential values');

  const payload = command.payload;
  const expectedDigest = requiredString(payload, ['configuration_digest']);
  if (get(result, 'configuration_digest') !== expectedDigest) {
    fail('invalid', 'adapter reconciliation result does not match the requested configuration');
  }
  if (!has(payload, 'writeback_contract')) fail('invalid', 'adapter reconciliation writeback contract is required');
  const writeback = payload.writeback_contract;
  const sourceCollection = requiredString(writeback, ['source_collection']);
  const adapterCollection = requiredString(writeback, ['adapter_collection']);
  if (!(isSafeRxdbCollectionName(sourceCollection) && isSafeRxdbCollectionName(adapterCollection))) {
    fail('invalid', 'invalid adapter reconciliation writeback collection');
  }
  const sourceModule = sourceModuleOf(command);
  if (sourceModule == null) fail('invalid', 'adapter reconciliation source module is required');
  const modulePrefix = modulePrefixOf(sourceModule);
  if (!(modulePrefix !== '' && sourceCollection.startsWith(`${modulePrefix}_`) && adapterCollection.startsWith(`${modulePrefix}_`))) {
    fail('invalid', 'adapter reconciliation writeback must stay inside the source module');
  }

  const requestedSources = get(payload, 'sources');
  if (!Array.isArray(requestedSources)) fail('invalid', 'adapter reconciliation sources are required');
  const sourceIds = new Set();
  const requestedById = new Map();
  for (const source of requestedSources) {
    const sourceId = requiredString(source, ['id']);
    if (!safeIdentifier(sourceId, 160)) fail('invalid', 'invalid source id in adapter reconciliation request');
    sourceIds.add(sourceId);
    requestedById.set(sourceId, clone(source));
  }

  const resultStatus = requiredString(result, ['status']);
  if (!(resultStatus === 'completed' || resultStatus === 'needs_attention')) fail('invalid', 'adapter reconciliation result has an unsupported status');

  // Validate the complete agent result before the first projection.
  const discoveredSources = Array.isArray(get(result, 'discovered_sources')) ? result.discovered_sources : [];
  const validatedSourceIds = new Set(sourceIds);
  for (const discovered of discoveredSources) {
    const sourceId = requiredString(discovered, ['id']);
    const url = requiredString(discovered, ['url']);
    if (!safeIdentifier(sourceId, 160)) fail('invalid', 'invalid discovered source id');
    if (validatedSourceIds.has(sourceId)) fail('invalid', 'adapter reconciliation returned a source more than once');
    validatedSourceIds.add(sourceId);
    httpUrl(url);
    const targetKey = str(discovered, ['target_key']);
    if (targetKey != null && !safeIdentifier(targetKey, 128)) fail('invalid', 'invalid discovered target key');
  }
  const adapterResults = get(result, 'adapters');
  if (!Array.isArray(adapterResults)) fail('invalid', 'adapter reconciliation result requires adapters');
  const validatedAdapterSources = new Set();
  let hasAttention = false;
  for (const adapter of adapterResults) {
    const sourceId = requiredString(adapter, ['source_id']);
    if (!validatedSourceIds.has(sourceId)) fail('invalid', 'adapter result references an undeclared source');
    if (validatedAdapterSources.has(sourceId)) fail('invalid', 'adapter reconciliation returned a source more than once');
    validatedAdapterSources.add(sourceId);
    const status = requiredString(adapter, ['status']);
    if (!ADAPTER_STATUSES.includes(status)) fail('invalid', 'unsupported adapter reconciliation status');
    hasAttention ||= ['auth_required', 'failed', 'needs_attention'].includes(status);
    const scrapeStatus = requiredString(adapter, ['scrape_status']);
    if (!safeIdentifier(scrapeStatus, 80)) fail('invalid', 'invalid adapter scrape status');
    const revision = str(adapter, ['adapter_revision']);
    if (revision != null && !safeRevision(revision)) fail('invalid', 'invalid adapter revision');
    const scriptPath = str(adapter, ['script_path']);
    if (scriptPath != null && !safeRelativeScriptPath(scriptPath)) fail('invalid', 'invalid adapter script path');
    if (has(adapter, 'test') && !isObject(adapter.test)) fail('invalid', 'adapter test result must be an object');
    if (status === 'ready') {
      if (revision == null) fail('invalid', 'ready adapter requires adapter_revision');
      if (scriptPath == null) fail('invalid', 'ready adapter requires script_path');
      if (get(get(adapter, 'test'), 'ok') !== true) fail('invalid', 'ready adapter requires a passing test result');
    }
  }
  // BTreeSet difference → sorted.
  let missing = [...validatedSourceIds].filter((id) => !validatedAdapterSources.has(id)).sort(byteOrder);
  if (missing.length) fail('invalid', `adapter reconciliation omitted sources: ${missing.join(', ')}`);
  if ((resultStatus === 'needs_attention') !== hasAttention) {
    fail('invalid', 'adapter reconciliation status is inconsistent with adapter results');
  }
  let policyCollection = null;
  if (str(payload, ['policy_id']) != null) {
    const collection = requiredString(writeback, ['policy_collection']);
    if (!(isSafeRxdbCollectionName(collection) && collection.startsWith(`${modulePrefix}_`))) {
      fail('invalid', 'adapter reconciliation policy writeback must stay inside the source module');
    }
    policyCollection = collection;
  }

  const now = ctx.now();
  const projectedSources = [];
  for (const discovered of discoveredSources) {
    const sourceId = requiredString(discovered, ['id']);
    const url = requiredString(discovered, ['url']);
    if (!safeIdentifier(sourceId, 160)) fail('invalid', 'invalid discovered source id');
    httpUrl(url);
    sourceIds.add(sourceId);
    const existing = await ctx.get(sourceCollection, sourceId);
    const createdAt = Number.isInteger(get(existing, 'created_at_ms')) ? existing.created_at_ms : now;
    const targetKey = str(discovered, ['target_key']) ?? reconciliationTargetKey(sourceId);
    if (!safeIdentifier(targetKey, 128)) fail('invalid', 'invalid discovered target key');
    const requiresCredential = typeof get(discovered, 'requires_credential') === 'boolean' ? discovered.requires_credential : false;
    const record = {
      id: sourceId,
      label: str(discovered, ['label']) ?? sourceId,
      url,
      countries: has(discovered, 'countries') ? clone(discovered.countries) : ['DE', 'AT', 'CH'],
      field_keys: has(discovered, 'field_keys') ? clone(discovered.field_keys)
        : (has(payload, 'field_keys') ? clone(payload.field_keys) : []),
      enabled: typeof get(discovered, 'enabled') === 'boolean' ? discovered.enabled : true,
      requires_credential: requiresCredential,
      credential_secret_name: '',
      target_key: targetKey,
      adapter_status: 'reconciliation_completed',
      scrape_status: 'target_available',
      auth_status: requiresCredential ? 'required' : 'not_required',
      payload: {
        builtin: false,
        discovered_by_command_id: commandId,
        discovery_reason: str(discovered, ['reason']) ?? '',
        secret_value_in_payload: false,
      },
      created_at_ms: createdAt,
      updated_at_ms: now,
    };
    await ctx.put(sourceCollection, record);
    requestedById.set(sourceId, clone(record));
    projectedSources.push(record);
  }

  const completedSourceIds = new Set();
  const projectedAdapters = [];
  let scope = null;
  for (const adapterResult of adapterResults) {
    const sourceId = requiredString(adapterResult, ['source_id']);
    if (!sourceIds.has(sourceId)) fail('invalid', 'adapter result references an undeclared source');
    if (completedSourceIds.has(sourceId)) fail('invalid', 'adapter reconciliation returned a source more than once');
    completedSourceIds.add(sourceId);
    const requested = requestedById.get(sourceId);
    if (requested === undefined) fail('invalid', 'adapter source definition is unavailable');
    const targetKey = firstString([str(adapterResult, ['target_key']), str(requested, ['target_key'])]) ?? reconciliationTargetKey(sourceId);
    if (!safeIdentifier(targetKey, 128)) fail('invalid', 'invalid adapter target key');
    const adapterId = firstString([str(requested, ['adapter_id']), str(adapterResult, ['id'])]) ?? `adapter_leadgen_${targetKey}`;
    if (!safeIdentifier(adapterId, 180)) fail('invalid', 'invalid adapter id');
    const existing = await ctx.get(adapterCollection, adapterId);
    const createdAt = Number.isInteger(get(existing, 'created_at_ms')) ? existing.created_at_ms : now;
    const status = requiredString(adapterResult, ['status']);
    if (!ADAPTER_STATUSES.includes(status)) fail('invalid', 'unsupported adapter reconciliation status');
    const scrapeStatus = requiredString(adapterResult, ['scrape_status']);
    const authStatus = str(adapterResult, ['auth_status']) ?? (status === 'auth_required' ? 'auth_required' : 'not_required');
    const lastError = str(adapterResult, ['last_error']) ?? '';
    const payloadFields = isObject(get(existing, 'payload')) ? clone(existing.payload) : {};
    payloadFields.configuration_digest = expectedDigest;
    payloadFields.reconciliation_command_id = commandId;
    payloadFields.reconciliation_task_id = taskId;
    payloadFields.reconciled_command_id = commandId;
    payloadFields.reconciled_command_status = resultStatus;
    payloadFields.adapter_revision = has(adapterResult, 'adapter_revision') ? clone(adapterResult.adapter_revision) : null;
    payloadFields.script_path = has(adapterResult, 'script_path') ? clone(adapterResult.script_path) : null;
    payloadFields.test = has(adapterResult, 'test') ? clone(adapterResult.test) : null;
    payloadFields.secret_value_in_payload = false;
    const record = {
      id: adapterId,
      source_id: sourceId,
      status,
      scrape_status: scrapeStatus,
      auth_status: authStatus,
      last_command_id: commandId,
      last_task_id: taskId,
      last_error: lastError,
      payload: payloadFields,
      created_at_ms: createdAt,
      updated_at_ms: now,
    };
    await ctx.put(adapterCollection, record);

    const sourceRecord = await ctx.get(sourceCollection, sourceId);
    if (sourceRecord) {
      putString(sourceRecord, 'adapter_status', status);
      putString(sourceRecord, 'scrape_status', scrapeStatus);
      putString(sourceRecord, 'auth_status', authStatus);
      // A stored credential wins over the worker's portal guess (thesen 11.09.2026).
      if (sourceRecord.requires_credential === true
        && ['required', 'not_required', 'credential_missing'].includes(sourceRecord.auth_status)) {
        const secretName = str(sourceRecord, ['credential_secret_name']);
        if (secretName != null) {
          let exists = false;
          try {
            scope ??= (await ctx.cap.secret.scope()).scope;
            exists = (await ctx.cap.secret.exists({ scope, name: secretName }))?.exists === true;
          } catch {
            exists = false; // Rust `.unwrap_or(false)`
          }
          if (exists) putString(sourceRecord, 'auth_status', 'credential_available');
        }
      }
      putI64(sourceRecord, 'updated_at_ms', now);
      await ctx.put(sourceCollection, sourceRecord);
    }
    projectedAdapters.push(record);
  }
  missing = [...sourceIds].filter((id) => !completedSourceIds.has(id)).sort(byteOrder);
  if (missing.length) fail('invalid', `adapter reconciliation omitted sources: ${missing.join(', ')}`);

  const policyId = str(payload, ['policy_id']);
  if (policyId != null && policyCollection != null) {
    const policy = await ctx.get(policyCollection, policyId);
    if (policy) {
      putString(policy, 'reconciliation_status', resultStatus);
      putString(policy, 'reconciliation_command_id', commandId);
      putString(policy, 'reconciliation_task_id', taskId);
      putString(policy, 'configuration_digest', expectedDigest);
      putString(policy, 'reconciliation_error', resultStatus === 'needs_attention' ? 'one or more adapters require operator attention' : '');
      putI64(policy, 'updated_at_ms', now);
      await ctx.put(policyCollection, policy);
    }
  }
  return {
    ok: true,
    schema: SCHEMA,
    configuration_digest: expectedDigest,
    status: has(result, 'status') ? clone(result.status) : 'completed',
    command_id: commandId,
    execution_task_id: taskId,
    task_id: taskId,
    sources: projectedSources,
    adapters: projectedAdapters,
    secret_value_in_payload: false,
  };
}

/** Rust BTreeSet<String> order = byte order of UTF-8; for ASCII ids = code unit order. */
function byteOrder(a, b) {
  return a < b ? -1 : a > b ? 1 : 0;
}

/** ref: :2602 mark_outbound_adapter_reconciliation_failed */
export async function markReconciliationFailed(ctx, command, taskId, error) {
  if (command.type !== 'outbound.research.adapters.reconcile') return;
  const commandId = command.id;
  const payload = command.payload;
  if (!has(payload, 'writeback_contract')) fail('invalid', 'adapter reconciliation writeback contract is required');
  const writeback = payload.writeback_contract;
  const sourceCollection = requiredString(writeback, ['source_collection']);
  const adapterCollection = requiredString(writeback, ['adapter_collection']);
  if (!(isSafeRxdbCollectionName(sourceCollection) && isSafeRxdbCollectionName(adapterCollection))) {
    fail('invalid', 'invalid adapter reconciliation writeback collection');
  }
  const sourceModule = sourceModuleOf(command);
  if (sourceModule == null) fail('invalid', 'adapter reconciliation source module is required');
  const modulePrefix = modulePrefixOf(sourceModule);
  if (!(modulePrefix !== '' && sourceCollection.startsWith(`${modulePrefix}_`) && adapterCollection.startsWith(`${modulePrefix}_`))) {
    fail('invalid', 'adapter reconciliation failure writeback must stay inside the source module');
  }
  const now = ctx.now();
  const sources = Array.isArray(get(payload, 'sources')) ? payload.sources : [];
  for (const requested of sources) {
    const sourceId = requiredString(requested, ['id']);
    const source = await ctx.get(sourceCollection, sourceId);
    if (source) {
      putString(source, 'adapter_status', 'needs_attention');
      putString(source, 'scrape_status', 'failed');
      putString(source, 'last_error', error);
      putI64(source, 'updated_at_ms', now);
      await ctx.put(sourceCollection, source);
    }
    const targetKey = str(requested, ['target_key']) ?? reconciliationTargetKey(sourceId);
    const adapterId = str(requested, ['adapter_id']) ?? `adapter_leadgen_${targetKey}`;
    const adapter = await ctx.get(adapterCollection, adapterId);
    if (adapter) {
      putString(adapter, 'status', 'failed');
      putString(adapter, 'scrape_status', 'failed');
      putString(adapter, 'last_command_id', commandId);
      putString(adapter, 'last_task_id', taskId);
      putString(adapter, 'last_error', error);
      putI64(adapter, 'updated_at_ms', now);
      await ctx.put(adapterCollection, adapter);
    }
  }
  const policyId = str(payload, ['policy_id']);
  const policyCollection = str(writeback, ['policy_collection']);
  if (policyId != null && policyCollection != null
    && isSafeRxdbCollectionName(policyCollection) && policyCollection.startsWith(`${modulePrefix}_`)) {
    const policy = await ctx.get(policyCollection, policyId);
    if (policy) {
      putString(policy, 'reconciliation_status', 'failed');
      putString(policy, 'reconciliation_command_id', commandId);
      putString(policy, 'reconciliation_task_id', taskId);
      putString(policy, 'reconciliation_error', error);
      putI64(policy, 'updated_at_ms', now);
      await ctx.put(policyCollection, policy);
    }
  }
}

export default async function handle(command, ctx) {
  const reply = command.agent_reply;
  if (!isObject(reply)) {
    // Phase 1: one bounded agent turn (Rust: queue task with suggested skill
    // universal-scraping, store.rs:27739). Priority/title come from the payload.
    const payload = isObject(command.payload) ? command.payload : {};
    const task = await ctx.cap.agent.task.enqueue({
      instruction: typeof payload.instruction === 'string' ? payload.instruction : '',
      title: typeof payload.title === 'string' ? payload.title : 'Recherche-Adapter abgleichen',
      priority: typeof payload.priority === 'string' ? payload.priority : 'low',
      skill: 'universal-scraping',
      idempotency_key: `${command.type}:${command.id}`,
      // Der Rückruf liest den Ausgangsbefehl aus der Aufgabe (agent.task.get → payload.command).
      payload: { command: { id: command.id, type: command.type, payload, client_context: command.client_context ?? null } },
      on_complete: RECONCILE_DONE,
      on_failed: RECONCILE_DONE,
    });
    return { ok: true, queued: true, task_id: task?.task_id ?? null };
  }
  const taskId = typeof reply.task_id === 'string' ? reply.task_id : '';
  if (typeof reply.error === 'string') {
    // store.rs:15080 — queue failure / deadline / cancel; a projection error is
    // only logged in Rust, the command result stays the queue error.
    try {
      await markReconciliationFailed(ctx, command, taskId, reply.error);
    } catch (writebackError) {
      ctx.log('warn', 'failed to project adapter reconciliation queue error', { error: writebackError.message });
    }
    fail('unavailable', reply.error);
  }
  try {
    return await applyReconciliationReply(ctx, command, taskId, reply.text);
  } catch (error) {
    try {
      await markReconciliationFailed(ctx, command, taskId, error.message);
    } catch (writebackError) {
      ctx.log('warn', 'failed to project adapter reconciliation validation error', { error: writebackError.message });
    }
    throw error;
  }
}
