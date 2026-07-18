import { withTimeout } from './async-timeout.js';
import { CTOX_COMMAND_AUTHORIZATION } from './command-lifecycle.generated.js';

const COMMAND_ACCEPT_TIMEOUT_MS = 45000;
const COMMAND_SYNC_READY_TIMEOUT_MS = 45000;
const COMMAND_SYNC_FLUSH_TIMEOUT_MS = 15000;
const COMMAND_CAPABILITY_TIMEOUT_MS = 5000;
const COMMAND_CAPABILITY_NEGATIVE_CACHE_MS = 10000;
const MAX_SIMULTANEOUS_COMMAND_WATCHERS = 128;
let activeCommandWatcherCount = 0;
const DEMAND_ONLY_SYNC_COLLECTIONS = new Set([
  'desktop_file_chunks',
  'document_blob_chunks',
  'spreadsheet_blob_chunks',
]);

export function createCommandBus({ db, sync = null, session = null } = {}) {
  return {
    async submit(command) {
      return submitRxdbCommand({ db, sync, session, command });
    },
    async waitForAccepted(commandId, options = {}) {
      return waitForCommandState({ db, sync, commandId, until: 'accepted', options });
    },
    async waitForTerminal(commandId, options = {}) {
      return waitForCommandState({ db, sync, commandId, until: 'terminal', options });
    },
    async resumeTracking(commandId, options = {}) {
      return waitForCommandState({ db, sync, commandId, until: options.until || 'terminal', options });
    },
    activeCommandIds() {
      return readActiveCommandIds();
    },
    async getStatus(commandId) {
      const currentDb = await resolveCommandDb(db);
      return findDoc(currentDb?.raw?.business_commands, commandId, { swallowErrors: false });
    },
    subscribe(commandId, observer) {
      return subscribeToCommand({ db, sync, commandId, observer });
    },
    async cancel(commandId, { reason = 'cancelled by user', until = 'terminal' } = {}) {
      const targetCommandId = cleanContextText(commandId);
      if (!targetCommandId) throw commandError('', 'command_id is required for cancellation.', {
        code: 'invalid_transition',
        retryable: false,
      });
      const cancellation = {
        id: `cmd_cancel_${crypto.randomUUID()}`,
        module: 'ctox',
        command_type: 'ctox.command.cancel',
        type: 'ctox.command.cancel',
        record_id: targetCommandId,
        payload: {
          target_command_id: targetCommandId,
          reason: cleanContextText(reason) || 'cancelled by user',
        },
      };
      const receipt = await submitRxdbCommand({ db, sync, session, command: cancellation });
      if (until === 'local') return receipt;
      return waitForCommandState({
        db,
        sync,
        commandId: receipt.command_id,
        until: until === 'accepted' ? 'accepted' : 'terminal',
        options: {},
      });
    },
    async dispatch(command, options = {}) {
      const commandId = command.id || '';
      emitCommandLifecycle(commandId, command.command_type || command.type, 'dispatch_started');
      const receipt = await submitRxdbCommand({ db, sync, session, command });
      emitCommandLifecycle(receipt.command_id, command.command_type || command.type, 'local_receipt');
      const until = options.until || command?.until || 'accepted';
      if (until === 'local') return receipt;
      if (until === 'terminal') {
        return waitForCommandState({
          db,
          sync,
          commandId: receipt.command_id,
          until,
          options: { ...command, ...options },
        });
      }
      if (until !== 'accepted') {
        throw commandError(receipt.command_id, `Unknown command wait target: ${until}`, {
          code: 'invalid_transition',
          retryable: false,
        });
      }
      const accepted = await waitForCommandState({
        db,
        sync,
        commandId: receipt.command_id,
        until,
        options: { ...command, ...options },
      });
      emitCommandLifecycle(receipt.command_id, command.command_type || command.type, 'accepted');
      return accepted;
    },
  };
}

// §9.1 capability token: the native side authorizes commands from a signed
// token rather than the (spoofable) browser-asserted actor. The generated v2
// contract currently requires this capability for every mutation and forbids
// an unauthorised offline intent. Cached until just before expiry.
let capabilityTokenCache = {
  token: null,
  expiresAtMs: 0,
  failureUntilMs: 0,
  failureCode: '',
};

export async function getBusinessOsCapabilityToken({
  timeoutMs = COMMAND_CAPABILITY_TIMEOUT_MS,
} = {}) {
  const now = Date.now();
  if (capabilityTokenCache.token && now < capabilityTokenCache.expiresAtMs - 60_000) {
    return capabilityTokenCache.token;
  }
  if (now < capabilityTokenCache.failureUntilMs) return null;
  const injected = injectedBusinessOsCapabilityToken(now);
  if (injected?.token) {
    capabilityTokenCache = injected;
    return capabilityTokenCache.token;
  }
  const abortController = typeof AbortController === 'function' ? new AbortController() : null;
  try {
    const res = await withTimeout(
      fetch('/api/business-os/auth/capability', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        credentials: 'same-origin',
        cache: 'no-store',
        signal: abortController?.signal,
      }),
      timeoutMs,
      {
        code: 'native_unavailable',
        message: 'Business OS capability request timed out.',
        onTimeout: () => abortController?.abort(),
      },
    );
    if (!res.ok) {
      rememberCapabilityFailure(now, `capability_http_${res.status}`);
      return null;
    }
    const data = await res.json();
    if (data && data.capability_token) {
      capabilityTokenCache = {
        token: data.capability_token,
        expiresAtMs: Number(data.expires_at_ms) || now + 11 * 60 * 60 * 1000,
        failureUntilMs: 0,
        failureCode: '',
      };
      return capabilityTokenCache.token;
    }
    rememberCapabilityFailure(now, 'capability_missing');
  } catch (error) {
    // Control plane unreachable. Remember the failure briefly; command
    // submission remains fail-closed and never falls back to a direct write.
    rememberCapabilityFailure(now, String(error?.code || 'capability_unavailable'));
  }
  return null;
}

