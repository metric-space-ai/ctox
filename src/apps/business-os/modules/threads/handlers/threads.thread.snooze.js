// threads.thread.snooze — Port von snooze_thread (threads.rs:1973-2009).
import {
  asI64, clone, ensureThreadParticipantOrAdmin, fail, firstPresent, idFromPayloadOrRecord, loadRecord, payloadOf,
  preflight, projectionValues, refreshThreadStates, upsertRecord,
} from './_lib.js';

export default async function handle(command, ctx) {
  await preflight(ctx, command);
  const now = ctx.now();
  const threadId = idFromPayloadOrRecord(command, ['thread_id'], 'thread_id is required');
  const until = asI64(firstPresent(payloadOf(command), ['snoozed_until_ms', 'until_ms']));
  if (until === undefined || !(until > now)) fail('invalid', 'future snoozed_until_ms is required');
  const thread = await loadRecord(ctx, 'user_threads', threadId);
  if (!thread) fail('not_found', `thread ${threadId} not found`);
  ensureThreadParticipantOrAdmin(command, thread);
  const next = clone(thread);
  next.status = 'snoozed';
  next.snoozed_until_ms = until;
  next.updated_at_ms = now;
  const projections = [{ collection: 'user_threads', record_id: threadId }];
  await ctx.transaction(async (tx) => {
    await upsertRecord(tx, 'user_threads', threadId, now, next);
    await refreshThreadStates(tx, threadId, now, projections);
  });
  return { ok: true, thread_id: threadId, status: 'snoozed', snoozed_until_ms: until, projections: projectionValues(projections) };
}
