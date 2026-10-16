import test from 'node:test';
import assert from 'node:assert/strict';
import { createCtx, run, seedBasic } from './_ctx.mjs';
import { stableDigest } from '../_lib.js';
const NOW = 1_700_000_000_000;
const T = 'customers.contact.archive';

test('archiviert Kontakt ohne search_text-Neuberechnung, berührt Konto', async () => {
  const seed = seedBasic();
  seed.customer_contacts[0].search_text = 'alt';
  const ctx = createCtx({ seed });
  const out = await run(ctx, T, { contact_id: 'contact_1' });
  const doc = ctx.$.doc('customer_contacts', 'contact_1');
  assert.equal(doc.is_deleted, true);
  assert.equal(doc.deleted_at_ms, NOW);
  assert.equal(doc.search_text, 'alt');
  assert.equal(out.activity.linked_record_name, 'Ada Lovelace');
  assert.equal(ctx.$.doc('customer_accounts', 'acct_1').last_activity_at_ms, NOW);
});

test('Kontakt ohne account_id: Aktivität mit leerer account_id, kein Touch', async () => {
  const seed = seedBasic();
  seed.customer_contacts.push({ ...seed.customer_contacts[0], id: 'c_orphan', account_id: ' ' });
  const ctx = createCtx({ seed });
  const out = await run(ctx, T, { contact_id: 'c_orphan' });
  assert.equal(out.activity.account_id, '');
  assert.equal(ctx.$.doc('customer_accounts', 'acct_1').last_activity_at_ms, undefined);
});
