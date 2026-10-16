// customers.contact.create – ref: store_customer_commands.rs:232-292 (customers_contact_create) @ 4794af03d
import {
  idFromCommand, ensureAbsent, requiredStr, requireExisting, objectPayload, putDefaultString, putDefaultBool,
  putDefaultObject, putDefaultI64, refreshSearchText, putRecord, writeActivity, contactDisplayName,
  touchAccountLastActivity, str, fail, clone,
} from './_lib.js';

export default async function handle(command, ctx) {
  const now = ctx.now();
  return ctx.transaction(async (tx) => {
    const contactId = idFromCommand(command, ['contact_id', 'id'], 'contact');
    await ensureAbsent(tx, 'customer_contacts', contactId, 'customer contact already exists');
    const accountId = requiredStr(command.payload, ['account_id']);
    await requireExisting(tx, 'customer_accounts', accountId, 'customer account not found');
    const contact = objectPayload(command.payload);
    contact.id = contactId;
    putDefaultString(contact, 'first_name', '');
    putDefaultString(contact, 'last_name', '');
    putDefaultString(contact, 'email', '');
    const hasName = str(contact, ['first_name']) !== undefined || str(contact, ['last_name']) !== undefined;
    const hasEmail = str(contact, ['email']) !== undefined;
    if (!hasName && !hasEmail) fail('invalid', 'contact name or email is required');
    putDefaultBool(contact, 'is_primary_contact', false);
    putDefaultBool(contact, 'is_deleted', false);
    putDefaultObject(contact, 'payload');
    putDefaultI64(contact, 'created_at_ms', now);
    refreshSearchText(contact, ['first_name', 'last_name', 'email', 'job_title', 'city']);
    await putRecord(tx, 'customer_contacts', contactId, now, contact);
    const activity = await writeActivity(tx, command, now, {
      activity_type: 'contact_created',
      name: 'Kontakt erstellt',
      account_id: accountId,
      contact_id: contactId,
      linked_record_type: 'contact',
      linked_record_id: contactId,
      linked_record_name: contactDisplayName(contact),
      properties: { contact: clone(contact) },
    });
    await touchAccountLastActivity(tx, accountId, now);
    return { ok: true, collection: 'customer_contacts', contact, activity };
  });
}
