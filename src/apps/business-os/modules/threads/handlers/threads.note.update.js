// threads.note.update — Port von update_note (threads.rs:1237-1299).
import {
  clone, ensureMessageAuthorOrAdmin, fail, idFromPayloadOrRecord, loadRecord, payloadOf, preflight,
  projectionValues, refreshThreadStates, requiredString, targetUserIds, threadSourceContext, upsertNotifications,
  upsertRecord, upsertThreadStatusDelta, valueString,
} from './_lib.js';

export default async function handle(command, ctx) {
  await preflight(ctx, command);
  const now = ctx.now();
  const payload = payloadOf(command);
  const messageId = idFromPayloadOrRecord(command, ['message_id', 'note_id', 'id'], 'message_id is required');
  const body = requiredString(payload, ['body', 'message', 'note']);
  const message = await loadRecord(ctx, 'user_thread_messages', messageId);
  if (!message) fail('not_found', `thread message ${messageId} not found`);
  ensureMessageAuthorOrAdmin(command, message);
  const next = clone(message);
  next.body = body;
  next.updated_at_ms = now;
  const nextTargets = targetUserIds(payload);
  if (nextTargets.length > 0) next.target_user_ids = nextTargets;
  const threadId = valueString(message, 'thread_id');
  // Rust: thread_source_context ist immer Some → command-Kontext wird nie genutzt.
  const source = threadSourceContext(message);
  const projections = [];
  await ctx.transaction(async (tx) => {
    await upsertRecord(tx, 'user_thread_messages', messageId, now, next);
    projections.push({ collection: 'user_thread_messages', record_id: messageId });
    if (threadId !== '') await upsertThreadStatusDelta(tx, threadId, 'open', messageId, now, 0, projections);
    if (nextTargets.length > 0) {
      await upsertNotifications(tx, {
        threadId, messageId, approvalRequestId: '', notificationType: 'mention', userIds: nextTargets,
        title: 'Thread aktualisiert', body, source, now,
      }, projections);
    }
    await refreshThreadStates(tx, threadId, now, projections);
  });
  return { ok: true, message_id: messageId, thread_id: threadId, projections: projectionValues(projections) };
}
