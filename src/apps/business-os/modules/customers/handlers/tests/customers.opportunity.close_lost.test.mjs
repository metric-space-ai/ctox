import test from 'node:test';
import assert from 'node:assert/strict';
import { createCtx, run, seedBasic } from './_ctx.mjs';
import { stableDigest } from '../_lib.js';
const NOW = 1_700_000_000_000;
const T = 'customers.opportunity.close_lost';

test('setzt closed_lost; lost_reason Pflicht, aber nicht gespeichert (Befund B1)', async () => {
  const ctx = createCtx({ seed: seedBasic() });
  const out = await run(ctx, T, { opportunity_id: 'opp_1', lost_reason: 'Closed lost' });
  const doc = ctx.$.doc('customer_opportunities', 'opp_1');
  assert.equal(doc.stage, 'closed_lost');
  assert.equal(doc.lost_reason, undefined);
  assert.equal(doc.probability, undefined);
  assert.equal(out.activity.properties.to_stage, 'closed_lost');
});

test('ohne lost_reason invalid, nichts geschrieben', async () => {
  const ctx = createCtx({ seed: seedBasic() });
  await assert.rejects(run(ctx, T, { opportunity_id: 'opp_1' }), { code: 'invalid', message: 'lost_reason is required' });
  assert.equal(ctx.$.writes.length, 0);
});
