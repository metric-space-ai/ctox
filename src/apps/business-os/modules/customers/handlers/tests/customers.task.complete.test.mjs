import test from 'node:test';
import assert from 'node:assert/strict';
import { createCtx, run, seedBasic } from './_ctx.mjs';
import { stableDigest } from '../_lib.js';
const NOW = 1_700_000_000_000;
const T = 'customers.task.complete';

test('erledigt Aufgabe ohne weitere Prüfung', async () => {
  const ctx = createCtx({ seed: { customer_tasks: [{ id: 't1', title: 'Call', status: 'cancelled', search_text: '', is_deleted: false, created_at_ms: 1, updated_at_ms: 1 }] } });
  const out = await run(ctx, T, { task_id: 't1' });
  const doc = ctx.$.doc('customer_tasks', 't1');
  assert.equal(doc.status, 'completed');
  assert.equal(doc.completed_at_ms, NOW);
  assert.equal(doc.search_text, 'call completed');
  assert.equal(out.activity.activity_type, 'task_completed');
  assert.equal(out.activity.account_id, '');
});

test('fehlende Aufgabe', async () => {
  const ctx = createCtx();
  await assert.rejects(run(ctx, T, {}, { record_id: 'x' }), { code: 'not_found' });
});
