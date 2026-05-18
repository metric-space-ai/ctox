export function createCommandBus({ baseUrl, db, config }) {
  return {
    async dispatch(command) {
      let transportError = null;
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
            if (result.task_id) {
              await mirrorQueueTask({ db, command, commandId: result.command_id, taskId: result.task_id, status: result.task_status || result.status || 'queued' }).catch(() => {});
            }
            return result;
          }
          transportError = new Error(`CTOX API rejected command (${res.status})`);
        } catch (error) {
          transportError = error;
        }
      } else {
        transportError = new Error('CTOX API bridge is not available');
      }
      if (requiresCtoxQueue(command)) {
        const blocked = await recordLocalCommand({ db, command, status: 'blocked_no_ctox_api', error: transportError?.message || String(transportError || '') }).catch(() => null);
        const error = new Error('CTOX API ist nicht verbunden. Auftrag wurde nicht in die echte Queue geschrieben.');
        if (blocked?.command_id) error.command_id = blocked.command_id;
        if (blocked?.status) error.status = blocked.status;
        throw error;
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
    inbound_channel: command.inbound_channel || command.module || '',
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
  return recordLocalCommand({ db, command, status: 'queued_local' });
}

async function recordLocalCommand({ db, command, status, error = '' }) {
  const collection = db?.raw?.business_commands;
  if (!collection) {
    throw new Error('business_commands collection is required for offline/P2P commands');
  }
  const command_id = command.id || `cmd_${crypto.randomUUID()}`;
  const now = Date.now();
  const doc = {
    id: command_id,
    command_id,
    module: command.module,
    command_type: command.type,
    record_id: command.record_id || '',
    status,
    inbound_channel: command.inbound_channel || command.module || '',
    task_id: '',
    task_status: '',
    payload: command.payload || {},
    client_context: {
      ...(command.client_context || {}),
      ...(error ? { dispatch_error: error } : {}),
    },
    updated_at_ms: now,
  };
  await collection.insert(doc);
  return { ok: status !== 'blocked_no_ctox_api', command_id, status };
}

async function mirrorQueueTask({ db, command, commandId, taskId, status = 'queued', updatedAt = Date.now() }) {
  const collection = db?.raw?.ctox_queue_tasks;
  if (!collection || !taskId) return;
  const payload = command.payload || {};
  const clientContext = command.client_context || {};
  const doc = {
    id: taskId,
    command_id: commandId || '',
    title: payload.title || payload.instruction || command.type || 'CTOX task',
    status: normalizeTaskStatus(status),
    route_status: status || 'queued',
    module: 'ctox',
    source_module: clientContext.source_module || clientContext.module || payload.source_module || command.module || '',
    inbound_channel: command.inbound_channel || payload.inbound_channel || clientContext.inbound_channel || command.module || '',
    command_type: command.type || '',
    priority: payload.priority || clientContext.priority || 'normal',
    thread_key: payload.thread_key || `business-os/${command.module || 'ctox'}`,
    prompt: payload.prompt || payload.instruction || '',
    updated_at_ms: updatedAt,
  };
  const existing = await collection.findOne(taskId).exec();
  if (existing) {
    await existing.incrementalPatch(doc);
  } else {
    await collection.insert(doc);
  }
}

function normalizeTaskStatus(status) {
  const value = String(status || '').toLowerCase();
  if (value === 'accepted' || value === 'queued_local' || value === 'pending') return 'queued';
  if (value === 'leased' || value === 'working') return 'running';
  return value || 'queued';
}

function requiresCtoxQueue(command) {
  const type = String(command?.type || command?.command_type || '');
  const inbound = String(command?.inbound_channel || command?.payload?.inbound_channel || command?.client_context?.inbound_channel || '');
  const response = String(command?.payload?.response_channel || command?.client_context?.response_channel || '');
  return type === 'business_os.chat.task'
    || type === 'ctox.business_os.app.modify'
    || type.startsWith('ctox.documents.')
    || inbound === 'business_os.llm.chat'
    || response === 'business_os_chat';
}
