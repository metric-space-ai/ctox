import test from 'node:test';
import assert from 'node:assert/strict';
import { createCtx, run, seedBasic } from './_ctx.mjs';
import { stableDigest } from '../_lib.js';
const NOW = 1_700_000_000_000;
const T = 'customers.account.update';

test('mischt nur erlaubte Felder, prüft und protokolliert patch', async () => {
  const ctx = createCtx({ seed: seedBasic(), now: NOW });
  const payload = { account_id: 'acct_1', name: 'Metric Space AG', industry: 'AI', customer_stage: 'renewal', foo: 'bar' };
  const out = await run(ctx, T, payload);
  const doc = ctx.$.doc('customer_accounts', 'acct_1');
  assert.equal(doc.name, 'Metric Space AG');
  assert.equal(doc.customer_stage, 'renewal');
  assert.equal(doc.foo, undefined, 'foo ist nicht in der Merge-Liste');
  assert.equal(doc.search_text, 'metric space ag metric-space.ai ai');
  assert.equal(doc.updated_at_ms, NOW);
  assert.deepEqual(out.activity.properties.patch, payload);
  assert.equal(out.activity.activity_type, 'account_updated');
});

test('record_id-Rückfall und Fehlerpfade', async () => {
  const ctx = createCtx({ seed: seedBasic() });
  const out = await run(ctx, T, { name: 'Neu' }, { record_id: 'acct_1' });
  assert.equal(out.account.name, 'Neu');
  await assert.rejects(run(ctx, T, { name: 'x' }), { code: 'invalid', message: 'account_id is required' });
  await assert.rejects(run(ctx, T, { account_id: 'nope' }), { code: 'not_found', message: 'customer account not found' });
  await assert.rejects(run(ctx, T, { account_id: 'acct_1', account_status: 'zombie' }), { code: 'invalid' });
  assert.equal(ctx.$.doc('customer_accounts', 'acct_1').account_status, 'active');
});
