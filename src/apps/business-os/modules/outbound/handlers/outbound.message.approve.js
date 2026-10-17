// ref: store_outbound_commands.rs:586-652 ("outbound.message.approve")
import {
  requiredFromPayloadOrRecord, loadRequired, str, fail, requireMessageContent, messageRevision, messageContentHash,
  idFromCommand, objectPayload, putDefaultObject, putDefaultI64, actorId, updateEngagementStatus,
  recordApprovalDecisionEvent,
} from './_lib/outbound.js';

export default async function handle(command, ctx) {
  const now = ctx.now();
  const messageId = requiredFromPayloadOrRecord(command, ['message_id', 'id'], 'message_id is required');
  const result = await ctx.transaction(async (tx) => {
    const message = await loadRequired(tx, 'outbound_messages', messageId, 'message not found');
    if (str(message, ['approval_status']) !== 'awaiting_approval') {
      fail('conflict', 'only messages awaiting approval can be approved');
    }
    requireMessageContent(message);
    // Approve recomputes the revision from content (unlike reject/request_changes).
    const revisionId = messageRevision(message);
    let approvalId;
    try {
      approvalId = idFromCommand(command, ['approval_id'], 'approval');
    } catch {
      approvalId = `approval_${messageId}_${revisionId}`;
    }
    const engagementId = str(message, ['engagement_id']) ?? '';
    const approval = objectPayload(command.payload);
    approval.id = approvalId;
    approval.message_id = messageId;
    approval.engagement_id = engagementId;
    approval.revision_id = revisionId;
    // Bind the approval to the approved content (INV1-B1); set after the payload copy
    // so a command cannot supply its own hash.
    approval.content_hash = messageContentHash(message);
    approval.actor_user_id = actorId(command);
    approval.decision = 'approved';
    putDefaultObject(approval, 'payload');
    putDefaultI64(approval, 'created_at_ms', now);
    approval.updated_at_ms = now;
    await tx.put('outbound_approvals', approval);
    message.revision_id = revisionId;
    message.approval_status = 'approved';
    message.draft_status = 'approved';
    message.send_status = 'approved_not_sent';
    message.updated_at_ms = now;
    await tx.put('outbound_messages', message);
    if (engagementId) await updateEngagementStatus(tx, engagementId, 'approved_for_send', now);
    return { ok: true, message, approval };
  });
  await recordApprovalDecisionEvent(ctx, command, result.approval, result.message, now);
  return result;
}
