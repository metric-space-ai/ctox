import test from 'node:test';
import assert from 'node:assert/strict';
import { createTestCtx, cmd, rejects } from './_ctx.mjs';
import handle from '../outbound.campaign.apply_setup.js';

test('merges payload patch and stamps setup task', async () => {
  const t = createTestCtx({ now: 12, data: { outbound_campaigns: { c1: { id: 'c1', status: 'setup_required', payload: { keep: 1, icp: 'alt' } } } } });
  await handle(cmd('outbound.campaign.apply_setup', { campaign_id: 'c1', status: 'active', campaign_payload_patch: { icp: 'neu' }, source_command_id: 'src' }, { id: 'apply' }), t.ctx);
  const doc = t.doc('outbound_campaigns', 'c1');
  assert.equal(doc.status, 'active');
  assert.equal(doc.payload.keep, 1);
  assert.equal(doc.payload.icp, 'neu');
  assert.deepEqual(doc.payload.campaign_setup_task, {
    command_id: 'src', apply_command_id: 'apply', source_command_id: 'src', status: 'completed',
    skill: 'business-os-outbound-campaign-setup', applied_at_ms: 12,
  });
});

test('invalid status ignored; patch required', async () => {
  const t = createTestCtx({ data: { outbound_campaigns: { c1: { id: 'c1', status: 'paused' } } } });
  await handle(cmd('outbound.campaign.apply_setup', { campaign_id: 'c1', status: 'bogus', campaign_payload_patch: {} }, { id: 'a' }), t.ctx);
  assert.equal(t.doc('outbound_campaigns', 'c1').status, 'paused');
  assert.equal(t.doc('outbound_campaigns', 'c1').payload.campaign_setup_task.command_id, 'a');
  assert.equal((await rejects(handle(cmd('outbound.campaign.apply_setup', { campaign_id: 'c1' }), t.ctx))).code, 'invalid');
});
