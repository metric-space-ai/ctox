// ref: store_outbound_commands.rs:971-1037 ("outbound.engagement.close")
import { requiredFromPayloadOrRecord, loadRequired, str, loadByStringField, payloadInsert } from './_lib/outbound.js';

const UNTOUCHED = new Set(['sent', 'accepted', 'queued_for_provider', 'cancelled']);

export default async function handle(command, ctx) {
  const now = ctx.now();
  const engagementId = requiredFromPayloadOrRecord(command, ['engagement_id', 'id'], 'engagement_id is required');
  const reason = str(command.payload, ['reason']);
  return ctx.transaction(async (tx) => {
    const engagement = await loadRequired(tx, 'outbound_engagements', engagementId, 'engagement not found');
    engagement.status = 'closed';
    if (reason != null && reason.trim()) engagement.closed_reason = reason;
    engagement.closed_at_ms = now;
    engagement.updated_at_ms = now;
    await tx.put('outbound_engagements', engagement);
    const closedMessageIds = [];
    for (const message of await loadByStringField(tx, 'outbound_messages', 'engagement_id', engagementId)) {
      if (UNTOUCHED.has(str(message, ['send_status']) ?? '')) continue;
      message.send_status = 'cancelled';
      if (reason != null) payloadInsert(message, 'close_reason', reason);
      message.updated_at_ms = now;
      const messageId = str(message, ['id']);
      if (messageId != null) {
        await tx.put('outbound_messages', message);
        closedMessageIds.push(messageId);
      }
    }
    return { ok: true, engagement, closed_message_ids: closedMessageIds };
  });
}
