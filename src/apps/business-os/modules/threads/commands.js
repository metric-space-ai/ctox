export const THREAD_COLLECTIONS = [
  'user_threads',
  'user_thread_states',
  'user_thread_messages',
  'user_thread_links',
  'user_notifications',
  'ctox_task_approval_requests',
  'business_commands',
  'ctox_queue_tasks',
];

export function splitUserIds(value) {
  return String(value || '')
    .split(/[,\s]+/)
    .map((item) => item.trim())
    .filter(Boolean)
    .filter((item, index, list) => list.indexOf(item) === index);
}

export function compactText(value, maxLength = 240) {
  const text = String(value || '').replace(/\s+/g, ' ').trim();
  return text.length > maxLength ? `${text.slice(0, maxLength - 1)}...` : text;
}

export function buildThreadsCommand({
  commandType,
  payload = {},
  recordId = '',
  sourceModule = 'threads',
  actor = null,
  clientContext = {},
} = {}) {
  const id = `cmd_${crypto.randomUUID()}`;
  const moduleId = 'threads';
  return {
    id,
    module: moduleId,
    command_type: commandType,
    record_id: recordId,
    inbound_channel: sourceModule || moduleId,
    payload,
    client_context: {
      action: commandType,
      module: moduleId,
      module_id: moduleId,
      app_id: moduleId,
      source_module: sourceModule || moduleId,
      actor,
      ...clientContext,
    },
  };
}

export function buildNotePayload({
  body,
  targetUserIds = [],
  threadId = '',
  title = '',
  kind = 'note',
  sourceContext = {},
} = {}) {
  return {
    body: String(body || '').trim(),
    kind: String(kind || 'note').trim() || 'note',
    target_user_ids: Array.isArray(targetUserIds) ? targetUserIds : splitUserIds(targetUserIds),
    thread_id: String(threadId || '').trim(),
    title: compactText(title || body || sourceContext?.label || 'Notiz', 120),
    source_context: sourceContext && typeof sourceContext === 'object' ? sourceContext : {},
  };
}

export function buildApprovalRequestPayload({
  prompt,
  reviewerUserId,
  threadId = '',
  sourceContext = {},
  targetCommandType = 'business_os.chat.task',
  targetModule = '',
  targetRecordId = '',
  targetPayload = {},
} = {}) {
  const cleanPrompt = String(prompt || '').trim();
  const context = sourceContext && typeof sourceContext === 'object' ? sourceContext : {};
  const moduleId = String(targetModule || context.module || context.module_id || 'ctox').trim() || 'ctox';
  const recordId = String(targetRecordId || context.record_id || moduleId).trim();
  const title = compactText(cleanPrompt || context.label || 'CTOX Freigabe', 120);
  return {
    prompt: cleanPrompt,
    instruction: cleanPrompt,
    reviewer_user_id: String(reviewerUserId || '').trim(),
    thread_id: String(threadId || '').trim(),
    title,
    target_command_type: String(targetCommandType || 'business_os.chat.task').trim(),
    target_module: moduleId,
    target_record_id: recordId,
    target_payload: targetPayload && typeof targetPayload === 'object' ? targetPayload : {},
    source_context: context,
  };
}
