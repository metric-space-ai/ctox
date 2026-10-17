// threads.thread.assign — Port von assign_thread(…, claim=false) (threads.rs:2078-2141).
import {
  activeBusinessUserDisplayName, actorId, arrayStrings, asI64, clone, documentUpdatedAtMs, ensureThreadParticipantOrAdmin,
  fail, firstStringField, idFromPayloadOrRecord, loadRecord, payloadOf, preflight, projectionValues,
  refreshThreadStates, requiredString, sortedUnique, upsertRecord, valueString,
} from './_lib.js';

/** Gemeinsamer Kern für assign und claim; auch von threads.handoff.create genutzt. */
export async function assignThread(ctx, command, claim) {
  const now = ctx.now();
  const payload = payloadOf(command);
  const threadId = idFromPayloadOrRecord(command, ['thread_id'], 'thread_id is required');
  const thread = await loadRecord(ctx, 'user_threads', threadId);
  if (!thread) fail('not_found', `thread ${threadId} not found`);
  ensureThreadParticipantOrAdmin(command, thread);
  const expected = asI64(payload.expected_updated_at_ms);
  if (expected !== undefined && documentUpdatedAtMs(thread) !== expected) {
    fail('conflict', 'thread changed; refresh before assigning');
  }
  if (claim) {
    const current = valueString(thread, 'assigned_user_id');
    if (!(current === '' || current === actorId(command))) fail('conflict', 'thread was already claimed');
  }
  const assignee = claim ? actorId(command) : requiredString(payload, ['assigned_user_id', 'user_id']);
  await activeBusinessUserDisplayName(ctx, assignee);
  const next = clone(thread);
  next.participant_ids = sortedUnique([...arrayStrings(thread.participant_ids), assignee]);
  next.assigned_user_id = assignee;
  next.status = 'open';
  const due = asI64(payload.due_at_ms);
  if (due !== undefined) next.due_at_ms = Math.max(0, due);
  const nextStep = firstStringField(payload, ['next_step']);
  if (nextStep !== undefined) next.next_step = nextStep;
  next.updated_at_ms = now;
  const projections = [{ collection: 'user_threads', record_id: threadId }];
  await ctx.transaction(async (tx) => {
    await upsertRecord(tx, 'user_threads', threadId, now, next);
    await refreshThreadStates(tx, threadId, now, projections);
  });
  return { ok: true, thread_id: threadId, assigned_user_id: assignee, projections: projectionValues(projections) };
}

export default async function handle(command, ctx) {
  await preflight(ctx, command);
  return assignThread(ctx, command, false);
}
