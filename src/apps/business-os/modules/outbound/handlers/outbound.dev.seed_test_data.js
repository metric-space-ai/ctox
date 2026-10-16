// ref: store_outbound_commands.rs:5343 outbound_handle_dev_seed_test_data (developer helper)
import { requiredString, asI64 } from './_lib/outbound.js';

export default async function handle(command, ctx) {
  const now = ctx.now();
  const campaignId = requiredString(command.payload, ['campaign_id']);
  const count = Math.min(25, Math.max(1, asI64(command.payload?.count) ?? 3));
  return ctx.transaction(async (tx) => {
    const created = [];
    for (let idx = 0; idx < count; idx += 1) {
      const engId = `dev_eng_${campaignId}_${idx}`;
      if (await tx.get('outbound_engagements', engId)) continue;
      await tx.put('outbound_engagements', {
        id: engId,
        campaign_id: campaignId,
        company_id: `dev_co_${idx}`,
        contact_id: `dev_ct_${idx}`,
        status: 'ready_for_assignment',
        payload: { contact_email: `lead${idx}@example.com`, source: 'outbound.dev.seed_test_data' },
        created_at_ms: now,
        updated_at_ms: now,
      });
      created.push(engId);
    }
    return { ok: true, campaign_id: campaignId, count: created.length, engagement_ids: created };
  });
}
