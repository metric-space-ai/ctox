// core/scorecard.js — pure structured-form (interview scorecard) model.
// No DOM, no RxDB.
//
// Baukasten note: a generic weighted structured-form engine — a "scorecard" is
// one form definition; the same engine powers any rubric/assessment. Recruiting
// ships interview competency templates; another vertical swaps the criteria.
// AGG: scorecards score competencies, never protected attributes — criteria are
// validated against the same guard the screening engine uses upstream.

/**
 * @typedef ScorecardCriterion
 * @property {string} key
 * @property {string} label
 * @property {number} [weight] default 1
 * @property {number} [scaleMax] default 5
 */

/** Normalize a scorecard definition into a stable shape. */
export function normalizeScorecard(def) {
  const source = def && typeof def === 'object' ? def : {};
  const criteria = (Array.isArray(source.criteria) ? source.criteria : [])
    .filter((c) => c && c.key)
    .map((c) => ({
      key: String(c.key),
      label: String(c.label || c.key),
      weight: Number.isFinite(Number(c.weight)) && Number(c.weight) > 0 ? Number(c.weight) : 1,
      scaleMax: Number.isFinite(Number(c.scaleMax)) && Number(c.scaleMax) > 0 ? Number(c.scaleMax) : 5,
    }));
  return {
    id: String(source.id || ''),
    role_template: String(source.role_template || 'generic'),
    criteria,
  };
}

/** Are all criteria rated (0..scaleMax)? */
export function isScorecardComplete(def, ratings) {
  const card = normalizeScorecard(def);
  const r = ratings && typeof ratings === 'object' ? ratings : {};
  return card.criteria.length > 0 && card.criteria.every((c) => Number.isFinite(Number(r[c.key])));
}

/**
 * Weighted overall score on a 0..100 scale plus per-criterion breakdown.
 * @param {object} def scorecard definition
 * @param {Record<string, number>} ratings key -> rating (0..scaleMax)
 */
export function scoreScorecard(def, ratings) {
  const card = normalizeScorecard(def);
  const r = ratings && typeof ratings === 'object' ? ratings : {};
  let weighted = 0;
  let weights = 0;
  const breakdown = [];
  for (const c of card.criteria) {
    const raw = Number(r[c.key]);
    if (!Number.isFinite(raw)) continue;
    const normalized = Math.max(0, Math.min(1, raw / c.scaleMax));
    weighted += normalized * c.weight;
    weights += c.weight;
    breakdown.push({ key: c.key, rating: raw, normalized: Math.round(normalized * 100) });
  }
  const overall = weights ? Math.round((weighted / weights) * 100) : 0;
  return { overall, breakdown, complete: isScorecardComplete(card, ratings) };
}
