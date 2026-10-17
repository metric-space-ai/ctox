// threads.ctox_approval.request — Port von request_approval (threads.rs:1419-1537) + approval_record (:3394).
import {
  activeBusinessUserDisplayName, actorDisplayName, actorId, clientContextOf, clone, ensureExistingThreadParticipantOrAdmin,
  ensureSourceContextReadPolicy, fail, firstStringField, has, isAdmin, newUuid, participantSet, payloadOf, preflight,
  projectionValues, refreshThreadStates, requiredCommandId, requiredString, sourceContext, sourceString,
  supervisorBindingForThread, threadIdForCommand, threadOwnerForBinding, upsertMessage, upsertNotifications, upsertRecord,
  upsertSourceLink, upsertThread, commandRecordId,
} from './_lib.js';
import { approvalCallback } from './_agent.js';
import { AGENT_TASK_TARGET, ensureAllowedApprovalTarget } from './_approval_targets.js';

/** Zieltyp einer Freigabe-Anfrage (Vorgabe Agentenarbeit). */
export const approvalTargetTypeOf = (payload) => firstStringField(payload, ['target_command_type', 'command_type']) ?? AGENT_TASK_TARGET;

/** approval_record (threads.rs:3394). */
export function approvalRecord({ approvalId, threadId, status, command, reviewerUserId, reviewerDisplayName, decisionNote, source, prompt, now, decidedAtMs }) {
  const payload = payloadOf(command);
  const targetModule = firstStringField(payload, ['target_module']) ?? sourceString(source, 'module') ?? 'ctox';
  const targetRecordId = firstStringField(payload, ['target_record_id'])
    ?? sourceString(source, 'record_id') ?? commandRecordId(command) ?? '';
  return {
    id: approvalId,
    approval_request_id: approvalId,
    thread_id: threadId,
    status,
    requester_user_id: actorId(command),
    requester_display_name: actorDisplayName(command),
    reviewer_user_id: reviewerUserId,
    reviewer_display_name: reviewerDisplayName,
    prompt,
    instruction: firstStringField(payload, ['instruction']) ?? prompt,
    target_command_type: approvalTargetTypeOf(payload),
    target_module: targetModule,
    target_record_id: targetRecordId,
    source_module: sourceString(source, 'module') ?? '',
    source_record_type: sourceString(source, 'record_type') ?? '',
    source_record_id: sourceString(source, 'record_id') ?? '',
    source_label: sourceString(source, 'label') ?? '',
    source_deep_link: sourceString(source, 'deep_link') ?? '',
    requested_at_ms: now,
    decided_at_ms: decidedAtMs ?? 0,
    decision_by_id: '',
    decision_note: decisionNote,
    approved_command_id: '',
    approved_task_id: '',
    target_payload: has(payload, 'target_payload') ? clone(payload.target_payload) : {},
    // SKL-A2: Rückruf der anfragenden App (Ergebnis der freigegebenen Arbeit), siehe _agent.js approvalCallback.
    callback: approvalCallback(payload, source),
    source_context: source,
    // U-3: v4-Befehl hat kein client_context; Rust speichert command.client_context (sonst null).
    client_context: clone(clientContextOf(command)),
    created_at_ms: now,
    updated_at_ms: now,
  };
}

export default async function handle(command, ctx) {
  await preflight(ctx, command);
  const now = ctx.now();
  const payload = payloadOf(command);
  const prompt = requiredString(payload, ['prompt', 'instruction', 'body']);
  const reviewerUserId = requiredString(payload, ['reviewer_user_id', 'reviewer']);
  const reviewerDisplayName = await activeBusinessUserDisplayName(ctx, reviewerUserId);
  const source = sourceContext(command);
  // vor dem ersten Schreiben prüfen: festes Ziel (§16.40) und fester Rückruf der anfragenden App
  ensureAllowedApprovalTarget(approvalTargetTypeOf(payload));
  approvalCallback(payload, source);
  await ensureSourceContextReadPolicy(ctx, source);
  const threadId = threadIdForCommand(ctx, command, source);
  await ensureExistingThreadParticipantOrAdmin(ctx, command, threadId);
  const approvalId = firstStringField(payload, ['approval_request_id', 'id']) ?? `approval_${newUuid(ctx, command)}`;
  const messageId = `msg_${newUuid(ctx, command)}`;
  const title = firstStringField(payload, ['title', 'subject']) ?? sourceString(source, 'label') ?? 'CTOX Freigabe';
  const actor = actorId(command);
  const selfOverride = payload.allow_self_approval_admin_override === true && isAdmin(command);
  if (!(reviewerUserId !== actor || selfOverride)) {
    fail('invalid', 'requester cannot assign themselves as approval reviewer');
  }
  const participants = await participantSet(ctx, threadId, [actor, reviewerUserId], []);
  const commandId = requiredCommandId(command);
  const binding = await supervisorBindingForThread(ctx, await threadOwnerForBinding(ctx, command, threadId), threadId);
  const projections = [];
  await ctx.transaction(async (tx) => {
    await upsertThread(tx, {
      threadId, title, kind: 'approval', status: 'open', participants, source, command,
      assignedUserId: reviewerUserId, lastMessageId: messageId, now, pendingDelta: 1, binding,
    }, projections);
    await upsertSourceLink(tx, threadId, source, now, projections);
    const approval = approvalRecord({
      approvalId, threadId, status: 'pending', command, reviewerUserId, reviewerDisplayName,
      decisionNote: '', source, prompt, now, decidedAtMs: undefined,
    });
    await upsertRecord(tx, 'ctox_task_approval_requests', approvalId, now, approval);
    projections.push({ collection: 'ctox_task_approval_requests', record_id: approvalId });
    await upsertMessage(tx, {
      threadId, messageId, kind: 'approval_request', command, targetUserIds: [reviewerUserId], body: prompt,
      source, approvalRequestId: approvalId, commandId, now,
    }, projections);
    await upsertNotifications(tx, {
      threadId, messageId, approvalRequestId: approvalId, notificationType: 'approval_request',
      userIds: [reviewerUserId], title, body: prompt, source, now,
    }, projections);
    await refreshThreadStates(tx, threadId, now, projections);
  });
  return { ok: true, thread_id: threadId, approval_request_id: approvalId, status: 'pending', projections: projectionValues(projections) };
}
