// tests/ui-totals.test.mjs — protects the browser editor totals.

import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { computeInvoiceTotals } from '../index.js';

test('computeInvoiceTotals treats quantity as thousandths', () => {
  const totals = computeInvoiceTotals({
    lines: [
      {
        quantity: 1000,
        unit_price_cents: 12_000,
        tax_rate: 0.19,
      },
    ],
  });

  assert.deepEqual(totals, {
    subtotal_cents: 12_000,
    tax_cents: 2_280,
    total_cents: 14_280,
    tax_breakdown: [{ tax_rate: 0.19, net_cents: 12_000, tax_cents: 2_280 }],
  });
});

test('computeInvoiceTotals applies line discounts before tax', () => {
  const totals = computeInvoiceTotals({
    lines: [
      {
        quantity: 1500,
        unit_price_cents: 10_000,
        discount_percent: 10,
        tax_rate: 0.19,
      },
    ],
  });

  assert.deepEqual(totals, {
    subtotal_cents: 13_500,
    tax_cents: 2_565,
    total_cents: 16_065,
    tax_breakdown: [{ tax_rate: 0.19, net_cents: 13_500, tax_cents: 2_565 }],
  });
});
