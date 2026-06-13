// core/invoice-validate.js — pure validation for invoice drafts.
// Ported from
// archive/reorg-review/templates/business-basic/packages/accounting/src/invoice/validate.ts
// (TypeScript stripping + simplification; no Drizzle dependency).
//
// The native handler also enforces this contract; the UI uses the same
// helpers to surface field errors before submitting a business_command.

/**
 * @typedef {{ field: string, message: string, severity?: 'error' | 'warning' }} ValidationIssue
 */

import { isInvoiceType, isInvoiceState, INVOICE_STATES } from './invoice-types.js';

/**
 * Validate an invoice draft. Returns an array of issues (empty when valid).
 *
 * @param {object} invoice the invoice draft
 * @returns {ValidationIssue[]}
 */
export function validateInvoice(invoice) {
  const issues = [];
  if (!invoice || typeof invoice !== 'object') {
    issues.push({ field: '$', message: 'invoice is required' });
    return issues;
  }

  if (!isInvoiceType(invoice.invoice_type)) {
    issues.push({
      field: 'invoice_type',
      message: `invoice_type must be one of: sale_out, sale_in, credit_note_out, credit_note_in, recurring_template`,
    });
  }

  if (typeof invoice.party_id !== 'string' || invoice.party_id.length === 0) {
    issues.push({ field: 'party_id', message: 'party_id is required' });
  }

  if (typeof invoice.currency !== 'string' || invoice.currency.length === 0) {
    issues.push({ field: 'currency', message: 'currency is required' });
  }

  if (!Number.isInteger(invoice.invoice_date_ms) || invoice.invoice_date_ms <= 0) {
    issues.push({ field: 'invoice_date_ms', message: 'invoice_date_ms must be a positive integer ms timestamp' });
  }

  if (invoice.state !== undefined && !isInvoiceState(invoice.state)) {
    issues.push({
      field: 'state',
      message: `state must be one of: ${INVOICE_STATES.join(', ')}`,
    });
  }

  if (invoice.small_business && invoice.tax_breakdown && invoice.tax_breakdown.length > 0) {
    issues.push({
      field: 'small_business',
      severity: 'error',
      message: 'small_business invoices must not carry tax_breakdown entries',
    });
  }

  if (invoice.reverse_charge && invoice.invoice_type !== 'sale_out' && invoice.invoice_type !== 'sale_in') {
    issues.push({
      field: 'reverse_charge',
      severity: 'error',
      message: 'reverse_charge is only valid for sale_out and sale_in invoices',
    });
  }

  if (invoice.eu_ic_supply && invoice.invoice_type !== 'sale_out') {
    issues.push({
      field: 'eu_ic_supply',
      severity: 'error',
      message: 'eu_ic_supply (innergemeinschaftliche Lieferung) is only valid for sale_out invoices',
    });
  }

  if (invoice.invoice_type === 'credit_note_out' || invoice.invoice_type === 'credit_note_in') {
    if (!invoice.credit_note_for_id) {
      issues.push({
        field: 'credit_note_for_id',
        message: 'credit_note_* invoice types require a credit_note_for_id reference to the original invoice',
      });
    }
  }

  if (!Array.isArray(invoice.lines) || invoice.lines.length === 0) {
    issues.push({ field: 'lines', message: 'at least one line item is required' });
  } else {
    invoice.lines.forEach((line, idx) => validateLine(line, idx, issues));
  }

  if (typeof invoice.skonto_percent === 'number') {
    if (invoice.skonto_percent < 0 || invoice.skonto_percent > 100) {
      issues.push({ field: 'skonto_percent', message: 'skonto_percent must be 0..100' });
    }
    if (typeof invoice.skonto_days !== 'number' || invoice.skonto_days <= 0) {
      issues.push({ field: 'skonto_days', message: 'skonto_days must be positive when skonto_percent is set' });
    }
  }

  if (Array.isArray(invoice.tax_breakdown)) {
    invoice.tax_breakdown.forEach((entry, idx) => {
      if (!Number.isFinite(entry.tax_rate) || entry.tax_rate < 0 || entry.tax_rate > 1) {
        issues.push({ field: `tax_breakdown[${idx}].tax_rate`, message: 'tax_rate must be in [0, 1]' });
      }
      if (!Number.isInteger(entry.net_cents) || entry.net_cents < 0) {
        issues.push({ field: `tax_breakdown[${idx}].net_cents`, message: 'net_cents must be a non-negative integer' });
      }
      if (!Number.isInteger(entry.tax_cents) || entry.tax_cents < 0) {
        issues.push({ field: `tax_breakdown[${idx}].tax_cents`, message: 'tax_cents must be a non-negative integer' });
      }
    });
  }

  return issues;
}

/**
 * @param {object} line
 * @param {number} idx
 * @param {ValidationIssue[]} issues
 */
function validateLine(line, idx, issues) {
  if (!line || typeof line !== 'object') {
    issues.push({ field: `lines[${idx}]`, message: 'line is required' });
    return;
  }
  if (typeof line.description !== 'string' || line.description.length === 0) {
    issues.push({ field: `lines[${idx}].description`, message: 'description is required' });
  }
  if (!Number.isInteger(line.quantity) || line.quantity <= 0) {
    issues.push({ field: `lines[${idx}].quantity`, message: 'quantity must be a positive integer (in thousandths)' });
  }
  if (!Number.isInteger(line.unit_price_cents) || line.unit_price_cents < 0) {
    issues.push({ field: `lines[${idx}].unit_price_cents`, message: 'unit_price_cents must be a non-negative integer' });
  }
  if (!Number.isFinite(line.tax_rate) || line.tax_rate < 0 || line.tax_rate > 1) {
    issues.push({ field: `lines[${idx}].tax_rate`, message: 'tax_rate must be in [0, 1]' });
  }
  if (typeof line.account_code !== 'string' || line.account_code.length === 0) {
    issues.push({ field: `lines[${idx}].account_code`, message: 'account_code is required' });
  }
  if (typeof line.discount_percent === 'number' && (line.discount_percent < 0 || line.discount_percent > 100)) {
    issues.push({ field: `lines[${idx}].discount_percent`, message: 'discount_percent must be 0..100' });
  }
}

/**
 * Returns true when the array of issues contains no `severity: 'error'`
 * entries. Warnings are tolerated.
 *
 * @param {ValidationIssue[]} issues
 * @returns {boolean}
 */
export function isValid(issues) {
  return !issues.some((issue) => (issue.severity || 'error') === 'error');
}

export default { validateInvoice, isValid };
