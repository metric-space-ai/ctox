// create / update / delete / post – nachgebildet: post_writes_journal_entry_and_marks_invoice_posted,
// create_then_update_then_delete_draft_lifecycle_persists, missing_invoice_id_is_rejected_for_create,
// create_with_empty_party_id_is_rejected, post_with_zero_lines_is_rejected, post_with_skonto_*,
// post_with_invalid_invoice_type_is_rejected, handle_rejects_wrong_module (invoices.rs Tests).
import assert from 'node:assert/strict';
import test from 'node:test';
import create from '../invoices.invoice.create.js';
import update from '../invoices.invoice.update.js';
import del from '../invoices.invoice.delete.js';
import post from '../invoices.invoice.post.js';
import { cmd, rejects } from './_ctx.mjs';
import { NOW, draftPayload, line, setup } from './_fixtures.mjs';

test('create: Entwurf, currency fest EUR, Summen 0, search_text, idempotent', async () => {
  const t = await setup();
  const r = await create(cmd('invoices.invoice.create', draftPayload({ currency: 'USD' }), { id: 'cmd_c' }), t.ctx);
  assert.equal(r.invoice.state, 'draft');
  assert.equal(r.invoice.currency, 'EUR');
  assert.equal(r.invoice.command_id, 'cmd_c');
  assert.equal(r.invoice.total_cents, 0);
  assert.equal(r.invoice.search_text, 'cust_p eur');
  assert.equal(r.invoice.created_at_ms, NOW);
  const again = await create(cmd('invoices.invoice.create', draftPayload({ party_id: 'other' })), t.ctx);
  assert.equal(again.idempotent, true);
  assert.equal(again.invoice.party_id, 'cust_p');
});

test('create: Pflichtfelder und Validierung', async () => {
  const t = await setup();
  await rejects(create(cmd('invoices.invoice.create', { invoice_type: 'sale_out' }), t.ctx), 'invalid', /missing required field.*\["invoice_id", "id"\]/);
  await rejects(create(cmd('invoices.invoice.create', draftPayload({ party_id: '' })), t.ctx), 'invalid', /party_id is required/);
  await rejects(create(cmd('invoices.invoice.create', draftPayload({ invoice_type: 'bogus' })), t.ctx), 'invalid', /invoice_type must be one of/);
  await rejects(create(cmd('invoices.invoice.create', draftPayload({ skonto_percent: 2 })), t.ctx), 'invalid', /skonto_days must be positive/);
  await rejects(create(cmd('invoices.invoice.create', draftPayload({ small_business: true, tax_breakdown: [{ tax_rate: 0.19, net_cents: 1, tax_cents: 0 }] })), t.ctx), 'invalid', /small_business/);
  await rejects(create(cmd('invoices.invoice.create', draftPayload({ invoice_type: 'credit_note_out', reverse_charge: true, credit_note_for_id: 'x' })), t.ctx), 'invalid', /reverse_charge/);
  await rejects(create(cmd('invoices.invoice.create', draftPayload({ invoice_type: 'sale_in', eu_ic_supply: true })), t.ctx), 'invalid', /eu_ic_supply/);
  await rejects(create(cmd('invoices.invoice.create', draftPayload({ invoice_type: 'credit_note_in' })), t.ctx), 'invalid', /credit_note_for_id/);
  await rejects(create(cmd('invoices.invoice.create', draftPayload(), { app_id: 'customers' }), t.ctx), 'forbidden', /requires module=invoices/);
  // record_id-Rueckfall (Rust invoices_record_requirement)
  const r = await create(cmd('invoices.invoice.create', { invoice_type: 'sale_out', party_id: 'p' }, { record_id: 'inv_rec' }), t.ctx);
  assert.equal(r.invoice.id, 'inv_rec');
});