export function resetBusinessOsCapabilityTokenCacheForTests() {
  capabilityTokenCache = {
    token: null,
    expiresAtMs: 0,
    failureUntilMs: 0,
    failureCode: '',
  };
}

function rememberCapabilityFailure(now, code) {
  capabilityTokenCache = {
    token: null,
    expiresAtMs: 0,
    failureUntilMs: now + COMMAND_CAPABILITY_NEGATIVE_CACHE_MS,
    failureCode: code,
  };
}

export const getCapabilityToken = getBusinessOsCapabilityToken;

function injectedBusinessOsCapabilityToken(now = Date.now()) {
  const candidates = [
    globalThis.CTOX_BUSINESS_OS_SESSION,
    globalThis.ctoxBusinessOsSession,
    globalThis.ctoxBusinessOsLaunch?.session,
    globalThis.CTOX_DESKTOP_SESSION,
    globalThis.ctoxDesktop?.session,
  ].filter((item) => item && typeof item === 'object');
  for (const candidate of candidates) {
    const token = String(candidate.capability_token || candidate.capabilityToken || '').trim();
    if (!token) continue;
    const expiresAtMs = Number(candidate.capability_expires_at_ms || candidate.capabilityExpiresAtMs || 0)
      || now + 11 * 60 * 60 * 1000;
    if (expiresAtMs <= now + 60_000) continue;
    return { token, expiresAtMs };
  }
  return null;
}

async function submitRxdbCommand({ db, sync, session, command }) {
  const submitStartedAt = Date.now();
  const commandId = command.id || `cmd_${crypto.randomUUID()}`;
  const capabilityToken = await getBusinessOsCapabilityToken();
  emitCommandLifecycle(commandId, command.command_type || command.type, 'capability_resolved', submitStartedAt);
  // Every Business OS command mutates native/domain state. Offline reads stay
  // local-first, but mutation intent without a current server-issued actor
  // capability would be immutable and can never become authorized later.
  // Fail before insertion instead of creating a command that is guaranteed to
  // be rejected after replication.
  if (
    CTOX_COMMAND_AUTHORIZATION.defaultRequirement === 'capability'
    && !CTOX_COMMAND_AUTHORIZATION.offlineIntentAllowed
    && !capabilityToken
  ) {
    throw commandError(commandId, 'Business OS authorization is currently unavailable.', {
      code: 'auth_required',
      retryable: true,
    });
  }
  const doc = await commandDocument(
    command,
    commandId,
    resolveActorContext(command, session),
    capabilityToken,
  );
  const currentDb = await resolveCommandDb(db);
  emitCommandLifecycle(commandId, command.command_type || command.type, 'database_resolved', submitStartedAt);
  const collection = currentDb?.raw?.business_commands;
  if (!collection) throw commandError(commandId, 'business_commands collection is required.');

  const syncPlan = await prepareCommandSync({ db: currentDb, sync, command });
  emitCommandLifecycle(commandId, command.command_type || command.type, 'sync_ready', submitStartedAt);
  try {
    await flushSyncBridges(syncPlan.beforeCommand);
    const localWriteStartedAt = Date.now();
    await insertOrPatchCommandDocument(collection, commandId, doc);
    emitCommandLifecycle(commandId, command.command_type || command.type, 'local_inserted', submitStartedAt);
    recordCommandMetric(sync, 'local_submit', commandId, Date.now() - localWriteStartedAt);
    await flushSyncBridges(syncPlan.submitBridges, [doc]);
    emitCommandLifecycle(commandId, command.command_type || command.type, 'push_confirmed', submitStartedAt);
    recordCommandMetric(sync, 'submit_receipt', commandId, Date.now() - submitStartedAt);
    return {
      ok: true,
      command_id: commandId,
      status: 'local',
      transport: 'rxdb-command-bus',
    };
  } finally {
    await releaseSyncPlan(syncPlan);
  }
}

function emitCommandLifecycle(commandId, commandType, phase, startedAt = 0) {
  const detail = {
    command_id: String(commandId || '').slice(0, 120),
    command_type: String(commandType || '').slice(0, 120),
    phase: String(phase || '').slice(0, 80),
    elapsed_ms: startedAt ? Math.max(0, Date.now() - Number(startedAt)) : 0,
  };
  globalThis.dispatchEvent?.(new CustomEvent('ctox-business-command-lifecycle', { detail }));
  console.info('[command-bus]', JSON.stringify(detail));
}

