// ref: store_outbound_commands.rs:551-585 ("outbound.message.request_approval")
import {
  requiredFromPayloadOrRecord, loadRequired, str, requireMessageContent, messageRevision,
  updateEngagementStatus,
} from './_lib/outbound.js';

export default async function handle(command, ctx) {
  const now = ctx.now();
  const messageId = requiredFromPayloadOrRecord(command, ['message_id', 'id'], 'message_id is required');
  return ctx.transaction(async (tx) => {
    const message = await loadRequired(tx, 'outbound_messages', messageId, 'message not found');
    requireMessageContent(message);
    message.revision_id = messageRevision(message);
    message.draft_status = 'ready_for_review';
    message.approval_status = 'awaiting_approval';
    message.send_status = 'awaiting_approval';
    message.updated_at_ms = now;
    await tx.put('outbound_messages', message);
    const engagementId = str(message, ['engagement_id']);
    if (engagementId != null) await updateEngagementStatus(tx, engagementId, 'awaiting_approval', now);
    return { ok: true, message };
  });
}
