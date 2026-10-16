// ref: store_outbound_commands.rs:5771 outbound_handle_reply_match
// Fachlogik here; annotating the communication_messages row (App conversations, foreign write)
// is best effort (annotateCommunicationMessage in _lib/send.js; vormals ctx.cap.mail.message.annotate).
import { requiredString, loadRequired, str, payloadInsert, applyReplySuppression, loadByStringField } from './_lib/outbound.js';
import { annotateCommunicationMessage } from './_lib/send.js';

const UNTOUCHED = new Set(['sent', 'delivered', 'queued_for_provider', 'cancelled', 'paused']);

export default async function handle(command, ctx) {
  const now = ctx.now();
  const payload = command.payload ?? {};
  const engagementId = requiredString(payload, ['engagement_id']);
  const replyMessageKey = str(payload, ['reply_message_id']) ?? requiredString(payload, ['communication_message_key']);
  const classification = str(payload, ['classification']) ?? 'unclear';
  const outboundMessageId = str(payload, ['outbound_message_id']) ?? '';

  const result = await ctx.transaction(async (tx) => {
    const engagement = await loadRequired(tx, 'outbound_engagements', engagementId, 'engagement not found');
    engagement.status = 'reply_received';
    payloadInsert(engagement, 'reply_classification', classification);
    payloadInsert(engagement, 'reply_message_id', replyMessageKey);
    payloadInsert(engagement, 'reply_matched_at_ms', now);
    engagement.updated_at_ms = now;
    await tx.put('outbound_engagements', engagement);
    const suppressionId = await applyReplySuppression(tx, engagement, engagementId, classification, now);
    const cancelled = [];
    for (const message of await loadByStringField(tx, 'outbound_messages', 'engagement_id', engagementId)) {
      const sendStatus = str(message, ['send_status']) ?? 'draft';
      const direction = str(message, ['direction']) ?? 'outbound';
      if (direction !== 'outbound' || UNTOUCHED.has(sendStatus)) continue;
      const messageId = str(message, ['id']);
      if (messageId == null) continue;
      message.send_status = 'cancelled';
      payloadInsert(message, 'cancelled_reason', 'reply_received');
      payloadInsert(message, 'cancelled_at_ms', now);
      message.updated_at_ms = now;
      await tx.put('outbound_messages', message);
      cancelled.push(messageId);
    }
    return {
      ok: true,
      engagement,
      classification,
      reply_message_key: replyMessageKey,
      cancelled_message_ids: cancelled,
      suppression_id: suppressionId,
    };
  });

  // Best effort, errors ignored (Rust: `let _ = annotate_...`).
  try {
    const metadata = { outbound_engagement_id: engagementId, outbound_reply_classification: classification };
    if (outboundMessageId) metadata.outbound_message_id = outboundMessageId;
    await annotateCommunicationMessage(ctx, replyMessageKey, metadata);
  } catch (error) {
    ctx.log('warn', 'reply.match: annotate communication message failed', { code: error?.code, message: error?.message });
  }
  return result;
}
