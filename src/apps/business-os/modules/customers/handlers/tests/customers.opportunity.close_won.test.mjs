import test from 'node:test';
import assert from 'node:assert/strict';
import { createCtx, run, seedBasic } from './_ctx.mjs';
import { stableDigest } from '../_lib.js';
const NOW = 1_700_000_000_000;
const T = 'customers.opportunity.close_won';

test('setzt closed_won; probability bleibt wie in Rust unverändert (Befund B1)', async () => {
  const seed = seedBasic();
  seed.customer_opportunities[0].probability = 60;
  const ctx = createCtx({ seed });
  const out = await run(ctx, T, { opportunity_id: 'opp_1' });
  const doc = ctx.$.doc('customer_opportunities', 'opp_1');
  assert.equal(doc.stage, 'closed_won');
  assert.equal(doc.probability, 60);
  assert.equal(out.activity.activity_type, 'opportunity_stage_changed');
  assert.equal(out.activity.properties.command_type, T);
  assert.equal(out.activity.id, 'act_' + stableDigest('cmd_' + T + ':opportunity_stage_changed:opp_1'));
});

test('Payload-Stufe wird überschrieben; geschlossen → nicht erneut verloren', async () => {
  const ctx = createCtx({ seed: seedBasic() });
  await run(ctx, T, { opportunity_id: 'opp_1', stage: 'proposal' });
  assert.equal(ctx.$.doc('customer_opportunities', 'opp_1').stage, 'closed_won');
  await assert.rejects(run(ctx, 'customers.opportunity.close_lost', { opportunity_id: 'opp_1', lost_reason: 'x' }), { code: 'conflict' });
});

test('record_id-Rückfall wie im UI-Builder (index.js:3393)', async () => {
  const ctx = createCtx({ seed: seedBasic() });
  const out = await run(ctx, T, {}, { record_id: 'opp_1' });
  assert.equal(out.opportunity.stage, 'closed_won');
});
