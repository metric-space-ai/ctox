// threads.note.delete — Port von delete_note (threads.rs:1301-1328). Soft-Delete (is_deleted).
import {
  clone, ensureMessageAuthorOrAdmin, fail, idFromPayloadOrRecord, loadRecord, preflight, projectionValues,
  refreshThreadStates, upsertRecord, valueString,
} from './_lib.js';

export default async function handle(command, ctx) {
  await preflight(ctx, command);
  const now = ctx.now();
  const messageId = idFromPayloadOrRecord(command, ['message_id', 'note_id', 'id'], 'message_id is required');
  const message = await loadRecord(ctx, 'user_thread_messages', messageId);
  if (!message) fail('not_found', `thread message ${messageId} not found`);
  ensureMessageAuthorOrAdmin(command, message);
  const threadId = valueString(message, 'thread_id');
  const next = clone(message);
  next.is_deleted = true;
  next.deleted_at_ms = now;
  next.updated_at_ms = now;
  const projections = [{ collection: 'user_thread_messages', record_id: messageId }];
  await ctx.transaction(async (tx) => {
    await upsertRecord(tx, 'user_thread_messages', messageId, now, next);
    await refreshThreadStates(tx, threadId, now, projections);
  });
  return { ok: true, message_id: messageId, thread_id: threadId, projections: projectionValues(projections) };
}
