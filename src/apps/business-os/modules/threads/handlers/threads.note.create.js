// threads.note.create — Port von create_note (threads.rs:1134-1235).
import {
  actorId, ensureExistingThreadParticipantOrAdmin, ensureSourceContextReadPolicy, firstStringField, newUuid,
  nonEmptyString, participantSet, payloadOf, preflight, projectionValues, refreshThreadStates, requiredCommandId,
  requiredString, sourceContext, sourceString, supervisorBindingForThread, targetUserIds, threadIdForCommand,
  threadOwnerForBinding, upsertMessage, upsertNotifications, upsertSourceLink, upsertThread, valueString,
} from './_lib.js';

export async function createNote(ctx, command) {
  const now = ctx.now();
  const payload = payloadOf(command);
  const body = requiredString(payload, ['body', 'message', 'note']);
  const source = sourceContext(command);
  await ensureSourceContextReadPolicy(ctx, source);
  // Rust: erster nicht-leerer Wert von message_type|kind, danach Filter note|mention.
  const rawKind = firstStringField(payload, ['message_type', 'kind']);
  const messageKind = rawKind === 'note' || rawKind === 'mention' ? rawKind : 'note';
  const threadId = threadIdForCommand(ctx, command, source);
  const existing = await ensureExistingThreadParticipantOrAdmin(ctx, command, threadId);
  const title = firstStringField(payload, ['title', 'subject'])
    ?? (existing ? nonEmptyString(existing, 'title') : undefined)
    ?? sourceString(source, 'label')
    ?? 'Notiz';
  const threadKind = (existing ? nonEmptyString(existing, 'kind') : undefined) ?? messageKind;
  const threadStatus = (existing ? nonEmptyString(existing, 'status') : undefined) ?? 'open';
  // Rust: Adressieren einer Notiz/Erwähnung weist nicht zu und öffnet die Quelle nicht neu.
  const assignedUserId = existing ? valueString(existing, 'assigned_user_id') : '';
  const targets = targetUserIds(payload);
  const actor = actorId(command);
  const participants = await participantSet(ctx, threadId, [actor], targets);
  const messageId = firstStringField(payload, ['message_id', 'note_id']) ?? `msg_${newUuid(ctx, command)}`;
  const commandId = requiredCommandId(command);
  const binding = await supervisorBindingForThread(ctx, await threadOwnerForBinding(ctx, command, threadId), threadId);
  const projections = [];
  await ctx.transaction(async (tx) => {
    await upsertThread(tx, {
      threadId, title, kind: threadKind, status: threadStatus, participants, source, command,
      assignedUserId, lastMessageId: messageId, now, pendingDelta: 0, binding,
    }, projections);
    await upsertSourceLink(tx, threadId, source, now, projections);
    await upsertMessage(tx, {
      threadId, messageId, kind: messageKind, command, targetUserIds: targets, body, source,
      approvalRequestId: '', commandId, now,
    }, projections);
    await upsertNotifications(tx, {
      threadId, messageId, approvalRequestId: '',
      notificationType: messageKind === 'mention' ? 'mention' : 'note',
      userIds: targets, title, body, source, now,
    }, projections);
    await refreshThreadStates(tx, threadId, now, projections);
  });
  return { ok: true, thread_id: threadId, message_id: messageId, projections: projectionValues(projections) };
}

export default async function handle(command, ctx) {
  await preflight(ctx, command);
  return createNote(ctx, command);
}
