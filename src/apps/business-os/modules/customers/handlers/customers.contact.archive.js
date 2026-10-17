// customers.contact.archive – ref: store_customer_commands.rs:365-407 (customers_contact_archive) @ 4794af03d
import {
  requiredFromPayloadOrRecord, loadRequired, putRecord, writeActivity, contactDisplayName,
  touchAccountLastActivity, str,
} from './_lib.js';

export default async function handle(command, ctx) {
  const now = ctx.now();
  return ctx.transaction(async (tx) => {
    const contactId = requiredFromPayloadOrRecord(command, ['contact_id', 'id'], 'contact_id is required');
    const contact = await loadRequired(tx, 'customer_contacts', contactId, 'customer contact not found');
    const accountId = str(contact, ['account_id']);
    contact.is_deleted = true;
    contact.deleted_at_ms = now;
    // Rust: kein refresh_search_text beim Kontakt-Archivieren (anders als beim Konto).
    await putRecord(tx, 'customer_contacts', contactId, now, contact);
    const activity = await writeActivity(tx, command, now, {
      activity_type: 'contact_archived',
      name: 'Kontakt archiviert',
      account_id: accountId,
      contact_id: contactId,
      linked_record_type: 'contact',
      linked_record_id: contactId,
      linked_record_name: contactDisplayName(contact),
      properties: { contact_id: contactId },
    });
    if (accountId !== undefined) await touchAccountLastActivity(tx, accountId, now);
    return { ok: true, contact, activity };
  });
}
