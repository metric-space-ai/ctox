const COMMAND_SYNC_PUSH_TIMEOUT_MS = 8000;
const COMMAND_SYNC_READY_TIMEOUT_MS = 5000;

export function createCommandBus({ db, sync = null } = {}) {
  return {
    async dispatch(command) {
      return recordRxdbCommand({ db, sync, command });
    },
  };
}

async function resolveCommandDb(db, timeoutMs = 15000) {
  if (typeof db !== 'function') return db;
  const deadline = Date.now() + timeoutMs;
  let current = db();
  while (Date.now() < deadline) {
    current = db();
    if (current?.raw?.business_commands) return current;
    await delay(100);
  }
  return current;
}

async function resolveCommandSync(sync) {
  return typeof sync === 'function' ? sync() : sync;
}

async function recordRxdbCommand({ db, sync, command }) {
  const command_id = command.id || `cmd_${crypto.randomUUID()}`;
  const now = Date.now();
  let localWriteSucceeded = false;
  const doc = {
    id: command_id,
    command_id,
    module: command.module,
    command_type: command.type || command.command_type,
    record_id: command.record_id || '',
    status: 'pending_sync',
    inbound_channel: command.inbound_channel || command.module || '',
    payload: command.payload || {},
    client_context: command.client_context || {},
    created_at_ms: now,
    updated_at_ms: now,
  };
  try {
    await writeCommandDocument(db, command_id, doc);
    localWriteSucceeded = true;
    const bridge = await withTimeout(
      prepareBusinessCommandsSync({ db, sync }),
      COMMAND_SYNC_READY_TIMEOUT_MS,
      'Timed out waiting for business_commands sync bridge readiness',
    );
    await flushCommandSyncBridge(bridge, COMMAND_SYNC_PUSH_TIMEOUT_MS);
  } catch (error) {
    if (localWriteSucceeded) {
      await patchLocalCommandDispatchResult(
        db,
        command_id,
        doc,
        { ok: true, command_id, status: 'pending_sync' },
        'rxdb-local-pending',
        error,
      );
      console.warn('[business-os] command remains pending until RxDB sync recovers', error);
      return normalizeAcceptedCommandResult(
        { ok: true, command_id, status: 'pending_sync' },
        command_id,
        'rxdb-local-pending',
      );
    }
    error.command_id = command_id;
    error.status = 'failed';
    console.warn('[business-os] command RxDB dispatch failed before local write', error);
    throw error;
  }
  return normalizeAcceptedCommandResult({ ok: true, command_id, status: 'accepted' }, command_id, 'rxdb-webrtc');
}

async function prepareBusinessCommandsSync({ db, sync }) {
  const currentDb = await resolveCommandDb(db, 15000);
  if (!currentDb?.raw?.business_commands) {
    throw new Error('business_commands collection is required for RxDB commands');
  }
  const currentSync = await resolveCommandSync(sync);
  const bridge = await currentSync?.startCollection?.('business_commands');
  await waitForSyncBridgeReady(bridge, 15000);
  return bridge;
}

async function writeCommandDocument(db, commandId, doc) {
  const deadline = Date.now() + 15000;
  let lastError = null;
  while (Date.now() < deadline) {
    const currentDb = await resolveCommandDb(db, Math.max(100, deadline - Date.now()));
    const collection = currentDb?.raw?.business_commands;
    if (!collection) {
      lastError = new Error('business_commands collection is required for RxDB commands');
      await delay(100);
      continue;
    }
    try {
      await insertOrPatchCommandDocument(collection, commandId, doc);
      return;
    } catch (error) {
      if (!isClosedRxDbCollectionError(error)) throw error;
      lastError = error;
      await delay(100);
    }
  }
  throw lastError || new Error('business_commands collection is required for RxDB commands');
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

async function patchLocalCommandDispatchResult(db, commandId, doc, result, transport, error = null) {
  const currentDb = await resolveCommandDb(db, 1500).catch(() => null);
  const collection = currentDb?.raw?.business_commands;
  if (!collection?.findOne) return;
  const command_id = result?.command_id || result?.id || doc.command_id || commandId;
  const task_id = String(result?.task_id || '').trim();
  const status = String(result?.status || (task_id ? 'accepted' : doc.status || 'pending_sync')).trim();
  const task_status = String(result?.task_status || (task_id ? 'queued' : status)).trim();
  const patch = {
    ...doc,
    command_id,
    status,
    task_id,
    task_status,
    queue_status_note: result?.queue_status_note || doc.queue_status_note || '',
    updated_at_ms: Date.now(),
    client_context: {
      ...(doc.client_context || {}),
      dispatch_transport: transport,
      rxdb_sync_error: error ? String(error?.message || error) : '',
    },
  };
  const existing = await collection.findOne(commandId).exec().catch(() => null);
  if (existing?.incrementalPatch) {
    await existing.incrementalPatch(patch).catch(() => {});
  } else if (collection.insert) {
    await collection.insert(patch).catch(() => {});
  }
}

function normalizeAcceptedCommandResult(result, commandId, transport) {
  const taskId = String(result?.task_id || '').trim();
  const status = String(result?.status || (taskId ? 'accepted' : 'accepted')).trim() || 'accepted';
  return {
    ok: result?.ok !== false,
    command_id: result?.command_id || result?.id || commandId,
    status,
    task_id: taskId,
    task_status: String(result?.task_status || (taskId ? 'queued' : status)).trim() || status,
    transport,
  };
}

async function waitForSyncBridgeReady(bridge, timeoutMs = 15000) {
  const state = bridge?.state;
  if (!state) return;
  await Promise.race([
    Promise.resolve()
      .then(() => state.awaitInSync?.() || state.awaitInitialReplication?.())
      .catch(() => {}),
    delay(timeoutMs),
  ]);
}

async function flushCommandSyncBridge(bridge, timeoutMs = 60000) {
  const state = bridge?.state;
  if (!state) return;
  await withTimeout(Promise.resolve().then(() => state.awaitInitialReplication?.()), Math.min(3000, timeoutMs));
  if (typeof state.pushToRemotePeers === 'function') {
    await withTimeout(state.pushToRemotePeers(), timeoutMs);
    return;
  }
  if (typeof state.awaitInSync === 'function') {
    await withTimeout(state.awaitInSync(), timeoutMs);
  }
}

async function withTimeout(promise, timeoutMs, message = null) {
  return Promise.race([
    Promise.resolve(promise),
    delay(timeoutMs).then(() => {
      throw new Error(message || `Timed out waiting for business_commands sync push after ${timeoutMs}ms`);
    }),
  ]);
}

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function isRxDbConflictError(error) {
  const message = String(error?.message || error || '');
  return message.includes('RxDB Error-Code: CONFLICT')
    || message.includes('conflict')
    || message.includes('document already exists')
    || message.includes('Document update conflict');
}

function isClosedRxDbCollectionError(error) {
  const message = String(error?.message || error || '');
  return message.includes('RxDB Error-Code: COL21')
    || message.includes('collection is closed')
    || message.includes('closed collection');
}
