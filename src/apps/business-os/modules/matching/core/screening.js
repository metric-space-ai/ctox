// core/screening.js — pure pre-screen, AGG guardrail, ranking and decision
// audit for the matching engine. No persistence, no RxDB, no DOM.
//
// Baukasten note: knock-out rules, ranking and the decision-audit shape are a
// generic "filter + rank + record why" engine. Recruiting supplies the rule
// fields and the rejection-reason vocabulary; another vertical swaps those.
//
// AGG (Allgemeines Gleichbehandlungsgesetz) guardrail: a knock-out rule must
// never screen on a protected attribute. Such rules are blocked, and every
// rejection must carry a reason code from a controlled, non-discriminatory
// vocabulary so the decision trail is legally defensible.

/** Protected attributes per AGG §1 — substrings matched case-insensitively. */
export const AGG_PROTECTED_ATTRIBUTES = [
  'geschlecht', 'gender', 'sex', 'alter', 'age', 'geburtsdatum', 'birthdate',
  'ethnie', 'ethnic', 'herkunft', 'nationalit', 'nationality', 'rasse', 'race',
  'religion', 'weltanschauung', 'behinderung', 'disab', 'sexuell', 'orientierung',
  'familienstand', 'marital', 'schwanger', 'pregnan',
];

/** @param {string} field */
export function isProtectedAttribute(field) {
  const f = String(field || '').toLowerCase();
  return AGG_PROTECTED_ATTRIBUTES.some((token) => f.includes(token));
}

/**
 * Throw if any knock-out rule targets a protected attribute. Call this at the
 * point a recruiter saves screening rules so the UI can refuse them.
 * @param {Array<{field: string}>} rules
 */
export function assertNonDiscriminatory(rules) {
  const offending = (rules || []).filter((rule) => isProtectedAttribute(rule?.field));
  if (offending.length) {
    throw new Error(
      `AGG: knock-out rule references a protected attribute: ${offending.map((r) => r.field).join(', ')}`,
    );
  }
  return true;
}

/**
 * @typedef KnockoutRule
 * @property {string} [id]
 * @property {string} field   dotted path into the candidate/object record
 * @property {string} [label]
 * @property {'present'|'equals'|'gte'|'lte'|'includes'} op
 * @property {*} [value]
 */

function getField(object, field) {
  if (!object || !field) return undefined;
  if (Object.prototype.hasOwnProperty.call(object, field)) return object[field];
  return String(field)
    .split('.')
    .reduce((acc, key) => (acc == null ? acc : acc[key]), object);
}

function ruleHolds(object, rule) {
  const value = getField(object, rule?.field);
  switch (rule?.op) {
    case 'present':
      return value !== undefined && value !== null && String(value).trim() !== '';
    case 'equals':
      return String(value).toLowerCase() === String(rule.value).toLowerCase();
    case 'gte':
      return Number(value) >= Number(rule.value);
    case 'lte':
      return Number(value) <= Number(rule.value);
    case 'includes': {
      const needle = String(rule.value).toLowerCase();
      if (Array.isArray(value)) return value.some((entry) => String(entry).toLowerCase().includes(needle));
      return String(value || '').toLowerCase().includes(needle);
    }
    default:
      return true;
  }
}

/**
 * Evaluate must-have knock-out rules. Rules on protected attributes are skipped
 * (defense-in-depth on top of assertNonDiscriminatory).
 * @param {object} object candidate/object record
 * @param {KnockoutRule[]} rules
 */
export function evaluateKnockouts(object, rules) {
  const safeRules = (rules || []).filter((rule) => !isProtectedAttribute(rule?.field));
  const failed = [];
  for (const rule of safeRules) {
    if (!ruleHolds(object, rule)) {
      failed.push({
        ruleId: rule.id || rule.field,
        label: rule.label || rule.field,
        reasonCode: 'knockout_unmet',
      });
    }
  }
  return { passed: failed.length === 0, failed };
}

/** Controlled, non-discriminatory rejection-reason vocabulary. */
export const REJECTION_REASON_CODES = {
  knockout_unmet: 'Muss-Kriterium nicht erfüllt',
  qualification_gap: 'Qualifikation passt nicht ausreichend',
  experience_gap: 'Berufserfahrung passt nicht ausreichend',
  location_mismatch: 'Einsatzort/Mobilität passt nicht',
  availability_mismatch: 'Verfügbarkeit/Startzeitpunkt passt nicht',
  language_gap: 'Sprachanforderung nicht erfüllt',
  other_better_fit: 'Andere Kandidaten passen besser',
  withdrawn: 'Kandidat zurückgezogen',
};

/** @param {string} code */
export function isValidRejectionReason(code) {
  return Object.prototype.hasOwnProperty.call(REJECTION_REASON_CODES, code);
}

/**
 * @typedef ScoredCandidate
 * @property {string} objectId
 * @property {number|null} score 0..100 or null when not yet evaluated
 * @property {boolean} [evaluated]
 * @property {boolean} [knockoutFailed]
 */

/**
 * Rank scored candidates into a shortlist. Knocked-out candidates are excluded;
 * evaluated candidates rank by score desc; unevaluated fall to the bottom.
 * @param {ScoredCandidate[]} scored
 * @param {{topN?: number}} [opts]
 */
export function rankShortlist(scored, { topN = 5 } = {}) {
  const eligible = (scored || []).filter((entry) => entry && !entry.knockoutFailed);
  const ranked = [...eligible].sort((a, b) => {
    const sa = Number.isFinite(a.score) ? a.score : -1;
    const sb = Number.isFinite(b.score) ? b.score : -1;
    return sb - sa;
  });
  return ranked.slice(0, topN).map((entry, index) => ({
    objectId: entry.objectId,
    score: Number.isFinite(entry.score) ? entry.score : null,
    rank: index + 1,
    reason: entry.evaluated
      ? Number.isFinite(entry.score)
        ? `Rang ${index + 1} nach Match-Score ${entry.score}`
        : 'Bewertet'
      : 'Noch nicht bewertet — Matching ausführen',
  }));
}

/**
 * Build an immutable, AGG-defensible screening-decision audit record. Stored as
 * a generic business record (or, server-side, an immutable audit event).
 * @param {{requirementId: string, objectId: string, decision: 'advanced'|'rejected'|'shortlisted', reasonCode?: string, actor?: string, atMs: number, evidence?: *}} input
 */
export function buildScreeningAudit({ requirementId, objectId, decision, reasonCode, actor, atMs, evidence }) {
  const code = isValidRejectionReason(reasonCode) ? reasonCode : 'other_better_fit';
  return {
    id: `screen_${requirementId}_${objectId}_${atMs}`,
    kind: 'screening_decision',
    requirement_id: requirementId,
    object_id: objectId,
    decision,
    reason_code: decision === 'rejected' ? code : null,
    reason_label: decision === 'rejected' ? REJECTION_REASON_CODES[code] : null,
    actor: actor || 'system',
    evidence: evidence ?? null,
    created_at_ms: atMs,
    immutable: true,
  };
}
