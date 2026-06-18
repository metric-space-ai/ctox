// core/tariff.js — pure worker master-data + tariff/Eingruppierung model for
// temp-staffing payroll and client billing inputs. No DOM, no RxDB.
//
// Baukasten note: the engine validates a master-data record and computes a
// charge rate from a base wage + a configurable surcharge schedule. The
// concrete Euro amounts and the Branchenzuschlag percentages are CONFIG/DATA
// (they change yearly and per Tarifvertrag) — never hardcoded here. The shipped
// profile names the iGZ/BAP Entgeltgruppen and the standard stepped
// Branchenzuschlag shape; another vertical swaps both.

/** Fields a placed temp worker needs before payroll can run. */
export const WORKER_REQUIRED_FIELDS = [
  'tax_id', // Steuer-Identifikationsnummer
  'social_security_number', // Sozialversicherungsnummer
  'tax_class', // Steuerklasse
  'health_insurance', // Krankenkasse
  'iban',
];

/** iGZ/BAP/GVP pay-grade keys (labels only; rates are config). */
export const ENTGELTGRUPPEN = ['E1', 'E2', 'E3', 'E4', 'E5', 'E6', 'E7', 'E8', 'E9'];

/** @param {string} key */
export function isEntgeltgruppe(key) {
  return ENTGELTGRUPPEN.includes(String(key));
}

/**
 * Validate a worker master-data record for payroll readiness.
 * @param {Record<string, unknown>} worker
 */
export function validateWorkerMasterData(worker) {
  const source = worker && typeof worker === 'object' ? worker : {};
  const missing = WORKER_REQUIRED_FIELDS.filter((field) => {
    const value = source[field] ?? source.payload?.[field];
    return value === undefined || value === null || String(value).trim() === '';
  });
  return { complete: missing.length === 0, missing };
}

/**
 * Standard Branchenzuschlag step shape: ordered thresholds of
 * `{ after_weeks, surcharge_pct }`. Pass the actual percentages as config.
 * @param {number} weeks weeks the worker has been at the Entleiher
 * @param {Array<{after_weeks: number, surcharge_pct: number}>} schedule
 * @returns {number} surcharge percent (0 if before the first step)
 */
export function branchenzuschlagForWeeks(weeks, schedule) {
  const steps = Array.isArray(schedule) ? [...schedule].sort((a, b) => a.after_weeks - b.after_weeks) : [];
  let pct = 0;
  for (const step of steps) {
    if (Number(weeks) >= Number(step.after_weeks)) pct = Number(step.surcharge_pct) || 0;
  }
  return pct;
}

/**
 * Compute the client charge rate from a base wage, an internal markup factor,
 * and a Branchenzuschlag percentage, with an Equal-Pay floor check.
 * All money is in the caller's unit (e.g. cents) — pure arithmetic.
 * @param {{ baseWage: number, markupFactor?: number, branchenzuschlagPct?: number, equalPayWage?: number }} input
 */
export function computeChargeRate({ baseWage, markupFactor = 1, branchenzuschlagPct = 0, equalPayWage }) {
  const base = Number(baseWage) || 0;
  const surcharge = base * (Number(branchenzuschlagPct) || 0) / 100;
  const payRate = base + surcharge;
  const equalPay = Number.isFinite(equalPayWage) ? Number(equalPayWage) : null;
  // Equal-Pay (§8 AÜG): after the lead-in, pay must not fall below the
  // comparable in-house wage. Flag when it does.
  const equalPayApplies = equalPay !== null && payRate < equalPay;
  const effectivePay = equalPayApplies ? equalPay : payRate;
  const chargeRate = round2(effectivePay * (Number(markupFactor) || 1));
  return {
    payRate: round2(payRate),
    effectivePay: round2(effectivePay),
    chargeRate,
    surcharge: round2(surcharge),
    equalPayApplies,
  };
}

function round2(value) {
  return Math.round((Number(value) || 0) * 100) / 100;
}
