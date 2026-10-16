// cancel + create_credit_note (§5.11 Storno, §17 UStG)
import assert from 'node:assert/strict';
import test from 'node:test';
import cancel from '../invoices.invoice.cancel.js';
import create from '../invoices.invoice.create.js';
import creditNote from '../invoices.invoice.create_credit_note.js';
import post from '../invoices.invoice.post.js';
import { cmd, rejects } from './_ctx.mjs';
import { NOW, draftPayload, line, setup } from './_fixtures.mjs';

async function posted(t, o = {}) {
  await create(cmd('invoices.invoice.create', draftPayload(o)), t.ctx);
  return (await post(cmd('invoices.invoice.post', { invoice_id: o.invoice_id ?? 'inv_post' }), t.ctx)).invoice;
}

test('cancel: Entwurf wird storniert und entfernt; erneut -> not_found (Grabstein)', async () => {
  const t = await setup();
  await create(cmd('invoices.invoice.create', draftPayload()), t.ctx);
  assert.deepEqual(await cancel(cmd('invoices.invoice.cancel', { invoice_id: 'inv_post' }), t.ctx), { ok: true, invoice_id: 'inv_post', state: 'cancelled' });
  assert.equal(t.doc('accounting_invoices', 'inv_post'), null);
  await rejects(cancel(cmd('invoices.invoice.cancel', { invoice_id: 'inv_post' }), t.ctx), 'not_found');
});

test('cancel: gebuchte Rechnung -> gebuchte Storno-Gutschrift mit Gegenbuchung, Original cancelled, idempotent', async () => {
  const t = await setup();
  await posted(t);
  const r = await cancel(cmd('invoices.invoice.cancel', { invoice_id: 'inv_post' }, { id: 'cmd_storno' }), t.ctx);
  assert.equal(r.storno_credit_note_id, 'cn_inv_post_storno');
  assert.equal(r.storno_posted.invoice.invoice_number, 'GS-2025-0001'); // Gutschriftdatum = jetzt (2025)
  assert.equal(r.storno_posted.journal_entry.narration, 'Credit note for GS-2025-0001');
  const cn = t.doc('accounting_invoices', 'cn_inv_post_storno');
  assert.deepEqual([cn.invoice_type, cn.state, cn.credit_note_for_id, cn.credit_note_for_number, cn.total_cents], ['credit_note_out', 'posted', 'inv_post', 'RE-2023-0001', 14280]);
  const lines = t.all('accounting_journal_entry_lines').filter((l) => l.journal_entry_id === 'je_cn_inv_post_storno_post').sort((a, b) => a.line_no - b.line_no);
  assert.deepEqual(lines.map((l) => [l.account_id, l.debit, l.credit]), [['8400', 12000, 0], ['3806', 2280, 0], ['1400', 0, 14280]]);
  const orig = t.doc('accounting_invoices', 'inv_post');
  assert.deepEqual([orig.state, orig.cancelled_at_ms, orig.storno_credit_note_id], ['cancelled', NOW, 'cn_inv_post_storno']);
  assert.deepEqual(await cancel(cmd('invoices.invoice.cancel', { invoice_id: 'inv_post' }), t.ctx), { ok: true, idempotent: true, invoice_id: 'inv_post', state: 'cancelled' });
});

test('cancel: Reverse-Charge-Original -> steuerfreie Gutschrift; unzulaessiger Zustand', async () => {
  const t = await setup();
  await posted(t, { reverse_charge: true });
  await cancel(cmd('invoices.invoice.cancel', { invoice_id: 'inv_post' }), t.ctx);
  const cn = t.doc('accounting_invoices', 'cn_inv_post_storno');
  assert.deepEqual([cn.reverse_charge, cn.reverse_charge_origin, cn.lines[0].tax_rate, cn.total_cents], [false, true, 0, 12000]);
  const t2 = await setup({ seed: { accounting_invoices: [{ id: 'x', invoice_type: 'sale_out', state: 'paid', currency: 'EUR', search_text: '', is_deleted: false, created_at_ms: 1, updated_at_ms: 1 }] } });
  await rejects(cancel(cmd('invoices.invoice.cancel', { invoice_id: 'x' }), t2.ctx), 'conflict', /only draft or posted/);
});

test('create_credit_note: nur zu gebuchter Rechnung, idempotent cn_<id>, kumulative Obergrenze', async () => {
  const t = await setup();
  await create(cmd('invoices.invoice.create', draftPayload()), t.ctx);
  await rejects(creditNote(cmd('invoices.invoice.create_credit_note', { credit_note_for_id: 'inv_post' }), t.ctx), 'conflict', /requires a posted invoice/);
  await post(cmd('invoices.invoice.post', { invoice_id: 'inv_post' }), t.ctx);
  const half = [line({ quantity: 500 })];
  const a = await creditNote(cmd('invoices.invoice.create_credit_note', { credit_note_for_id: 'inv_post', lines: half, reason: 'Teil' }), t.ctx);
  assert.deepEqual([a.invoice.id, a.invoice.state, a.invoice.reason, a.invoice.search_text], ['cn_inv_post', 'draft', 'Teil', 'cust_p eur']);
  assert.equal((await creditNote(cmd('invoices.invoice.create_credit_note', { credit_note_for_id: 'inv_post' }), t.ctx)).idempotent, true);
  const b = await creditNote(cmd('invoices.invoice.create_credit_note', { credit_note_for_id: 'inv_post', credit_note_id: 'cn_2', lines: half }), t.ctx);
  assert.equal(b.invoice.id, 'cn_2');
  await rejects(
    creditNote(cmd('invoices.invoice.create_credit_note', { credit_note_for_id: 'inv_post', credit_note_id: 'cn_3', lines: [line({ quantity: 1 })] }), t.ctx),
    'conflict',
    /would exceed the invoice net \(12000 \+ 12 > 12000\)/,
  );
  await rejects(creditNote(cmd('invoices.invoice.create_credit_note', { credit_note_for_id: 'nope' }), t.ctx), 'not_found');
});
