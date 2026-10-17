// Origin: CTOX
// License: Apache-2.0
// ats.submission.present – ref: store_ats_commands.rs:616-708 @ 4794af03d
// Kandidat beim Kunden vorstellen: Gate 1 gültige Einwilligung present_to_client (business_consents, App consent),
// Gate 2 keine aktive Doppelvorstellung desselben Kandidaten beim selben Kunden in 180 Tagen.
import {
  ATS_REASON_CONSENT_MISSING, ATS_REASON_DOUBLE_SUBMISSION, atsActorValue, configFlag, evaluateConsentGate, fail,
  findDoubleSubmission, firstStringField, loadByField, payloadOf, recordAtsGovernanceEvent, upsertRecord,
} from './_ats.js';

export default async function handle(command, ctx) {
  const p = payloadOf(command);
  const candidateId = firstStringField(p, ['candidate_id']);
  if (candidateId === undefined) fail('invalid', 'ats.submission.present requires candidate_id');
  const clientAccountId = firstStringField(p, ['client_account_id']);
  if (clientAccountId === undefined) fail('invalid', 'ats.submission.present requires client_account_id');
  const now = ctx.now();
  const requireEvidence = await configFlag(ctx, 'CTOX_BUSINESS_OS_REQUIRE_LEGAL_BASIS_EVIDENCE');
  const actor = atsActorValue(command);
  const outcome = await ctx.transaction(async (tx) => {
    const consents = await loadByField(tx, 'business_consents', 'subject_id', candidateId);
    const hasConsent = evaluateConsentGate('present_to_client', consents, now, requireEvidence);
    const existing = await tx.read('submissions', { selector: {} });
    const conflict = findDoubleSubmission(existing, candidateId, clientAccountId, 180, now);
    const blockers = [];
    const reasonCodes = [];
    if (!hasConsent) {
      reasonCodes.push(ATS_REASON_CONSENT_MISSING);
      blockers.push({ reason: ATS_REASON_CONSENT_MISSING });
    }
    if (conflict !== undefined) {
      reasonCodes.push(ATS_REASON_DOUBLE_SUBMISSION);
      blockers.push({ reason: ATS_REASON_DOUBLE_SUBMISSION, conflicting_submission_id: conflict });
    }
    if (blockers.length > 0) return { denied: true, blockers, reasonCodes };
    const id = `subm_${now}`;
    await upsertRecord(tx, 'submissions', id, now, {
      id,
      candidate_id: candidateId,
      client_account_id: clientAccountId,
      vacancy_id: firstStringField(p, ['vacancy_id']) ?? '',
      client_contact_id: firstStringField(p, ['client_contact_id']) ?? '',
      sent_at_ms: now,
      status: 'sent',
      created_at_ms: now,
      updated_at_ms: now,
    });
    return { denied: false, id };
  });
  if (outcome.denied) {
    await recordAtsGovernanceEvent(ctx, command, actor, 'business_os.ats.candidate_present_denied', {
      candidate_id: candidateId, client_account_id: clientAccountId, decision: 'denied', reason_codes: outcome.reasonCodes,
    }, now);
    return { ok: true, allowed: false, blockers: outcome.blockers };
  }
  await recordAtsGovernanceEvent(ctx, command, actor, 'business_os.ats.candidate_presented', {
    candidate_id: candidateId, client_account_id: clientAccountId, submission_id: outcome.id, decision: 'allowed',
  }, now);
  return { ok: true, allowed: true, submission_id: outcome.id };
}
