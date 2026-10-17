import test from 'node:test';
import assert from 'node:assert/strict';
import { createCtx, run, seedBasic } from './_ctx.mjs';
import { stableDigest } from '../_lib.js';
const NOW = 1_700_000_000_000;
const T = 'customers.activity.record';

test('schreibt freie Aktivität; name fällt auf activity_type zurück', async () => {
  const ctx = createCtx({ seed: seedBasic() });
  const out = await run(ctx, T, { activity_type: 'call_logged', account_id: 'acct_1', linked_record_id: 'acct_1', properties: { minutes: 5 } });
  assert.equal(out.activity.name, 'call_logged');
  assert.equal(out.activity.properties.minutes, 5);
  assert.equal(out.activity.id, 'act_' + stableDigest('cmd_' + T + ':call_logged:acct_1'));
  assert.ok(ctx.$.doc('customer_activities', out.activity.id));
});

test('Verknüpfung optional, aber geprüft; properties null → {value:null}', async () => {
  const ctx = createCtx({ seed: seedBasic() });
  const out = await run(ctx, T, { activity_type: 'x', properties: null });
  assert.equal(out.activity.properties.value, null);
  await assert.rejects(run(ctx, T, { activity_type: 'x', contact_id: 'zz' }), { code: 'not_found' });
  await assert.rejects(run(ctx, T, {}), { code: 'invalid', message: 'activity_type is required' });
});
