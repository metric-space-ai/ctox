// Projektion App-Datensätze (14 Collections) → user_threads/… („App-Relevanz“).
// Port von APP_RELEVANCE_SPECS (threads.rs:44-150), project_app_relevance (:1056-1088),
// project_app_record_document (:2689-2786) und Helfern (:2788-3245). KEIN Befehlstyp (W-4).
// Braucht foreign_collections read für alle 14 Quell-Collections + exports der Eigentümer-Apps (§16.13).
import {
  arrayStrings, clone, compareRust, documentIsDeleted, documentUpdatedAtMs, firstNonEmpty, firstPresent,
  firstStringField, isObject, loadRecord, nestedString, nonEmptyString, participantSet, refreshThreadStates,
  slugPart, sourceString, supervisorBindingForThread, threadOwnerForBinding, truncate, upsertIfChanged,
  upsertMessage, upsertThread, valueString,
} from './_lib.js';

export const APP_RELEVANCE_SPECS = [
  { collection: 'kundenpipeline_entscheidungen', module: 'kundenpipeline', record_type: 'entscheidung', link_type: 'kundenpipeline_entscheidung', kind: 'approval' },
  { collection: 'ctox_ticket_approvals', module: 'tickets', record_type: 'ticket_case', link_type: 'ticket_case', kind: 'approval' },
  { collection: 'ctox_ticket_clarification_requests', module: 'tickets', record_type: 'ticket_case', link_type: 'ticket_case', kind: 'app_event' },
  { collection: 'ctox_ticket_self_work_items', module: 'tickets', record_type: 'ticket_self_work', link_type: 'ticket_self_work', kind: 'app_event' },
  { collection: 'ctox_ticket_self_work_notes', module: 'tickets', record_type: 'ticket_self_work', link_type: 'ticket_self_work', kind: 'app_event' },
  { collection: 'support_conversations', module: 'support', record_type: 'conversation', link_type: 'support_conversation', kind: 'app_event' },
  { collection: 'support_agent_requests', module: 'support', record_type: 'conversation', link_type: 'support_conversation', kind: 'ctox_task' },
  { collection: 'support_notes', module: 'support', record_type: 'conversation', link_type: 'support_conversation', kind: 'note' },
  { collection: 'outbound_approvals', module: 'outbound', record_type: 'engagement', link_type: 'outbound_engagement', kind: 'approval' },
  { collection: 'outbound_research_runs', module: 'outbound', record_type: 'research_run', link_type: 'research_run', kind: 'ctox_task' },
  { collection: 'research_tasks', module: 'research', record_type: 'research_task', link_type: 'research_run', kind: 'app_event' },
  { collection: 'research_runs', module: 'research', record_type: 'research_task', link_type: 'research_run', kind: 'ctox_task' },
  { collection: 'documents', module: 'documents', record_type: 'document', link_type: 'document', kind: 'app_event' },
  { collection: 'notes', module: 'notes', record_type: 'note', link_type: 'app_record', kind: 'note' },
];

const TICKETS = ['ctox_ticket_approvals', 'ctox_ticket_clarification_requests', 'ctox_ticket_self_work_items', 'ctox_ticket_self_work_notes'];
export const appRelevanceSpec = (collection) => APP_RELEVANCE_SPECS.find((s) => s.collection === collection) ?? null;

function contextRecordType(collection, fallback) {
  if (TICKETS.includes(collection)) return 'ticket_case';
  if (['support_conversations', 'support_agent_requests', 'support_notes'].includes(collection)) return 'conversation';
  if (collection === 'outbound_approvals') return 'engagement';
  if (collection === 'research_runs') return 'research_task';
  return fallback;
}

function contextRecordId(collection, document, fallbackId) {
  let keys = [];
  if (TICKETS.includes(collection)) keys = ['case_id', 'ticket_case_id', 'ticket_id', 'case_key'];
  else if (['support_agent_requests', 'support_notes'].includes(collection)) keys = ['conversation_id'];
  else if (collection === 'outbound_approvals') keys = ['engagement_id', 'message_id'];
  else if (collection === 'research_runs') keys = ['task_id'];
  return firstStringField(document, keys) ?? fallbackId;
}

function labelOf(document) {
  const v = firstStringField(document, ['title', 'subject', 'name', 'filename'])
    ?? firstStringField(document, ['summary', 'search_text', 'body', 'content']);
  return v === undefined ? undefined : truncate(v, 120);
}

