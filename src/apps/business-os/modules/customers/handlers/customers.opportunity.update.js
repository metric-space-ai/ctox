// customers.opportunity.update – ref: store_customer_commands.rs:482-553 (customers_opportunity_update) @ 4794af03d
import {
  requiredFromPayloadOrRecord, loadRequired, mergeFields, validateOpportunity, requiredStr, requireExisting,
  refreshSearchText, putRecord, writeActivity, touchAccountLastActivity, str, clone,
} from './_lib.js';

// Rust: `stage` fehlt bewusst – Stufenwechsel nur über move_stage/close_*.
const FIELDS = [
  'name', 'account_id', 'primary_contact_id', 'owner_id', 'opportunity_type', 'amount_cents', 'currency',
  'close_date_ms', 'probability', 'position', 'source', 'source_record_id', 'payload',
];

export default async function handle(command, ctx) {
  const now = ctx.now();
  return ctx.transaction(async (tx) => {
    const opportunityId = requiredFromPayloadOrRecord(command, ['opportunity_id', 'id'], 'opportunity_id is required');
    const opportunity = await loadRequired(tx, 'customer_opportunities', opportunityId, 'customer opportunity not found');
    mergeFields(opportunity, command.payload, FIELDS);
    validateOpportunity(opportunity);
    const accountId = requiredStr(opportunity, ['account_id']);
    await requireExisting(tx, 'customer_accounts', accountId, 'customer account not found');
    refreshSearchText(opportunity, ['name', 'stage', 'opportunity_type']);
    await putRecord(tx, 'customer_opportunities', opportunityId, now, opportunity);
    const activity = await writeActivity(tx, command, now, {
      activity_type: 'opportunity_updated',
      name: 'Opportunity aktualisiert',
      account_id: accountId,
      contact_id: str(opportunity, ['primary_contact_id']),
      opportunity_id: opportunityId,
      linked_record_type: 'opportunity',
      linked_record_id: opportunityId,
      linked_record_name: str(opportunity, ['name']),
      properties: { patch: clone(command.payload) ?? null },
    });
    await touchAccountLastActivity(tx, accountId, now);
    return { ok: true, opportunity, activity };
  });
}
