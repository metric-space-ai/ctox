// core/invoice-types.js — pure type definitions (JSDoc) and helpers for the
// invoices domain. Ported from
// archive/reorg-review/templates/business-basic/packages/accounting/src/invoice/types.ts.
//
// No persistence, no RxDB, no business_commands. The UI and the native
// invoices handler use these helpers to reason about invoice state, types and
// currency without depending on storage.

/**
 * @typedef {'sale_out' | 'sale_in' | 'credit_note_out' | 'credit_note_in' | 'recurring_template'} InvoiceType
 *   sale_out = outgoing sales invoice, sale_in = incoming purchase invoice,
 *   credit_note_out / credit_note_in = §17 UStG corrections, recurring_template = template for periodic generation.
 */

/**
 * @typedef {'draft' | 'posted' | 'partially_paid' | 'paid' | 'overdue' | 'cancelled' | 'credited'} InvoiceState
 */

/**
 * @typedef InvoiceLine
 * @property {string} id
 * @property {number} position
 * @property {string} description
 * @property {string} [article_number]
 * @property {number} quantity integer count, three decimals stored as
 *   thousandths. This is the XRechnung / UBL convention: quantity=1000
 *   means 1.000 natural units, quantity=1500 means 1.500. The native
 *   poster and the JS poster must use the same convention.
 * @property {string} unit e.g. 'Stk' | 'h' | 'kg' | 'm'
 * @property {number} unit_price_cents in cent (integer)
 * @property {number} [discount_percent] 0..100
 * @property {number} tax_rate e.g. 0.19 for 19%
 * @property {string} account_code SKR03/04 code
 */

/**
 * @typedef PaymentTerms
 * @property {string} id
 * @property {string} name
 * @property {number} net_days
 * @property {number} [skonto_percent] 0..100
 * @property {number} [skonto_days]
 */

/**
 * @typedef Invoice
 * @property {string} id
 * @property {string} [invoice_number]
 * @property {InvoiceType} invoice_type
 * @property {string} party_id
 * @property {object} [party_snapshot]
 * @property {number} invoice_date_ms
 * @property {number} [due_date_ms]
 * @property {number} [service_period_start_ms]
 * @property {number} [service_period_end_ms]
 * @property {string} currency
 * @property {InvoiceLine[]} lines
 * @property {PaymentTerms} [payment_terms]
 * @property {number} [skonto_percent]
 * @property {number} [skonto_days]
 * @property {InvoiceState} state
 * @property {boolean} [reverse_charge]
 * @property {boolean} [small_business]
 * @property {boolean} [eu_ic_supply]
 * @property {string} [linked_invoice_id] for credit notes
 * @property {string} [credit_note_for_id]
 */

const INVOICE_TYPES = ['sale_out', 'sale_in', 'credit_note_out', 'credit_note_in', 'recurring_template'];
const INVOICE_STATES = ['draft', 'posted', 'partially_paid', 'paid', 'overdue', 'cancelled', 'credited'];

/**
 * @param {string} candidate
 * @returns {candidate is InvoiceType}
 */
function isInvoiceType(candidate) {
  return typeof candidate === 'string' && INVOICE_TYPES.indexOf(candidate) !== -1;
}

/**
 * @param {string} candidate
 * @returns {candidate is InvoiceState}
 */
function isInvoiceState(candidate) {
  return typeof candidate === 'string' && INVOICE_STATES.indexOf(candidate) !== -1;
}

/**
 * Returns true when an invoice in `state` can transition to `next`. Mirrors
 * the state machine in the customers opportunity lifecycle. Drafts can
 * become posted or cancelled; posted can become partially_paid, paid,
 * overdue, cancelled (with a storno) or credited (with a credit note);
 * partially_paid can become paid or overdue; paid and cancelled and
 * credited are terminal.
 *
 * @param {InvoiceState} state
 * @param {InvoiceState} next
 * @returns {boolean}
 */
function canTransition(state, next) {
  const allowed = {
    draft: ['posted', 'cancelled'],
    posted: ['partially_paid', 'paid', 'overdue', 'cancelled', 'credited'],
    partially_paid: ['paid', 'overdue', 'cancelled', 'credited'],
    overdue: ['partially_paid', 'paid', 'cancelled', 'credited'],
    paid: [],
    cancelled: [],
    credited: [],
  };
  if (!isInvoiceState(state) || !isInvoiceState(next)) return false;
  return (allowed[state] || []).indexOf(next) !== -1;
}

/**
 * @param {InvoiceType} type
 * @returns {string} the receivable/payable account code in SKR03/04.
 *   SKR03: 1400 = Forderungen, 1600 = Verbindlichkeiten.
 *   SKR04: 1200 = Forderungen, 3300 = Verbindlichkeiten.
 *   Default SKR03 — caller can override by passing a chart mapping.
 */
function defaultPartyAccountCode(type) {
  if (type === 'sale_out' || type === 'credit_note_out') return '1400';
  if (type === 'sale_in' || type === 'credit_note_in') return '1600';
  return '1400';
}

/**
 * @param {InvoiceType} type
 * @returns {string} default revenue/expense account (SKR03).
 */
function defaultRevenueAccountCode(type) {
  if (type === 'sale_out' || type === 'credit_note_out') return '8400';
  if (type === 'sale_in' || type === 'credit_note_in') return '3400';
  return '8400';
}

/**
 * Returns the tax account code for a given tax rate (SKR03). Negative for
 * Vorsteuer (input tax), positive for Umsatzsteuer (output tax). Reverse
 * charge (0%) and small business (0%) yield no tax account.
 *
 * @param {number} taxRate e.g. 0.19
 * @param {boolean} isInput true for input tax (Eingangsrechnung), false for output tax (Ausgangsrechnung)
 * @returns {string}
 */
function taxAccountCodeForRate(taxRate, isInput) {
  if (!taxRate || taxRate === 0) return '';
  if (isInput) {
    if (taxRate === 0.19) return '1406'; // 19% Vorsteuer
    if (taxRate === 0.07) return '1407'; // 7% Vorsteuer
    return '1406';
  }
  if (taxRate === 0.19) return '3806'; // 19% USt
  if (taxRate === 0.07) return '3801'; // 7% USt
  return '3806';
}

/**
 * Cent-based integer arithmetic. All amounts in invoices are integer cents.
 * Returns the sum of `a` and `b`. Throws if either is not an integer.
 *
 * @param {number} a
 * @param {number} b
 * @returns {number}
 */
function addCents(a, b) {
  if (!Number.isInteger(a) || !Number.isInteger(b)) {
    throw new Error('addCents requires integer cents');
  }
  return a + b;
}

const invoiceTypes = {
  INVOICE_TYPES,
  INVOICE_STATES,
  isInvoiceType,
  isInvoiceState,
  canTransition,
  defaultPartyAccountCode,
  defaultRevenueAccountCode,
  taxAccountCodeForRate,
  addCents,
};

if (typeof module !== 'undefined' && module.exports) {
  module.exports = invoiceTypes;
}

export {
  INVOICE_TYPES,
  INVOICE_STATES,
  isInvoiceType,
  isInvoiceState,
  canTransition,
  defaultPartyAccountCode,
  defaultRevenueAccountCode,
  taxAccountCodeForRate,
  addCents,
};
