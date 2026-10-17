// threads.ctox_approval.reject — Port von reject_approval → decide_without_queue (threads.rs:1754, 1779-1888).
import {
  approvalIdFromCommand, approvalSource, clone, ensureApprovalVersion, ensurePendingApproval, ensureReviewerOrAdmin,
  firstStringField, loadApproval, newUuid, payloadOf, preflight, projectionValues, recordApprovalDecisionEvent,
  refreshThreadStates, actorId, upsertMessage, upsertNotifications, upsertRecord, upsertThreadStatusDelta, valueString,
} from './_lib.js';

/** decide_without_queue: status "rejected" | "cancelled". */
export async function decideWithoutQueue(ctx, command, status, messageKind) {
  const now = ctx.now();
  const approvalId = approvalIdFromCommand(command);
  const approval = await loadApproval(ctx, approvalId);
  ensurePendingApproval(approval);
  ensureApprovalVersion(command, approval);
  if (status === 'rejected') ensureReviewerOrAdmin(command, approval);
  const threadId = valueString(approval, 'thread_id');
  const source = approvalSource(approval);
  const requester = valueString(approval, 'requester_user_id');
  const prompt = valueString(approval, 'prompt');
  const decisionNote = firstStringField(payloadOf(command), ['decision_note', 'note']) ?? '';
  const messageBody = decisionNote === '' ? `${status}: ${prompt}` : `${status}: ${decisionNote}\n\n${prompt}`;
  const messageId = `msg_${newUuid(ctx, command)}`;
  const projections = [];
  await ctx.transaction(async (tx) => {
    const next = clone(approval);
    next.status = status;
    next.decided_at_ms = now;
    next.decision_by_id = actorId(command);
    next.decision_note = decisionNote;
    await upsertRecord(tx, 'ctox_task_approval_requests', approvalId, now, next);
    projections.push({ collection: 'ctox_task_approval_requests', record_id: approvalId });
    await upsertThreadStatusDelta(tx, threadId, 'open', messageId, now, -1, projections);
    await upsertMessage(tx, {
      threadId, messageId, kind: messageKind, command, targetUserIds: [requester], body: messageBody, source,
      approvalRequestId: approvalId, commandId: typeof command.id === 'string' ? command.id : '', now,
    }, projections);
    await upsertNotifications(tx, {
      threadId, messageId, approvalRequestId: approvalId, notificationType: messageKind, userIds: [requester],
      title: status === 'rejected' ? 'CTOX Freigabe abgelehnt' : 'CTOX Freigabe storniert',
      body: messageBody, source, now,
    }, projections);
    await refreshThreadStates(tx, threadId, now, projections);
  });
  // U-5: Rust schreibt das Audit-Ereignis direkt nach dem Freigabe-Datensatz (ohne Transaktion);
  // hier nach Commit der Transaktion über ctx.cap.audit.event.
  await recordApprovalDecisionEvent(ctx, command, approval, status, '', '', decisionNote, now);
  return { ok: true, approval_request_id: approvalId, thread_id: threadId, status, projections: projectionValues(projections) };
}

export default async function handle(command, ctx) {
  await preflight(ctx, command);
  return decideWithoutQueue(ctx, command, 'rejected', 'approval_rejected');
}
