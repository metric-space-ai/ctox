// outbound.contacts.email_validate — Kontaktadressen eines Leads über experte.de prüfen (Strang OLG3).
// Vorher Core-Fähigkeit mail.validate_contacts (Hintergrund-Thread spawn_contact_email_validation,
// contact_email_validation.rs:1290). Payload {record_id}. Höchstens ein Durchlauf je Lead gleichzeitig
// (ref: :1192 in_flight → Fence `email-validation:<record_id>`); ein zweiter Auftrag endet ohne Lauf.
import { fail, str } from './_lib/olg.js';
import { validateLeadContacts } from './_lib/contact-email-validation.js';

export default async function handle(command, ctx) {
  const recordId = str(command.payload, ['record_id'])
    ?? (typeof command.record_id === 'string' && command.record_id.trim() !== '' ? command.record_id.trim() : null);
  if (recordId == null) fail('invalid', 'record_id is required');
  const fence = await ctx.cap.fence.claim({ key: `email-validation:${recordId}` });
  if (fence?.token == null) return { ok: true, record_id: recordId, changed: 0, in_flight: true };
  try {
    const changed = await validateLeadContacts(ctx, recordId);
    if (changed > 0) ctx.log('info', `[email-validation] ${recordId}: ${changed} contact(s) checked`, { record_id: recordId });
    return { ok: true, record_id: recordId, changed };
  } finally {
    await ctx.cap.fence.release({ token: fence.token });
  }
}
