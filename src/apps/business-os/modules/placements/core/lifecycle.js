// core/lifecycle.js — pure offer/placement lifecycle + guarantee-clock model.
// No DOM, no RxDB.
//
// Baukasten note: a generic quote/offer/agreement state machine plus a
// time-window ("guarantee") clock. Recruiting maps it to placement fees and the
// replacement guarantee; another vertical maps it to any signable commitment.

export const OFFER_STATES = ['draft', 'extended', 'negotiating', 'accepted', 'declined', 'withdrawn'];

const OFFER_TRANSITIONS = {
  draft: ['extended', 'withdrawn'],
  extended: ['negotiating', 'accepted', 'declined', 'withdrawn'],
  negotiating: ['extended', 'accepted', 'declined', 'withdrawn'],
  accepted: [],
  declined: [],
  withdrawn: [],
};

export function isOfferState(state) {
  return OFFER_STATES.includes(String(state));
}

export function canTransitionOffer(from, to) {
  if (!isOfferState(from) || !isOfferState(to)) return false;
  return (OFFER_TRANSITIONS[from] || []).includes(to);
}

export function isTerminalOfferState(state) {
  return ['accepted', 'declined', 'withdrawn'].includes(String(state));
}

/**
 * A placement is created when an offer is accepted. Compute the placement fee
 * from the agreed basis (percent of salary or a flat fee). Money in caller unit.
 * @param {{feeType?: 'percent'|'flat', feePercent?: number, flatFee?: number, annualSalary?: number}} terms
 */
export function computePlacementFee(terms) {
  const source = terms && typeof terms === 'object' ? terms : {};
  if (source.feeType === 'flat') return round2(Number(source.flatFee) || 0);
  const pct = Number(source.feePercent) || 0;
  const salary = Number(source.annualSalary) || 0;
  return round2((salary * pct) / 100);
}

/**
 * Guarantee/replacement clock. The clock starts at the candidate start date.
 * @param {{start_ms?: number, guarantee_days?: number}} placement
 * @param {number} nowMs
 * @returns {{ status: 'pending'|'active'|'elapsed', remainingDays: number }}
 */
export function guaranteeStatus(placement, nowMs) {
  const start = Number(placement?.start_ms);
  const days = Number(placement?.guarantee_days);
  if (!Number.isFinite(start) || !Number.isFinite(days)) return { status: 'pending', remainingDays: 0 };
  if (nowMs < start) return { status: 'pending', remainingDays: days };
  const end = start + days * 24 * 60 * 60 * 1000;
  if (nowMs >= end) return { status: 'elapsed', remainingDays: 0 };
  return { status: 'active', remainingDays: Math.ceil((end - nowMs) / (24 * 60 * 60 * 1000)) };
}

/**
 * Handle an early leave: if within the guarantee window, the agency owes a
 * replacement search or a pro-rata clawback of the fee.
 * @param {{start_ms?: number, guarantee_days?: number, fee?: number}} placement
 * @param {number} leftAtMs
 */
export function earlyLeaveOutcome(placement, leftAtMs) {
  const { status } = guaranteeStatus(placement, leftAtMs);
  if (status !== 'active') return { withinGuarantee: false, clawback: 0, action: 'none' };
  const start = Number(placement.start_ms);
  const days = Number(placement.guarantee_days);
  const served = Math.max(0, Math.min(days, Math.floor((leftAtMs - start) / (24 * 60 * 60 * 1000))));
  const remainingRatio = (days - served) / days;
  const clawback = round2((Number(placement.fee) || 0) * remainingRatio);
  return { withinGuarantee: true, clawback, action: 'replacement_or_clawback', servedDays: served };
}

function round2(value) {
  return Math.round((Number(value) || 0) * 100) / 100;
}
