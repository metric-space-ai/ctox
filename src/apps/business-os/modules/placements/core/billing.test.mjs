import assert from 'node:assert/strict';
import { test } from 'node:test';

import { draftClawbackCreditNote, draftPlacementFeeInvoice } from './billing.js';

test('draftPlacementFeeInvoice builds a sale_out draft', () => {
  const inv = draftPlacementFeeInvoice({ id: 'p1', client_account_id: 'a1', candidate_name: 'Ada', fee: 15000 }, { atMs: 5 });
  assert.equal(inv.invoice_type, 'sale_out');
  assert.equal(inv.account_id, 'a1');
  assert.equal(inv.source_placement_id, 'p1');
  assert.equal(inv.net_total, 15000);
  assert.match(inv.lines[0].description, /Ada/);
});

test('draftClawbackCreditNote builds a credit_note_out draft', () => {
  const cn = draftClawbackCreditNote({ id: 'p1', client_account_id: 'a1' }, 6000, { atMs: 9 });
  assert.equal(cn.invoice_type, 'credit_note_out');
  assert.equal(cn.net_total, 6000);
});
