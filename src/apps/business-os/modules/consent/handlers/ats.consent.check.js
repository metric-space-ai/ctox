// Origin: CTOX
// License: Apache-2.0
// ats.consent.check – ref: store_ats_commands.rs:89-110 (handle_ats_active_command) @ 4794af03d
// Lesender Gate-Befehl: gilt für purpose eine gültige Einwilligung des Subjekts? Konfig
// CTOX_BUSINESS_OS_REQUIRE_LEGAL_BASIS_EVIDENCE=1 verlangt basis_evidence bei Nicht-Einwilligungs-Rechtsgrundlagen.
import { configFlag, evaluateConsentGate, fail, loadByField, payloadOf } from './_ats.js';

export default async function handle(command, ctx) {
  const p = payloadOf(command);
  if (typeof p.subject_id !== 'string') fail('invalid', 'ats.consent.check requires subject_id');
  const purpose = typeof p.purpose === 'string' ? p.purpose : undefined;
  const consents = await loadByField(ctx, 'business_consents', 'subject_id', p.subject_id);
  const requireEvidence = await configFlag(ctx, 'CTOX_BUSINESS_OS_REQUIRE_LEGAL_BASIS_EVIDENCE');
  const allowed = evaluateConsentGate(purpose, consents, ctx.now(), requireEvidence);
  return { ok: true, allowed, purpose: purpose ?? null };
}