async function commandDocument(command, commandId, actor, capabilityToken = null) {
  const now = Date.now();
  const commandClientContext = command.client_context && typeof command.client_context === 'object'
    ? command.client_context
    : {};
  const moduleId = String(
    command.module
      || commandClientContext.module
      || commandClientContext.module_id
      || commandClientContext.app_id
      || commandClientContext.source_module
      || 'ctox',
  ).trim() || 'ctox';
  const canonicalCommandType = String(command.command_type || '').trim();
  const legacyCommandType = String(command.type || '').trim();
  if (canonicalCommandType && legacyCommandType && canonicalCommandType !== legacyCommandType) {
    throw commandError(commandId, 'type and command_type must identify the same command.', {
      code: 'invalid_command_contract',
      retryable: false,
    });
  }
  const commandType = canonicalCommandType || legacyCommandType || 'business_os.chat.task';
  const inboundChannel = String(command.inbound_channel || command.client_context?.inbound_channel || moduleId).trim();
  if (!commandType) throw commandError(commandId, 'command_type is required.');
  const recordId = command.record_id || '';
  const dependencies = commandDependencyManifest(command);
  const commandDeadlineAtMs = Number(
    command.deadline_at_ms
      || command.command_deadline_at_ms
      || command.payload?.command_deadline_at_ms
      || 0,
  );
  const clientContext = normalizeCommandClientContext({
    command,
    moduleId,
    commandType,
    recordId,
    inboundChannel,
    actor,
  });
  if (capabilityToken) {
    clientContext.capability_token = capabilityToken;
  }
  const doc = {
    id: commandId,
    command_id: commandId,
    contract_version: 2,
    idempotency_key: String(command.idempotency_key || commandId),
    module: moduleId,
    command_type: commandType,
    record_id: recordId,
    status: 'pending_sync',
    inbound_channel: inboundChannel,
    payload: {
      ...(command.payload || {}),
      inbound_channel: inboundChannel,
      ...(dependencies.length > 0 ? { dependencies } : {}),
      ...(Number.isFinite(commandDeadlineAtMs) && commandDeadlineAtMs > 0
        ? { command_deadline_at_ms: Math.floor(commandDeadlineAtMs) }
        : {}),
    },
    client_context: clientContext,
    created_at_ms: now,
    updated_at_ms: now,
  };
  doc.payload_hash = await payloadHashForCommandDocument(doc);
  return doc;
}

export function normalizeCommandClientContext({
  command = {},
  moduleId = '',
  commandType = '',
  recordId = '',
  inboundChannel = '',
  actor = null,
} = {}) {
  const context = command.client_context && typeof command.client_context === 'object'
    ? { ...command.client_context }
    : {};
  const payload = command.payload && typeof command.payload === 'object'
    ? command.payload
    : {};
  const payloadContext = payload.context && typeof payload.context === 'object'
    ? payload.context
    : {};
  const normalizedModule = cleanContextText(
    context.module || context.module_id || context.app_id || context.source_module || moduleId || command.module || 'ctox',
  ) || 'ctox';
  const normalizedCommandType = cleanContextText(commandType || command.type || command.command_type || 'business_os.chat.task');
  const normalizedRecordId = cleanContextText(context.record_id || recordId || command.record_id || payload.record_id || payloadContext.record_id);
  const normalizedRecordType = cleanContextText(context.record_type || payloadContext.record_type);
  const normalizedMode = cleanContextText(context.mode || payload.mode);
  const normalizedTarget = cleanContextText(context.target || payload.target);
  const normalizedAction = cleanContextText(context.action || normalizedCommandType);

  context.module = cleanContextText(context.module || normalizedModule) || normalizedModule;
  context.module_id = cleanContextText(context.module_id || normalizedModule) || normalizedModule;
  context.source_module = cleanContextText(context.source_module || normalizedModule) || normalizedModule;
  context.app_id = cleanContextText(context.app_id || normalizedModule) || normalizedModule;
  context.command_type = cleanContextText(context.command_type || normalizedCommandType) || normalizedCommandType;
  context.action = normalizedAction;
  if (normalizedMode) context.mode = normalizedMode;
  if (normalizedTarget) context.target = normalizedTarget;
  if (normalizedRecordId) context.record_id = normalizedRecordId;
  if (normalizedRecordType) context.record_type = normalizedRecordType;
  context.inbound_channel = cleanContextText(inboundChannel || context.inbound_channel || normalizedModule) || normalizedModule;
  context.dispatch_transport = 'rxdb-command-bus';
  if (actor && !context.actor) {
    context.actor = actor;
  }
  context.scope = normalizeCommandScope({
    context,
    payloadContext,
    moduleId: normalizedModule,
    commandType: normalizedCommandType,
    recordId: normalizedRecordId,
    recordType: normalizedRecordType,
    mode: normalizedMode,
    target: normalizedTarget,
    action: normalizedAction,
  });
  return context;
}

function normalizeCommandScope({
  context,
  payloadContext,
  moduleId,
  commandType,
  recordId,
  recordType,
  mode,
  target,
  action,
}) {
  const current = context.scope && typeof context.scope === 'object'
    ? { ...context.scope }
    : {};
  if (!current.app || typeof current.app !== 'object') {
    current.app = {};
  }
  current.app.module_id = cleanContextText(current.app.module_id || context.module_id || moduleId);
  current.app.app_id = cleanContextText(current.app.app_id || context.app_id || moduleId);
  if (!current.command || typeof current.command !== 'object') {
    current.command = {};
  }
  current.command.type = cleanContextText(current.command.type || commandType);
  current.command.action = cleanContextText(current.command.action || action || commandType);
  if (mode) current.command.mode = mode;
  if (target) current.command.target = target;

  if (!current.selection || typeof current.selection !== 'object') {
    current.selection = {};
  }
  current.selection.module_id = cleanContextText(current.selection.module_id || context.module_id || moduleId);
  current.selection.column = cleanContextText(current.selection.column || context.column || payloadContext.column || '');
  current.selection.record_type = cleanContextText(current.selection.record_type || recordType || payloadContext.record_type || '');
  current.selection.record_id = cleanContextText(current.selection.record_id || recordId || payloadContext.record_id || '');
  current.selection.label = cleanContextText(current.selection.label || context.label || payloadContext.label || '');

  if (context.visible_scope && typeof context.visible_scope === 'object') {
    current.visible_scope = context.visible_scope;
    current.app = {
      ...current.app,
      ...(context.visible_scope.app && typeof context.visible_scope.app === 'object' ? context.visible_scope.app : {}),
    };
    current.data = context.visible_scope.data && typeof context.visible_scope.data === 'object'
      ? context.visible_scope.data
      : current.data;
    current.external_actions = context.visible_scope.external_actions && typeof context.visible_scope.external_actions === 'object'
      ? context.visible_scope.external_actions
      : current.external_actions;
    current.selection = {
      ...current.selection,
      ...(context.visible_scope.selection && typeof context.visible_scope.selection === 'object'
        ? context.visible_scope.selection
        : {}),
    };
  }
  return current;
}

