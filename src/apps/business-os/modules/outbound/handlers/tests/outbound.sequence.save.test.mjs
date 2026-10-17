import test from 'node:test';
import assert from 'node:assert/strict';
import { createTestCtx, cmd, rejects } from './_ctx.mjs';
import handle from '../outbound.sequence.save.js';

test('saves sequence with defaults', async () => {
  const t = createTestCtx({ now: 7 });
  await handle(cmd('outbound.sequence.save', { sequence_id: 's1', campaign_id: 'c' }), t.ctx);
  const doc = t.doc('outbound_sequences', 's1');
  assert.equal(doc.name, 'Outbound Sequence');
  assert.equal(doc.strategy_text, '');
  assert.deepEqual(doc.approval_policy, {});
  assert.equal(doc.created_at_ms, 7);
});

test('blank name gets default, campaign required', async () => {
  const t = createTestCtx();
  await handle(cmd('outbound.sequence.save', { id: 's2', campaign_id: 'c', name: '  ' }), t.ctx);
  assert.equal(t.doc('outbound_sequences', 's2').name, 'Outbound Sequence');
  const error = await rejects(handle(cmd('outbound.sequence.save', { id: 's3' }), t.ctx));
  assert.equal(error.code, 'invalid');
});
