// core/submission.js — pure candidate-submission guard: double-submission /
// ownership-conflict protection + consent-to-present gate. No DOM, no RxDB.
//
// Baukasten note: a generic "present a record to an external party, once, with
// permission" guard. Recruiting presents candidates to client contacts and the
// double-submission rule protects the placement-fee entitlement; another
// vertical reuses the same once-and-consented delivery gate.

const DAY = 24 * 60 * 60 * 1000;

/**
 * Find an existing active submission of the same candidate to the same client
 * within the protection window — that is an ownership conflict.
 * @param {Array<object>} existing prior submissions
 * @param {{candidate_id: string, client_account_id: string}} next
 * @param {{withinDays?: number, nowMs: number}} opts
 */
export function findDoubleSubmission(existing, next, { withinDays = 180, nowMs }) {
  const windowMs = withinDays * DAY;
  return (Array.isArray(existing) ? existing : []).find((sub) => {
    if (!sub || sub.status === 'withdrawn') return false;
    if (sub.candidate_id !== next.candidate_id) return false;
    if (sub.client_account_id !== next.client_account_id) return false;
    const sentAt = Number(sub.sent_at_ms);
    if (!Number.isFinite(sentAt)) return true;
    return nowMs - sentAt <= windowMs;
  }) || null;
}

/**
 * Decide whether a candidate may be submitted: no double-submission and a valid
 * consent-to-present. The consent check is injected (boolean) so this stays
 * decoupled from the consent module.
 * @param {object} submission the intended submission { candidate_id, client_account_id }
 * @param {{ existingSubmissions?: Array<object>, hasConsent?: boolean, withinDays?: number, nowMs: number }} opts
 */
export function evaluateSubmissionGuard(submission, { existingSubmissions = [], hasConsent = false, withinDays = 180, nowMs }) {
  const blockers = [];
  const conflict = findDoubleSubmission(existingSubmissions, submission, { withinDays, nowMs });
  if (conflict) {
    blockers.push({ reason: 'double_submission', conflicting_submission_id: conflict.id });
  }
  if (!hasConsent) {
    blockers.push({ reason: 'consent_to_present_missing' });
  }
  return { allowed: blockers.length === 0, blockers };
}

/** Normalize a client-feedback signal so it can flow back to matching_results. */
export const FEEDBACK_OUTCOMES = ['interested', 'interview', 'rejected', 'hired', 'no_response'];

export function normalizeFeedback(input) {
  const source = input && typeof input === 'object' ? input : {};
  const outcome = FEEDBACK_OUTCOMES.includes(source.outcome) ? source.outcome : 'no_response';
  return {
    submission_id: String(source.submission_id || ''),
    outcome,
    reason: source.reason ? String(source.reason) : '',
    at_ms: Number(source.at_ms) || 0,
  };
}
