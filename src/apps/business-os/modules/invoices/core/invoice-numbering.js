// core/invoice-numbering.js — pure numbering helpers.
// Ported from
// archive/reorg-review/templates/business-basic/packages/accounting/src/number-series.ts
// (TypeScript stripping; no DB access).
//
// The persistent reservation of numbers happens in the native invoices
// handler against `accounting_number_series`. These helpers compute the next
// number for a (series_key, fiscal_year) pair and detect gaps.

/**
 * @typedef {'strict_no_gaps' | 'reserved_then_voided'} GapPolicy
 */

/**
 * Format the next invoice number for a series.
 *
 * @param {{ prefix: string, next_value: number, fiscal_year: number, padding?: number }} opts
 * @returns {string}
 */
export function formatNumber({ prefix, next_value, fiscal_year, padding = 4 }) {
  if (!prefix || typeof prefix !== 'string') {
    throw new Error('formatNumber requires a string prefix');
  }
  if (!Number.isInteger(fiscal_year)) {
    throw new Error('formatNumber requires integer fiscal_year');
  }
  if (!Number.isInteger(next_value) || next_value <= 0) {
    throw new Error('formatNumber requires positive integer next_value');
  }
  const padded = String(next_value).padStart(padding, '0');
  // If the prefix already includes the fiscal year, append only the padded
  // counter; otherwise insert `{year}-` between prefix and counter.
  if (prefix.indexOf(String(fiscal_year)) !== -1) {
    return `${prefix}${padded}`;
  }
  return `${prefix}${fiscal_year}-${padded}`;
}

/**
 * Increment the next value in a series. Returns a new object; never mutates
 * the input. Honours the gap policy: strict_no_gaps simply increments;
 * reserved_then_voided allows the caller to "void" a number by passing it
 * via `voided_value` and skipping to `next_value + 1`.
 *
 * @param {{ next_value: number, gap_policy: GapPolicy, voided_value?: number }} series
 * @returns {{ next_value: number, last_issued_number: string | null }}
 */
export function advanceSeries(series) {
  if (!Number.isInteger(series.next_value)) {
    throw new Error('advanceSeries requires integer next_value');
  }
  if (series.gap_policy !== 'strict_no_gaps' && series.gap_policy !== 'reserved_then_voided') {
    throw new Error(`unknown gap_policy: ${series.gap_policy}`);
  }
  const next = series.next_value + 1;
  let last_issued = null;
  if (series.voided_value !== undefined) {
    if (!Number.isInteger(series.voided_value)) {
      throw new Error('voided_value must be an integer');
    }
    last_issued = `voided:${series.voided_value}`;
  }
  return { next_value: next, last_issued_number: last_issued };
}

/**
 * Detect gaps in a series given the set of issued numbers (excluding
 * voided). Returns the sorted list of missing counters.
 *
 * @param {number[]} issuedNumbers
 * @param {number} expectedStart
 * @param {number} expectedEnd
 * @returns {number[]}
 */
export function detectGaps(issuedNumbers, expectedStart, expectedEnd) {
  if (!Number.isInteger(expectedStart) || !Number.isInteger(expectedEnd)) {
    throw new Error('detectGaps requires integer bounds');
  }
  if (expectedEnd < expectedStart) return [];
  const issuedSet = new Set(issuedNumbers);
  const gaps = [];
  for (let i = expectedStart; i <= expectedEnd; i += 1) {
    if (!issuedSet.has(i)) gaps.push(i);
  }
  return gaps;
}

export default { formatNumber, advanceSeries, detectGaps };