export function appThreadLabel(document, fallbackId) {
  return labelOf(document) ?? fallbackId;
}

/** app_source_context (:2795). */
export function appSourceContext(spec, document, recordId) {
  const ctxRecordId = contextRecordId(spec.collection, document, recordId);
  const ctxRecordType = contextRecordType(spec.collection, spec.record_type);
  const label = appThreadLabel(document, recordId);
  const raw = firstPresent(document, ['source_context', 'context']);
  const source = isObject(raw) ? clone(raw) : {};
  const ensure = (key, fallback) => {
    const existing = typeof source[key] === 'string' ? source[key].trim() : '';
    if (existing === '') source[key] = fallback();
  };
  ensure('module', () => spec.module);
  ensure('record_type', () => ctxRecordType);
  ensure('record_id', () => ctxRecordId);
  ensure('label', () => label);
  ensure('deep_link', () => `#${spec.module}?record=${slugPart(ctxRecordId)}&record_type=${slugPart(ctxRecordType)}`);
  source.collection = spec.collection;
  return source;
}

export function appThreadId(spec, source, fallbackId) {
  const module = sourceString(source, 'module') ?? spec.module;
  const recordType = sourceString(source, 'record_type') ?? spec.record_type;
  const recordId = sourceString(source, 'record_id') ?? fallbackId;
  return truncate(`thread_${slugPart(module)}_${slugPart(recordType)}_${slugPart(recordId)}`, 220);
}

const TITLE_PREFIX = { tickets: 'Ticket', support: 'Support', outbound: 'Outbound', research: 'Research', documents: 'Dokument', notes: 'Notiz' };
export function appThreadTitle(spec, document, source, fallbackId) {
  const raw = sourceString(source, 'label') ?? firstStringField(document, ['title', 'subject', 'name', 'filename'])
    ?? firstStringField(document, ['summary', 'search_text', 'body', 'content']);
  const label = raw === undefined ? fallbackId : truncate(raw, 120);
  return TITLE_PREFIX[spec.module] ? `${TITLE_PREFIX[spec.module]}: ${label}` : label;
}

/** app_thread_status (:2895). */
export function appThreadStatus(collection, document) {
  const status = (firstStringField(document, ['status', 'approval_status', 'decision', 'task_status', 'route_status', 'draft_status', 'send_status'])
    ?? (collection.includes('approval') ? 'pending' : 'open')).toLowerCase();
  if (collection === 'kundenpipeline_entscheidungen' && document.typ === 'agent_escalation' && status === 'offen') return 'needs_review';
  const map = [
    [['pending', 'pending_review', 'review', 'needs_review', 'requested'], 'needs_review'],
    [['waiting', 'waiting_on_user', 'snoozed'], 'waiting'],
    [['queued', 'accepted', 'running', 'in_progress', 'collecting', 'processing'], 'running'],
    [['blocked', 'failed', 'error'], 'blocked'],
    [['completed', 'done', 'resolved', 'approved', 'sent', 'final', 'entschieden', 'freigegeben', 'abgeschlossen'], 'completed'],
    [['rejected', 'cancelled', 'canceled', 'closed', 'abgelehnt'], 'archived'],
    [['draft', 'imported', 'ready', 'open', 'new', 'offen'], 'open'],
  ];
  for (const [keys, value] of map) if (keys.includes(status)) return value;
  return status !== '' ? status : 'open';
}

export const appStatusDeservesEvent = (s) => ['needs_review', 'waiting', 'blocked', 'completed', 'archived'].includes(s);

const BODY_PREFIX = {
  needs_review: 'App-Record wartet auf Freigabe.',
  waiting: 'App-Record wartet auf Rueckmeldung.',
  blocked: 'App-Record ist blockiert oder fehlgeschlagen.',
  completed: 'App-Record wurde abgeschlossen.',
  archived: 'App-Record wurde beendet.',
};
export function appStatusBody(collection, document, status) {
  const detail = firstStringField(document, ['summary', 'comment', 'body', 'content', 'prompt', 'error', 'paused_reason', 'closed_reason', 'search_text']) ?? '';
  return [BODY_PREFIX[status] ?? 'App-Record wurde aktualisiert.', `${collection}/${valueString(document, 'id')}`, detail]
    .map((p) => p.trim()).filter((p) => p !== '').join('\n\n');
}

