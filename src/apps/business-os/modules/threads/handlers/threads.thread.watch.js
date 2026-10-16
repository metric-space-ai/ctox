// threads.thread.watch — Port von update_thread_watch(…, true) (threads.rs:1890-1936).
import {
  actorId, arrayStrings, clone, ensureThreadParticipantOrAdmin, fail, idFromPayloadOrRecord, loadRecord, preflight,
  projectionValues, refreshThreadStates, sortedUnique, upsertRecord,
} from './_lib.js';

export async function updateThreadWatch(ctx, command, watch) {
  const now = ctx.now();
  const threadId = idFromPayloadOrRecord(command, ['thread_id'], 'thread_id is required');
  const thread = await loadRecord(ctx, 'user_threads', threadId);
  if (!thread) fail('not_found', `thread ${threadId} not found`);
  ensureThreadParticipantOrAdmin(command, thread);
  const actor = actorId(command);
  const next = clone(thread);
  let watchers = arrayStrings(thread.watcher_user_ids);
  if (watch) {
    if (!watchers.includes(actor)) watchers.push(actor);
    const participants = arrayStrings(thread.participant_ids);
    if (!participants.includes(actor)) participants.push(actor);
    next.participant_ids = sortedUnique(participants);
  } else {
    watchers = watchers.filter((id) => id !== actor);
  }
  next.watcher_user_ids = sortedUnique(watchers);
  next.updated_at_ms = now;
  const projections = [{ collection: 'user_threads', record_id: threadId }];
  await ctx.transaction(async (tx) => {
    await upsertRecord(tx, 'user_threads', threadId, now, next);
    await refreshThreadStates(tx, threadId, now, projections);
  });
  return { ok: true, thread_id: threadId, watching: watch, projections: projectionValues(projections) };
}

export default async function handle(command, ctx) {
  await preflight(ctx, command);
  return updateThreadWatch(ctx, command, true);
}
