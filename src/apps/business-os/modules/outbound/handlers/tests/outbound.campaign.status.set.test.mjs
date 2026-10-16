import test from 'node:test';
import assert from 'node:assert/strict';
import { createTestCtx, cmd, rejects } from './_ctx.mjs';
import handle from '../outbound.campaign.status.set.js';

const campaign = (extra = {}) => ({ id: 'c1', name: 'C', status: 'setup_required', payload: {}, ...extra });

test('activate email campaign needs mailbox + ready limit', async () => {
  const t = createTestCtx({ now: 9, data: { outbound_campaigns: { c1: campaign() } } });
  assert.equal((await rejects(handle(cmd('outbound.campaign.status.set', { campaign_id: 'c1', status: 'active' }), t.ctx))).code, 'conflict');
  const t2 = createTestCtx({ now: 9, data: {
    outbound_campaigns: { c1: campaign({ communication_account_key: 'email:a@b.de' }) },
    outbound_account_limits: { 'email:a@b.de': { id: 'email:a@b.de', status: 'suspended' } },
  } });
  const error = await rejects(handle(cmd('outbound.campaign.status.set', { campaign_id: 'c1', status: 'active' }), t2.ctx));
  assert.match(error.message, /status: suspended/);
  const t3 = createTestCtx({ now: 9, data: {
    outbound_campaigns: { c1: campaign({ payload: { communication_account_key: 'k' } }) },
    outbound_account_limits: { k: { id: 'k' } },
  } });
  const result = await handle(cmd('outbound.campaign.status.set', { campaign_id: 'c1', status: 'active' }), t3.ctx);
  assert.equal(result.channel, 'email');
  const doc = t3.doc('outbound_campaigns', 'c1');
  assert.equal(doc.status, 'active');
  assert.equal(doc.payload.status_set_at_ms, 9);
  assert.equal(doc.payload.active_channel, 'email');
});

test('physical_letter activates without mailbox; bad status invalid', async () => {
  const t = createTestCtx({ data: { outbound_campaigns: { c1: campaign({ payload: { active_outreach: { default_channel: 'physical_letter' } } }) } } });
  const result = await handle(cmd('outbound.campaign.status.set', { campaign_id: 'c1', status: 'active' }), t.ctx);
  assert.equal(result.channel, 'physical_letter');
  assert.equal((await rejects(handle(cmd('outbound.campaign.status.set', { campaign_id: 'c1', status: 'running' }), t.ctx))).code, 'invalid');
});
