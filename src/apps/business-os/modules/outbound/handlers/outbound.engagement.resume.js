// ref: store_outbound_commands.rs:922-970 ("outbound.engagement.resume")
import {
  requiredFromPayloadOrRecord, loadRequired, str, fail, loadByStringField, payloadInsert,
  engagementStatusForMessageState,
} from './_lib/outbound.js';

export default async function handle(command, ctx) {
  const now = ctx.now();
  const engagementId = requiredFromPayloadOrRecord(command, ['engagement_id', 'id'], 'engagement_id is required');
  return ctx.transaction(async (tx) => {
    const engagement = await loadRequired(tx, 'outbound_engagements', engagementId, 'engagement not found');
    if (str(engagement, ['status']) !== 'paused') fail('conflict', 'only paused engagements can be resumed');
    const messages = await loadByStringField(tx, 'outbound_messages', 'engagement_id', engagementId);
    // Rust iterates SQLite row order (unspecified); we take the first paused one.
    const paused = messages.find((message) => str(message, ['send_status']) === 'paused');
    engagement.status = paused ? engagementStatusForMessageState(paused) : 'assigned';
    engagement.paused_reason = '';
    payloadInsert(engagement, 'resume_reason', str(command.payload, ['reason']) ?? 'manual_resume');
    engagement.updated_at_ms = now;
    await tx.put('outbound_engagements', engagement);
    return { ok: true, engagement };
  });
}
