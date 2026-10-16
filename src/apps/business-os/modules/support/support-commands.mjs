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
  'support.agent.task',
]);

// Abschlussbefehle des Agentenauftrags support.agent.task (Kern §16.30): der Core stellt sie zu, kein UI-Sender.
export const SUPPORT_AGENT_CALLBACK_TYPES = Object.freeze([
  'support.agent.task.complete',
  'support.agent.task.failed',
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

// „CTOX fragen“ (Strang SKL-A1): App-Befehl support.agent.task an den eigenen Handler. Der Handler füllt das
// Template prompts/agent-task.md und startet den Agenten mit dem App-Skill support/support-workflow; Vorschläge
// kommen über support.agent.task.complete in support_agent_suggestions (vorher: Kernbefehl business_os.chat.task mit
// Phantom-Skill business-os-support-workflow und writeback_contract).
export function buildSupportAgentTaskCommand({
  id = '',
  conversationId,
  title,
  requestKind = 'summary',
  status = '',
  priority = '',
  recordSnapshot = {},
  actor = {},
} = {}) {
  if (!conversationId) throw new Error('conversationId is required');
  const payload = {
    conversation_id: conversationId,
    request_kind: requestKind,
    record_snapshot: recordSnapshot && typeof recordSnapshot === 'object' ? recordSnapshot : {},
  };
  if (title) payload.title = title;
  if (status) payload.status = status;
  if (priority) payload.priority = priority;
  return buildSupportCommand({
    id: id || `cmd_support_agent_${cryptoRandomId()}`,
    commandType: 'support.agent.task',
    recordId: conversationId,
    actor,
    surface: `support.agent.${requestKind}`,
    payload,
  });
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
