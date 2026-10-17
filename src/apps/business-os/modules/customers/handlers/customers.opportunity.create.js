// customers.opportunity.create – ref: store_customer_commands.rs:409-480 (customers_opportunity_create) @ 4794af03d
import {
  idFromCommand, ensureAbsent, requiredStr, requireExisting, objectPayload, putDefaultString, putDefaultI64,
  putDefaultBool, putDefaultObject, validateOpportunity, refreshSearchText, putRecord, writeActivity,
  touchAccountLastActivity, str, clone,
} from './_lib.js';

export default async function handle(command, ctx) {
  const now = ctx.now();
  return ctx.transaction(async (tx) => {
    const opportunityId = idFromCommand(command, ['opportunity_id', 'id'], 'opp');
    await ensureAbsent(tx, 'customer_opportunities', opportunityId, 'customer opportunity already exists');
    const accountId = requiredStr(command.payload, ['account_id']);
    await requireExisting(tx, 'customer_accounts', accountId, 'customer account not found');
    const primaryContactId = str(command.payload, ['primary_contact_id']);
    if (primaryContactId !== undefined) {
      await requireExisting(tx, 'customer_contacts', primaryContactId, 'primary contact not found');
    }
    const opportunity = objectPayload(command.payload);
    opportunity.id = opportunityId;
    requiredStr(opportunity, ['name']);
    putDefaultString(opportunity, 'opportunity_type', 'new_business');
    putDefaultString(opportunity, 'stage', 'qualification');
    putDefaultI64(opportunity, 'amount_cents', 0);
    putDefaultString(opportunity, 'currency', 'EUR');
    putDefaultI64(opportunity, 'position', now);
    putDefaultBool(opportunity, 'is_deleted', false);
    putDefaultI64(opportunity, 'created_at_ms', now);
    putDefaultI64(opportunity, 'last_stage_changed_at_ms', now);
    putDefaultObject(opportunity, 'payload');
    validateOpportunity(opportunity);
    refreshSearchText(opportunity, ['name', 'stage', 'opportunity_type']);
    await putRecord(tx, 'customer_opportunities', opportunityId, now, opportunity);
    const activity = await writeActivity(tx, command, now, {
      activity_type: 'opportunity_created',
      name: 'Opportunity erstellt',
      account_id: accountId,
      contact_id: str(opportunity, ['primary_contact_id']),
      opportunity_id: opportunityId,
      linked_record_type: 'opportunity',
      linked_record_id: opportunityId,
      linked_record_name: str(opportunity, ['name']),
      properties: { opportunity: clone(opportunity) },
    });
    await touchAccountLastActivity(tx, accountId, now);
    return { ok: true, collection: 'customer_opportunities', opportunity, activity };
  });
}
