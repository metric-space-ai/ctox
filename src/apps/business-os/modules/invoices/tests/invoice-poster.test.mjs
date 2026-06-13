// tests/invoice-poster.test.mjs — exercises the pure poster.

import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import { buildJournalEntry } from '../core/invoice-poster.js';

const baseInvoice = () => ({
  id: 'inv_1',
  invoice_type: 'sale_out',
  party_id: 'cust_1',
  currency: 'EUR',
  invoice_date_ms: Date.UTC(2026, 5, 1),
  state: 'posted',
  invoice_number: 'RE-2026-0001',
  lines: [
    {
      id: 'l1',
      position: 1,
      description: 'Beratung',
      quantity: 1000,
      unit: 'h',
      unit_price_cents: 12000,
      discount_percent: 0,
      tax_rate: 0.19,
      account_code: '8400',
    },
  ],
});

test('buildJournalEntry requires state=posted', () => {
  const invoice = baseInvoice();
  invoice.state = 'draft';
  assert.throws(() => buildJournalEntry(invoice, { party_id: 'cust_1' }));
});

test('buildJournalEntry produces a balanced entry for sale_out', () => {
  const invoice = baseInvoice();
  const je = buildJournalEntry(invoice, { party_id: 'cust_1' });
  assert.equal(je.type, 'invoice');
  assert.equal(je.ref_id, 'inv_1');
  assert.equal(je.balanced, true, `lines=${JSON.stringify(je.lines, null, 2)} totals=${je.total_debit_cents}/${je.total_credit_cents}`);
  assert.equal(je.total_debit_cents, je.total_credit_cents);
  // quantity=1000 (thousandths = 1.000 units) * unit_price_cents=12_000 = 12_000 cent net
  // tax 19% = 2_280 cent, total 14_280 cent
  assert.equal(je.total_debit_cents, 14_280);
  // Should have revenue line + tax line + party line = 3 lines
  assert.equal(je.lines.length, 3);
});

test('buildJournalEntry omits tax lines for small_business', () => {
  const invoice = baseInvoice();
  invoice.small_business = true;
  const je = buildJournalEntry(invoice, { party_id: 'cust_1' });
  assert.equal(je.balanced, true);
  // No tax line, so 2 lines total
  assert.equal(je.lines.length, 2);
  // Total = net (12_000) only — no tax
  assert.equal(je.total_debit_cents, 12_000);
});

test('buildJournalEntry omits tax lines for reverse_charge', () => {
  const invoice = baseInvoice();
  invoice.reverse_charge = true;
  const je = buildJournalEntry(invoice, { party_id: 'cust_1' });
  assert.equal(je.balanced, true);
  assert.equal(je.lines.length, 2);
});

test('buildJournalEntry for sale_in (purchase) flips debit/credit on revenue line', () => {
  const invoice = baseInvoice();
  invoice.invoice_type = 'sale_in';
  // Remove explicit account_code so the default 3400 (expense) kicks in.
  delete invoice.lines[0].account_code;
  const je = buildJournalEntry(invoice, { party_id: 'cust_1' });
  assert.equal(je.balanced, true);
  // For purchase invoices, the expense line (3400) is debited.
  const revenueLine = je.lines.find((l) => l.account_id === '3400');
  assert.ok(revenueLine);
  assert.equal(revenueLine.debit, 12_000);
  assert.equal(revenueLine.credit, 0);
  // The tax line (Vorsteuer 1406) is also debited.
  const taxLine = je.lines.find((l) => l.account_id === '1406');
  assert.ok(taxLine);
  assert.equal(taxLine.debit, 2_280);
});

test('buildJournalEntry for credit_note flips party amount sign', () => {
  const invoice = baseInvoice();
  invoice.invoice_type = 'credit_note_out';
  invoice.credit_note_for_id = 'inv_1';
  invoice.invoice_number = 'RE-2026-0001-CN';
  const je = buildJournalEntry(invoice, { party_id: 'cust_1' });
  assert.equal(je.balanced, true);
  // The party line (1400 receivable) should reduce the receivable, so it
  // is credited instead of debited. The total_cents in this case is
  // negative on the party line.
  const partyLine = je.lines.find((l) => l.account_id === '1400');
  assert.ok(partyLine);
  assert.equal(partyLine.credit, 14_280);
  assert.equal(partyLine.debit, 0);
});

test('buildJournalEntry post_date uses invoice_date_ms', () => {
  const invoice = baseInvoice();
  const je = buildJournalEntry(invoice, { party_id: 'cust_1' });
  assert.equal(je.posting_date, '2026-06-01');
});

test('buildJournalEntry rejects missing party', () => {
  const invoice = baseInvoice();
  assert.throws(() => buildJournalEntry(invoice, null));
  assert.throws(() => buildJournalEntry(invoice, {}));
});
