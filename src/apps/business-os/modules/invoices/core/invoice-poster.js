// core/invoice-poster.js — pure poster: builds the journal entry payload
// from a posted invoice. Ported from
// archive/reorg-review/templates/business-basic/packages/accounting/src/invoice/poster.ts
// (TypeScript stripping; no DB access — persistence is the native handler's job).
//
// The poster returns the structured journal entry (header + lines) that the
// native invoices handler will write to `accounting_journal_entries` and
// `accounting_journal_entry_lines`. It also enforces the GoBD invariant:
// Σ debit == Σ credit across all lines.
//
// Line-level field names match the existing `accounting_journal_entry_lines`
// schema (`debit` / `credit`, both integer cent). Header-level totals use
// `total_debit_cents` / `total_credit_cents` (no currency) to avoid collision
// with the line amounts and to stay consistent with DATEV EXTF export.

import { defaultPartyAccountCode, defaultRevenueAccountCode, taxAccountCodeForRate } from './invoice-types.js';
import { aggregateTaxBreakdown, computeLineTotals } from './invoice-tax.js';

/**
 * @typedef JournalLine
 * @property {string} account_id
 * @property {number} debit
 * @property {number} credit
 * @property {string} [party_id]
 * @property {string} [tax_rate_id]
 * @property {string} [description]
 * @property {number} [line_no]
 */

/**
 * @typedef JournalEntry
 * @property {string} id
 * @property {string} posting_date YYYY-MM-DD
 * @property {string} type 'invoice'
 * @property {string} ref_type 'invoice'
 * @property {string} ref_id invoice id
 * @property {string} number invoice_number
 * @property {string} narration
 * @property {JournalLine[]} lines
 * @property {number} total_debit_cents
 * @property {number} total_credit_cents
 * @property {boolean} balanced
 */

/**
 * Build the journal entry for a posted invoice. Pure function — does not
 * write to storage. The native invoices handler in Phase 6 consumes the
 * returned structure and persists it via business_commands.
 *
 * @param {object} invoice the fully validated, posted invoice (state must be 'posted' at call time)
 * @param {{ party_id: string }} party resolved party record (account_id is the SKR receivable/payable code)
 * @returns {JournalEntry}
 */
export function buildJournalEntry(invoice, party) {
  if (!invoice || invoice.state !== 'posted') {
    throw new Error('buildJournalEntry requires invoice.state == "posted"');
  }
  if (!party || !party.party_id) {
    throw new Error('buildJournalEntry requires a resolved party');
  }
  const isCreditNote = invoice.invoice_type === 'credit_note_out' || invoice.invoice_type === 'credit_note_in';
  const isInput = invoice.invoice_type === 'sale_in' || invoice.invoice_type === 'credit_note_in';

  // 1. Per-line revenue (or expense for purchase invoices) entries.
  const linesWithTotals = invoice.lines.map((line) => ({
    ...line,
    ...computeLineTotals(line),
  }));

  const aggregate = aggregateTaxBreakdown(linesWithTotals);

  const party_account_id = defaultPartyAccountCode(invoice.invoice_type);
  // Tax side mirrors the revenue side: tax is debited for purchase flows
  // (Vorsteuer) and credited for sales flows (Umsatzsteuer), and credit notes
  // invert the side.
  const tax_is_debit = isInput !== isCreditNote;
  const tax_lines = [];
  if (!invoice.small_business && !invoice.reverse_charge) {
    for (const bucket of aggregate.tax_breakdown) {
      const account_id = taxAccountCodeForRate(bucket.tax_rate, isInput);
      if (!account_id) continue;
      tax_lines.push({
        account_id,
        debit: tax_is_debit ? bucket.tax_cents : 0,
        credit: tax_is_debit ? 0 : bucket.tax_cents,
        tax_rate_id: `tax_${bucket.tax_rate}`,
      });
    }
  }

  // 2. Revenue/expense lines — one per line (so COGS analysis is possible
  // without further joins).
  //   - sale_out: revenue 8400 is credited (income).
  //   - sale_in:  expense  3400 is debited (purchase cost).
  //   - credit_note_out: revenue 8400 is debited (Storno reduces income).
  //   - credit_note_in:  expense  3400 is credited (Storno reduces cost).
  // i.e. the side of the revenue/expense line is determined by whether the
  // invoice is an incoming purchase (debit) or outgoing sale (credit), and
  // credit notes invert that side.
  const revenue_is_debit = isInput !== isCreditNote;
  const revenue_lines = linesWithTotals.map((line, idx) => ({
    account_id: line.account_code || defaultRevenueAccountCode(invoice.invoice_type),
    debit: revenue_is_debit ? line.net_cents : 0,
    credit: revenue_is_debit ? 0 : line.net_cents,
    description: line.description,
    line_no: idx + 1,
  }));

  // 3. Party (receivable/payable) line. The party is the gross when tax is
  // present, the net otherwise. Side mirrors the invoice family:
  //   - sale_out: debit (we increase the receivable).
  //   - sale_in:  credit (we increase the payable).
  //   - credit_note_out: credit (Storno reduces the receivable).
  //   - credit_note_in:  debit (Storno reduces the payable).
  // i.e. the party is on the credit side for incoming flows and the debit
  // side for outgoing flows; credit notes invert that side.
  const party_base = tax_lines.length > 0 ? aggregate.total_cents : aggregate.subtotal_cents;
  const party_is_credit = isInput !== isCreditNote;
  const party_line = {
    account_id: party_account_id,
    debit: party_is_credit ? 0 : party_base,
    credit: party_is_credit ? party_base : 0,
    party_id: party.party_id,
  };

  const all_lines = [...revenue_lines, ...tax_lines, party_line];

  // 4. Sum and balance check.
  let total_debit = 0;
  let total_credit = 0;
  for (const l of all_lines) {
    total_debit += l.debit;
    total_credit += l.credit;
  }
  const balanced = total_debit === total_credit;

  return {
    id: `je_${invoice.id}_post`,
    posting_date: isoDateFromMs(invoice.invoice_date_ms),
    type: 'invoice',
    ref_type: 'invoice',
    ref_id: invoice.id,
    number: invoice.invoice_number || '',
    narration: isCreditNote
      ? `Credit note for ${invoice.invoice_number || invoice.id}`
      : `Invoice ${invoice.invoice_number || invoice.id}`,
    lines: all_lines,
    total_debit_cents: total_debit,
    total_credit_cents: total_credit,
    balanced,
  };
}

/**
 * @param {number} ms
 * @returns {string} YYYY-MM-DD
 */
function isoDateFromMs(ms) {
  if (!Number.isInteger(ms)) {
    throw new Error('isoDateFromMs requires integer ms');
  }
  const d = new Date(ms);
  const y = d.getUTCFullYear();
  const m = String(d.getUTCMonth() + 1).padStart(2, '0');
  const day = String(d.getUTCDate()).padStart(2, '0');
  return `${y}-${m}-${day}`;
}

export default { buildJournalEntry, isoDateFromMs };
