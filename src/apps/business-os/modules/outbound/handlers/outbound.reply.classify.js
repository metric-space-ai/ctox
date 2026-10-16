// ref: store_outbound_commands.rs:1038-1077 ("outbound.reply.classify")
import {
  requiredString, loadRequired, payloadInsert, mergeFields, applyOutOfOfficeWait, applyReplySuppression,
} from './_lib/outbound.js';

export default async function handle(command, ctx) {
  const now = ctx.now();
  const engagementId = requiredString(command.payload, ['engagement_id']);
  const classification = requiredString(command.payload, ['classification']);
  return ctx.transaction(async (tx) => {
    const engagement = await loadRequired(tx, 'outbound_engagements', engagementId, 'engagement not found');
    engagement.status = 'reply_received';
    payloadInsert(engagement, 'reply_classification', classification);
    mergeFields(engagement, command.payload, ['reply_message_id']);
    if (classification === 'out_of_office') applyOutOfOfficeWait(engagement, command.payload, now);
    engagement.updated_at_ms = now;
    await tx.put('outbound_engagements', engagement);
    const suppressionId = await applyReplySuppression(tx, engagement, engagementId, classification, now);
    // Rust returns the engagement as written BEFORE the suppression hard-stop.
    return { ok: true, engagement, suppression_id: suppressionId };
  });
}
