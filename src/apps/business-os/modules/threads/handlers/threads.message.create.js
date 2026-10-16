// threads.message.create — Port von create_message (threads.rs:1330-1417).
import {
  actorId, ensureSourceContextReadPolicy, ensureThreadParticipantOrAdmin, fail,
  firstStringField, idFromPayloadOrRecord, loadRecord, newUuid, participantSet, payloadOf, preflight,
  projectionValues, refreshThreadStates, requiredCommandId, requiredString, supervisorBindingForThread,
  targetUserIds, threadOwnerForBinding, threadSourceContext, upsertMessage, upsertNotifications, upsertThread,
} from './_lib.js';

export const MESSAGE_KINDS = ['message', 'handoff', 'ai_request'];

/** Interner Kern; auch von threads.handoff.create und threads.ai.request genutzt (Rust: delegated command). */
export async function createMessage(ctx, command) {
  const now = ctx.now();
  const payload = payloadOf(command);
  const threadId = idFromPayloadOrRecord(command, ['thread_id'], 'thread_id is required');
  const body = requiredString(payload, ['body', 'message', 'note']);
  const kind = firstStringField(payload, ['kind', 'event_type']) ?? 'message';
  if (!MESSAGE_KINDS.includes(kind)) fail('invalid', 'invalid thread message kind');
  const targets = targetUserIds(payload);
  const actor = actorId(command);
  const thread = await loadRecord(ctx, 'user_threads', threadId);
  if (!thread) fail('not_found', `thread ${threadId} not found`);
  ensureThreadParticipantOrAdmin(command, thread);
  // Rust: thread_source_context(&thread) ist immer Some → command-Kontext wird nie genutzt.
  const source = threadSourceContext(thread);
  await ensureSourceContextReadPolicy(ctx, source);
  // Rust: `.get("title").and_then(as_str)` – leerer Titel bleibt leer.
  const title = typeof thread.title === 'string' ? thread.title : 'Thread';
  const participants = await participantSet(ctx, threadId, [actor], targets);
  const messageId = firstStringField(payload, ['message_id']) ?? `msg_${newUuid(ctx, command)}`;
  const commandId = requiredCommandId(command);
  const binding = await supervisorBindingForThread(ctx, await threadOwnerForBinding(ctx, command, threadId), threadId);
  const projections = [];
  await ctx.transaction(async (tx) => {
    await upsertThread(tx, {
      threadId, title, kind: 'thread', status: 'open', participants, source, command,
      // Rust: erster Adressat wird Zuständiger, sonst '' (auch wenn vorher zugewiesen).
      assignedUserId: targets[0] ?? '', lastMessageId: messageId, now, pendingDelta: 0, binding,
    }, projections);
    await upsertMessage(tx, {
      threadId, messageId, kind, command, targetUserIds: targets, body, source,
      approvalRequestId: '', commandId, now,
    }, projections);
    await upsertNotifications(tx, {
      threadId, messageId, approvalRequestId: '', notificationType: 'message', userIds: targets,
      title, body, source, now,
    }, projections);
    await refreshThreadStates(tx, threadId, now, projections);
  });
  return { ok: true, thread_id: threadId, message_id: messageId, projections: projectionValues(projections) };
}

export default async function handle(command, ctx) {
  await preflight(ctx, command);
  return createMessage(ctx, command);
}

