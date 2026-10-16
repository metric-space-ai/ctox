import test from 'node:test';
import assert from 'node:assert/strict';
import { createTestCtx, cmd, rejects } from './_ctx.mjs';
import { engagement } from './_fixtures.mjs';
import handle from '../outbound.engagement.reapply_sequence.js';

test('snapshots sequence into engagement payload', async () => {
  const t = createTestCtx({ now: 8, data: {
    outbound_engagements: { eng_1: engagement({ sequence_id: 'seq' }) },
    outbound_sequences: { seq: { id: 'seq', name: 'S', updated_at_ms: 5 } },
  } });
  const result = await handle(cmd('outbound.engagement.reapply_sequence', { engagement_id: 'eng_1' }), t.ctx);
  assert.equal(result.sequence_id, 'seq');
  const eng = t.doc('outbound_engagements', 'eng_1');
  assert.equal(eng.payload.sequence_snapshot.name, 'S');
  assert.equal(eng.payload.sequence_reapplied_at_ms, 8);
});

test('errors: no sequence id, unknown sequence', async () => {
  const t = createTestCtx({ data: { outbound_engagements: { eng_1: engagement() } } });
  assert.equal((await rejects(handle(cmd('outbound.engagement.reapply_sequence', { engagement_id: 'eng_1' }), t.ctx))).code, 'invalid');
  assert.equal((await rejects(handle(cmd('outbound.engagement.reapply_sequence', { engagement_id: 'eng_1', sequence_id: 'x' }), t.ctx))).code, 'not_found');
});
