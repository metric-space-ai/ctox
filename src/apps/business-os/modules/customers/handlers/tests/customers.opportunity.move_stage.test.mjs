import test from 'node:test';
import assert from 'node:assert/strict';
import { createCtx, run, seedBasic } from './_ctx.mjs';
import { stableDigest } from '../_lib.js';
const NOW = 1_700_000_000_000;
const T = 'customers.opportunity.move_stage';

test('verschiebt Stufe, setzt Zeitstempel und optional position', async () => {
  const ctx = createCtx({ seed: seedBasic() });
  const out = await run(ctx, T, { opportunity_id: 'opp_1', stage: 'proposal', position: 3 });
  const doc = ctx.$.doc('customer_opportunities', 'opp_1');
  assert.equal(doc.stage, 'proposal');
  assert.equal(doc.position, 3);
  assert.equal(doc.last_stage_changed_at_ms, NOW);
  assert.equal(doc.search_text, 'renewal 2026 proposal renewal');
  assert.deepEqual({ from: out.activity.properties.from_stage, to: out.activity.properties.to_stage }, { from: 'qualification', to: 'proposal' });
  assert.equal(out.activity.name, 'Opportunity verschoben');
});

test('geschlossene Opportunity kann nicht zurück; gleiche Stufe erlaubt', async () => {
  const seed = seedBasic();
  seed.customer_opportunities[0].stage = 'closed_won';
  const ctx = createCtx({ seed });
  await assert.rejects(run(ctx, T, { opportunity_id: 'opp_1', stage: 'proposal' }), { code: 'conflict', message: 'closed opportunities cannot move stage' });
  const same = await run(ctx, T, { opportunity_id: 'opp_1', stage: 'closed_won' });
  assert.equal(same.ok, true);
});

test('Fehler: Stufe fehlt/unbekannt', async () => {
  const ctx = createCtx({ seed: seedBasic() });
  await assert.rejects(run(ctx, T, { opportunity_id: 'opp_1' }), { code: 'invalid', message: 'stage is required' });
  await assert.rejects(run(ctx, T, { opportunity_id: 'opp_1', stage: 'won' }), { code: 'invalid', message: 'opportunity stage `won` is not supported' });
});

test('ohne account_id: Rust schreibt vor dem Fehler – v4 rollt zurück (Abweichung A1)', async () => {
  const seed = seedBasic();
  delete seed.customer_opportunities[0].account_id;
  const ctx = createCtx({ seed: { ...seed, customer_opportunities: seed.customer_opportunities.map((o) => ({ ...o, account_id: '' })) } });
  await assert.rejects(run(ctx, T, { opportunity_id: 'opp_1', stage: 'proposal' }), { code: 'invalid', message: 'account_id is required' });
  assert.equal(ctx.$.doc('customer_opportunities', 'opp_1').stage, 'qualification');
});
