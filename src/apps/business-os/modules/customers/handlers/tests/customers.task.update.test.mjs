import test from 'node:test';
import assert from 'node:assert/strict';
import { createCtx, run, seedBasic } from './_ctx.mjs';
import { stableDigest } from '../_lib.js';
const NOW = 1_700_000_000_000;
const T = 'customers.task.update';
const seed = () => ({ ...seedBasic(), customer_tasks: [{ id: 't1', title: 'Call', status: 'open', account_id: 'acct_1', search_text: 'call open', is_deleted: false, created_at_ms: 1, updated_at_ms: 1, position: 1, payload: {} }] });

test('aktualisiert Status und Fälligkeit', async () => {
  const ctx = createCtx({ seed: seed() });
  const out = await run(ctx, T, { task_id: 't1', status: 'in_progress', due_at_ms: 99, body: 'Agenda' });
  const doc = ctx.$.doc('customer_tasks', 't1');
  assert.equal(doc.status, 'in_progress');
  assert.equal(doc.due_at_ms, 99);
  assert.equal(doc.search_text, 'call agenda in_progress');
  assert.equal(out.activity.name, 'Aufgabe aktualisiert');
});

test('alle Verknüpfungen entfernen → invalid', async () => {
  const ctx = createCtx({ seed: seed() });
  await assert.rejects(run(ctx, T, { task_id: 't1', account_id: '' }), { code: 'invalid' });
  await assert.rejects(run(ctx, T, { task_id: 't1', status: 'x' }), { code: 'invalid' });
  await assert.rejects(run(ctx, T, { task_id: 'zz' }), { code: 'not_found', message: 'customer task not found' });
});
