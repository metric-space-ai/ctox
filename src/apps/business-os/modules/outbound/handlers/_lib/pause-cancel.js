// ref: store_outbound_commands.rs:836-881 ("outbound.message.pause" | "outbound.message.cancel")
import {
  requiredFromPayloadOrRecord, loadRequired, str, payloadInsert, updateEngagementTerminalStatus,
} from './outbound.js';

export async function pauseOrCancel(command, ctx, status) {
  const now = ctx.now();
  const messageId = requiredFromPayloadOrRecord(command, ['message_id', 'id'], 'message_id is required');
  return ctx.transaction(async (tx) => {
    const message = await loadRequired(tx, 'outbound_messages', messageId, 'message not found');
    const reason = str(command.payload, ['reason']);
    message.send_status = status;
    if (reason != null) payloadInsert(message, status === 'paused' ? 'pause_reason' : 'cancel_reason', reason);
    message.updated_at_ms = now;
    await tx.put('outbound_messages', message);
    const engagementId = str(message, ['engagement_id']);
    if (engagementId != null) await updateEngagementTerminalStatus(tx, engagementId, status, reason, now);
    return { ok: true, message };
  });
}
