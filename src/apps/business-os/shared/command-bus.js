const COMMAND_ACCEPT_TIMEOUT_MS = 45000;
const COMMAND_SYNC_READY_TIMEOUT_MS = 15000;
const COMMAND_SYNC_FLUSH_TIMEOUT_MS = 15000;

export function createCommandBus({ db, sync = null, session = null } = {}) {
  return {
    async dispatch(command) {
      return dispatchRxdbCommand({ db, sync, session, command });
    },
  };
}

// §9.1 capability token: the native side authorizes commands from a signed
// token rather than the (spoofable) browser-asserted actor. We fetch a token
// bound to the logged-in session from the control plane and attach it to every
// command. Cached until just before expiry; if the control plane is unreachable
// we dispatch without it (legacy claimed-actor path) so the UI keeps working.
let capabilityTokenCache = { token: null, expiresAtMs: 0 };

async function getCapabilityToken() {
  const now = Date.now();
  if (capabilityTokenCache.token && now < capabilityTokenCache.expiresAtMs - 60_000) {
    return capabilityTokenCache.token;
  }
  try {
    const res = await fetch('/api/business-os/auth/capability', {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
    });
    if (!res.ok) return null;
    const data = await res.json();
    if (data && data.capability_token) {
      capabilityTokenCache = {
        token: data.capability_token,
        expiresAtMs: Number(data.expires_at_ms) || now + 11 * 60 * 60 * 1000,
      };
      return capabilityTokenCache.token;
    }
  } catch {
    // control plane unreachable — degrade to the legacy path
  }
  return null;
}

async function dispatchRxdbCommand({ db, sync, session, command }) {
  const commandId = command.id || `cmd_${crypto.randomUUID()}`;
  const capabilityToken = await getCapabilityToken();
  const doc = commandDocument(
    command,
    commandId,
    resolveActorContext(command, session),
    capabilityToken,
  );
  const currentDb = await resolveCommandDb(db);
  const collection = currentDb?.raw?.business_commands;
  if (!collection) throw commandError(commandId, 'business_commands collection is required.');

  const syncPlan = await prepareCommandSync({ db: currentDb, sync, command });
  await flushSyncBridges(syncPlan.beforeCommand);
  await insertOrPatchCommandDocument(collection, commandId, doc);
  await flushSyncBridges(syncPlan.afterCommand);
  return waitForAuthoritativeQueueProjection(currentDb, commandId, commandWaitTimeoutMs(command));
}

function commandDocument(command, commandId, actor, capabilityToken = null) {
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
  const commandType = String(command.type || command.command_type || 'business_os.chat.task').trim();
  const inboundChannel = String(command.inbound_channel || command.client_context?.inbound_channel || moduleId).trim();
  if (!commandType) throw commandError(commandId, 'command_type is required.');
  const recordId = command.record_id || '';
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
  return {
    id: commandId,
    command_id: commandId,
    module: moduleId,
    command_type: commandType,
    record_id: recordId,
    status: 'pending_sync',
    inbound_channel: inboundChannel,
    payload: {
      ...(command.payload || {}),
      inbound_channel: inboundChannel,
    },
    client_context: clientContext,
    created_at_ms: now,
    updated_at_ms: now,
  };
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
  const dependencyBridges = await Promise.all(
    dependencyCollections.map((collection) => currentSync?.startCollection?.(collection)),
  );
  const commandBridge = await currentSync?.startCollection?.('business_commands');
  const queueBridge = await currentSync?.startCollection?.('ctox_queue_tasks');
  const afterCommand = [commandBridge, queueBridge];
  await Promise.all(
    [...dependencyBridges, ...afterCommand].map((bridge) => (
      waitForSyncBridgeReady(bridge, COMMAND_SYNC_READY_TIMEOUT_MS)
    )),
  );
  return {
    beforeCommand: dependencyBridges,
    afterCommand,
  };
}

function commandDependencySyncCollections(command) {
  const collections = new Set();
  for (const collection of command?.sync_collections || []) {
    const normalized = cleanContextText(collection);
    if (normalized && normalized !== 'business_commands' && normalized !== 'ctox_queue_tasks') {
      collections.add(normalized);
    }
  }
  if (commandUsesDesktopFileAttachments(command)) {
    collections.add('desktop_files');
    collections.add('desktop_file_chunks');
  }
  return [...collections];
}

function commandUsesDesktopFileAttachments(command) {
  const payload = command?.payload && typeof command.payload === 'object' ? command.payload : {};
  const attachmentRefs = [
    ...(Array.isArray(payload.attachments) ? payload.attachments : []),
    ...(Array.isArray(payload.attachment_refs) ? payload.attachment_refs : []),
  ];
  if (attachmentRefs.some((item) => (
    item
    && typeof item === 'object'
    && cleanContextText(item.kind || 'desktop_file') === 'desktop_file'
    && cleanContextText(item.file_id || item.fileId)
  ))) {
    return true;
  }
  return Boolean(cleanContextText(payload.source_file_id || payload.file_id));
}

