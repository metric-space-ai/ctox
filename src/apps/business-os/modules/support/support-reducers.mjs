const CLOSED_STATUSES = new Set(['resolved', 'closed', 'done']);

export function filterSupportConversations(conversations = [], filters = {}) {
  const query = normalize(filters.query);
  return conversations
    .filter((item) => !item.is_deleted)
    .filter((item) => statusMatches(item, filters.status))
    .filter((item) => valueMatches(item.assignee_id, filters.assigneeId))
    .filter((item) => valueMatches(item.team_id, filters.teamId))
    .filter((item) => valueMatches(item.inbox_id, filters.inboxId))
    .filter((item) => valueMatches(item.priority, filters.priority))
    .filter((item) => {
      if (!filters.labelId) return true;
      return Array.isArray(item.label_ids) && item.label_ids.includes(filters.labelId);
    })
    .filter((item) => {
      if (!query) return true;
      return normalize(item.search_text || [
        item.id,
        item.primary_thread_key,
        item.customer_account_id,
        item.customer_contact_id,
        item.ticket_case_id,
      ].filter(Boolean).join(' ')).includes(query);
    })
    .sort((a, b) => Number(b.last_activity_at_ms || 0) - Number(a.last_activity_at_ms || 0));
}

export function supportQueueCounts(conversations = [], nowMs = Date.now(), currentUserId = '') {
  const userId = String(currentUserId || '').trim();
  const counts = {
    mine: 0,
    unassigned: 0,
    open: 0,
    needsReply: 0,
    slaRisk: 0,
    snoozed: 0,
    agentDrafts: 0,
  };
  for (const item of conversations) {
    if (item.is_deleted) continue;
    const status = String(item.status || '').toLowerCase();
    if (!CLOSED_STATUSES.has(status)) counts.open += 1;
    if (!item.assignee_id && !CLOSED_STATUSES.has(status)) counts.unassigned += 1;
    if (userId && item.assignee_id === userId && !CLOSED_STATUSES.has(status)) counts.mine += 1;
    if (Number(item.unread_count || 0) > 0 || Number(item.waiting_since_ms || 0) > 0) counts.needsReply += 1;
    if (Number(item.snoozed_until_ms || 0) > nowMs) counts.snoozed += 1;
    const dueAt = Number(item.sla_due_at_ms || item.resolution_due_at_ms || 0);
    if (dueAt && dueAt - nowMs <= 60 * 60 * 1000 && !CLOSED_STATUSES.has(status)) counts.slaRisk += 1;
    if (Number(item.agent_draft_count || 0) > 0) counts.agentDrafts += 1;
  }
  return counts;
}

export function mergeSupportTimeline({
  messages = [],
  notes = [],
  events = [],
  suggestions = [],
} = {}) {
  const rows = [];
  for (const message of messages) {
    rows.push({
      id: message.message_key || message.id,
      kind: 'message',
      at: timestampMs(message.external_created_at || message.observed_at, message.updated_at_ms),
      payload: message,
    });
  }
  for (const note of notes) {
    rows.push({
      id: note.id,
      kind: 'note',
      at: Number(note.created_at_ms || note.updated_at_ms || 0),
      payload: note,
    });
  }
  for (const event of events) {
    rows.push({
      id: event.id,
      kind: 'event',
      at: Number(event.occurred_at_ms || event.created_at_ms || event.updated_at_ms || 0),
      payload: event,
    });
  }
  for (const suggestion of suggestions) {
    rows.push({
      id: suggestion.id,
      kind: 'agent_suggestion',
      at: Number(suggestion.created_at_ms || suggestion.updated_at_ms || 0),
      payload: suggestion,
    });
  }
  return rows
    .filter((row) => row.id)
    .sort((a, b) => a.at - b.at || String(a.id).localeCompare(String(b.id)));
}

function statusMatches(item, status) {
  if (!status || status === 'all') return true;
  if (status === 'open') return !CLOSED_STATUSES.has(String(item.status || '').toLowerCase());
  return String(item.status || '').toLowerCase() === String(status).toLowerCase();
}

function valueMatches(actual, expected) {
  if (!expected || expected === 'all') return true;
  if (expected === 'unassigned') return !actual;
  return String(actual || '') === String(expected);
}

function normalize(value) {
  return String(value || '').trim().toLowerCase();
}

function timestampMs(isoOrMs, fallback = 0) {
  if (typeof isoOrMs === 'number') return isoOrMs;
  const parsed = Date.parse(String(isoOrMs || ''));
  return Number.isFinite(parsed) ? parsed : Number(fallback || 0);
}
