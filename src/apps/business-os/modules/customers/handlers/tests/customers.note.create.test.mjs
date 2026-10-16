import test from 'node:test';
import assert from 'node:assert/strict';
import { createCtx, run, seedBasic } from './_ctx.mjs';
import { stableDigest } from '../_lib.js';
const NOW = 1_700_000_000_000;
const T = 'customers.note.create';

test('legt Notiz mit Defaults an', async () => {
  const ctx = createCtx({ seed: seedBasic() });
  const out = await run(ctx, T, { note_id: 'n1', contact_id: 'contact_1', body: 'Hallo Welt' });
  const doc = ctx.$.doc('customer_notes', 'n1');
  assert.equal(doc.title, 'Notiz');
  assert.equal(doc.body_format, 'markdown');
  assert.equal(doc.search_text, 'notiz hallo welt');
  assert.equal(out.activity.linked_record_type, 'note');
  assert.equal(out.activity.contact_id, 'contact_1');
});

test('Verknüpfung Pflicht; Dublette conflict', async () => {
  const ctx = createCtx({ seed: seedBasic() });
  await assert.rejects(run(ctx, T, { note_id: 'n1', body: 'x' }), { code: 'invalid' });
  await run(ctx, T, { note_id: 'n1', account_id: 'acct_1' });
  await assert.rejects(run(ctx, T, { note_id: 'n1', account_id: 'acct_1' }), { code: 'conflict', message: 'customer note already exists' });
});
