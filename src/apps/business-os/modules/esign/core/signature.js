// core/signature.js — pure e-signature request state model. No DOM, no RxDB.
//
// Baukasten note: a generic "route a document to one or more signers and track
// status" engine. Recruiting maps it to Arbeitsvertrag / Vermittlungsvertrag /
// AÜG Überlassungsvertrag; another vertical signs any document. The actual
// provider call is a native control-plane effect (not the data plane).

export const SIGNER_STATES = ['pending', 'viewed', 'signed', 'declined'];
export const REQUEST_STATES = ['created', 'sent', 'partially_signed', 'completed', 'declined', 'expired'];

const SIGNER_TRANSITIONS = {
  pending: ['viewed', 'signed', 'declined'],
  viewed: ['signed', 'declined'],
  signed: [],
  declined: [],
};

export function canTransitionSigner(from, to) {
  if (!SIGNER_STATES.includes(from) || !SIGNER_STATES.includes(to)) return false;
  return (SIGNER_TRANSITIONS[from] || []).includes(to);
}

/**
 * Derive the overall request status from its signers and an optional expiry.
 * @param {{signers?: Array<{state?: string}>, expires_at_ms?: number, sent_at_ms?: number}} request
 * @param {number} nowMs
 */
export function signatureRequestStatus(request, nowMs) {
  const signers = Array.isArray(request?.signers) ? request.signers : [];
  if (signers.some((s) => s.state === 'declined')) return 'declined';
  const expires = Number(request?.expires_at_ms);
  const allSigned = signers.length > 0 && signers.every((s) => s.state === 'signed');
  if (allSigned) return 'completed';
  if (Number.isFinite(expires) && nowMs >= expires) return 'expired';
  if (signers.some((s) => s.state === 'signed')) return 'partially_signed';
  if (Number.isFinite(Number(request?.sent_at_ms))) return 'sent';
  return 'created';
}

export function isComplete(request, nowMs) {
  return signatureRequestStatus(request, nowMs) === 'completed';
}

/** Apply a signer event immutably, returning the next request (or throwing on illegal move). */
export function applySignerEvent(request, signerId, toState) {
  const signers = Array.isArray(request?.signers) ? request.signers : [];
  const next = signers.map((signer) => {
    if (signer.id !== signerId) return signer;
    if (!canTransitionSigner(signer.state || 'pending', toState)) {
      throw new Error(`illegal signer transition ${signer.state} -> ${toState}`);
    }
    return { ...signer, state: toState };
  });
  return { ...request, signers: next };
}
