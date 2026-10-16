// Origin: CTOX
// License: Apache-2.0
// ats.credential.verify – ref: store_ats_commands.rs:1004-1054 @ 4794af03d (H12)
// Stempelt verified=true, verified_by (authentifizierter Handelnder), verified_at_ms. Rechte chef/admin bleiben
// Kern-Policy (heute is_ats_mutating_command → rxdb_command_session); siehe V4-HANDLERS.md.
import { atsActorValue, authenticatedActorId, fail, firstStringField, payloadOf, recordAtsGovernanceEvent, upsertRecord } from './_ats.js';

export default async function handle(command, ctx) {
  const credentialId = firstStringField(payloadOf(command), ['credential_id', 'record_id', 'id']);
  if (credentialId === undefined) fail('invalid', 'ats.credential.verify requires credential_id');
  const verifiedBy = authenticatedActorId(command, 'ats.credential.verify requires an authenticated verifier');
  const now = ctx.now();
  await ctx.transaction(async (tx) => {
    const existing = await tx.get('business_credentials', credentialId);
    if (!existing) fail('not_found', 'credential not found');
    const payload = JSON.parse(JSON.stringify(existing));
    payload.verified = true;
    payload.verified_by = verifiedBy;
    payload.verified_at_ms = now;
    await upsertRecord(tx, 'business_credentials', credentialId, now, payload);
  });
  await recordAtsGovernanceEvent(ctx, command, atsActorValue(command), 'business_os.ats.credential_verified',
    { credential_id: credentialId, verified_by: verifiedBy }, now);
  return { ok: true, credential_id: credentialId, verified: true, verified_by: verifiedBy };
}
