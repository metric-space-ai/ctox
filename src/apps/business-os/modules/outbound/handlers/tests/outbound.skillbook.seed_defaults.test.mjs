import test from 'node:test';
import assert from 'node:assert/strict';
import { createTestCtx, cmd } from './_ctx.mjs';
import handle from '../outbound.skillbook.seed_defaults.js';

test('seeds three defaults once', async () => {
  const t = createTestCtx({ now: 11, data: { outbound_skillbooks: { 'business-os.outbound.scheduling.v1': { id: 'business-os.outbound.scheduling.v1', custom: true } } } });
  const first = await handle(cmd('outbound.skillbook.seed_defaults'), t.ctx);
  assert.deepEqual(first.seeded, ['business-os.outbound.message_drafting.v1', 'business-os.outbound.reply_handling.v1']);
  assert.equal(t.doc('outbound_skillbooks', 'business-os.outbound.scheduling.v1').custom, true);
  const drafting = t.doc('outbound_skillbooks', 'business-os.outbound.message_drafting.v1');
  assert.equal(drafting.non_negotiable_rules.length, 5);
  assert.equal(drafting.workflow_backbone[4].step, 'writeback');
  assert.equal(drafting.created_at_ms, 11);
  const second = await handle(cmd('outbound.skillbook.seed_defaults'), t.ctx);
  assert.deepEqual(second.seeded, []);
});
