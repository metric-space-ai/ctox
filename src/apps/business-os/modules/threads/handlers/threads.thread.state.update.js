// threads.thread.state.update — Port von update_user_thread_state (threads.rs:2011-2076).
import {
  actorId, clone, ensureThreadParticipantOrAdmin, fail, firstStringField, has, idFromPayloadOrRecord, loadRecord,
  payloadOf, preflight, projectionValues, refreshThreadStates, threadStateId, upsertRecord,
} from './_lib.js';

const COPIED_KEYS = ['pinned', 'unread_count', 'last_seen_at_ms', 'snoozed_until_ms', 'follow_up_at_ms'];
const PRIORITIES = ['low', 'normal', 'high', 'urgent'];

export default async function handle(command, ctx) {
  await preflight(ctx, command);
  const now = ctx.now();
  const payload = payloadOf(command);
  const threadId = idFromPayloadOrRecord(command, ['thread_id'], 'thread_id is required');
  const thread = await loadRecord(ctx, 'user_threads', threadId);
  if (!thread) fail('not_found', `thread ${threadId} not found`);
  ensureThreadParticipantOrAdmin(command, thread);
  const userId = actorId(command);
  const stateId = threadStateId(userId, threadId);
  const existing = await loadRecord(ctx, 'user_thread_states', stateId);
  const next = existing ? clone(existing) : {
    id: stateId, thread_id: threadId, user_id: userId, created_at_ms: now, updated_at_ms: now,
    unread_count: 0, pinned: false, priority: 'normal', attention_score: 0, attention_reasons: [],
  };
  // Rust: `payload.get(key)` → Wert wird ungeprüft übernommen (auch null). U-9: kein Typcheck.
  for (const key of COPIED_KEYS) if (has(payload, key)) next[key] = clone(payload[key]);
  const priority = firstStringField(payload, ['priority']);
  if (priority !== undefined) {
    if (!PRIORITIES.includes(priority)) fail('invalid', 'invalid priority');
    next.priority = priority;
  }
  next.updated_at_ms = now;
  const projections = [{ collection: 'user_thread_states', record_id: stateId }];
  await ctx.transaction(async (tx) => {
    await upsertRecord(tx, 'user_thread_states', stateId, now, next);
    // Rust: refresh überschreibt danach unread_count/attention_* aus den Benachrichtigungen.
    await refreshThreadStates(tx, threadId, now, projections);
  });
  return { ok: true, thread_id: threadId, user_id: userId, state_id: stateId, projections: projectionValues(projections) };
}
