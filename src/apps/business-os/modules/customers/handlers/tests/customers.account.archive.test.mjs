import test from 'node:test';
import assert from 'node:assert/strict';
import { createCtx, run, seedBasic } from './_ctx.mjs';
import { stableDigest } from '../_lib.js';
const NOW = 1_700_000_000_000;
const T = 'customers.account.archive';

test('archiviert Konto (Status, Stufe, is_deleted, deleted_at_ms)', async () => {
  const ctx = createCtx({ seed: seedBasic() });
  const out = await run(ctx, T, { account_id: 'acct_1' });
  const doc = ctx.$.doc('customer_accounts', 'acct_1');
  assert.equal(doc.account_status, 'archived');
  assert.equal(doc.customer_stage, 'archived');
  assert.equal(doc.is_deleted, true);
  assert.equal(doc.deleted_at_ms, NOW);
  assert.equal(out.activity.name, 'Kunde archiviert');
  assert.deepEqual(Object.keys(out).sort(), ['account', 'activity', 'ok']);
  assert.equal(out.activity.properties.account_id, 'acct_1');
});

test('archiviertes Konto bleibt ladbar (Rust prüft deleted-Spalte, nicht is_deleted)', async () => {
  const ctx = createCtx({ seed: seedBasic() });
  await run(ctx, T, { account_id: 'acct_1' }, { id: 'a1' });
  const again = await run(ctx, T, { account_id: 'acct_1' }, { id: 'a2' });
  assert.equal(again.ok, true);
  await assert.rejects(run(ctx, T, { account_id: 'x' }), { code: 'not_found' });
});
