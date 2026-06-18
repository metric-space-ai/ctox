// core/credential.js — pure credential/expiry-vault model. No DOM, no RxDB.
//
// Baukasten note: a generic "expiring verified artifact" engine — certificates,
// licences, right-to-work, insurance, ISO certs, any artifact with a validity
// window and a deployment/placement gate. The shipped profile names recruiting
// credential types; another vertical swaps the catalog. The native store is the
// generic `business_credentials` collection.

export const EXPIRY_WARN_DAYS = 30;
const DAY = 24 * 60 * 60 * 1000;

/** Recruiting credential-type catalog (config; the engine is type-agnostic). */
export const CREDENTIAL_TYPES = [
  { key: 'staplerschein', label: 'Staplerschein', deploymentBlocking: true },
  { key: 'g25', label: 'G25 (Fahr-/Steuertätigkeit)', deploymentBlocking: true },
  { key: 'g37', label: 'G37 (Bildschirmarbeit)', deploymentBlocking: false },
  { key: 'schweisserpruefung', label: 'Schweißerprüfung', deploymentBlocking: true },
  { key: 'fuehrerschein', label: 'Führerschein', deploymentBlocking: true },
  { key: 'pflege_fortbildung', label: 'Pflege-Fortbildung', deploymentBlocking: true },
  { key: 'aufenthaltstitel', label: 'Aufenthaltstitel/Arbeitserlaubnis', deploymentBlocking: true },
  { key: 'fuehrungszeugnis', label: 'Führungszeugnis', deploymentBlocking: false },
];

const TYPE_BY_KEY = new Map(CREDENTIAL_TYPES.map((type) => [type.key, type]));

export function isDeploymentBlockingType(key) {
  return Boolean(TYPE_BY_KEY.get(key)?.deploymentBlocking);
}

/** @param {{valid_until_ms?: number}} credential @param {number} nowMs */
export function daysUntilExpiry(credential, nowMs) {
  const until = Number(credential?.valid_until_ms);
  if (!Number.isFinite(until)) return Infinity; // no expiry set
  return Math.floor((until - nowMs) / DAY);
}

/**
 * @param {{valid_from_ms?: number, valid_until_ms?: number, verified?: boolean}} credential
 * @param {number} nowMs
 * @returns {'valid'|'expiring'|'expired'|'not_yet_valid'|'unverified'}
 */
export function credentialStatus(credential, nowMs) {
  if (!credential || credential.verified === false) return 'unverified';
  const from = Number(credential.valid_from_ms);
  if (Number.isFinite(from) && nowMs < from) return 'not_yet_valid';
  const days = daysUntilExpiry(credential, nowMs);
  if (days < 0) return 'expired';
  if (days <= EXPIRY_WARN_DAYS) return 'expiring';
  return 'valid';
}

/** A credential blocks deployment when its type is blocking and it is not valid/expiring-ok. */
export function isDeploymentBlocking(credential, nowMs) {
  if (!isDeploymentBlockingType(credential?.credential_type)) return false;
  const status = credentialStatus(credential, nowMs);
  return status === 'expired' || status === 'unverified' || status === 'not_yet_valid';
}

/**
 * Decide whether a subject may be deployed: every required credential type must
 * be present and not blocking.
 * @param {Array<object>} credentials credentials belonging to the subject
 * @param {{requiredTypes?: string[], nowMs: number}} opts
 */
export function evaluateDeploymentReadiness(credentials, { requiredTypes = [], nowMs }) {
  const list = Array.isArray(credentials) ? credentials : [];
  const blockers = [];
  // Required-but-missing types.
  for (const type of requiredTypes) {
    if (!list.some((c) => c.credential_type === type)) {
      blockers.push({ credential_type: type, reason: 'missing' });
    }
  }
  // Present-but-blocking credentials.
  for (const credential of list) {
    if (isDeploymentBlocking(credential, nowMs)) {
      blockers.push({
        credential_type: credential.credential_type,
        reason: credentialStatus(credential, nowMs),
        credential_id: credential.id,
      });
    }
  }
  return { ready: blockers.length === 0, blockers };
}
