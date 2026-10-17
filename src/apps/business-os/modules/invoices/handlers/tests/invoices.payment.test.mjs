// nachgebildet: allocate_payment_reduces_open_and_marks_partially_paid_then_paid, allocate_on_draft_invoice_is_rejected
import assert from 'node:assert/strict';
import test from 'node:test';
import create from '../invoices.invoice.create.js';
import post from '../invoices.invoice.post.js';
import allocate from '../invoices.payment.allocate.js';
import suggestions from '../invoices.payment.match_suggestions.js';
import unallocate from '../invoices.payment.unallocate.js';
import { cmd, rejects } from './_ctx.mjs';
import { draftPayload, setup } from './_fixtures.mjs';

// strict:false – Befund I5: Rust schreibt note:null (Schema string).
async function postedInvoice() {
  const t = await setup({ strict: false });
  await create(cmd('invoices.invoice.create', draftPayload()), t.ctx);
  await post(cmd('invoices.invoice.post', { invoice_id: 'inv_post' }), t.ctx);
  return t;
}

test('allocate: Teilzahlung, dann Rest mit Skonto -> paid; idempotent je payment_id', async () => {
  const t = await postedInvoice();
  const a = await allocate(cmd('invoices.payment.allocate', { invoice_id: 'inv_post', payment_id: 'pay1', amount_cents: 10000 }, { id: 'cmd_a' }), t.ctx);
  assert.deepEqual([a.invoice.state, a.invoice.paid_cents, a.invoice.open_cents], ['partially_paid', 10000, 4280]);
  assert.equal(a.allocation.id, 'alloc_pay1_inv_post');
  assert.equal(a.allocation.allocated_by_command_id, 'cmd_a');
  const dup = await allocate(cmd('invoices.payment.allocate', { invoice_id: 'inv_post', payment_id: 'pay1', amount_cents: 10000 }), t.ctx);
  assert.equal(dup.idempotent, true);
  assert.equal(dup.invoice.paid_cents, 10000);
  const b = await allocate(cmd('invoices.payment.allocate', { invoice_id: 'inv_post', payment_id: 'pay2', amount_cents: 4000, skonto_cents: 280 }), t.ctx);
  assert.deepEqual([b.invoice.state, b.invoice.open_cents], ['paid', 0]);
  await rejects(allocate(cmd('invoices.payment.allocate', { invoice_id: 'inv_post', payment_id: 'pay3', amount_cents: 1 }), t.ctx), 'conflict', /exceeds open invoice balance: total=14280 paid=14280 applied=1/);
});

test('allocate: ohne payment_id deterministisch je Befehl; Entwurf abgelehnt; Betrag Pflicht', async () => {
  const t = await postedInvoice();
  const a = await allocate(cmd('invoices.payment.allocate', { invoice_id: 'inv_post', amount_cents: 100 }, { id: 'cmd_x' }), t.ctx);
  assert.equal(a.allocation.id, 'alloc_no_payment_cmd_x_inv_post');
  assert.equal(a.allocation.payment_id, null);
  await rejects(allocate(cmd('invoices.payment.allocate', { invoice_id: 'inv_post' }), t.ctx), 'invalid', /amount_cents or skonto_cents/);
  await create(cmd('invoices.invoice.create', draftPayload({ invoice_id: 'd' })), t.ctx);
  await rejects(allocate(cmd('invoices.payment.allocate', { invoice_id: 'd', payment_id: 'p', amount_cents: 1 }), t.ctx), 'conflict', /requires posted\/overdue/);
});

test('unallocate: Rueckbuchung setzt Status zurueck und entfernt die Zuordnung', async () => {
  const t = await postedInvoice();
  await allocate(cmd('invoices.payment.allocate', { invoice_id: 'inv_post', payment_id: 'p1', amount_cents: 14280 }), t.ctx);
  const u = await unallocate(cmd('invoices.payment.unallocate', { allocation_id: 'alloc_p1_inv_post' }), t.ctx);
  assert.deepEqual([u.invoice.state, u.invoice.paid_cents, u.invoice.open_cents], ['posted', 0, 14280]);
  assert.equal(t.doc('accounting_payment_allocations', 'alloc_p1_inv_post'), null);
  await rejects(unallocate(cmd('invoices.payment.unallocate', { allocation_id: 'alloc_p1_inv_post' }), t.ctx), 'not_found');
});

test('match_suggestions ist ein echter Stub', async () => {
  assert.deepEqual(await suggestions(cmd('invoices.payment.match_suggestions', {})), { ok: true, stub: true, command: 'invoices.payment.match_suggestions', phase: 3, suggestions: [] });
});
