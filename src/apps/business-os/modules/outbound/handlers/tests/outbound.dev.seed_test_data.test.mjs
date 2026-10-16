import test from 'node:test';
import assert from 'node:assert/strict';
import { createTestCtx, cmd, rejects } from './_ctx.mjs';
import handle from '../outbound.dev.seed_test_data.js';

test('seeds count (clamped) idempotently', async () => {
  const t = createTestCtx();
  const first = await handle(cmd('outbound.dev.seed_test_data', { campaign_id: 'c', count: 2 }), t.ctx);
  assert.deepEqual(first.engagement_ids, ['dev_eng_c_0', 'dev_eng_c_1']);
  assert.equal(t.doc('outbound_engagements', 'dev_eng_c_1').payload.contact_email, 'lead1@example.com');
  const second = await handle(cmd('outbound.dev.seed_test_data', { campaign_id: 'c', count: 99 }), t.ctx);
  assert.equal(second.count, 23);
  assert.equal(t.all('outbound_engagements').length, 25);
  assert.equal((await rejects(handle(cmd('outbound.dev.seed_test_data', {}), t.ctx))).code, 'invalid');
});
