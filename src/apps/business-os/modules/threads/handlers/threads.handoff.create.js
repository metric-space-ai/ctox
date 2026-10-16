// threads.handoff.create — Port von create_handoff (threads.rs:2143-2210).
// Ablauf wie Rust: prüfen → Nachricht (kind=handoff) → Zuweisung → Zustände neu.
// U-7: Wie in Rust drei getrennte Schreibschritte, keine gemeinsame Transaktion;
// alle Prüfungen (Version, Ziel aktiv) laufen vor dem ersten Schreiben.
import {
  activeBusinessUserDisplayName, asI64, clone, documentUpdatedAtMs, ensureThreadParticipantOrAdmin, fail,
  firstStringField, idFromPayloadOrRecord, isObject, loadRecord, payloadOf, preflight, projectionValues,
  refreshThreadStates, requiredString, valueString,
} from './_lib.js';
import { createMessage } from './threads.message.create.js';
import { assignThread } from './threads.thread.assign.js';

export default async function handle(command, ctx) {
  await preflight(ctx, command);
  const payload = payloadOf(command);
  const target = requiredString(payload, ['target_user_id', 'assigned_user_id']);
  const expectation = requiredString(payload, ['expectation', 'body', 'message']);
  const threadId = idFromPayloadOrRecord(command, ['thread_id'], 'thread_id is required');
  const thread = await loadRecord(ctx, 'user_threads', threadId);
  if (!thread) fail('not_found', `thread ${threadId} not found`);
  ensureThreadParticipantOrAdmin(command, thread);
  const expected = asI64(payload.expected_updated_at_ms);
  if (expected !== undefined && documentUpdatedAtMs(thread) !== expected) {
    fail('conflict', 'thread changed; refresh before handing off');
  }
  await activeBusinessUserDisplayName(ctx, target);
  const returnReason = firstStringField(payload, ['return_reason']) ?? '';
  const dueAtMs = Math.max(0, asI64(payload.due_at_ms) ?? 0);
  const body = returnReason === '' ? expectation : `${expectation}\n\nRückgabe wenn: ${returnReason}`;

  const delegatedPayload = isObject(command.payload) ? clone(command.payload) : {};
  delegatedPayload.kind = 'handoff';
  delegatedPayload.body = body;
  delegatedPayload.target_user_ids = [target];
  const result = await createMessage(ctx, { ...command, type: 'threads.message.create', payload: delegatedPayload });
  const resultThreadId = valueString(result, 'thread_id');
  const assignment = await assignThread(ctx, {
    ...command,
    type: 'threads.thread.assign',
    record_id: resultThreadId,
    payload: { thread_id: resultThreadId, assigned_user_id: target, due_at_ms: dueAtMs, next_step: expectation },
  }, false);
  const now = ctx.now();
  const projections = [];
  await ctx.transaction((tx) => refreshThreadStates(tx, resultThreadId, now, projections));
  return {
    ok: true,
    thread_id: resultThreadId,
    message: result,
    assignment,
    projections: projectionValues(projections),
  };
}
