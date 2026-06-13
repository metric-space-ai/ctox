// commands/builders.test.mjs — exercises the command builders.

import { strict as assert } from 'node:assert';
import { test } from 'node:test';
import {
  buildCreateInvoiceCommand,
  buildUpdateInvoiceCommand,
  buildDeleteInvoiceCommand,
} from '../commands/builders.js';

test('buildCreateInvoiceCommand uses command_type and includes invoice_id in payload', () => {
  const cmd = buildCreateInvoiceCommand('inv_1', { party_id: 'cust_1' });
  assert.equal(cmd.module, 'invoices');
  assert.equal(cmd.command_type, 'invoices.invoice.create');
  assert.equal(cmd.record_id, 'inv_1');
  assert.equal(cmd.payload.invoice_id, 'inv_1');
  assert.equal(cmd.payload.party_id, 'cust_1');
  assert.ok(cmd.client_context.surface);
});

test('buildUpdateInvoiceCommand merges the patch into the payload', () => {
  const cmd = buildUpdateInvoiceCommand('inv_1', { currency: 'USD' });
  assert.equal(cmd.command_type, 'invoices.invoice.update');
  assert.equal(cmd.payload.invoice_id, 'inv_1');
  assert.equal(cmd.payload.currency, 'USD');
});

test('buildDeleteInvoiceCommand only carries the id', () => {
  const cmd = buildDeleteInvoiceCommand('inv_1');
  assert.equal(cmd.command_type, 'invoices.invoice.delete');
  assert.equal(cmd.payload.invoice_id, 'inv_1');
  assert.deepEqual(Object.keys(cmd.payload), ['invoice_id']);
});

test('buildCreateInvoiceCommand rejects empty invoiceId', () => {
  assert.throws(() => buildCreateInvoiceCommand(''));
  assert.throws(() => buildCreateInvoiceCommand(null));
});
