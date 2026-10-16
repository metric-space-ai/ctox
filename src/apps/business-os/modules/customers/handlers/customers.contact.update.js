// customers.contact.update – ref: store_customer_commands.rs:294-363 (customers_contact_update) @ 4794af03d
import {
  requiredFromPayloadOrRecord, loadRequired, mergeFields, requiredStr, requireExisting, refreshSearchText,
  putRecord, writeActivity, contactDisplayName, touchAccountLastActivity, clone,
} from './_lib.js';

const FIELDS = [
  'account_id', 'first_name', 'last_name', 'email', 'phone', 'job_title', 'city', 'linkedin_url', 'x_url',
  'is_primary_contact', 'contact_owner_id', 'last_activity_at_ms', 'source', 'source_record_id', 'payload',
];

export default async function handle(command, ctx) {
  const now = ctx.now();
  return ctx.transaction(async (tx) => {
    const contactId = requiredFromPayloadOrRecord(command, ['contact_id', 'id'], 'contact_id is required');
    const contact = await loadRequired(tx, 'customer_contacts', contactId, 'customer contact not found');
    mergeFields(contact, command.payload, FIELDS);
    const accountId = requiredStr(contact, ['account_id']);
    await requireExisting(tx, 'customer_accounts', accountId, 'customer account not found');
    refreshSearchText(contact, ['first_name', 'last_name', 'email', 'job_title', 'city']);
    await putRecord(tx, 'customer_contacts', contactId, now, contact);
    const activity = await writeActivity(tx, command, now, {
      activity_type: 'contact_updated',
      name: 'Kontakt aktualisiert',
      account_id: accountId,
      contact_id: contactId,
      linked_record_type: 'contact',
      linked_record_id: contactId,
      linked_record_name: contactDisplayName(contact),
      properties: { patch: clone(command.payload) ?? null },
    });
    await touchAccountLastActivity(tx, accountId, now);
    return { ok: true, contact, activity };
  });
}
