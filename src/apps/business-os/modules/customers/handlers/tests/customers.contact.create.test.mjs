import test from 'node:test';
import assert from 'node:assert/strict';
import { createCtx, run, seedBasic } from './_ctx.mjs';
import { stableDigest } from '../_lib.js';
const NOW = 1_700_000_000_000;
const T = 'customers.contact.create';

test('legt Kontakt an, Aktivität, berührt Konto last_activity_at_ms', async () => {
  const ctx = createCtx({ seed: seedBasic() });
  const out = await run(ctx, T, { contact_id: 'c2', account_id: 'acct_1', first_name: 'Grace', last_name: 'Hopper', job_title: 'CTO' });
  const doc = ctx.$.doc('customer_contacts', 'c2');
  assert.equal(doc.email, '');
  assert.equal(doc.is_primary_contact, false);
  assert.equal(doc.search_text, 'grace hopper cto');
  assert.equal(out.activity.linked_record_name, 'Grace Hopper');
  assert.equal(out.activity.account_id, 'acct_1');
  assert.equal(out.activity.contact_id, 'c2');
  assert.equal(ctx.$.doc('customer_accounts', 'acct_1').last_activity_at_ms, NOW);
});

test('nur E-Mail genügt; ohne Name und E-Mail invalid', async () => {
  const ctx = createCtx({ seed: seedBasic() });
  const out = await run(ctx, T, { contact_id: 'c3', account_id: 'acct_1', email: 'x@y.z' });
  assert.equal(out.activity.linked_record_name, 'x@y.z');
  await assert.rejects(run(ctx, T, { contact_id: 'c4', account_id: 'acct_1', first_name: ' ' }),
    { code: 'invalid', message: 'contact name or email is required' });
});

test('Fehler: Konto fehlt/unbekannt, Dublette', async () => {
  const ctx = createCtx({ seed: seedBasic() });
  await assert.rejects(run(ctx, T, { contact_id: 'c5', email: 'a@b' }), { code: 'invalid', message: 'account_id is required' });
  await assert.rejects(run(ctx, T, { contact_id: 'c5', account_id: 'zz', email: 'a@b' }), { code: 'not_found', message: 'customer account not found' });
  await assert.rejects(run(ctx, T, { contact_id: 'contact_1', account_id: 'acct_1', email: 'a@b' }), { code: 'conflict' });
  assert.equal(ctx.$.writes.length, 0);
});
