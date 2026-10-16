import test from 'node:test';
import assert from 'node:assert/strict';
import { createCtx, run, seedBasic } from './_ctx.mjs';
import { stableDigest } from '../_lib.js';
const NOW = 1_700_000_000_000;
const T = 'customers.dedupe.resolve';
const seed = () => ({ ...seedBasic(), customer_dedupe_candidates: [{ id: 'd1', object_type: 'account', match_key: 'acme.example', match_type: 'domain', status: 'open', confidence: 0.95, existing_record_id: 'acct_1', is_deleted: false, created_at_ms: 1, updated_at_ms: 1 }] });

test('hält Entscheidung fest (keine Konto-Änderung, Befund B2)', async () => {
  const ctx = createCtx({ seed: seed() });
  const out = await run(ctx, T, { candidate_id: 'd1', decision: 'merge' });
  const doc = ctx.$.doc('customer_dedupe_candidates', 'd1');
  assert.equal(doc.status, 'resolved');
  assert.equal(doc.decision, 'merge');
  assert.equal(doc.decided_by_id, 'tester');
  assert.equal(doc.decided_at_ms, NOW);
  assert.equal(out.activity.account_id, 'acct_1');
  assert.equal(out.activity.linked_record_name, 'acme.example');
  assert.equal(ctx.$.writes.filter((w) => w.collection === 'customer_accounts').length, 0);
});

test('merge braucht existing_record_id; Entscheidung geprüft; actor-Rückfall', async () => {
  const s = seed();
  s.customer_dedupe_candidates[0].existing_record_id = '';
  const ctx = createCtx({ seed: s });
  await assert.rejects(run(ctx, T, { candidate_id: 'd1', decision: 'merge' }), { code: 'invalid', message: 'existing_record_id is required' });
  await assert.rejects(run(ctx, T, { candidate_id: 'd1', decision: 'maybe' }), { code: 'invalid' });
  const out = await run(ctx, T, { dedupe_candidate_id: 'd1', decision: 'skip' }, { actor: null });
  assert.equal(out.dedupe_candidate.decided_by_id, 'business-os-user');
});
