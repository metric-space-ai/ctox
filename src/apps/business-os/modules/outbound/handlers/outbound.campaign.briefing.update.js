// ref: store_outbound_commands.rs:5704 outbound_handle_campaign_briefing_update
import { requiredString, loadRequired, str, isObject, putDefaultObject } from './_lib/outbound.js';

const PAYLOAD_KEYS = ['subtitle', 'scope', 'briefing', 'briefing_template_id', 'briefing_language', 'campaign_setup_task'];

export default async function handle(command, ctx) {
  const now = ctx.now();
  const payload = command.payload ?? {};
  const campaignId = requiredString(payload, ['campaign_id']);
  return ctx.transaction(async (tx) => {
    const campaign = await loadRequired(tx, 'outbound_campaigns', campaignId, 'campaign not found');
    campaign.id = campaignId;
    // str() already trims and drops blanks, so Rust's "campaign name is required"
    // ensure can never fire; a blank name is simply ignored.
    const name = str(payload, ['name']);
    if (name != null) campaign.name = name;
    const objective = str(payload, ['objective']);
    if (objective != null) campaign.objective = objective;
    putDefaultObject(campaign, 'payload');
    if (isObject(payload.payload_patch)) {
      for (const key of PAYLOAD_KEYS) {
        if (key in payload.payload_patch) campaign.payload[key] = JSON.parse(JSON.stringify(payload.payload_patch[key]));
      }
    }
    campaign.updated_at_ms = now;
    await tx.put('outbound_campaigns', campaign);
    return { ok: true, campaign, campaign_id: campaignId };
  });
}