const USER_KEYS = ['user_id', 'actor_user_id', 'actor_id', 'author_id', 'owner_user_id', 'owner_id', 'account_owner_id',
  'contact_owner_id', 'assignee_id', 'assigned_user_id', 'assignee_user_id', 'assigned_by_id', 'requester_user_id',
  'reviewer_user_id', 'decision_by_id', 'created_by_id', 'created_by_user_id', 'updated_by_id', 'recipient_user_id'];
const USER_PATHS = [['actor', 'id'], ['author', 'id'], ['owner', 'id'], ['assignee', 'id'], ['requester', 'id'], ['reviewer', 'id'],
  ['payload', 'actor', 'id'], ['payload', 'user_id'], ['payload', 'owner_user_id'], ['payload', 'assignee_id'], ['payload', 'reviewer_user_id']];
const USER_ARRAYS = ['participant_ids', 'participant_user_ids', 'target_user_ids', 'mention_user_ids', 'mentions_user_ids',
  'watcher_user_ids', 'assignee_user_ids', 'assigned_user_ids', 'reviewer_user_ids'];

/** app_record_user_ids (:2988) – sortierte Menge. */
export function appRecordUserIds(document) {
  const ids = new Set();
  for (const k of USER_KEYS) { const v = nonEmptyString(document, k); if (v !== undefined) ids.add(v); }
  for (const p of USER_PATHS) { const v = nestedString(document, p); if (v !== undefined) ids.add(v); }
  for (const k of USER_ARRAYS) for (const v of arrayStrings(document[k])) ids.add(v);
  return [...ids].sort(compareRust);
}

export function appAssignedUserId(document, userIds) {
  const preferred = firstStringField(document, ['reviewer_user_id', 'assignee_id', 'assigned_user_id', 'assignee_user_id',
    'owner_user_id', 'owner_id', 'actor_user_id', 'author_id', 'requester_user_id', 'user_id']);
  return preferred !== undefined && userIds.includes(preferred) ? preferred : undefined;
}

function actorFromAppRecord(document, userIds) {
  const id = appAssignedUserId(document, userIds) ?? userIds[0] ?? 'business-os';
  const displayName = firstStringField(document, ['actor_display_name', 'author_display_name', 'owner_display_name',
    'requester_display_name', 'reviewer_display_name'])
    ?? nestedString(document, ['actor', 'display_name']) ?? nestedString(document, ['actor', 'name']) ?? id;
  return { actor: { id, display_name: displayName, role: 'user', is_admin: false } };
}

export function appNotificationTargets(document, userIds, assignedUserId, status) {
  const targets = new Set();
  if (status === 'needs_review' || status === 'waiting') {
    const reviewer = firstStringField(document, ['reviewer_user_id', 'assignee_id', 'assigned_user_id', 'assignee_user_id', 'recipient_user_id']);
    if (reviewer !== undefined) targets.add(reviewer);
    else if (assignedUserId.trim() !== '') targets.add(assignedUserId);
  } else {
    for (const id of userIds) targets.add(id);
  }
  return [...targets].sort(compareRust);
}

const NOTIF_TYPE = { needs_review: 'approval_requested', waiting: 'waiting_on_user', blocked: 'app_blocked', completed: 'app_completed', archived: 'watch_update' };