async function insertOrPatchCommandDocument(collection, commandId, doc) {
  try {
    await collection.insert(doc);
    return;
  } catch (error) {
    if (!isRxDbConflictError(error)) throw error;
  }
  const existing = await collection.findOne(commandId).exec();
  if (existing) {
    await existing.incrementalPatch(doc);
  } else {
    await collection.insert(doc);
  }
}

function commandWaitTimeoutMs(command) {
  const raw = command?.wait_timeout_ms
    ?? command?.client_context?.command_wait_timeout_ms
    ?? command?.client_context?.wait_timeout_ms;
  const parsed = Number(raw);
  if (!Number.isFinite(parsed) || parsed <= 0) return COMMAND_ACCEPT_TIMEOUT_MS;
  return Math.min(Math.max(parsed, 1000), 10 * 60 * 1000);
}

async function waitForAuthoritativeQueueProjection(db, commandId, timeoutMs = COMMAND_ACCEPT_TIMEOUT_MS) {
  const commands = db?.raw?.business_commands;
  const queue = db?.raw?.ctox_queue_tasks;
  const deadline = Date.now() + timeoutMs;
  let lastCommand = null;
  while (Date.now() < deadline) {
    lastCommand = await findDoc(commands, commandId);
    const taskId = String(lastCommand?.task_id || '').trim();
    const task = taskId ? await findDoc(queue, taskId) : null;
    if (lastCommand?.status === 'failed') {
      const outcome = lastCommand.result?.outcome || lastCommand.payload?.outcome || null;
      throw commandError(
        commandId,
        lastCommand.error || outcome?.stderr || outcome?.error || 'CTOX command failed.',
      );
    }
    const directOutcome = lastCommand?.result?.outcome || lastCommand?.payload?.outcome || null;
    if (!taskId && directOutcome && (directOutcome.ok !== undefined || directOutcome.exit_code !== undefined)) {
      if (directOutcome.ok === false || Number(directOutcome.exit_code || 0) !== 0) {
        throw commandError(
          commandId,
          lastCommand.error || directOutcome.stderr || directOutcome.error || 'CTOX command failed.',
        );
      }
      return {
        ok: true,
        command_id: commandId,
        status: String(lastCommand?.status || 'completed'),
        task_id: '',
        task_status: String(lastCommand?.task_status || lastCommand?.status || 'completed'),
        payload: lastCommand?.payload || null,
        result: lastCommand?.result || null,
        transport: 'rxdb-command-bus',
      };
    }
    if (taskId && task) {
      return {
        ok: true,
        command_id: commandId,
        status: String(lastCommand?.status || 'accepted'),
        task_id: taskId,
        task_status: String(task.status || lastCommand?.task_status || 'queued'),
        payload: lastCommand?.payload || null,
        result: lastCommand?.result || null,
        transport: 'rxdb-command-bus',
      };
    }
    // Control commands (ctox.file.materialize, ctox.module.*, ...) are
    // executed directly by the daemon and acknowledged with a terminal
    // 'completed' command document that intentionally carries NO queue-task
    // projection (write_rxdb_control_command_outcome stamps an empty
    // task_id). That acknowledgement IS the authoritative result — waiting
    // for a task here timed out after 45s for every control command.
    // Queue-backed commands always carry a task_id alongside their status.
    if (!taskId && lastCommand?.status === 'completed') {
      return {
        ok: true,
        command_id: commandId,
        status: 'completed',
        task_id: '',
        task_status: String(lastCommand?.task_status || 'completed'),
        payload: lastCommand?.payload || null,
        result: lastCommand?.result || null,
        transport: 'rxdb-command-bus',
      };
    }
    await delay(250);
  }
  throw commandError(
    commandId,
    'CTOX hat aus diesem RxDB Command keinen echten Queue-Task zurueckprojiziert.',
  );
}

async function findDoc(collection, id) {
  if (!collection?.findOne || !id) return null;
  const doc = await collection.findOne(id).exec().catch(() => null);
  return doc?.toJSON?.() || doc || null;
}

async function waitForSyncBridgeReady(bridge, timeoutMs) {
  const state = bridge?.state;
  if (!state) return;
  await Promise.race([
    Promise.resolve()
      .then(() => state.awaitInSync?.() || state.awaitInitialReplication?.())
      .catch(() => {}),
    delay(timeoutMs),
  ]);
}

async function flushSyncBridges(bridges) {
  await Promise.all((bridges || []).map((bridge) => flushSyncBridge(bridge)));
}

async function flushSyncBridge(bridge) {
  const state = bridge?.state;
  if (!state) return;
  await Promise.race([
    Promise.resolve()
      .then(() => {
        if (typeof state.pushToRemotePeers === 'function') return state.pushToRemotePeers();
        return state.awaitInSync?.();
      })
      .catch(() => {}),
    delay(COMMAND_SYNC_FLUSH_TIMEOUT_MS),
  ]);
}

function commandError(commandId, message) {
  const error = new Error(message);
  error.command_id = commandId;
  error.status = 'failed';
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
