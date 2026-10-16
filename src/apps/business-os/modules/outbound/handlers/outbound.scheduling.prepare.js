// ref: store_outbound_commands.rs:1078-1097 ("outbound.scheduling.prepare")
import {
  idFromCommand, requiredString, objectPayload, putDefaultString, putDefaultI64, updateEngagementStatus,
} from './_lib/outbound.js';

export default async function handle(command, ctx) {
  const now = ctx.now();
  const requestId = idFromCommand(command, ['meeting_request_id', 'id'], 'meeting');
  const engagementId = requiredString(command.payload, ['engagement_id']);
  return ctx.transaction(async (tx) => {
    const request = objectPayload(command.payload);
    request.id = requestId;
    request.engagement_id = engagementId;
    putDefaultString(request, 'status', 'prepared');
    putDefaultI64(request, 'created_at_ms', now);
    request.updated_at_ms = now;
    await tx.put('outbound_meeting_requests', request);
    await updateEngagementStatus(tx, engagementId, 'scheduling', now);
    return { ok: true, meeting_request: request };
  });
}
