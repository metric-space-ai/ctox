// outbound.contacts.email_validation_sweep — Trigger-Handler (every PT1H) für Adressen ohne Urteil (Strang OLG3).
// ref: contact_email_validation.rs:1340 sweep_unchecked_leads, :1318 lead_due_for_sweep (Wiederholung nach 6 h).
// Statt eines Hintergrund-Threads je fälligem Lead ein Befehl outbound.contacts.email_validate (Fence je Lead).
import { LEAD_COLLECTION } from './_lib/update-digest.js';
import { leadDueForSweep, spawnContactEmailValidation } from './_lib/contact-email-validation.js';

export default async function handle(_command, ctx) {
  const now = ctx.now();
  const leads = await ctx.read(LEAD_COLLECTION, {});
  const due = leads.filter((lead) => !(lead?.is_deleted === true || lead?._deleted === true) && leadDueForSweep(lead, now))
    .map((lead) => lead.id).filter((id) => typeof id === 'string');
  for (const recordId of due) await spawnContactEmailValidation(ctx, recordId);
  return { ok: true, due: due.length };
}
