import test from 'node:test';
import assert from 'node:assert/strict';
import { createCtx, run, seedBasic } from './_ctx.mjs';
import { stableDigest } from '../_lib.js';
const NOW = 1_700_000_000_000;
const T = 'customers.contact.update';

test('aktualisiert Kontakt und berührt Konto', async () => {
  const ctx = createCtx({ seed: seedBasic() });
  const out = await run(ctx, T, { contact_id: 'contact_1', job_title: 'Analyst', city: 'London', unknown: 1 });
  const doc = ctx.$.doc('customer_contacts', 'contact_1');
  assert.equal(doc.job_title, 'Analyst');
  assert.equal(doc.unknown, undefined);
  assert.equal(doc.search_text, 'ada lovelace ada@example.com analyst london');
  assert.equal(out.activity.activity_type, 'contact_updated');
  assert.equal(ctx.$.doc('customer_accounts', 'acct_1').last_activity_at_ms, NOW);
});

test('Kontowechsel auf unbekanntes Konto scheitert ohne Schreiben', async () => {
  const ctx = createCtx({ seed: seedBasic() });
  await assert.rejects(run(ctx, T, { contact_id: 'contact_1', account_id: 'ghost' }), { code: 'not_found' });
  assert.equal(ctx.$.doc('customer_contacts', 'contact_1').account_id, 'acct_1');
  await assert.rejects(run(ctx, T, { contact_id: 'none' }), { code: 'not_found', message: 'customer contact not found' });
  await assert.rejects(run(ctx, T, {}), { code: 'invalid', message: 'contact_id is required' });
});
