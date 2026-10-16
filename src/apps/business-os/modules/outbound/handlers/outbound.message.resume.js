// ref: store_outbound_commands.rs:882-921 ("outbound.message.resume")
import {
  requiredFromPayloadOrRecord, loadRequired, str, fail, payloadInsert, sendStatusForResume,
  engagementStatusForMessageState, updateEngagementStatus,
} from './_lib/outbound.js';

export default async function handle(command, ctx) {
  const now = ctx.now();
  const messageId = requiredFromPayloadOrRecord(command, ['message_id', 'id'], 'message_id is required');
  return ctx.transaction(async (tx) => {
    const message = await loadRequired(tx, 'outbound_messages', messageId, 'message not found');
    if (str(message, ['send_status']) !== 'paused') fail('conflict', 'only paused outbound messages can be resumed');
    message.send_status = sendStatusForResume(message);
    payloadInsert(message, 'resume_reason', str(command.payload, ['reason']) ?? 'manual_resume');
    message.updated_at_ms = now;
    await tx.put('outbound_messages', message);
    const engagementId = str(message, ['engagement_id']);
    if (engagementId != null) {
      await updateEngagementStatus(tx, engagementId, engagementStatusForMessageState(message), now);
    }
    return { ok: true, message };
  });
}
