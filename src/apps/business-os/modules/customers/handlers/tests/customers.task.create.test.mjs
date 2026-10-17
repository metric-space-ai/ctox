import test from 'node:test';
import assert from 'node:assert/strict';
import { createCtx, run, seedBasic } from './_ctx.mjs';
import { stableDigest } from '../_lib.js';
const NOW = 1_700_000_000_000;
const T = 'customers.task.create';

test('legt Aufgabe mit Verknüpfungen an', async () => {
  const ctx = createCtx({ seed: seedBasic() });
  const out = await run(ctx, T, { task_id: 't1', account_id: 'acct_1', opportunity_id: 'opp_1', title: 'Renewal Call vorbereiten' });
  const doc = ctx.$.doc('customer_tasks', 't1');
  assert.equal(doc.status, 'open');
  assert.equal(doc.position, NOW);
  assert.equal(doc.search_text, 'renewal call vorbereiten open');
  assert.equal(out.activity.linked_record_type, 'task');
  assert.equal(out.activity.opportunity_id, 'opp_1');
  assert.equal(out.activity.properties.task.id, 't1');
  assert.equal(ctx.$.doc('customer_accounts', 'acct_1').last_activity_at_ms, undefined, 'Aufgaben berühren das Konto nicht');
});

test('Fehler: keine Verknüpfung, unbekannte Verknüpfung, Status, Titel', async () => {
  const ctx = createCtx({ seed: seedBasic() });
  await assert.rejects(run(ctx, T, { title: 'x' }), { code: 'invalid', message: 'account_id, contact_id or opportunity_id is required' });
  await assert.rejects(run(ctx, T, { title: 'x', contact_id: 'nope' }), { code: 'not_found', message: 'customer contact not found' });
  await assert.rejects(run(ctx, T, { title: 'x', account_id: 'acct_1', status: 'done' }), { code: 'invalid', message: 'task status `done` is not supported' });
  await assert.rejects(run(ctx, T, { account_id: 'acct_1' }), { code: 'invalid', message: 'title is required' });
  assert.equal(ctx.$.writes.length, 0);
});