/** project_app_record_document (:2689). */
export async function projectAppRecord(ctx, collection, document, projections) {
  const spec = appRelevanceSpec(collection);
  if (!spec) return;
  const recordId = valueString(document, 'id');
  if (recordId === '') return;
  const userIds = appRecordUserIds(document);
  if (userIds.length === 0) return;
  const pseudo = actorFromAppRecord(document, userIds);
  const actor = pseudo.actor.id;
  const source = appSourceContext(spec, document, recordId);
  const threadId = appThreadId(spec, source, recordId);
  const participants = await participantSet(ctx, threadId, [actor], userIds);
  const assigned = appAssignedUserId(document, userIds) ?? actor;
  const status = appThreadStatus(collection, document);
  const title = appThreadTitle(spec, document, source, recordId);
  const eventMessageId = appStatusDeservesEvent(status)
    ? `msg_app_${slugPart(collection)}_${slugPart(recordId)}_${slugPart(status)}` : undefined;
  const now = documentUpdatedAtMs(document);
  const binding = await supervisorBindingForThread(ctx, await threadOwnerForBinding(ctx, pseudo, threadId), threadId);
  await ctx.transaction(async (tx) => {
    await upsertThread(tx, {
      threadId, title, kind: spec.kind, status, participants, source, command: pseudo, assignedUserId: assigned,
      lastMessageId: eventMessageId, now, pendingDelta: 0, binding,
    }, projections);
    const linkId = `link_${slugPart(threadId)}_${slugPart(spec.collection)}_${slugPart(recordId)}`;
    await upsertIfChanged(tx, 'user_thread_links', linkId, now, {
      id: linkId,
      thread_id: threadId,
      source_module: spec.module,
      source_record_type: spec.record_type,
      source_record_id: recordId,
      source_label: appThreadLabel(document, recordId),
      link_role: 'app_record',
      link_type: spec.link_type,
      app_collection: spec.collection,
      command_id: valueString(document, 'command_id'),
      task_id: firstNonEmpty([valueString(document, 'task_id'), valueString(document, 'task_queue_id')]),
      deep_link: `#${spec.module}?record=${slugPart(recordId)}&record_type=${slugPart(spec.record_type)}`,
      context: { collection: spec.collection, record_id: recordId, status: firstStringField(document, ['status', 'approval_status', 'decision']) ?? '' },
      created_at_ms: now,
      updated_at_ms: now,
    }, projections);
    if (eventMessageId !== undefined) {
      const body = appStatusBody(collection, document, status);
      const targets = appNotificationTargets(document, userIds, assigned, status);
      await upsertMessage(tx, {
        threadId, messageId: eventMessageId, kind: 'app_event', command: pseudo, targetUserIds: targets, body, source,
        approvalRequestId: '', commandId: valueString(document, 'command_id'), now,
      }, projections);
      for (const userId of targets) {
        if (userId.trim() === '') continue;
        const id = `notif_app_${slugPart(status)}_${slugPart(collection)}_${slugPart(recordId)}_${slugPart(userId)}`;
        const existing = await loadRecord(tx, 'user_notifications', id);
        const existingStatus = existing ? valueString(existing, 'status') : '';
        await upsertIfChanged(tx, 'user_notifications', id, now, {
          id, notification_id: id, user_id: userId, thread_id: threadId, message_id: eventMessageId, approval_request_id: '',
          notification_type: NOTIF_TYPE[status] ?? 'watch_update', status: existingStatus !== '' ? existingStatus : 'unread',
          title, body_preview: truncate(body, 180), source_module: sourceString(source, 'module') ?? '',
          source_record_id: sourceString(source, 'record_id') ?? '', created_at_ms: now, updated_at_ms: now,
        }, projections);
      }
    }
    await refreshThreadStates(tx, threadId, now, projections);
  });
}

/**
 * project_app_relevance (:1056). input = {source_cursors:[{collection, since_ms}], limit} (Cursor) oder
 * {collection, docs} (Änderung, W-4). Ergebnis {changed_count, source_cursors, projections}.
 */
export default async function project(input, ctx) {
  const limit = Math.min(Math.max(Number(input?.limit) || 500, 1), 2000);
  const projections = [];
  const cursors = [];
  const batches = Array.isArray(input?.docs)
    ? [{ collection: input.collection, since_ms: 0, docs: input.docs }]
    : (input?.source_cursors || []).map((c) => ({ ...c }));
  for (const batch of batches) {
    if (!appRelevanceSpec(batch.collection)) continue;
    const since = Number(batch.since_ms) || 0;
    const docs = batch.docs ?? await ctx.read(batch.collection, {
      selector: { updated_at_ms: { $gte: since } }, sort: [{ updated_at_ms: 'asc' }, { id: 'asc' }], limit,
    });
    let max = since;
    for (const d of docs) {
      max = Math.max(max, documentUpdatedAtMs(d));
      if (documentIsDeleted(d)) continue;
      await projectAppRecord(ctx, batch.collection, d, projections);
    }
    cursors.push({ collection: batch.collection, since_ms: max });
  }
  const seen = new Set();
  const pairs = projections.filter((p) => { const k = `${p.collection}/${p.record_id}`; if (seen.has(k)) return false; seen.add(k); return true; })
    .map((p) => ({ collection: p.collection, record_id: p.record_id }));
  return { changed_count: pairs.length, source_cursors: cursors, projections: pairs };
}
