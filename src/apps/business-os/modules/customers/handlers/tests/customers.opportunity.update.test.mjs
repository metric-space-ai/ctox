import test from 'node:test';
import assert from 'node:assert/strict';
import { createCtx, run, seedBasic } from './_ctx.mjs';
import { stableDigest } from '../_lib.js';
const NOW = 1_700_000_000_000;
const T = 'customers.opportunity.update';

test('aktualisiert Betrag/Wahrscheinlichkeit, stage bleibt unberührt', async () => {
  const ctx = createCtx({ seed: seedBasic() });
  const out = await run(ctx, T, { opportunity_id: 'opp_1', amount_cents: 5, probability: 40, stage: 'committed', name: 'Neu' });
  const doc = ctx.$.doc('customer_opportunities', 'opp_1');
  assert.equal(doc.amount_cents, 5);
  assert.equal(doc.probability, 40);
  assert.equal(doc.stage, 'qualification');
  assert.equal(doc.search_text, 'neu qualification renewal');
  assert.equal(out.activity.activity_type, 'opportunity_updated');
});

test('Fehler: unbekanntes Konto, ungültige Wahrscheinlichkeit, fehlende Opportunity', async () => {
  const ctx = createCtx({ seed: seedBasic() });
  await assert.rejects(run(ctx, T, { opportunity_id: 'opp_1', account_id: 'nope' }), { code: 'not_found' });
  await assert.rejects(run(ctx, T, { opportunity_id: 'opp_1', probability: -1 }), { code: 'invalid' });
  await assert.rejects(run(ctx, T, { opportunity_id: 'x' }), { code: 'not_found', message: 'customer opportunity not found' });
  assert.equal(ctx.$.writes.length, 0);
});
