// Origin: CTOX
// License: Apache-2.0
// ats.intake.capture – ref: store_ats_commands.rs:429-474 @ 4794af03d
// Bewerbung anlegen: channel-Vorgabe email, E-Mail klein, Dublettenschlüssel email:<mail> sonst name:<name klein>.
import { atsActorValue, firstStringField, payloadOf, recordAtsGovernanceEvent, upsertRecord } from './_ats.js';

export default async function handle(command, ctx) {
  const p = payloadOf(command);
  const now = ctx.now();
  const channel = firstStringField(p, ['channel']) ?? 'email';
  const name = firstStringField(p, ['name']) ?? '';
  const email = (firstStringField(p, ['email']) ?? '').toLowerCase();
  const id = `appl_${now}`;
  const dedupeKey = email !== '' ? `email:${email}` : `name:${name.toLowerCase()}`;
  await upsertRecord(ctx, 'applications', id, now, {
    id,
    channel,
    vacancy_id: firstStringField(p, ['vacancy_id']) ?? '',
    candidate: { name, email, phone: firstStringField(p, ['phone']) ?? '' },
    dedupe_key: dedupeKey,
    status: 'new',
    received_at_ms: now,
    created_at_ms: now,
    updated_at_ms: now,
    _deleted: false,
  });
  await recordAtsGovernanceEvent(ctx, command, atsActorValue(command), 'business_os.ats.intake_captured', {
    application_id: id, channel, dedupe_key: dedupeKey, has_email: email !== '',
  }, now);
  return { ok: true, application_id: id, dedupe_key: dedupeKey };
}
