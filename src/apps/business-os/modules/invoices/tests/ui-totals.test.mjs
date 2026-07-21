// tests/ui-totals.test.mjs — protects the browser editor totals AND the
// 2-pane IA-Karte grammar: band counts, search/type/band filtering, the
// import/export handlers, auto-reveal gating, and the in-place selection flip.

import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import {
  computeInvoiceTotals,
  invoiceBand,
  countsFor,
  filterInvoices,
  shouldRevealDetail,
  buildInvoicesExport,
  parseInvoiceImport,
  renderInvoiceListMarkup,
  renderInvoiceRow,
} from '../index.js';

// Shared fixture: one invoice per real financial bucket plus edge states that
// only belong under "Alle".
const SAMPLE = [
  { id: 'inv_posted', state: 'posted', invoice_type: 'sale_out', party_id: 'cust_a', invoice_number: 'R-1', total_cents: 11900, updated_at_ms: 5 },
  { id: 'inv_partial', state: 'partially_paid', invoice_type: 'sale_out', party_id: 'cust_a', invoice_number: 'R-2', total_cents: 5000, updated_at_ms: 4 },
  { id: 'inv_paid', state: 'paid', invoice_type: 'sale_in', party_id: 'cust_b', invoice_number: 'R-3', total_cents: 2000, updated_at_ms: 3 },
  { id: 'inv_overdue', state: 'overdue', invoice_type: 'sale_out', party_id: 'cust_b', invoice_number: 'R-4', total_cents: 9900, updated_at_ms: 2 },
  { id: 'inv_draft', state: 'draft', invoice_type: 'sale_out', party_id: 'cust_a', invoice_number: '', total_cents: 0, updated_at_ms: 1 },
];

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

// --- Counted view band (Offen / Bezahlt / Überfällig, zeros included) --------

test('invoiceBand maps real status onto the three financial buckets', () => {
  assert.equal(invoiceBand({ state: 'posted' }), 'offen');
  assert.equal(invoiceBand({ state: 'partially_paid' }), 'offen');
  assert.equal(invoiceBand({ state: 'paid' }), 'bezahlt');
  assert.equal(invoiceBand({ state: 'overdue' }), 'ueberfaellig');
  // Draft / cancelled / credited are not one of the three buckets.
  assert.equal(invoiceBand({ state: 'draft' }), 'other');
  assert.equal(invoiceBand({ state: 'cancelled' }), 'other');
});

test('countsFor counts every band from real status with zeros included', () => {
  const counts = countsFor(SAMPLE);
  assert.deepEqual(counts, { alle: 5, offen: 2, bezahlt: 1, ueberfaellig: 1 });
  // Zeros are still present when a bucket is empty.
  const onlyDrafts = countsFor([{ state: 'draft' }, { state: 'draft' }]);
  assert.deepEqual(onlyDrafts, { alle: 2, offen: 0, bezahlt: 0, ueberfaellig: 0 });
});

// --- Grammar filtering (band + type + search) --------------------------------

test('filterInvoices honours band, type and search', () => {
  assert.equal(filterInvoices(SAMPLE, { band: 'alle' }).length, 5);
  assert.deepEqual(filterInvoices(SAMPLE, { band: 'offen' }).map((i) => i.id), ['inv_posted', 'inv_partial']);
  assert.deepEqual(filterInvoices(SAMPLE, { band: 'bezahlt' }).map((i) => i.id), ['inv_paid']);
  assert.deepEqual(filterInvoices(SAMPLE, { band: 'ueberfaellig' }).map((i) => i.id), ['inv_overdue']);
  // Type filter (from the collapsed tray).
  assert.deepEqual(filterInvoices(SAMPLE, { type: 'sale_in' }).map((i) => i.id), ['inv_paid']);
  // Search matches invoice number, and the party name via the resolver.
  assert.deepEqual(filterInvoices(SAMPLE, { search: 'R-4' }).map((i) => i.id), ['inv_overdue']);
  const nameOf = (inv) => (inv.party_id === 'cust_b' ? 'Beispiel GmbH' : 'Acme');
  assert.deepEqual(filterInvoices(SAMPLE, { search: 'beispiel' }, nameOf).map((i) => i.id), ['inv_paid', 'inv_overdue']);
});

