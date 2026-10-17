// ref: store_outbound_commands.rs:5464 outbound_handle_scheduling_update_slots
import { requiredString, loadRequired } from './_lib/outbound.js';

export default async function handle(command, ctx) {
  const now = ctx.now();
  const payload = command.payload ?? {};
  const requestId = requiredString(payload, ['meeting_request_id']);
  return ctx.transaction(async (tx) => {
    const request = await loadRequired(tx, 'outbound_meeting_requests', requestId, 'meeting_request not found');
    for (const key of ['proposed_slots', 'duration_minutes', 'slot_strategy', 'calendar_account_id']) {
      if (key in payload) request[key] = payload[key];
    }
    request.updated_at_ms = now;
    await tx.put('outbound_meeting_requests', request);
    return { ok: true, meeting_request: request };
  });
}
