export function createCommandBus({ db }) {
  return {
    async dispatch(command) {
      return recordRxdbCommand({ db, command });
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

async function recordRxdbCommand({ db, command }) {
  const command_id = command.id || `cmd_${crypto.randomUUID()}`;
  const now = Date.now();
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
  await writeCommandDocument(db, command_id, doc);
  return {
    ok: true,
    command_id,
    status: 'pending_sync',
    task_id: '',
    task_status: 'pending_sync',
    transport: 'rxdb-webrtc',
  };
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
      const existing = await collection.findOne(commandId).exec();
      if (existing) {
        await existing.incrementalPatch(doc);
      } else {
        await collection.insert(doc);
      }
      return;
    } catch (error) {
      if (!isClosedRxDbCollectionError(error)) throw error;
      lastError = error;
      await delay(100);
    }
  }
  throw lastError || new Error('business_commands collection is required for RxDB commands');
}

function delay(ms) {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

function isClosedRxDbCollectionError(error) {
  const message = String(error?.message || error || '');
  return message.includes('RxDB Error-Code: COL21')
    || message.includes('collection is closed')
    || message.includes('closed collection');
}
