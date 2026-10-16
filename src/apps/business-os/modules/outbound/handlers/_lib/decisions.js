// Approval decisions shared by approve / reject / request_changes.
// ref: store_outbound_commands.rs:586-652 (approve), :6804 outbound_record_rejection,
//      :6857 outbound_record_change_request
import {
  requiredFromPayloadOrRecord, loadRequired, str, idFromCommand, objectPayload,
  putDefaultI64, messageRevision, actorId, updateEngagementStatus, recordApprovalDecisionEvent,
} from './outbound.js';

/** ref: rejection / change request (identical shape, different constants). */
export async function recordDecision(command, ctx, spec) {
  const now = ctx.now();
  const messageId = requiredFromPayloadOrRecord(command, ['message_id', 'id'], 'message_id is required');
  const result = await ctx.transaction(async (tx) => {
    const message = await loadRequired(tx, 'outbound_messages', messageId, 'message not found');
    const revisionId = str(message, ['revision_id']) ?? messageRevision(message);
    let approvalId;
    try {
      approvalId = idFromCommand(command, ['approval_id'], spec.idPrefix);
    } catch {
      approvalId = `${spec.fallbackPrefix}_${messageId}_${revisionId}`;
    }
    const engagementId = str(message, ['engagement_id']) ?? '';
    const approval = objectPayload(command.payload);
    approval.id = approvalId;
    approval.message_id = messageId;
    approval.engagement_id = engagementId;
    approval.revision_id = revisionId;
    approval.actor_user_id = actorId(command);
    approval.decision = spec.decision;
    putDefaultI64(approval, 'created_at_ms', now);
    approval.updated_at_ms = now;
    await tx.put('outbound_approvals', approval);
    for (const [key, value] of Object.entries(spec.messageFields)) message[key] = value;
    message.updated_at_ms = now;
    await tx.put('outbound_messages', message);
    if (engagementId) await updateEngagementStatus(tx, engagementId, spec.engagementStatus, now);
    return { ok: true, message, approval };
  });
  await recordApprovalDecisionEvent(ctx, command, result.approval, result.message, now);
  return result;
}

