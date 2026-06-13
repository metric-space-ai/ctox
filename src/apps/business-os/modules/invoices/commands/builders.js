// commands/builders.js — UI command builders for the invoices module.
// All builders emit objects shaped for `business_commands` with `command_type`
// as the canonical key (the `type` alias is allowed by commandBus but the
// persisted form is `command_type`).

import { buildXRechnungXml } from '../core/invoice-xrechnung.js';

const BUILD = 'invoices-ui-v0.1';

function buildCreateInvoiceCommand(invoiceId, payload = {}) {
  if (!invoiceId || typeof invoiceId !== 'string') {
    throw new Error('buildCreateInvoiceCommand requires a non-empty invoiceId');
  }
  return {
    module: 'invoices',
    command_type: 'invoices.invoice.create',
    record_id: invoiceId,
    payload: { invoice_id: invoiceId, ...payload },
    client_context: { build: BUILD, surface: 'invoices.invoice.create' },
  };
}

function buildUpdateInvoiceCommand(invoiceId, patch) {
  if (!invoiceId || typeof invoiceId !== 'string') {
    throw new Error('buildUpdateInvoiceCommand requires a non-empty invoiceId');
  }
  if (!patch || typeof patch !== 'object') {
    throw new Error('buildUpdateInvoiceCommand requires an object patch');
  }
  return {
    module: 'invoices',
    command_type: 'invoices.invoice.update',
    record_id: invoiceId,
    payload: { invoice_id: invoiceId, ...patch },
    client_context: { build: BUILD, surface: 'invoices.invoice.update' },
  };
}

function buildDeleteInvoiceCommand(invoiceId) {
  if (!invoiceId || typeof invoiceId !== 'string') {
    throw new Error('buildDeleteInvoiceCommand requires a non-empty invoiceId');
  }
  return {
    module: 'invoices',
    command_type: 'invoices.invoice.delete',
    record_id: invoiceId,
    payload: { invoice_id: invoiceId },
    client_context: { build: BUILD, surface: 'invoices.invoice.delete' },
  };
}

export {
  buildCreateInvoiceCommand,
  buildUpdateInvoiceCommand,
  buildDeleteInvoiceCommand,
  buildXRechnungXml,
};
