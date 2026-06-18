// core/funnel.js — pure recruiting funnel / KPI aggregation. No DOM, no RxDB.
//
// Baukasten note: a generic read-only aggregation over staged records with
// timestamps — counts, conversion rates, time-in-stage. Recruiting maps it to
// the candidate funnel + time-to-fill; another vertical reports any pipeline.

const DAY = 24 * 60 * 60 * 1000;

/**
 * Count records per stage in a fixed stage order.
 * @param {Array<{stage?: string}>} records
 * @param {string[]} stageOrder
 */
export function countByStage(records, stageOrder) {
  const counts = Object.fromEntries(stageOrder.map((s) => [s, 0]));
  for (const record of records || []) {
    const stage = record?.stage;
    if (stage in counts) counts[stage] += 1;
  }
  return counts;
}

/**
 * Stage-to-stage conversion ratios along the ordered funnel.
 * @returns {Array<{from: string, to: string, rate: number}>}
 */
export function stageConversions(records, stageOrder) {
  const counts = countByStage(records, stageOrder);
  // Cumulative "reached at least this stage" using the order (stages are terminal-agnostic).
  const reached = stageOrder.map((stage) => counts[stage]);
  const out = [];
  for (let i = 1; i < stageOrder.length; i += 1) {
    const prev = reached[i - 1];
    out.push({ from: stageOrder[i - 1], to: stageOrder[i], rate: prev ? Math.round((reached[i] / prev) * 100) : 0 });
  }
  return out;
}

/** Fill rate = filled / total (percent). */
export function fillRate(vacancies, { filledStatuses = ['filled', 'eingestellt'] } = {}) {
  const list = Array.isArray(vacancies) ? vacancies : [];
  if (!list.length) return 0;
  const filled = list.filter((v) => filledStatuses.includes(v?.status)).length;
  return Math.round((filled / list.length) * 100);
}

/**
 * Average time-to-fill in days over vacancies that carry opened/filled stamps.
 * @param {Array<{opened_at_ms?: number, filled_at_ms?: number}>} vacancies
 */
export function avgTimeToFillDays(vacancies) {
  const spans = (Array.isArray(vacancies) ? vacancies : [])
    .filter((v) => Number.isFinite(Number(v?.opened_at_ms)) && Number.isFinite(Number(v?.filled_at_ms)))
    .map((v) => (Number(v.filled_at_ms) - Number(v.opened_at_ms)) / DAY)
    .filter((d) => d >= 0);
  if (!spans.length) return 0;
  return Math.round((spans.reduce((a, b) => a + b, 0) / spans.length) * 10) / 10;
}

/** Source-of-hire breakdown: hires grouped by source channel. */
export function sourceOfHire(records, { hiredStage = 'eingestellt' } = {}) {
  const counts = {};
  for (const record of records || []) {
    if (record?.stage !== hiredStage) continue;
    const source = record?.source || 'unknown';
    counts[source] = (counts[source] || 0) + 1;
  }
  return counts;
}
