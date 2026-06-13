// tests/invoice-validate.test.mjs — exercises the pure validator.

import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { validateInvoice, isValid } from '../core/invoice-validate.js';

const baseInvoice = () => ({
  invoice_type: 'sale_out',
  party_id: 'cust_1',
  currency: 'EUR',
  invoice_date_ms: Date.UTC(2026, 5, 1),
  state: 'draft',
  lines: [
    {
      id: 'line_1',
      position: 1,
      description: 'Stundensatz Beratung',
      quantity: 1000,
      unit: 'h',
      unit_price_cents: 12000,
      tax_rate: 0.19,
      account_code: '8400',
    },
  ],
});

test('valid sale_out invoice with one line passes', () => {
  const issues = validateInvoice(baseInvoice());
  assert.equal(isValid(issues), true, JSON.stringify(issues));
});

test('missing invoice_type is reported', () => {
  const invoice = baseInvoice();
  invoice.invoice_type = 'bogus';
  const issues = validateInvoice(invoice);
  assert.ok(issues.some((i) => i.field === 'invoice_type'));
});

test('missing party_id is reported', () => {
  const invoice = baseInvoice();
  invoice.party_id = '';
  const issues = validateInvoice(invoice);
  assert.ok(issues.some((i) => i.field === 'party_id'));
});

test('empty lines is reported', () => {
  const invoice = baseInvoice();
  invoice.lines = [];
  const issues = validateInvoice(invoice);
  assert.ok(issues.some((i) => i.field === 'lines'));
});

test('small_business rejects tax_breakdown', () => {
  const invoice = baseInvoice();
  invoice.small_business = true;
  invoice.tax_breakdown = [{ tax_rate: 0.19, net_cents: 100, tax_cents: 19 }];
  const issues = validateInvoice(invoice);
  assert.ok(issues.some((i) => i.field === 'small_business'));
});

test('reverse_charge on non-sale invoice is rejected', () => {
  const invoice = baseInvoice();
  invoice.invoice_type = 'credit_note_in';
  invoice.credit_note_for_id = 'inv_original';
  invoice.reverse_charge = true;
  const issues = validateInvoice(invoice);
  assert.ok(issues.some((i) => i.field === 'reverse_charge'));
});

test('credit_note without credit_note_for_id is rejected', () => {
  const invoice = baseInvoice();
  invoice.invoice_type = 'credit_note_out';
  const issues = validateInvoice(invoice);
  assert.ok(issues.some((i) => i.field === 'credit_note_for_id'));
});

test('skonto_percent out of range is rejected', () => {
  const invoice = baseInvoice();
  invoice.skonto_percent = 150;
  invoice.skonto_days = 7;
  const issues = validateInvoice(invoice);
  assert.ok(issues.some((i) => i.field === 'skonto_percent'));
});

test('line without description is rejected', () => {
  const invoice = baseInvoice();
  invoice.lines[0].description = '';
  const issues = validateInvoice(invoice);
  assert.ok(issues.some((i) => i.field === 'lines[0].description'));
});

test('line with non-integer quantity is rejected', () => {
  const invoice = baseInvoice();
  invoice.lines[0].quantity = 1.5;
  const issues = validateInvoice(invoice);
  assert.ok(issues.some((i) => i.field === 'lines[0].quantity'));
});

test('line with tax_rate out of [0, 1] is rejected', () => {
  const invoice = baseInvoice();
  invoice.lines[0].tax_rate = 1.5;
  const issues = validateInvoice(invoice);
  assert.ok(issues.some((i) => i.field === 'lines[0].tax_rate'));
});
