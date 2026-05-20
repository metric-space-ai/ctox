export function createCommandBus({ db }) {
  return {
    async dispatch(command) {
      return recordRxdbCommand({ db, command });
    },
  };
}

async function recordRxdbCommand({ db, command }) {
  const collection = db?.raw?.business_commands;
  if (!collection) {
    throw new Error('business_commands collection is required for RxDB commands');
  }
  const command_id = command.id || `cmd_${crypto.randomUUID()}`;
  const now = Date.now();
  const doc = {
    id: command_id,
    command_id,
    module: command.module,
    command_type: command.type,
    record_id: command.record_id || '',
    status: 'pending_sync',
    inbound_channel: command.inbound_channel || command.module || '',
    payload: command.payload || {},
    client_context: command.client_context || {},
    updated_at_ms: now,
  };
  const existing = await collection.findOne(command_id).exec();
  if (existing) {
    await existing.incrementalPatch(doc);
  } else {
    await collection.insert(doc);
  }
  return {
    ok: true,
    command_id,
    status: 'pending_sync',
    task_id: '',
    task_status: 'pending_sync',
    transport: 'rxdb-webrtc',
  };
}