test('update nur im Entwurf; delete schreibt Soft-Delete und Grabstein', async () => {
  const t = await setup();
  await create(cmd('invoices.invoice.create', draftPayload()), t.ctx);
  const u = await update(cmd('invoices.invoice.update', { invoice_id: 'inv_post', party_id: 'cust_q' }), t.ctx);
  assert.equal(u.invoice.search_text, 'cust_q eur');
  await rejects(update(cmd('invoices.invoice.update', { invoice_id: 'inv_post', party_id: '' }), t.ctx), 'invalid');
  const d = await del(cmd('invoices.invoice.delete', { invoice_id: 'inv_post' }), t.ctx);
  assert.equal(d.invoice.is_deleted, true);
  assert.equal(d.invoice.deleted_at_ms, NOW);
  assert.equal(t.doc('accounting_invoices', 'inv_post'), null);
  await rejects(update(cmd('invoices.invoice.update', { invoice_id: 'inv_post' }), t.ctx), 'not_found');
});

test('post: Nummer RE-2023-0001, Summen, ausgeglichene Buchung, Zeilen, Status posted, idempotent, Folgenummer', async () => {
  const t = await setup();
  await create(cmd('invoices.invoice.create', draftPayload()), t.ctx);
  const r = await post(cmd('invoices.invoice.post', { invoice_id: 'inv_post' }, { id: 'cmd_post' }), t.ctx);
  const inv = r.invoice;
  assert.equal(inv.state, 'posted');
  assert.equal(inv.invoice_number, 'RE-2023-0001'); // invoice_date 2023-11-14
  assert.deepEqual([inv.subtotal_cents, inv.tax_cents, inv.total_cents, inv.paid_cents, inv.open_cents], [12000, 2280, 14280, 0, 14280]);
  assert.equal(inv.post_journal_entry_id, 'je_inv_post_post');
  assert.equal(inv.state_changed_by_command_id, 'cmd_post');
  const je = r.journal_entry;
  assert.deepEqual(
    { type: je.type, ref_id: je.ref_id, balanced: je.balanced, posting_date: je.posting_date, narration: je.narration, d: je.total_debit_cents, c: je.total_credit_cents },
    { type: 'invoice', ref_id: 'inv_post', balanced: true, posting_date: '2023-11-14', narration: 'Invoice RE-2023-0001', d: 14280, c: 14280 },
  );
  const lines = t.all('accounting_journal_entry_lines').sort((a, b) => a.line_no - b.line_no);
  assert.deepEqual(lines.map((l) => [l.account_id, l.debit, l.credit]), [['8400', 0, 12000], ['3806', 0, 2280], ['1400', 14280, 0]]);
  assert.equal(lines[1].tax_rate_id, 'tax_0.19');
  assert.equal(t.doc('accounting_number_series', 'sale_out_2023').next_value, 2);
  const again = await post(cmd('invoices.invoice.post', { invoice_id: 'inv_post' }), t.ctx);
  assert.equal(again.idempotent, true);
  assert.equal(again.journal_entry.ref_id, 'inv_post');
  await rejects(update(cmd('invoices.invoice.update', { invoice_id: 'inv_post', currency: 'USD' }), t.ctx), 'conflict', /state=draft/);
  await create(cmd('invoices.invoice.create', draftPayload({ invoice_id: 'inv_post_b' })), t.ctx);
  const b = await post(cmd('invoices.invoice.post', { invoice_id: 'inv_post_b' }), t.ctx);
  assert.equal(b.invoice.invoice_number, 'RE-2023-0002');
});

