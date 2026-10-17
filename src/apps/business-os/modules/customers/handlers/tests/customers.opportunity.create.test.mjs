import test from 'node:test';
import assert from 'node:assert/strict';
import { createCtx, run, seedBasic } from './_ctx.mjs';
import { stableDigest } from '../_lib.js';
const NOW = 1_700_000_000_000;
const T = 'customers.opportunity.create';

test('legt Opportunity mit Defaults an (Rust-Testfall Renewal 2026)', async () => {
  const ctx = createCtx({ seed: seedBasic() });
  const out = await run(ctx, T, {
    opportunity_id: 'opp_x', name: 'Renewal 2026', account_id: 'acct_1', primary_contact_id: 'contact_1',
    amount_cents: 1200000, opportunity_type: 'renewal',
  });
  const doc = ctx.$.doc('customer_opportunities', 'opp_x');
  assert.equal(doc.stage, 'qualification');
  assert.equal(doc.currency, 'EUR');
  assert.equal(doc.position, NOW);
  assert.equal(doc.last_stage_changed_at_ms, NOW);
  assert.equal(doc.search_text, 'renewal 2026 qualification renewal');
  assert.equal(out.activity.contact_id, 'contact_1');
  assert.equal(out.activity.opportunity_id, 'opp_x');
  assert.equal(ctx.$.doc('customer_accounts', 'acct_1').last_activity_at_ms, NOW);
});

test('Defaults ohne Angaben; prefix opp', async () => {
  const ctx = createCtx({ seed: seedBasic() });
  const out = await run(ctx, T, { name: 'Neu', account_id: 'acct_1' }, { id: 'k' });
  assert.equal(out.opportunity.id, 'opp_' + stableDigest('k'));
  assert.equal(out.opportunity.opportunity_type, 'new_business');
  assert.equal(out.opportunity.amount_cents, 0);
});

test('Fehler: primärer Kontakt, Typ, Wahrscheinlichkeit, Dublette', async () => {
  const ctx = createCtx({ seed: seedBasic() });
  await assert.rejects(run(ctx, T, { name: 'n', account_id: 'acct_1', primary_contact_id: 'zz' }), { code: 'not_found', message: 'primary contact not found' });
  await assert.rejects(run(ctx, T, { name: 'n', account_id: 'acct_1', opportunity_type: 'upsell' }), { code: 'invalid' });
  await assert.rejects(run(ctx, T, { name: 'n', account_id: 'acct_1', probability: 101 }), { code: 'invalid', message: 'opportunity probability must be between 0 and 100' });
  await assert.rejects(run(ctx, T, { opportunity_id: 'opp_1', name: 'n', account_id: 'acct_1' }), { code: 'conflict' });
  await assert.rejects(run(ctx, T, { account_id: 'acct_1' }), { code: 'invalid', message: 'name is required' });
  assert.equal(ctx.$.writes.length, 0);
});

test('Bruch-Wahrscheinlichkeit wird wie in Rust (as_i64) nicht geprüft', async () => {
  const ctx = createCtx({ seed: seedBasic() });
  const out = await run(ctx, T, { name: 'n', account_id: 'acct_1', probability: 150.5 });
  assert.equal(out.opportunity.probability, 150.5);
});
