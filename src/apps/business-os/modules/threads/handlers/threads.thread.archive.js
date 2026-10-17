// threads.thread.archive — Port von archive_thread (threads.rs:1938-1971).
import {
  clone, ensureThreadParticipantOrAdmin, fail, idFromPayloadOrRecord, loadRecord, preflight, projectionValues,
  refreshThreadStates, upsertRecord,
} from './_lib.js';

export default async function handle(command, ctx) {
  await preflight(ctx, command);
  const now = ctx.now();
  const threadId = idFromPayloadOrRecord(command, ['thread_id'], 'thread_id is required');
  const thread = await loadRecord(ctx, 'user_threads', threadId);
  if (!thread) fail('not_found', `thread ${threadId} not found`);
  ensureThreadParticipantOrAdmin(command, thread, 'only participants or admins can archive this thread');
  const next = clone(thread);
  next.status = 'archived';
  next.archived_at_ms = now;
  const projections = [{ collection: 'user_threads', record_id: threadId }];
  await ctx.transaction(async (tx) => {
    await upsertRecord(tx, 'user_threads', threadId, now, next);
    await refreshThreadStates(tx, threadId, now, projections);
  });
  return { ok: true, thread_id: threadId, status: 'archived', projections: projectionValues(projections) };
}
