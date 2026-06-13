// tests/invoice-tax.test.mjs — exercises the pure tax helpers.

import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import {
  computeLineTotals,
  aggregateTaxBreakdown,
  computeDueDateMs,
  computeSkonto,
  applySkonto,
} from '../core/invoice-tax.js';

test('computeLineTotals applies discount and rounds tax', () => {
  const line = {
    quantity: 1000,
    unit_price_cents: 10000,
    discount_percent: 0,
    tax_rate: 0.19,
  };
  const totals = computeLineTotals(line);
  // quantity=1000 (thousandths) = 1.000 natural units, unit_price_cents=10000
  // = 100.00 EUR/unit. Net is therefore (10000 * 1000) / 1000 = 10_000.
  assert.equal(totals.net_cents, 10_000);
  assert.equal(totals.tax_cents, Math.round(totals.net_cents * 0.19));
  assert.equal(totals.gross_cents, totals.net_cents + totals.tax_cents);
});

test('computeLineTotals with 10% discount', () => {
  const line = {
    quantity: 1000,
    unit_price_cents: 10000,
    discount_percent: 10,
    tax_rate: 0.19,
  };
  const totals = computeLineTotals(line);
  // 10000 * 0.9 = 9000 unit, 1000 thousandths * 9000 = 9_000_000, /1000 = 9_000
  assert.equal(totals.net_cents, 9_000);
});

test('computeLineTotals rejects non-integer quantity', () => {
  assert.throws(() => computeLineTotals({ quantity: 1.5, unit_price_cents: 100, tax_rate: 0.19 }));
});

test('aggregateTaxBreakdown buckets by tax_rate', () => {
  const lines = [
    { tax_rate: 0.19, net_cents: 10000, tax_cents: 1900, gross_cents: 11900 },
    { tax_rate: 0.19, net_cents: 5000, tax_cents: 950, gross_cents: 5950 },
    { tax_rate: 0.07, net_cents: 2000, tax_cents: 140, gross_cents: 2140 },
  ];
  const result = aggregateTaxBreakdown(lines);
  assert.equal(result.subtotal_cents, 17000);
  assert.equal(result.tax_cents, 2990);
  assert.equal(result.total_cents, 19990);
  assert.equal(result.tax_breakdown.length, 2);
  const bucket19 = result.tax_breakdown.find((b) => b.tax_rate === 0.19);
  assert.equal(bucket19.net_cents, 15000);
  assert.equal(bucket19.tax_cents, 2850);
});

test('computeDueDateMs adds net_days to invoice_date_ms', () => {
  const invoice_ms = Date.UTC(2026, 0, 1);
  const due_ms = computeDueDateMs(invoice_ms, 14);
  assert.equal(due_ms, invoice_ms + 14 * 86400000);
});

test('computeSkonto returns null when no skonto', () => {
  assert.equal(computeSkonto(Date.UTC(2026, 0, 1), undefined, undefined, 11900), null);
  assert.equal(computeSkonto(Date.UTC(2026, 0, 1), 0, 7, 11900), null);
});

test('computeSkonto returns deadline and amount when configured', () => {
  const invoice_ms = Date.UTC(2026, 0, 1);
  const skonto = computeSkonto(invoice_ms, 2, 7, 11900);
  assert.equal(skonto.deadline_ms, invoice_ms + 7 * 86400000);
  assert.equal(skonto.amount_cents, Math.round(11900 * 0.02));
});

test('applySkonto applies discount when payment is before deadline', () => {
  const invoice_ms = Date.UTC(2026, 0, 1);
  const skonto = computeSkonto(invoice_ms, 2, 7, 11900);
  const payment_ms = invoice_ms + 3 * 86400000;
  const result = applySkonto(11900, skonto, payment_ms);
  assert.equal(result.used_skonto, true);
  assert.equal(result.skonto_cents, skonto.amount_cents);
  assert.equal(result.applied_cents, 11900 - skonto.amount_cents);
});

test('applySkonto does not apply when payment is after deadline', () => {
  const invoice_ms = Date.UTC(2026, 0, 1);
  const skonto = computeSkonto(invoice_ms, 2, 7, 11900);
  const payment_ms = invoice_ms + 14 * 86400000;
  const result = applySkonto(11900, skonto, payment_ms);
  assert.equal(result.used_skonto, false);
  assert.equal(result.skonto_cents, 0);
  assert.equal(result.applied_cents, 11900);
});
