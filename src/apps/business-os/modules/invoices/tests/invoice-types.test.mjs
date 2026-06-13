// tests/invoice-types.test.mjs — exercises the pure type/state helpers.

import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import {
  INVOICE_TYPES,
  INVOICE_STATES,
  isInvoiceType,
  isInvoiceState,
  canTransition,
  defaultPartyAccountCode,
  defaultRevenueAccountCode,
  taxAccountCodeForRate,
  addCents,
} from '../core/invoice-types.js';

test('INVOICE_TYPES contains the five documented types', () => {
  assert.deepEqual(
    [...INVOICE_TYPES].sort(),
    ['credit_note_in', 'credit_note_out', 'recurring_template', 'sale_in', 'sale_out']
  );
});

test('INVOICE_STATES contains the seven documented states', () => {
  assert.equal(INVOICE_STATES.length, 7);
  assert.ok(INVOICE_STATES.includes('draft'));
  assert.ok(INVOICE_STATES.includes('posted'));
  assert.ok(INVOICE_STATES.includes('paid'));
});

test('isInvoiceType is a type guard', () => {
  assert.equal(isInvoiceType('sale_out'), true);
  assert.equal(isInvoiceType('bogus'), false);
  assert.equal(isInvoiceType(null), false);
});

test('canTransition allows draft -> posted -> paid and rejects invalid moves', () => {
  assert.equal(canTransition('draft', 'posted'), true);
  assert.equal(canTransition('posted', 'paid'), true);
  assert.equal(canTransition('draft', 'paid'), false);
  assert.equal(canTransition('paid', 'posted'), false);
  assert.equal(canTransition('paid', 'cancelled'), false);
});

test('canTransition returns false for unknown state names', () => {
  assert.equal(canTransition('bogus', 'posted'), false);
  assert.equal(canTransition('posted', 'bogus'), false);
});

test('defaultPartyAccountCode returns SKR03 defaults', () => {
  assert.equal(defaultPartyAccountCode('sale_out'), '1400');
  assert.equal(defaultPartyAccountCode('sale_in'), '1600');
  assert.equal(defaultPartyAccountCode('credit_note_out'), '1400');
});

test('defaultRevenueAccountCode returns SKR03 defaults', () => {
  assert.equal(defaultRevenueAccountCode('sale_out'), '8400');
  assert.equal(defaultRevenueAccountCode('sale_in'), '3400');
});

test('taxAccountCodeForRate returns 0% and unknown with empty string', () => {
  assert.equal(taxAccountCodeForRate(0, false), '');
  assert.equal(taxAccountCodeForRate(0.19, false), '3806');
  assert.equal(taxAccountCodeForRate(0.19, true), '1406');
  assert.equal(taxAccountCodeForRate(0.07, false), '3801');
  assert.equal(taxAccountCodeForRate(0.07, true), '1407');
});

test('addCents enforces integer cents', () => {
  assert.equal(addCents(100, 250), 350);
  assert.throws(() => addCents(1.5, 2), /integer cents/);
  assert.throws(() => addCents(1, '2'), /integer cents/);
});
