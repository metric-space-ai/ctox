// threads.ctox_approval.edit — Port von edit_approval (threads.rs:1539-1642).
import {
  actorId, approvalIdFromCommand, approvalSource, clone, ensureApprovalEditor, ensureApprovalTargetUnchanged,
  ensureApprovalVersion, ensurePendingApproval, firstStringField, loadApproval, newUuid, payloadOf, preflight,
  projectionValues, refreshThreadStates, upsertMessage, upsertNotifications, upsertRecord, upsertThreadStatusDelta,
  valueString,
} from './_lib.js';

export default async function handle(command, ctx) {
  await preflight(ctx, command);
  const now = ctx.now();
  const payload = payloadOf(command);
  const approvalId = approvalIdFromCommand(command);
  const approval = await loadApproval(ctx, approvalId);
  ensurePendingApproval(approval);
  ensureApprovalVersion(command, approval);
  ensureApprovalEditor(command, approval);
  ensureApprovalTargetUnchanged(command, approval);
  const next = clone(approval);
  const prompt = firstStringField(payload, ['prompt', 'instruction', 'body']) ?? valueString(approval, 'prompt');
  next.prompt = prompt;
  next.instruction = firstStringField(payload, ['instruction']) ?? prompt;
  next.updated_at_ms = now;
  const threadId = valueString(approval, 'thread_id');
  const source = approvalSource(approval);
  const requester = valueString(approval, 'requester_user_id');
  const reviewer = valueString(approval, 'reviewer_user_id');
  const notifyUser = actorId(command) === requester ? reviewer : requester;
  const messageId = `msg_${newUuid(ctx, command)}`;
  const targets = notifyUser === '' ? [] : [notifyUser];
  const projections = [];
  await ctx.transaction(async (tx) => {
    await upsertRecord(tx, 'ctox_task_approval_requests', approvalId, now, next);
    projections.push({ collection: 'ctox_task_approval_requests', record_id: approvalId });
    await upsertThreadStatusDelta(tx, threadId, 'open', messageId, now, 0, projections);
    await upsertMessage(tx, {
      threadId, messageId, kind: 'approval_edited', command, targetUserIds: targets, body: prompt, source,
      approvalRequestId: approvalId, commandId: typeof command.id === 'string' ? command.id : '', now,
    }, projections);
    await upsertNotifications(tx, {
      threadId, messageId, approvalRequestId: approvalId, notificationType: 'approval_edited', userIds: targets,
      title: 'CTOX Freigabe aktualisiert', body: prompt, source, now,
    }, projections);
    await refreshThreadStates(tx, threadId, now, projections);
  });
  return { ok: true, approval_request_id: approvalId, thread_id: threadId, status: 'pending', projections: projectionValues(projections) };
}
