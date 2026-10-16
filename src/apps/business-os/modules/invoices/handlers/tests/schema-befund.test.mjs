// Befunde: Datensaetze, die Rust schreibt, verletzen collections.schema.json. Unter v4-Schemapruefung
// (required + type) werden diese Schreibvorgaenge abgelehnt. Die Handler bleiben treu; der Schema-Port
// muss entscheiden (siehe V4-HANDLERS.md I4–I6).
import test from 'node:test';
import create from '../invoices.invoice.create.js';
import post from '../invoices.invoice.post.js';
import run from '../invoices.dunning.run.js';
import allocate from '../invoices.payment.allocate.js';
import { cmd, rejects } from './_ctx.mjs';
import { NOW, draftPayload, setup } from './_fixtures.mjs';

test('I4: accounting_dunning_runs.invoices_total ist in Rust ein ID-Array, im Schema number', async () => {
  const t = await setup();
  await rejects(run(cmd('invoices.dunning.run', { run_id: 'r' }), t.ctx), 'invalid', /invoices_total has wrong type/);
});

test('I5: accounting_payment_allocations.note = null (Schema string); ohne payment_id fehlt Pflichtfeld', async () => {
  const t = await setup();
  await create(cmd('invoices.invoice.create', draftPayload()), t.ctx);
  await post(cmd('invoices.invoice.post', { invoice_id: 'inv_post' }), t.ctx);
  await rejects(allocate(cmd('invoices.payment.allocate', { invoice_id: 'inv_post', payment_id: 'p', amount_cents: 1 }), t.ctx), 'invalid', /note has wrong type/);
  await rejects(allocate(cmd('invoices.payment.allocate', { invoice_id: 'inv_post', amount_cents: 1 }), t.ctx), 'invalid', /(note|payment_id) has wrong type/);
});

test('I6: accounting_dunning_letters mit sent_via/sent_at_ms/pdf_attachment_id = null', async () => {
  const t = await setup({
    seed: { accounting_invoices: [{ id: 'x', invoice_type: 'sale_out', state: 'overdue', currency: 'EUR', search_text: '', is_deleted: false, created_at_ms: 1, updated_at_ms: 1, open_cents: 5, due_date_ms: NOW - 1 }] },
  });
  // Lauf scheitert bereits an I4; Brief einzeln pruefen ueber die Attrappe:
  await rejects(t.ctx.put('accounting_dunning_letters', { id: 'l', dunning_run_id: 'r', invoice_id: 'x', level: 1, letter_date_ms: NOW, total_cents: 0, status: 'draft', created_at_ms: NOW, sent_via: null }), 'invalid', /sent_via has wrong type/);
});