function resolveActorContext(command, session) {
  if (command?.client_context?.actor) return null;
  const currentSession = typeof session === 'function' ? session() : session;
  const user = currentSession?.user || {};
  const id = String(user.id || '').trim();
  if (!id) return null;
  return {
    id,
    display_name: user.display_name || user.name || id,
    role: user.role || (user.is_admin ? 'admin' : 'user'),
    is_admin: Boolean(user.is_admin),
  };
}

function cleanContextText(value) {
  return String(value ?? '').trim();
}

async function resolveCommandDb(db, timeoutMs = COMMAND_SYNC_READY_TIMEOUT_MS) {
  if (typeof db !== 'function') return db;
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    const current = db();
    if (current?.raw?.business_commands) return current;
    await delay(100);
  }
  return db();
}

async function resolveCommandSync(sync) {
  return typeof sync === 'function' ? sync() : sync;
}

async function prepareCommandSync({ db, sync, command = null }) {
  const currentSync = await resolveCommandSync(sync);
  const dependencyCollections = commandDependencySyncCollections(command);
  const queueProjectionRequired = command?.sync_queue_tasks !== false;
  const readyTimeoutMs = commandSyncReadyTimeoutMs(command);
  const leases = [];
  try {
    const dependencyBridges = await Promise.all(
      dependencyCollections.map((collection) => startScopedSyncCollection(
        currentSync,
        collection,
        `command-dependency:${command?.type || command?.command_type || 'unknown'}`,
        leases,
      )),
    );
    const commandBridge = await startScopedSyncCollection(
      currentSync,
      'business_commands',
      `command-core:${command?.type || command?.command_type || 'unknown'}`,
      leases,
    );
    const queueBridge = queueProjectionRequired
      ? await startScopedSyncCollection(
        currentSync,
        'ctox_queue_tasks',
        `command-queue:${command?.type || command?.command_type || 'unknown'}`,
        leases,
      )
      : null;
    const submitBridges = [commandBridge].filter(Boolean);
    const afterCommand = [commandBridge, queueBridge].filter(Boolean);
    await Promise.all(
      [...dependencyBridges, ...submitBridges].map((bridge) => (
        waitForSyncBridgeReady(bridge, readyTimeoutMs)
      )),
    );
    return {
      beforeCommand: dependencyBridges,
      submitBridges,
      afterCommand,
      leases,
      sync: currentSync,
    };
  } catch (error) {
    await releaseSyncLeases(leases);
    throw error;
  }
}

function commandSyncReadyTimeoutMs(command) {
  const explicit = Number(command?.sync_ready_timeout_ms || 0);
  if (Number.isFinite(explicit) && explicit > 0) {
    return Math.max(25, Math.min(COMMAND_SYNC_READY_TIMEOUT_MS, explicit));
  }
  const deadlineAtMs = Number(
    command?.deadline_at_ms
      || command?.command_deadline_at_ms
      || command?.payload?.command_deadline_at_ms
      || 0,
  );
  if (Number.isFinite(deadlineAtMs) && deadlineAtMs > Date.now()) {
    return Math.max(25, Math.min(COMMAND_SYNC_READY_TIMEOUT_MS, deadlineAtMs - Date.now()));
  }
  return COMMAND_SYNC_READY_TIMEOUT_MS;
}

async function startScopedSyncCollection(sync, collection, reason, leases) {
  if (typeof sync?.leaseCollection === 'function') {
    const lease = await sync.leaseCollection(collection, reason);
    leases.push(lease);
    return lease;
  }
  if (DEMAND_ONLY_SYNC_COLLECTIONS.has(collection)) {
    throw new Error(`${collection} requires sync.leaseCollection().`);
  }
  return sync?.startCollection?.(collection);
}

async function releaseSyncPlan(syncPlan) {
  await releaseSyncLeases(syncPlan?.leases || []);
}

async function releaseSyncLeases(leases) {
  await Promise.all((leases || []).map((lease) => lease?.release?.().catch(() => null)));
}

function commandDependencySyncCollections(command) {
  const collections = new Set();
  for (const dependency of commandDependencyManifest(command)) {
    const normalized = cleanContextText(dependency.collection);
    if (normalized) collections.add(normalized);
  }
  for (const collection of command?.sync_collections || []) {
    const normalized = cleanContextText(collection);
    if (normalized && normalized !== 'business_commands' && normalized !== 'ctox_queue_tasks') {
      collections.add(normalized);
    }
  }
  if (commandUsesDesktopFileMetadata(command)) {
    collections.add('desktop_files');
  }
  if (commandUsesDesktopFileChunks(command)) {
    collections.add('desktop_file_chunks');
  }
  return [...collections];
}