// --- Auto-reveal detail (visible = hasSelection && !userCollapsed) -----------

test('shouldRevealDetail gates the main detail on selection and collapse', () => {
  assert.equal(shouldRevealDetail(false, false), false, 'no selection → no detail');
  assert.equal(shouldRevealDetail(true, false), true, 'selection revealed');
  assert.equal(shouldRevealDetail(true, true), false, 'user collapsed → hidden even with a selection');
});

// --- In-place selection flip: selecting never changes list membership --------

function rowIds(markup) {
  return [...markup.matchAll(/data-invoice-id="([^"]+)"/g)].map((m) => m[1]);
}
function selectedIds(markup) {
  return [...markup.matchAll(/<div class="[^"]*\bis-selected\b[^"]*"[^>]*data-invoice-id="([^"]+)"/g)].map((m) => m[1]);
}

test('renderInvoiceListMarkup keeps the same rows/order when the selection changes', () => {
  const a = renderInvoiceListMarkup(SAMPLE, { view: 'cards', selectedId: 'inv_posted' });
  const b = renderInvoiceListMarkup(SAMPLE, { view: 'cards', selectedId: 'inv_overdue' });
  // List membership + order is identical — selection is a class flip, not a rebuild.
  assert.deepEqual(rowIds(a), SAMPLE.map((i) => i.id));
  assert.deepEqual(rowIds(a), rowIds(b));
  // Exactly the selected row carries is-selected, and it moves with the selection.
  assert.deepEqual(selectedIds(a), ['inv_posted']);
  assert.deepEqual(selectedIds(b), ['inv_overdue']);
});

test('renderInvoiceRow stamps the agent right-click context trio', () => {
  const html = renderInvoiceRow(SAMPLE[0], { view: 'cards', selected: false });
  assert.match(html, /data-context-record-id="inv_posted"/);
  assert.match(html, /data-context-record-type="accounting_invoices"/);
  assert.match(html, /data-context-label="R-1"/);
  assert.match(html, /aria-selected="false"/);
});

// --- Export / Import handlers ------------------------------------------------

test('buildInvoicesExport serialises the visible invoices as JSON', () => {
  const payload = buildInvoicesExport(filterInvoices(SAMPLE, { band: 'offen' }), 1781990000000);
  assert.equal(payload.kind, 'ctox-invoices-export');
  assert.equal(payload.exported_at_ms, 1781990000000);
  assert.deepEqual(payload.invoices.map((i) => i.id), ['inv_posted', 'inv_partial']);
  // Round-trips through JSON without throwing.
  assert.doesNotThrow(() => JSON.parse(JSON.stringify(payload)));
});

test('parseInvoiceImport yields create payloads only for customer + line entries', () => {
  const raw = {
    invoices: [
      { party_id: 'cust_a', invoice_type: 'sale_out', lines: [{ description: 'X', quantity: 1000, unit_price_cents: 5000, tax_rate: 0.19 }] },
      { party_id: '', lines: [{ description: 'no customer', quantity: 1000, unit_price_cents: 1, tax_rate: 0.19 }] },
      { party_id: 'cust_b', lines: [] },
    ],
  };
  const entries = parseInvoiceImport(raw, 1781990000000);
  assert.equal(entries.length, 1, 'only the complete entry survives');
  const [entry] = entries;
  assert.equal(entry.party_id, 'cust_a');
  assert.equal(entry.invoice_type, 'sale_out');
  assert.equal(entry.currency, 'EUR');
  assert.equal(entry.due_date_ms, 1781990000000 + 14 * 86_400_000, 'due date defaults to net-14');
  assert.equal(entry.lines.length, 1);
  assert.equal(entry.lines[0].account_code, '8400', 'lines are sanitised with a default SKR account');
  // A bare array is accepted too.
  assert.equal(parseInvoiceImport([{ party_id: 'c', lines: [{ quantity: 1000, unit_price_cents: 1 }] }]).length, 1);
});
