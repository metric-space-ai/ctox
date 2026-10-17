// ref: store_outbound_commands.rs:653 → outbound_record_rejection :6804
import { recordDecision } from './_lib/decisions.js';

export default async function handle(command, ctx) {
  return recordDecision(command, ctx, {
    // Rust: outbound_id_from_command(.., "approval") with fallback "rejection_<msg>_<rev>"
    idPrefix: 'approval',
    fallbackPrefix: 'rejection',
    decision: 'rejected',
    messageFields: { approval_status: 'rejected', send_status: 'blocked' },
    engagementStatus: 'draft_rejected',
  });
}
