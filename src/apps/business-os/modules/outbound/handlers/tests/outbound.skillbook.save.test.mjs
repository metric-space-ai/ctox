import test from 'node:test';
import assert from 'node:assert/strict';
import { createTestCtx, cmd, rejects } from './_ctx.mjs';
import handle from '../outbound.skillbook.save.js';

test('merges onto prior, bumps version, id from record_id', async () => {
  const t = createTestCtx({ now: 3, data: { outbound_skillbooks: { sb: { id: 'sb', mission: 'alt', title: 'T', version_number: 2, created_at_ms: 1, keep: 1 } } } });
  const result = await handle(cmd('outbound.skillbook.save', { skillbook_id: 'sb', mission: 'neu', stop_rules: 'x' }, { record_id: 'sb' }), t.ctx);
  const doc = t.doc('outbound_skillbooks', 'sb');
  assert.equal(result.version_number, 3);
  assert.equal(doc.mission, 'neu');
  assert.equal(doc.keep, 1);
  assert.deepEqual(doc.stop_rules, []);
  assert.deepEqual(doc.workflow_backbone, []);
  assert.equal(doc.created_at_ms, 1);
});

test('payload.skillbook_id alone is not an id (Rust path quirk)', async () => {
  const t = createTestCtx();
  const error = await rejects(handle(cmd('outbound.skillbook.save', { skillbook_id: 'sb' }), t.ctx));
  assert.equal(error.code, 'invalid');
  await handle(cmd('outbound.skillbook.save', { skillbook_id: { id: 'nested' } }), t.ctx);
  assert.equal(t.doc('outbound_skillbooks', 'nested').version_number, 1);
});
