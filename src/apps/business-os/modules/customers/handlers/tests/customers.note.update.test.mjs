import test from 'node:test';
import assert from 'node:assert/strict';
import { createCtx, run, seedBasic } from './_ctx.mjs';
import { stableDigest } from '../_lib.js';
const NOW = 1_700_000_000_000;
const T = 'customers.note.update';
const seed = () => ({ ...seedBasic(), customer_notes: [{ id: 'n1', title: 'Notiz', body: '', body_format: 'markdown', account_id: 'acct_1', search_text: 'notiz', is_deleted: false, created_at_ms: 1, updated_at_ms: 1 }] });

test('aktualisiert Titel/Text', async () => {
  const ctx = createCtx({ seed: seed() });
  const out = await run(ctx, T, { note_id: 'n1', title: 'Protokoll', body: 'Punkt 1', linked_note_id: 'n0' });
  const doc = ctx.$.doc('customer_notes', 'n1');
  assert.equal(doc.search_text, 'protokoll punkt 1');
  assert.equal(doc.linked_note_id, 'n0');
  assert.equal(out.activity.name, 'Notiz aktualisiert');
});

test('ungültige Verknüpfung', async () => {
  const ctx = createCtx({ seed: seed() });
  await assert.rejects(run(ctx, T, { note_id: 'n1', opportunity_id: 'nope' }), { code: 'not_found', message: 'customer opportunity not found' });
});
