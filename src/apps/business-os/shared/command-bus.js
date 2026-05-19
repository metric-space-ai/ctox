export function createCommandBus({ baseUrl = '/api/business-os', db }) {
  return {
    async dispatch(command) {
      return recordRxdbCommand({ baseUrl, db, command });
    },
  };
}

async function recordRxdbCommand({ baseUrl, db, command }) {
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
  const result = await pushNativeCommand({ baseUrl, document: doc });
  const accepted = Array.isArray(result?.accepted) ? result.accepted[0] : result;
  const status = accepted?.status || result?.status || 'accepted';
  const patch = {
    status,
    task_id: accepted?.task_id || result?.task_id || '',
    task_status: accepted?.task_status || result?.task_status || status,
    updated_at_ms: Date.now(),
  };
  const latest = await collection.findOne(command_id).exec();
  if (latest) await latest.incrementalPatch(patch);
  return {
    ok: result?.ok !== false,
    command_id,
    status,
    task_id: patch.task_id,
    task_status: patch.task_status,
    transport: 'native-rxdb-http',
  };
}

async function pushNativeCommand({ baseUrl, document }) {
  const res = await fetch(`${baseUrl}/rxdb/push`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      collection: 'business_commands',
      documents: [stripRxdbInternals(document)],
    }),
  });
  if (!res.ok) {
    throw new Error(`CTOX konnte den Command nicht annehmen (${res.status}).`);
  }
  const result = await res.json();
  const ignored = Array.isArray(result?.ignored) ? result.ignored : [];
  if (ignored.length) {
    const message = ignored[0]?.error || 'Command wurde vom CTOX Backend ignoriert.';
    throw new Error(message);
  }
  return result;
}

function stripRxdbInternals(document) {
  const {
    _attachments,
    _deleted,
    _meta,
    _rev,
    ...plain
  } = document || {};
  return plain;
}