function commandDependencyManifest(command) {
  const payload = command?.payload && typeof command.payload === 'object' ? command.payload : {};
  const explicit = Array.isArray(command?.dependencies)
    ? command.dependencies
    : (Array.isArray(payload.dependencies) ? payload.dependencies : []);
  const dependencies = explicit.map((dependency) => normalizeCommandDependency(dependency)).filter(Boolean);
  const known = new Set(dependencies.map((dependency) => `${dependency.collection}:${dependency.record_id}`));
  const add = (dependency) => {
    const normalized = normalizeCommandDependency(dependency);
    if (!normalized) return;
    const key = `${normalized.collection}:${normalized.record_id}`;
    if (known.has(key)) return;
    known.add(key);
    dependencies.push(normalized);
  };
  const sourceFileId = cleanContextText(payload.source_file_id || payload.file_id);
  if (sourceFileId) {
    add({
      collection: 'desktop_files',
      record_id: sourceFileId,
      generation_id: payload.generation_id,
      content_hash: payload.content_hash || payload.sha256,
      required: true,
    });
  }
  for (const attachment of desktopFileAttachmentRefs(payload)) {
    const fileId = cleanContextText(attachment.file_id || attachment.fileId);
    if (fileId) {
      add({
        collection: 'desktop_files',
        record_id: fileId,
        generation_id: attachment.generation_id || attachment.generationId,
        content_hash: attachment.content_hash || attachment.contentHash || attachment.sha256,
        required: attachment.required !== false,
      });
    }
    const chunkId = cleanContextText(attachment.chunk_id || attachment.chunkId);
    if (chunkId) {
      add({
        collection: cleanContextText(attachment.chunk_collection || attachment.chunkCollection) || 'desktop_file_chunks',
        record_id: chunkId,
        generation_id: attachment.generation_id || attachment.generationId,
        content_hash: attachment.content_hash || attachment.contentHash || attachment.sha256,
        required: attachment.required !== false,
      });
    }
  }
  return dependencies;
}

function normalizeCommandDependency(dependency) {
  if (!dependency || typeof dependency !== 'object') return null;
  const collection = cleanContextText(dependency.collection);
  const recordId = cleanContextText(dependency.record_id || dependency.recordId || dependency.id);
  if (!collection || !recordId) return null;
  const normalized = {
    collection,
    record_id: recordId,
    required: dependency.required !== false,
  };
  const generationId = cleanContextText(dependency.generation_id || dependency.generationId);
  const contentHash = cleanContextText(dependency.content_hash || dependency.contentHash || dependency.sha256);
  if (generationId) normalized.generation_id = generationId;
  if (contentHash) normalized.content_hash = contentHash;
  return normalized;
}

function commandUsesDesktopFileMetadata(command) {
  const payload = command?.payload && typeof command.payload === 'object' ? command.payload : {};
  if (desktopFileAttachmentRefs(payload).some((item) => cleanContextText(item.file_id || item.fileId))) {
    return true;
  }
  return Boolean(cleanContextText(payload.source_file_id || payload.file_id));
}

function commandUsesDesktopFileChunks(command) {
  const payload = command?.payload && typeof command.payload === 'object' ? command.payload : {};
  const commandType = cleanContextText(command?.type || command?.command_type || '');
  const fileId = cleanContextText(payload.source_file_id || payload.file_id);
  if (fileId && (
    cleanContextText(payload.source_kind) === 'zip'
    || cleanContextText(payload.generation_id)
    || commandType.includes('install')
    || commandType.includes('parse')
  )) {
    return true;
  }
  return desktopFileAttachmentRefs(payload).some((item) => (
    cleanContextText(item.chunk_collection || item.chunkCollection) === 'desktop_file_chunks'
    || cleanContextText(item.storage_collection || item.storageCollection) === 'desktop_file_chunks'
    || cleanContextText(item.chunk_id || item.chunkId)
    || Number(item.chunk_count || item.chunkCount || 0) > 0
  ));
}

function desktopFileAttachmentRefs(payload) {
  return [
    ...(Array.isArray(payload.attachments) ? payload.attachments : []),
    ...(Array.isArray(payload.attachment_refs) ? payload.attachment_refs : []),
  ].filter((item) => (
    item
    && typeof item === 'object'
    && cleanContextText(item.kind || 'desktop_file') === 'desktop_file'
  ));
}

async function insertOrPatchCommandDocument(collection, commandId, doc) {
  try {
    await collection.insert(doc);
    return;
  } catch (error) {
    if (!isRxDbConflictError(error)) throw error;
  }
  const existingDoc = await collection.findOne(commandId).exec();
  const existing = existingDoc?.toJSON?.() || existingDoc || null;
  if (!existing) {
    await collection.insert(doc);
    return;
  }
  const existingHash = String(existing.payload_hash || await payloadHashForCommandDocument(existing));
  if (existingHash !== doc.payload_hash) {
    throw commandError(commandId, 'The command id is already bound to a different immutable payload.', {
      code: 'idempotency_conflict',
      retryable: false,
    });
  }
}

async function payloadHashForCommandDocument(document) {
  const clientContext = document?.client_context && typeof document.client_context === 'object'
    ? { ...document.client_context }
    : {};
  delete clientContext.capability_token;
  const immutable = {
    command_id: String(document?.command_id || document?.id || ''),
    idempotency_key: String(document?.idempotency_key || document?.command_id || document?.id || ''),
    module: String(document?.module || ''),
    command_type: String(document?.command_type || document?.type || ''),
    record_id: String(document?.record_id || ''),
    payload: document?.payload || {},
    client_context: clientContext,
  };
  const subtle = globalThis.crypto?.subtle;
  if (!subtle || typeof TextEncoder !== 'function') {
    throw commandError(immutable.command_id, 'SHA-256 is unavailable for command idempotency.', {
      code: 'sync_unavailable',
      retryable: false,
    });
  }
  const digest = await subtle.digest('SHA-256', new TextEncoder().encode(canonicalJson(immutable)));
  const hex = [...new Uint8Array(digest)]
    .map((byte) => byte.toString(16).padStart(2, '0'))
    .join('');
  return `sha256:${hex}`;
}

