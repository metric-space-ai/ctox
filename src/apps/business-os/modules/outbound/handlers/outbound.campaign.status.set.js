// ref: store_outbound_commands.rs:4415 outbound_handle_campaign_status_set
import { requiredString, loadRequired, load, str, firstString, fail, putDefaultObject, payloadInsert, asBool } from './_lib/outbound.js';

const ALLOWED = new Set(['setup_required', 'active', 'paused', 'closed']);
const NOT_READY = new Set(['blocked', 'locked', 'suspended', 'disabled']);

export default async function handle(command, ctx) {
  const now = ctx.now();
  const campaignId = requiredString(command.payload, ['campaign_id']);
  const requestedStatus = requiredString(command.payload, ['status']);
  if (!ALLOWED.has(requestedStatus)) {
    fail('invalid', `unsupported campaign status: ${requestedStatus}; allowed: setup_required, active, paused, closed`);
  }
  return ctx.transaction(async (tx) => {
    const campaign = await loadRequired(tx, 'outbound_campaigns', campaignId, 'campaign not found');
    const defaultChannel = firstString([
      str(command.payload, ['channel']),
      str(campaign, ['payload', 'active_outreach', 'default_channel']),
      str(campaign, ['channel']),
      'email',
    ]) ?? 'email';
    if (requestedStatus === 'active' && defaultChannel !== 'physical_letter') {
      const accountKey = firstString([
        str(campaign, ['communication_account_key']),
        str(campaign, ['payload', 'communication_account_key']),
      ]);
      if (accountKey == null) fail('conflict', 'campaign cannot activate email channel without a linked mailbox');
      const limit = await load(tx, 'outbound_account_limits', accountKey);
      if (!limit) fail('conflict', 'campaign cannot activate email channel without outbound_account_limits');
      const limitStatus = str(limit, ['status']) ?? 'active';
      if (NOT_READY.has(limitStatus)) fail('conflict', `campaign mailbox is not ready (status: ${limitStatus})`);
      if (asBool(limit.blocked) === true) fail('conflict', 'campaign mailbox is blocked for outbound communication');
    }
    campaign.status = requestedStatus;
    putDefaultObject(campaign, 'payload');
    payloadInsert(campaign, 'status_set_at_ms', now);
    payloadInsert(campaign, 'active_channel', defaultChannel);
    campaign.updated_at_ms = now;
    await tx.put('outbound_campaigns', campaign);
    return { ok: true, campaign, status: requestedStatus, channel: defaultChannel };
  });
}
