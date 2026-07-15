export const SUPPORT_COMMAND_TYPES = Object.freeze([
  'support.inbox.upsert',
  'support.conversation.open_from_thread',
  'support.conversation.claim',
  'support.conversation.assign',
  'support.conversation.status',
  'support.conversation.priority',
  'support.conversation.snooze',
  'support.conversation.resolve',
  'support.conversation.reopen',
  'support.identity.link',
  'support.note.create',
  'support.ticket.link',
  'support.ticket.create_from_conversation',
  'support.reply.draft',
  'support.reply.send',
  'support.view.upsert',
  'support.view_filter.upsert',
  'support.bulk.assign',
  'support.bulk.status',
  'support.bulk.priority',
  'support.bulk.snooze',
  'support.bulk.resolve',
  'support.assignment_policy.upsert',
  'support.macro.upsert',
  'support.macro.run',
  'support.automation_rule.upsert',
  'support.automation.evaluate',
  'support.sla_policy.upsert',
  'support.sla.apply',
  'support.sla.recalculate',
  'support.reporting.rebuild_rollups',
  'support.agent.writeback',
  'support.agent.apply_suggestion',
  'support.agent.reject_suggestion',
]);

export const SUPPORT_AGENT_SUGGESTION_KINDS = Object.freeze([
  'summary',
  'draft_reply',
  'classification',
  'next_action',
  'customer_update',
  'ticket_action',
]);

const SUPPORT_COMMAND_SET = new Set(SUPPORT_COMMAND_TYPES);

export function buildSupportCommand({
  id = '',
  commandType,
  recordId = '',
  payload = {},
  actor = {},
  surface = '',
} = {}) {
  if (!SUPPORT_COMMAND_SET.has(commandType)) {
    throw new Error(`unsupported support command type: ${commandType}`);
  }
  const commandId = id || `cmd_support_${cryptoRandomId()}`;
  return {
    id: commandId,
    module: 'support',
    command_type: commandType,
    record_id: recordId || '',
    inbound_channel: 'support',
    payload: { ...payload },
    client_context: withOptionalActor({
      source: 'business-os.support',
      module: 'support',
      surface: surface || commandType,
    }, actor),
  };
}

export function buildSupportAgentTaskCommand({
  id = '',
  conversationId,
  title,
  instruction,
  prompt,
  requestKind = 'summary',
  recordSnapshot = {},
  requiredSkills = ['business-os-support-workflow'],
  actor = {},
  priority = 'normal',
} = {}) {
  if (!conversationId) throw new Error('conversationId is required');
  const commandId = id || `cmd_support_agent_${cryptoRandomId()}`;
  const safeTitle = title || `Support CTOX task ${conversationId}`;
  const safePrompt = prompt || instruction || safeTitle;
  return {
    id: commandId,
    module: 'support',
    command_type: 'business_os.chat.task',
    record_id: conversationId,
    inbound_channel: 'support',
    payload: {
      title: safeTitle,
      instruction: instruction || safePrompt,
      prompt: safePrompt,
      user_message: safePrompt,
      mode: 'data',
      target: 'data',
      priority,
      source_module: 'support',
      thread_key: `business-os/support/${conversationId}`,
      required_skills: [...requiredSkills],
      record_snapshot: recordSnapshot && typeof recordSnapshot === 'object' ? recordSnapshot : {},
      writeback_contract: {
        command_type: 'support.agent.writeback',
        collection: 'support_agent_suggestions',
        record_id: conversationId,
        source_collection: 'support_conversations',
        allowed_suggestion_kinds: [...SUPPORT_AGENT_SUGGESTION_KINDS],
      },
      response_channel: 'business_os_chat',
      outbound_channel: 'business_os_chat',
    },
    client_context: withOptionalActor({
      source: 'support-agent-task',
      module: 'support',
      surface: `support.agent.${requestKind}`,
      record_type: 'support_conversation',
      record_id: conversationId,
    }, actor),
  };
}

export function buildAgentWritebackCommand({
  id = '',
  conversationId,
  sourceCommandId,
  taskId = '',
  suggestionKind,
  payload = {},
  confidence = 0,
  requiredHumanAction = 'review',
  summary = '',
  actor = {},
} = {}) {
  if (!conversationId) throw new Error('conversationId is required');
  if (!sourceCommandId) throw new Error('sourceCommandId is required');
  if (!SUPPORT_AGENT_SUGGESTION_KINDS.includes(suggestionKind)) {
    throw new Error(`unsupported support suggestion kind: ${suggestionKind}`);
  }
  return buildSupportCommand({
    id: id || `cmd_support_agent_writeback_${cryptoRandomId()}`,
    commandType: 'support.agent.writeback',
    recordId: conversationId,
    actor,
    surface: 'support.agent.writeback',
    payload: {
      conversation_id: conversationId,
      source_command_id: sourceCommandId,
      task_id: taskId,
      suggestion_kind: suggestionKind,
      confidence,
      required_human_action: requiredHumanAction,
      summary,
      payload,
    },
  });
}

function cryptoRandomId() {
  const cryptoApi = globalThis.crypto;
  if (cryptoApi?.randomUUID) return cryptoApi.randomUUID();
  return `${Date.now()}_${Math.random().toString(16).slice(2)}`;
}

function withOptionalActor(context, actor) {
  if (!actor || typeof actor !== 'object') return context;
  if (!Object.values(actor).some((value) => String(value || '').trim())) return context;
  return { ...context, actor };
}
