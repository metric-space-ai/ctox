// core/invoice-tax.js — pure tax calculation helpers.
// Ported from
// archive/reorg-review/templates/business-basic/packages/accounting/src/tax.ts
// (TypeScript stripping; no DB access).
//
// All amounts in cent (integer). All tax rates as decimals (0.19 for 19%).

/**
 * Compute net, tax and gross for a single invoice line, applying
 * discount_percent at the line level. Returns integer cents.
 *
 * @param {{ quantity: number, unit_price_cents: number, discount_percent?: number, tax_rate: number }} line
 * @returns {{ net_cents: number, tax_cents: number, gross_cents: number }}
 */
export function computeLineTotals(line) {
  if (!line || !Number.isInteger(line.quantity) || !Number.isInteger(line.unit_price_cents)) {
    throw new Error('computeLineTotals requires integer quantity and integer unit_price_cents');
  }
  const discountMultiplier = Number.isFinite(line.discount_percent)
    ? Math.max(0, Math.min(100, line.discount_percent)) / 100
    : 0;
  const gross_unit_cents = Math.round(line.unit_price_cents * (1 - discountMultiplier));
  // `quantity` is stored in thousandths (XRechnung/UBL convention):
  // quantity=1000 means 1.000 natural units, quantity=1500 means 1.500.
  // Net is therefore gross_unit_cents * quantity / 1000.
  const net_cents = Math.round((gross_unit_cents * line.quantity) / 1000);
  const tax_cents = Math.round(net_cents * line.tax_rate);
  return { net_cents, tax_cents, gross_cents: net_cents + tax_cents };
}

/**
 * Aggregate the line totals into per-rate tax_breakdown buckets plus
 * invoice-level net/tax/gross sums.
 *
 * @param {Array<{ tax_rate: number, net_cents: number, tax_cents: number, gross_cents: number }>} lines
 * @returns {{ subtotal_cents: number, tax_cents: number, total_cents: number, tax_breakdown: Array<{ tax_rate: number, net_cents: number, tax_cents: number, tax_account_code: string }> }}
 */
export function aggregateTaxBreakdown(lines) {
  const buckets = new Map();
  let subtotal = 0;
  let tax = 0;
  for (const line of lines) {
    subtotal += line.net_cents;
    tax += line.tax_cents;
    const key = line.tax_rate.toString();
    const existing = buckets.get(key) || { tax_rate: line.tax_rate, net_cents: 0, tax_cents: 0 };
    existing.net_cents += line.net_cents;
    existing.tax_cents += line.tax_cents;
    buckets.set(key, existing);
  }
  const tax_breakdown = Array.from(buckets.values()).map((b) => ({
    tax_rate: b.tax_rate,
    net_cents: b.net_cents,
    tax_cents: b.tax_cents,
  }));
  return {
    subtotal_cents: subtotal,
    tax_cents: tax,
    total_cents: subtotal + tax,
    tax_breakdown,
  };
}

/**
 * Compute a payment due date from invoice_date_ms and a net_days payment
 * term. Returns a ms timestamp.
 *
 * @param {number} invoice_date_ms
 * @param {number} net_days
 * @returns {number}
 */
export function computeDueDateMs(invoice_date_ms, net_days) {
  if (!Number.isInteger(invoice_date_ms)) {
    throw new Error('computeDueDateMs requires integer ms timestamp');
  }
  if (!Number.isInteger(net_days) || net_days < 0) {
    throw new Error('computeDueDateMs requires non-negative integer days');
  }
  const ms = invoice_date_ms + net_days * 24 * 60 * 60 * 1000;
  return ms;
}

/**
 * Compute the skonto deadline (ms timestamp) and the skonto amount in cents.
 * Returns null when skonto_terms are not configured.
 *
 * @param {number} invoice_date_ms
 * @param {number | undefined} skonto_percent
 * @param {number | undefined} skonto_days
 * @param {number} gross_cents
 * @returns {{ deadline_ms: number, amount_cents: number } | null}
 */
export function computeSkonto(invoice_date_ms, skonto_percent, skonto_days, gross_cents) {
  if (!skonto_percent || !skonto_days) return null;
  if (skonto_percent <= 0 || skonto_percent > 100) {
    throw new Error('skonto_percent must be in (0, 100]');
  }
  const deadline_ms = computeDueDateMs(invoice_date_ms, skonto_days);
  const amount_cents = Math.round((gross_cents * skonto_percent) / 100);
  return { deadline_ms, amount_cents };
}

/**
 * Apply skonto to an allocation. Returns the effective amount in cents that
 * reduces the open invoice balance, and the skonto discount in cents (posted
 * to the skonto expense account).
 *
 * @param {number} allocated_cents
 * @param {{ deadline_ms: number, amount_cents: number } | null} skonto
 * @param {number} payment_date_ms
 * @returns {{ applied_cents: number, skonto_cents: number, used_skonto: boolean }}
 */
export function applySkonto(allocated_cents, skonto, payment_date_ms) {
  if (!skonto) return { applied_cents: allocated_cents, skonto_cents: 0, used_skonto: false };
  if (payment_date_ms <= skonto.deadline_ms && allocated_cents >= skonto.amount_cents) {
    return {
      applied_cents: allocated_cents - skonto.amount_cents,
      skonto_cents: skonto.amount_cents,
      used_skonto: true,
    };
  }
  return { applied_cents: allocated_cents, skonto_cents: 0, used_skonto: false };
}

export default {
  computeLineTotals,
  aggregateTaxBreakdown,
  computeDueDateMs,
  computeSkonto,
  applySkonto,
};
