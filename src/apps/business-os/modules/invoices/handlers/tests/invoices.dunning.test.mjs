// nachgebildet: dunning_run_creates_letters_for_overdue_invoices, dunning_letter_send_marks_letter_sent_and_run_executed
import assert from 'node:assert/strict';
import test from 'node:test';
import send from '../invoices.dunning.letter.send.js';
import run from '../invoices.dunning.run.js';
import { cmd, rejects } from './_ctx.mjs';
import { NOW, setup } from './_fixtures.mjs';

const inv = (id, o = {}) => ({ id, invoice_type: 'sale_out', state: 'overdue', currency: 'EUR', search_text: '', is_deleted: false, created_at_ms: 1, updated_at_ms: 1, open_cents: 500, due_date_ms: NOW - 1000, ...o });

// strict:false – Befunde I4/I6: invoices_total ist ein Array (Schema number), Brief-Felder null (Schema string/number).
async function seeded() {
  return setup({
    strict: false,
    seed: {
      accounting_invoices: [
        inv('b_late', { due_date_ms: NOW - 5000 }),
        inv('a_late', { due_date_ms: NOW - 5000 }),
        inv('c_late'),
        inv('future', { due_date_ms: NOW + 1000 }),
        inv('nodue', { due_date_ms: undefined }),
        inv('zero', { open_cents: 0 }),
        inv('posted', { state: 'posted' }),
        inv('strdue', { due_date_ms: '1000' }),
      ],
    },
  });
}

test('run: faellige ueberfaellige Rechnungen sortiert nach Faelligkeit, dann ID; Briefe Stufe 1; idempotent', async () => {
  const t = await seeded();
  const r = await run(cmd('invoices.dunning.run', { run_id: 'run1' }, { actor: { id: 'buchhalter', role: 'user' } }), t.ctx);
  assert.deepEqual(r.run.invoices_total, ['strdue', 'a_late', 'b_late', 'c_late']);
  assert.equal(r.run.run_by, 'buchhalter');
  assert.deepEqual(r.letters.map((l) => [l.id, l.level, l.status]), [
    ['let_run1_strdue', 1, 'draft'], ['let_run1_a_late', 1, 'draft'], ['let_run1_b_late', 1, 'draft'], ['let_run1_c_late', 1, 'draft'],
  ]);
  const again = await run(cmd('invoices.dunning.run', { run_id: 'run1' }), t.ctx);
  assert.equal(again.idempotent, true);
  const f = await run(cmd('invoices.dunning.run', { run_id: 'run2', filter: { state: 'posted', min_open_cents: 100 } }), t.ctx);
  assert.deepEqual(f.run.invoices_total, ['posted']);
});

test('letter.send: Brief gesendet (ohne Versand), Lauf executed und hochgezaehlt', async () => {
  const t = await seeded();
  await run(cmd('invoices.dunning.run', { run_id: 'run1' }, { actor: {} }), t.ctx);
  assert.equal(t.doc('accounting_dunning_runs', 'run1').run_by, 'system');
  const s = await send(cmd('invoices.dunning.letter.send', { letter_id: 'let_run1_a_late' }), t.ctx);
  assert.deepEqual([s.letter.status, s.letter.sent_via, s.letter.sent_at_ms], ['sent', 'email', NOW]);
  await send(cmd('invoices.dunning.letter.send', { letter_id: 'let_run1_b_late' }), t.ctx);
  const r = t.doc('accounting_dunning_runs', 'run1');
  assert.deepEqual([r.letters_sent, r.state], [2, 'executed']);
  assert.equal(t.calls.length, 0, 'kein Capability-Aufruf: Rust versendet nichts');
  await rejects(send(cmd('invoices.dunning.letter.send', { letter_id: 'nope' }), t.ctx), 'not_found');
});
