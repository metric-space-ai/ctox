export function createCommandBus({ baseUrl, db, config }) {
  return {
    async dispatch(command) {
      if (config?.http_bridge_available === true || config?.transport === 'http') {
        try {
          const res = await fetch(`${baseUrl}/commands`, {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify(command),
          });
          if (res.ok) {
            const result = await res.json();
            await mirrorAcceptedCommand({ db, command, result }).catch(() => {});
            return result;
          }
        } catch {}
      }
      return enqueueLocalCommand({ db, command });
    },
  };
}

async function mirrorAcceptedCommand({ db, command, result }) {
  const collection = db?.raw?.business_commands;
  if (!collection || !result?.command_id) return;
  const command_id = result.command_id;
  const doc = {
    id: command_id,
    command_id,
    module: command.module,
    command_type: command.type,
    record_id: command.record_id || '',
    status: result.status || 'accepted',
    task_id: result.task_id || '',
    task_status: result.task_status || 'queued',
    payload: command.payload || {},
    client_context: command.client_context || {},
    updated_at_ms: Date.now(),
  };
  const existing = await collection.findOne(command_id).exec();
  if (existing) {
    await existing.incrementalPatch(doc);
  } else {
    await collection.insert(doc);
  }
}

async function enqueueLocalCommand({ db, command }) {
  const collection = db?.raw?.business_commands;
  if (!collection) {
    throw new Error('business_commands collection is required for offline/P2P commands');
  }
  const command_id = command.id || `cmd_${crypto.randomUUID()}`;
  const doc = {
    id: command_id,
    command_id,
    module: command.module,
    command_type: command.type,
    record_id: command.record_id || '',
    status: 'queued_local',
    payload: command.payload || {},
    client_context: command.client_context || {},
    updated_at_ms: Date.now(),
  };
  await collection.insert(doc);
  return { ok: true, command_id, status: 'queued_local' };
}