function canonicalJson(value) {
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(',')}]`;
  if (value && typeof value === 'object') {
    return `{${Object.keys(value).sort().map((key) => `${JSON.stringify(key)}:${canonicalJson(value[key])}`).join(',')}}`;
  }
  return JSON.stringify(value);
}

function commandWaitTimeoutMs(options) {
  const raw = options?.timeoutMs
    ?? options?.timeout_ms
    ?? options?.wait_timeout_ms
    ?? options?.client_context?.command_wait_timeout_ms
    ?? options?.client_context?.wait_timeout_ms;
  const parsed = Number(raw);
  if (!Number.isFinite(parsed) || parsed <= 0) return COMMAND_ACCEPT_TIMEOUT_MS;
  return Math.min(Math.max(parsed, 1000), 10 * 60 * 1000);
}

function subscribeToCommand({ db, sync, commandId, observer }) {
  let closed = false;
  let subscription = null;
  const releaseWatcher = reserveCommandWatcher(commandId);
  recordCommandMetric(sync, 'watcher_started', commandId);
  const ready = Promise.resolve()
    .then(() => resolveCommandDb(db))
    .then((currentDb) => {
      if (closed) return null;
      const collection = currentDb?.raw?.business_commands;
      if (!collection?.findOne) {
        throw commandError(commandId, 'business_commands collection is required.', {
          code: 'sync_unavailable',
          retryable: true,
        });
      }
      const stream = collection.findOne(commandId)?.$;
      if (!stream?.subscribe) {
        throw commandError(commandId, 'business_commands does not support reactive tracking.', {
          code: 'sync_unavailable',
          retryable: true,
        });
      }
      subscription = stream.subscribe((value) => {
        if (typeof observer === 'function') observer(value);
        else observer?.next?.(value);
        const command = value?.toJSON?.() || value;
        if (commandIsTerminal(command)) {
          closed = true;
          subscription?.unsubscribe?.();
          releaseWatcher();
        }
      });
      return subscription;
    })
    .catch((error) => {
      releaseWatcher();
      throw error;
    });
  return {
    ready,
    unsubscribe() {
      closed = true;
      subscription?.unsubscribe?.();
      releaseWatcher();
    },
  };
}

async function waitForCommandState({ db, sync, commandId, until, options = {} }) {
  const releaseWatcher = reserveCommandWatcher(commandId);
  const timeoutMs = commandWaitTimeoutMs(options);
  let currentDb = null;
  let syncPlan = null;
  let lastCommand = null;
  let subscription = null;
  let boundRawDb = null;
  rememberActiveCommandId(commandId);
  try {
    currentDb = await resolveCommandDb(db);
    syncPlan = await prepareCommandSync({ db: currentDb, sync, command: options });
    return await new Promise((resolve, reject) => {
      let settled = false;
      let rebindInFlight = false;
      const settle = (handler, value) => {
        if (settled) return;
        settled = true;
        clearTimeout(timeout);
        clearInterval(rebindTimer);
        subscription?.unsubscribe?.();
        if (commandIsTerminal(lastCommand)) forgetActiveCommandId(commandId);
        handler(value);
      };
      const inspect = (value) => {
        if (settled || !value) return;
        lastCommand = value?.toJSON?.() || value;
        if (commandIsFailed(lastCommand)) {
          const outcome = lastCommand.result?.outcome || lastCommand.payload?.outcome || null;
          settle(reject, commandError(
            commandId,
            lastCommand.error_message || lastCommand.error || outcome?.stderr || outcome?.error || 'CTOX command failed.',
            {
              code: lastCommand.error_code || 'command_terminal_failure',
              retryable: Boolean(lastCommand.retryable),
            },
          ));
          return;
        }
        if (!commandHasReached(lastCommand, until)) return;
        recordObservedCommandMetrics(sync, commandId, lastCommand);
        settle(resolve, commandReceipt(lastCommand, commandId));
      };
      const bind = async () => {
        if (settled || rebindInFlight) return;
        rebindInFlight = true;
        try {
          currentDb = await resolveCommandDb(db);
          const commands = currentDb?.raw?.business_commands;
          if (!commands) throw commandError(commandId, 'business_commands collection is required.', {
            code: 'sync_unavailable', retryable: true,
          });
          if (boundRawDb !== currentDb.raw) {
            subscription?.unsubscribe?.();
            boundRawDb = currentDb.raw;
            const stream = commands.findOne(commandId)?.$;
            if (!stream?.subscribe) throw commandError(commandId, 'Reactive command tracking is unavailable.', {
              code: 'sync_unavailable', retryable: true,
            });
            subscription = stream.subscribe(inspect);
            recordCommandMetric(sync, 'watcher_started', commandId);
          }
          // Close the subscribe/read race and every data-plane rebind window.
          inspect(await findDoc(commands, commandId, { swallowErrors: false }));
        } catch (error) {
          settle(reject, error);
        } finally {
          rebindInFlight = false;
        }
      };
      const timeout = setTimeout(() => {
        recordCommandMetric(sync, 'wait_timeout', commandId, timeoutMs);
        settle(reject, commandError(
          commandId,
          'CTOX wartet noch auf die Rueckmeldung. Der Vorgang bleibt verfolgbar.',
          {
            code: 'projection_delayed',
            status: 'projection_pending',
            transient: true,
            retryable: true,
            receipt: lastCommand ? commandReceipt(lastCommand, commandId) : { command_id: commandId },
          },
        ));
      }, timeoutMs);
      const rebindTimer = setInterval(() => {
        bind();
        refreshProjectionBridges(syncPlan?.afterCommand).catch(() => {});
      }, 1500);
      bind();
    });
  } finally {
    subscription?.unsubscribe?.();
    releaseWatcher();
    await releaseSyncPlan(syncPlan);
  }
}

function reserveCommandWatcher(commandId) {
  if (activeCommandWatcherCount >= MAX_SIMULTANEOUS_COMMAND_WATCHERS) {
    throw commandError(commandId, 'Too many simultaneous command watchers.', {
      code: 'projection_delayed',
      transient: true,
      retryable: true,
    });
  }
  activeCommandWatcherCount += 1;
  let released = false;
  return () => {
    if (released) return;
    released = true;
    activeCommandWatcherCount = Math.max(0, activeCommandWatcherCount - 1);
  };
}

function recordObservedCommandMetrics(sync, commandId, command) {
  const createdAtMs = Number(command.created_at_ms || 0);
  if (command.replication_phase === 'native_observed' && createdAtMs > 0) {
    recordCommandMetric(sync, 'submit_to_native_observed', commandId, Math.max(0, Date.now() - createdAtMs));
  }
  if (commandIsTerminal(command)) {
    const terminalAtMs = Number(command.updated_at_ms || 0);
    if (terminalAtMs > 0) {
      recordCommandMetric(sync, 'terminal_to_browser_observed', commandId, Math.max(0, Date.now() - terminalAtMs));
    }
  }
}

const ACTIVE_COMMAND_STORAGE_KEY = 'ctox.businessOs.activeCommandIds.v1';
const MAX_ACTIVE_COMMAND_IDS = 128;

function readActiveCommandIds() {
  try {
    const ids = JSON.parse(globalThis.localStorage?.getItem?.(ACTIVE_COMMAND_STORAGE_KEY) || '[]');
    return Array.isArray(ids) ? ids.map(String).filter(Boolean).slice(-MAX_ACTIVE_COMMAND_IDS) : [];
  } catch {
    return [];
  }
}

function rememberActiveCommandId(commandId) {
  const ids = readActiveCommandIds().filter((id) => id !== commandId);
  ids.push(commandId);
  try { globalThis.localStorage?.setItem?.(ACTIVE_COMMAND_STORAGE_KEY, JSON.stringify(ids.slice(-MAX_ACTIVE_COMMAND_IDS))); } catch {}
}

function forgetActiveCommandId(commandId) {
  const ids = readActiveCommandIds().filter((id) => id !== commandId);
  try { globalThis.localStorage?.setItem?.(ACTIVE_COMMAND_STORAGE_KEY, JSON.stringify(ids)); } catch {}
}

function recordCommandMetric(sync, name, commandId, durationMs) {
  sync?.recordCommandMetric?.({ name, commandId, durationMs });
}

function commandHasReached(command, until) {
  if (until === 'terminal') return commandIsTerminal(command);
  return commandIsTerminal(command)
    || command.replication_phase === 'native_observed'
    || ['accepted', 'waiting_dependencies', 'queued', 'leased', 'running', 'awaiting_review', 'validating', 'retry_wait', 'blocked']
      .includes(String(command.execution_phase || command.status || ''));
}

function commandIsTerminal(command) {
  return command?.execution_phase === 'terminal'
    || ['completed', 'failed', 'cancelled'].includes(String(command?.terminal_status || command?.status || ''));
}

function commandIsFailed(command) {
  if (!command) return false;
  if (command.terminal_status === 'failed' || command.status === 'failed') return true;
  const outcome = command.result?.outcome || command.payload?.outcome || null;
  return outcome?.ok === false || Number(outcome?.exit_code || 0) !== 0;
}

function commandReceipt(command, commandId) {
  const executionTaskId = String(command?.execution_task_id || '').trim();
  const compatibilityTaskId = String(command?.task_id || '').trim();
  const taskId = executionTaskId || (
    command?.execution_mode === 'control' || commandIsTerminal(command)
      ? ''
      : compatibilityTaskId
  );
  return {
    ok: !commandIsFailed(command),
    command_id: commandId,
    status: String(command?.status || command?.execution_phase || 'accepted'),
    execution_mode: command?.execution_mode || null,
    execution_task_id: taskId,
    task_id: taskId,
    target_task_id: command?.target_task_id || (
      taskId ? '' : compatibilityTaskId
    ),
    target_record_id: command?.target_record_id || command?.record_id || '',
    task_status: String(command?.task_status || command?.status || ''),
    payload: command?.payload || null,
    result: command?.result || null,
    transport: 'rxdb-command-bus',
  };
}

async function findDoc(collection, id, { swallowErrors = true } = {}) {
  if (!collection?.findOne || !id) return null;
  let doc;
  try {
    doc = await collection.findOne(id).exec();
  } catch (error) {
    if (swallowErrors) return null;
    throw error;
  }
  return doc?.toJSON?.() || doc || null;
}

async function waitForSyncBridgeReady(bridge, timeoutMs) {
  const resolvedBridge = syncBridgeFromHandle(bridge);
  const state = resolvedBridge?.state;
  const collection = cleanContextText(
    bridge?.collection || resolvedBridge?.collection || state?.collection?.name,
  ) || 'unknown';
  if (!state) {
    if (resolvedBridge?.mode === 'pending' || resolvedBridge?.mode === 'paused') {
      throw commandError('', `CTOX Sync Engine collection "${collection}" is ${resolvedBridge.mode}.`, {
        code: 'sync_unavailable',
        retryable: true,
      });
    }
    return;
  }
  // Command submission needs an authenticated, open native peer so the new
  // local row can be pushed; it must not wait for the complete historical
  // pull of business_commands. A mature instance can contain many thousands
  // of immutable command records, making that cold pull much longer than the
  // command deadline even though the transport is already usable.
  if (syncBridgeHasPeerStatus(state)) {
    await waitForConnectedSyncPeer(state, collection, timeoutMs);
    return;
  }
  await withTimeout(
    () => state.awaitInSync?.() || state.awaitInitialReplication?.(),
    timeoutMs,
    {
      code: 'native_unavailable',
      message: `CTOX Sync Engine collection "${collection}" did not become ready before the command deadline.`,
    },
  );
}

function syncBridgeHasPeerStatus(state) {
  return typeof state?.getTransportStatus === 'function'
    || Boolean(state?.demandStatus)
    || Boolean(state?.peerStates$)
    || Boolean(state?.active$)
    || Boolean(state?.transportStatus$);
}

function waitForConnectedSyncPeer(state, collection, timeoutMs) {
  return new Promise((resolve, reject) => {
    let settled = false;
    let lastStatus = syncBridgeStatus(state);
    const subscriptions = [];
    const finish = (handler, value) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      clearInterval(pollTimer);
      subscriptions.forEach((subscription) => subscription?.unsubscribe?.());
      handler(value);
    };
    const inspect = () => {
      if (settled) return;
      lastStatus = syncBridgeStatus(state);
      if (state.cancelled || state.canceled$?.getValue?.() === true) {
        finish(reject, commandError('', `CTOX Sync Engine collection "${collection}" was cancelled.`, {
          code: 'sync_unavailable',
          retryable: true,
        }));
        return;
      }
      if (syncBridgePeerConnected(state, lastStatus)) finish(resolve);
    };
    const timer = setTimeout(() => {
      const summary = syncBridgeStatusSummary(lastStatus);
      finish(reject, commandError(
        '',
        `CTOX Sync Engine collection "${collection}" has no authenticated WebRTC peer after ${timeoutMs} ms (${summary}).`,
        { code: 'native_unavailable', retryable: true },
      ));
    }, timeoutMs);
    const pollTimer = setInterval(inspect, 50);
    for (const observable of [state.peerStates$, state.active$, state.transportStatus$, state.canceled$]) {
      if (settled) break;
      const subscription = observable?.subscribe?.(inspect);
      if (!subscription) continue;
      if (settled) subscription.unsubscribe?.();
      else subscriptions.push(subscription);
    }
    inspect();
  });
}

function syncBridgeStatus(state) {
  try {
    return state?.getTransportStatus?.() || state?.transportStatus$?.getValue?.() || {};
  } catch {
    return {};
  }
}

function syncBridgePeerConnected(state, status = syncBridgeStatus(state)) {
  if (state?.demandStatus?.peerConnected === true) return true;
  if (status?.demandLoading?.peerConnected === true || status?.peerConnected === true) return true;
  const peerStates = state?.peerStates$?.getValue?.();
  if (peerStates instanceof Map && peerStates.size > 0) return true;
  if (Array.isArray(peerStates) && peerStates.length > 0) return true;
  if (state?.active$?.getValue?.() === true && cleanContextText(state?.activeRemotePeerId)) return true;
  return Array.isArray(status?.connectionStates)
    && status.connectionStates.some((connection) => {
      const channelState = connection?.channelState || connection?.channelReadyState || '';
      const peerState = connection?.peerConnectionState || '';
      return connection?.open === true
        || (channelState === 'open' && !['closed', 'failed', 'disconnected'].includes(peerState));
    });
}

function syncBridgeStatusSummary(status) {
  const activePeerCount = Number(status?.activePeerCount || 0);
  const connectionCount = Number(status?.connectionCount || 0);
  const demandPeer = status?.demandLoading?.peerConnected === true ? 'connected' : 'not-connected';
  return `active peers: ${activePeerCount}, connections: ${connectionCount}, collection peer: ${demandPeer}`;
}

async function flushSyncBridges(bridges, documents = []) {
  await Promise.all((bridges || []).map((bridge) => flushSyncBridge(bridge, documents)));
}

async function flushSyncBridge(bridge, documents = []) {
  const resolvedBridge = syncBridgeFromHandle(bridge);
  if (resolvedBridge?.mode === 'follower') {
    if (typeof resolvedBridge.flush !== 'function') {
      throw commandError('', 'Multi-tab sync follower cannot confirm the leader push.', {
        code: 'sync_unavailable',
        retryable: true,
      });
    }
    await withTimeout(
      () => resolvedBridge.flush(),
      COMMAND_SYNC_FLUSH_TIMEOUT_MS,
      {
        code: 'sync_unavailable',
        message: 'CTOX Sync Engine leader did not confirm the command push before the deadline.',
      },
    );
    return;
  }
  const state = resolvedBridge?.state;
  if (!state) return;
  await withTimeout(
    () => {
      if (documents.length && typeof state.pushDocumentsToRemotePeers === 'function') {
        return state.pushDocumentsToRemotePeers(documents);
      }
      if (typeof state.pushToRemotePeers === 'function') return state.pushToRemotePeers();
      return state.awaitInSync?.();
    },
    COMMAND_SYNC_FLUSH_TIMEOUT_MS,
    {
      code: 'sync_unavailable',
      message: 'CTOX Sync Engine could not push command dependencies before the deadline.',
    },
  );
}

async function refreshProjectionBridges(bridges) {
  await Promise.all((bridges || []).map((bridge) => refreshProjectionBridge(bridge)));
}

async function refreshProjectionBridge(bridge) {
  const state = syncBridgeFromHandle(bridge)?.state;
  if (!state) return;
  await withTimeout(
    () => {
      if (typeof state.pullFromRemotePeers === 'function') return state.pullFromRemotePeers();
      return state.awaitInSync?.();
    },
    COMMAND_SYNC_FLUSH_TIMEOUT_MS,
    {
      code: 'projection_delayed',
      message: 'CTOX command projection refresh exceeded its deadline.',
    },
  );
}

function syncBridgeFromHandle(handle) {
  return handle?.bridge || handle;
}

function commandError(commandId, message, options = {}) {
  const error = new Error(message);
  error.command_id = commandId;
  error.code = options.code || 'command_terminal_failure';
  error.status = options.status || 'failed';
  error.transient = Boolean(options.transient);
  error.retryable = Boolean(options.retryable);
  if (options.receipt) error.receipt = options.receipt;
  return error;
}

function isRxDbConflictError(error) {
  const message = String(error?.message || error || '');
  return message.includes('RxDB Error-Code: CONFLICT')
    || message.includes('conflict')
    || message.includes('document already exists')
    || message.includes('Document update conflict');
}

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
