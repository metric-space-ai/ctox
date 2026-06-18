// core/consent.js — pure DSGVO consent / legal-basis + retention model.
// No DOM, no RxDB. The native store is a generic `business_consents` ledger and
// a retention/deletion engine that purges over-retained records.
//
// Baukasten note: a generic "may we hold/process this record, and for how long"
// engine reusable by every module. Recruiting supplies the purposes and
// retention windows; another vertical swaps them.

/** DSGVO Art. 6 / Art. 9 legal bases. */
export const LEGAL_BASES = [
  'consent', // Art. 6(1)(a) Einwilligung
  'contract', // Art. 6(1)(b) Vertrag
  'legal_obligation', // Art. 6(1)(c)
  'legitimate_interest', // Art. 6(1)(f)
  'special_category_consent', // Art. 9(2)(a)
];

export function isLegalBasis(basis) {
  return LEGAL_BASES.includes(String(basis));
}

/**
 * A consent record is valid when granted, not withdrawn, and not expired.
 * @param {{legal_basis?: string, granted_at_ms?: number, withdrawn_at_ms?: number, expires_at_ms?: number}} consent
 * @param {number} nowMs
 */
export function isConsentValid(consent, nowMs) {
  if (!consent) return false;
  // Bases other than (special-category) consent are not subject to withdrawal.
  if (consent.legal_basis && consent.legal_basis !== 'consent' && consent.legal_basis !== 'special_category_consent') {
    if (!isLegalBasis(consent.legal_basis)) return false;
    return true;
  }
  if (!Number.isFinite(Number(consent.granted_at_ms))) return false;
  if (Number.isFinite(Number(consent.withdrawn_at_ms)) && Number(consent.withdrawn_at_ms) <= nowMs) return false;
  if (Number.isFinite(Number(consent.expires_at_ms)) && Number(consent.expires_at_ms) <= nowMs) return false;
  return true;
}

/**
 * Is there a valid consent for a given subject+purpose in the ledger?
 * @param {Array<object>} consents
 * @param {string} purpose
 * @param {number} nowMs
 */
export function hasValidConsent(consents, purpose, nowMs) {
  return (Array.isArray(consents) ? consents : []).some(
    (c) => c.purpose === purpose && isConsentValid(c, nowMs),
  );
}

/**
 * Gate a command that requires consent. Returns an allow/deny decision the
 * native command handler enforces server-side.
 * @param {{purpose: string}} command something carrying a consent purpose
 * @param {Array<object>} consents subject's consent ledger
 * @param {number} nowMs
 */
export function evaluateConsentGate(command, consents, nowMs) {
  const purpose = command?.purpose;
  if (!purpose) return { allowed: true, reason: 'no_consent_required' };
  if (hasValidConsent(consents, purpose, nowMs)) return { allowed: true, reason: 'consent_present' };
  return { allowed: false, reason: 'consent_missing', purpose };
}

/**
 * Retention: a record is due for deletion when its retention window has elapsed
 * since the reference timestamp (Aufbewahrungs-/Löschfrist).
 * @param {{reference_ms?: number}|number} recordOrRefMs
 * @param {{retentionDays: number, nowMs: number}} opts
 */
export function retentionDue(recordOrRefMs, { retentionDays, nowMs }) {
  const ref = typeof recordOrRefMs === 'number' ? recordOrRefMs : Number(recordOrRefMs?.reference_ms);
  if (!Number.isFinite(ref) || !Number.isFinite(Number(retentionDays))) return false;
  const cutoff = ref + Number(retentionDays) * 24 * 60 * 60 * 1000;
  return nowMs >= cutoff;
}

/** Select the records from a set that are due for retention purge. */
export function selectExpiredForRetention(records, { retentionDays, nowMs }) {
  return (Array.isArray(records) ? records : []).filter((record) =>
    retentionDue(record, { retentionDays, nowMs }),
  );
}
