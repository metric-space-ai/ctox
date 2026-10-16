import assert from 'node:assert/strict';
import test from 'node:test';
import { cmd, rejects } from './_ctx.mjs';

const STUBS = [
  'invoices.invoice.assign_payment_terms', 'invoices.recurring.create', 'invoices.recurring.update', 'invoices.recurring.run',
  'invoices.recurring.pause', 'invoices.import.from_outbound', 'invoices.proposal.create', 'invoices.proposal.approve', 'invoices.proposal.reject',
];

test('echte Rust-Stubs melden unavailable mit woertlicher Meldung', async () => {
  for (const type of STUBS) {
    const mod = await import(`../${type}.js`);
    assert.equal(mod.STUB, true);
    await rejects(mod.default(cmd(type, {})), 'unavailable', new RegExp(`^${type.replaceAll('.', '\\.')} is not yet implemented \\(planned for Phase 7\\+\\)`));
    await rejects(mod.default(cmd(type, {}, { app_id: 'other' })), 'forbidden');
  }
});

test('alle 23 Rust-Befehlstypen (is_invoices_active_command) haben genau eine Handler-Datei', async () => {
  const { readdir } = await import('node:fs/promises');
  const files = (await readdir(new URL('..', import.meta.url))).filter((f) => f.endsWith('.js')).map((f) => f.slice(0, -3)).sort();
  const expected = [
    'invoices.invoice.create', 'invoices.invoice.update', 'invoices.invoice.delete', 'invoices.invoice.post', 'invoices.invoice.cancel',
    'invoices.invoice.create_credit_note', 'invoices.invoice.assign_payment_terms', 'invoices.line.create', 'invoices.line.update',
    'invoices.line.delete', 'invoices.payment.allocate', 'invoices.payment.unallocate', 'invoices.payment.match_suggestions',
    'invoices.dunning.run', 'invoices.dunning.letter.send', 'invoices.recurring.create', 'invoices.recurring.update', 'invoices.recurring.run',
    'invoices.recurring.pause', 'invoices.import.from_outbound', 'invoices.proposal.create', 'invoices.proposal.approve', 'invoices.proposal.reject',
  ].sort();
  assert.deepEqual(files, expected);
});
