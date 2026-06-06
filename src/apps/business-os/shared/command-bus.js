const COMMAND_ACCEPT_TIMEOUT_MS = 45000;
const COMMAND_SYNC_READY_TIMEOUT_MS = 15000;

export function createCommandBus({ db, sync = null } = {}) {
  return {
    async dispatch(command) {
      return dispatchRxdbCommand({ db, sync, command });
    },
  };
}

async function dispatchRxdbCommand({ db, sync, command }) {
  const commandId = command.id || `cmd_${crypto.randomUUID()}`;
  const doc = commandDocument(command, commandId);
  const currentDb = await resolveCommandDb(db);
  const collection = currentDb?.raw?.business_commands;
  if (!collection) throw commandError(commandId, 'business_commands collection is required.');

  const bridges = await prepareCommandSync({ db: currentDb, sync });
  await insertOrPatchCommandDocument(collection, commandId, doc);
  await flushSyncBridges(bridges);
  return waitForAuthoritativeQueueProjection(currentDb, commandId);
}

function commandDocument(command, commandId) {
  const now = Date.now();
  const moduleId = String(command.module || command.client_context?.module || 'ctox').trim() || 'ctox';
  const commandType = String(command.type || command.command_type || 'business_os.chat.task').trim();
  const inboundChannel = String(command.inbound_channel || command.client_context?.inbound_channel || moduleId).trim();
  if (!commandType) throw commandError(commandId, 'command_type is required.');
  return {
    id: commandId,
    command_id: commandId,
    module: moduleId,
    command_type: commandType,
    record_id: command.record_id || '',
    status: 'pending_sync',
    inbound_channel: inboundChannel,
    payload: {
      ...(command.payload || {}),
      inbound_channel: inboundChannel,
    },
    client_context: {
      ...(command.client_context || {}),
      inbound_channel: inboundChannel,
      dispatch_transport: 'rxdb-command-bus',
    },
    created_at_ms: now,
    updated_at_ms: now,
  };
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

async function prepareCommandSync({ db, sync }) {
  const currentSync = await resolveCommandSync(sync);
  const commandBridge = await currentSync?.startCollection?.('business_commands');
  const queueBridge = await currentSync?.startCollection?.('ctox_queue_tasks');
  await Promise.all([
    waitForSyncBridgeReady(commandBridge, COMMAND_SYNC_READY_TIMEOUT_MS),
    waitForSyncBridgeReady(queueBridge, COMMAND_SYNC_READY_TIMEOUT_MS),
  ]);
  return [commandBridge, queueBridge];
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

async function waitForAuthoritativeQueueProjection(db, commandId) {
  const commands = db?.raw?.business_commands;
  const queue = db?.raw?.ctox_queue_tasks;
  const deadline = Date.now() + COMMAND_ACCEPT_TIMEOUT_MS;
  let lastCommand = null;
  while (Date.now() < deadline) {
    lastCommand = await findDoc(commands, commandId);
    const taskId = String(lastCommand?.task_id || '').trim();
    const task = taskId ? await findDoc(queue, taskId) : null;
    if (lastCommand?.status === 'failed') {
      throw commandError(commandId, lastCommand.error || 'CTOX command failed.');
    }
    if (taskId && task) {
      return {
        ok: true,
        command_id: commandId,
        status: String(lastCommand?.status || 'accepted'),
        task_id: taskId,
        task_status: String(task.status || lastCommand?.task_status || 'queued'),
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
  for (const bridge of bridges || []) {
    await flushSyncBridge(bridge);
  }
}

async function flushSyncBridge(bridge) {
  const state = bridge?.state;
  if (!state) return;
  if (typeof state.pushToRemotePeers === 'function') {
    await state.pushToRemotePeers();
    return;
  }
  await Promise.resolve()
    .then(() => state.awaitInSync?.())
    .catch(() => {});
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