test('post: Eingangsrechnung, Rabatt, 7 %, Kleinunternehmer, Reverse Charge, Jahreswechsel Januar', async () => {
  const t = await setup();
  const jan = Date.UTC(2025, 0, 1, 12);
  await create(cmd('invoices.invoice.create', draftPayload({ invoice_id: 'in1', invoice_type: 'sale_in', invoice_date_ms: jan, lines: [line({ tax_rate: 0.07, discount_percent: 10, quantity: 1500, account_code: '3400' })] })), t.ctx);
  const r = await post(cmd('invoices.invoice.post', { invoice_id: 'in1' }), t.ctx);
  assert.equal(r.invoice.invoice_number, 'ER-2025-0001');
  // 12000*(0.9)=10800; *1.5=16200; 7 % = 1134
  assert.deepEqual([r.invoice.subtotal_cents, r.invoice.tax_cents], [16200, 1134]);
  const lines = t.all('accounting_journal_entry_lines').filter((l) => l.journal_entry_id === 'je_in1_post').sort((a, b) => a.line_no - b.line_no);
  assert.deepEqual(lines.map((l) => [l.account_id, l.debit, l.credit]), [['3400', 16200, 0], ['1407', 1134, 0], ['1600', 0, 17334]]);

  await create(cmd('invoices.invoice.create', draftPayload({ invoice_id: 'sb', small_business: true })), t.ctx);
  const sb = await post(cmd('invoices.invoice.post', { invoice_id: 'sb' }), t.ctx);
  assert.deepEqual([sb.invoice.tax_cents, sb.invoice.total_cents, sb.journal_entry.total_debit_cents], [0, 12000, 12000]);
  await create(cmd('invoices.invoice.create', draftPayload({ invoice_id: 'rc', reverse_charge: true })), t.ctx);
  const rc = await post(cmd('invoices.invoice.post', { invoice_id: 'rc' }), t.ctx);
  assert.equal(rc.invoice.total_cents, 12000);
});

test('post: strenge Pruefung und atomarer Abbruch (keine Nummer verbraucht)', async () => {
  const t = await setup();
  await create(cmd('invoices.invoice.create', draftPayload({ invoice_id: 'empty', lines: [] })), t.ctx);
  await rejects(post(cmd('invoices.invoice.post', { invoice_id: 'empty' }), t.ctx), 'invalid', /at least one line item/);
  await create(cmd('invoices.invoice.create', draftPayload({ invoice_id: 'nodate', invoice_date_ms: undefined })), t.ctx);
  await rejects(post(cmd('invoices.invoice.post', { invoice_id: 'nodate' }), t.ctx), 'invalid', /invoice_date_ms is required/);
  await create(cmd('invoices.invoice.create', draftPayload({ invoice_id: 'noacc', lines: [line({ account_code: '' })] })), t.ctx);
  await rejects(post(cmd('invoices.invoice.post', { invoice_id: 'noacc' }), t.ctx), 'invalid', /account_code/);
  await create(cmd('invoices.invoice.create', draftPayload({ invoice_id: 'sk', skonto_percent: 2, skonto_days: 10 })), t.ctx);
  assert.equal((await post(cmd('invoices.invoice.post', { invoice_id: 'sk' }), t.ctx)).invoice.state, 'posted');
  // Befund I3: 16 % bekommt keine Steuerzeile, Personenkonto enthaelt die Steuer trotzdem -> unausgeglichen
  await create(cmd('invoices.invoice.create', draftPayload({ invoice_id: 'mix', lines: [line(), line({ id: 'l2', tax_rate: 0.16 })] })), t.ctx);
  await rejects(post(cmd('invoices.invoice.post', { invoice_id: 'mix' }), t.ctx), 'invalid', /journal entry unbalanced/);
  assert.equal(t.doc('accounting_number_series', 'sale_out_2023').next_value, 2, 'nur sk hat eine Nummer verbraucht');
  assert.equal(t.doc('accounting_invoices', 'mix').state, 'draft');
  await rejects(post(cmd('invoices.invoice.post', { invoice_id: 'missing' }), t.ctx), 'not_found');
});

test('Befund I3b: nur 16 % bucht ohne Steuerzeile, Rechnung zeigt aber Steuer', async () => {
  const t = await setup();
  await create(cmd('invoices.invoice.create', draftPayload({ invoice_id: 'r16', lines: [line({ tax_rate: 0.16 })] })), t.ctx);
  const r = await post(cmd('invoices.invoice.post', { invoice_id: 'r16' }), t.ctx);
  assert.deepEqual([r.invoice.tax_cents, r.invoice.total_cents, r.journal_entry.total_debit_cents], [1920, 13920, 12000]);
});
