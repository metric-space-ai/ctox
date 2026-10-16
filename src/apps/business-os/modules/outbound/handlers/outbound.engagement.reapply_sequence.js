// ref: store_outbound_commands.rs:5389 outbound_handle_engagement_reapply_sequence
import { requiredString, loadRequired, str, firstString, fail, payloadInsert } from './_lib/outbound.js';

export default async function handle(command, ctx) {
  const now = ctx.now();
  const engagementId = requiredString(command.payload, ['engagement_id']);
  return ctx.transaction(async (tx) => {
    const engagement = await loadRequired(tx, 'outbound_engagements', engagementId, 'engagement not found');
    const sequenceId = firstString([
      str(command.payload, ['sequence_id']),
      str(engagement, ['sequence_id']),
      str(engagement, ['payload', 'sequence_id']),
    ]);
    if (sequenceId == null) fail('invalid', 'sequence_id is required to reapply');
    const sequence = await loadRequired(tx, 'outbound_sequences', sequenceId, 'sequence not found');
    payloadInsert(engagement, 'sequence_id', sequenceId);
    payloadInsert(engagement, 'sequence_snapshot', sequence);
    payloadInsert(engagement, 'sequence_reapplied_at_ms', now);
    engagement.updated_at_ms = now;
    await tx.put('outbound_engagements', engagement);
    return { ok: true, engagement, sequence_id: sequenceId };
  });
}
