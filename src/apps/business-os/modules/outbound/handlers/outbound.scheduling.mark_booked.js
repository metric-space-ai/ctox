// ref: store_outbound_commands.rs:1098-1139 ("outbound.scheduling.mark_booked")
import {
  requiredFromPayloadOrRecord, loadRequired, str, mergeFields, updateEngagementStatus,
} from './_lib/outbound.js';

export default async function handle(command, ctx) {
  const now = ctx.now();
  const requestId = requiredFromPayloadOrRecord(command, ['meeting_request_id', 'id'], 'meeting_request_id is required');
  return ctx.transaction(async (tx) => {
    const request = await loadRequired(tx, 'outbound_meeting_requests', requestId, 'meeting request not found');
    request.status = 'booked';
    mergeFields(request, command.payload, ['meeting_url', 'booked_at_ms', 'payload']);
    request.updated_at_ms = now;
    await tx.put('outbound_meeting_requests', request);
    let engagement = null;
    const engagementId = str(request, ['engagement_id']);
    if (engagementId != null) {
      await updateEngagementStatus(tx, engagementId, 'meeting_booked', now);
      engagement = await loadRequired(tx, 'outbound_engagements', engagementId, 'engagement not found');
    }
    return { ok: true, meeting_request: request, engagement };
  });
}
