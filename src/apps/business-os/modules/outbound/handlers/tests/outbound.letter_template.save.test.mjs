import test from 'node:test';
import assert from 'node:assert/strict';
import { createTestCtx, cmd, rejects } from './_ctx.mjs';
import handle from '../outbound.letter_template.save.js';

test('replaces template (no merge), bumps version', async () => {
  const t = createTestCtx({ now: 4, data: { outbound_letter_templates: { lt: { id: 'lt', title: 'Alt', closing: 'MfG', version_number: 1, created_at_ms: 1 } } } });
  const result = await handle(cmd('outbound.letter_template.save', { template_id: 'lt', title: 'Neu' }, { record_id: 'lt' }), t.ctx);
  const doc = t.doc('outbound_letter_templates', 'lt');
  assert.equal(result.version_number, 2);
  assert.equal(doc.title, 'Neu');
  assert.equal(doc.closing, '');
  assert.equal(doc.created_at_ms, 4);
  assert.equal(doc.template_id, 'lt');
});

test('requires record id', async () => {
  const t = createTestCtx();
  assert.equal((await rejects(handle(cmd('outbound.letter_template.save', { title: 'x' }), t.ctx))).code, 'invalid');
});
