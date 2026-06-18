// core/leistungsnachweis.js — pure timesheet (Leistungsnachweis) totals +
// Entleiher sign-off → billing-release gate. No DOM, no RxDB.
//
// Baukasten note: a generic "tally time entries by category, gate billing on an
// external sign-off" engine. Recruiting/Zeitarbeit maps it to the
// Entleiher-signed Leistungsnachweis with Nacht/Sonn/Feiertag surcharges; the
// surcharge percentages are config.

export const HOUR_TYPES = ['regular', 'nacht', 'sonntag', 'feiertag', 'mehrarbeit'];

/**
 * Sum hours per category from timesheet entries.
 * @param {Array<{type?: string, hours?: number}>} entries
 */
export function computeNachweisTotals(entries) {
  const totals = Object.fromEntries(HOUR_TYPES.map((t) => [t, 0]));
  for (const entry of entries || []) {
    const type = HOUR_TYPES.includes(entry?.type) ? entry.type : 'regular';
    totals[type] += Number(entry?.hours) || 0;
  }
  totals.total = HOUR_TYPES.reduce((sum, t) => sum + totals[t], 0);
  return totals;
}

/**
 * Apply category surcharges (config percentages) to a base hourly rate to get
 * the gross pay for a Leistungsnachweis. Money in caller unit.
 * @param {object} totals output of computeNachweisTotals
 * @param {number} baseHourly
 * @param {Record<string, number>} surchargePct e.g. { nacht: 25, sonntag: 50, feiertag: 100, mehrarbeit: 25 }
 */
export function computeNachweisPay(totals, baseHourly, surchargePct = {}) {
  const base = Number(baseHourly) || 0;
  let pay = 0;
  for (const type of HOUR_TYPES) {
    const hours = Number(totals?.[type]) || 0;
    const pct = type === 'regular' ? 0 : Number(surchargePct[type]) || 0;
    pay += hours * base * (1 + pct / 100);
  }
  return Math.round(pay * 100) / 100;
}

/** Billing is released only when the Entleiher has signed the Leistungsnachweis. */
export function isBillingReleased(nachweis) {
  return Boolean(nachweis?.entleiher_signed) && Number.isFinite(Number(nachweis?.signed_at_ms));
}

/**
 * Gate: a Leistungsnachweis may be invoiced only when signed and non-empty.
 * @param {{entleiher_signed?: boolean, signed_at_ms?: number, entries?: Array}} nachweis
 */
export function evaluateBillingGate(nachweis) {
  const blockers = [];
  if (!isBillingReleased(nachweis)) blockers.push({ reason: 'entleiher_signature_missing' });
  if (!(Array.isArray(nachweis?.entries) && nachweis.entries.length)) blockers.push({ reason: 'no_time_entries' });
  return { allowed: blockers.length === 0, blockers };
}
