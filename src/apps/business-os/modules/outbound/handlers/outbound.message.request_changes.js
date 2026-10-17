// ref: store_outbound_commands.rs:654 → outbound_record_change_request :6857
import { recordDecision } from './_lib/decisions.js';

export default async function handle(command, ctx) {
  return recordDecision(command, ctx, {
    idPrefix: 'change_request',
    fallbackPrefix: 'change_request',
    decision: 'changes_requested',
    messageFields: {
      approval_status: 'changes_requested',
      draft_status: 'changes_requested',
      send_status: 'blocked',
    },
    engagementStatus: 'draft_changes_requested',
  });
}
