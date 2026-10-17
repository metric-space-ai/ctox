// ref: store_outbound_commands.rs:4894 outbound_handle_audit_export (read-only)
import { str } from './_lib/outbound.js';

const COLLECTIONS = [
  'outbound_campaigns', 'outbound_engagements', 'outbound_messages', 'outbound_approvals',
  'outbound_sequences', 'outbound_sender_assignments', 'outbound_meeting_requests',
  'outbound_suppression_entries', 'outbound_account_limits', 'outbound_skillbooks',
  'outbound_letter_templates',
];

export default async function handle(command, ctx) {
  const campaignFilter = str(command.payload, ['campaign_id']) ?? '';
  const exported = {};
  for (const collection of COLLECTIONS) {
    const records = await ctx.read(collection, { selector: {} });
    exported[collection] = records.filter((value) => {
      if (!campaignFilter) return true;
      return (str(value, ['campaign_id']) ?? '') === campaignFilter
        || (str(value, ['payload', 'campaign_id']) ?? '') === campaignFilter
        || str(value, ['id']) === campaignFilter;
    });
  }
  // NOTE: the whole export is the command result; the 1 MiB result cap of the
  // v4 runtime can be exceeded for large tenants (see V4-HANDLERS.md P5).
  return { ok: true, campaign_id: campaignFilter, export: exported, exported_at_ms: ctx.now() };
}
