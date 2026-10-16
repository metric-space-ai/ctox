import test from 'node:test';
import assert from 'node:assert/strict';
import { createTestCtx, cmd, rejects } from './_ctx.mjs';
import handle from '../outbound.campaign.briefing.update.js';

test('updates name/objective and whitelisted payload keys', async () => {
  const t = createTestCtx({ data: { outbound_campaigns: { c1: { id: 'c1', name: 'Alt', payload: { scope: 'x' } } } } });
  await handle(cmd('outbound.campaign.briefing.update', { campaign_id: 'c1', name: '  Neu  ', objective: 'Ziel', payload_patch: { briefing: 'B', evil: 1 } }), t.ctx);
  const doc = t.doc('outbound_campaigns', 'c1');
  assert.equal(doc.name, 'Neu');
  assert.equal(doc.objective, 'Ziel');
  assert.equal(doc.payload.briefing, 'B');
  assert.equal(doc.payload.scope, 'x');
  assert.equal(doc.payload.evil, undefined);
});

test('blank name ignored, unknown campaign not_found', async () => {
  const t = createTestCtx({ data: { outbound_campaigns: { c1: { id: 'c1', name: 'Alt' } } } });
  await handle(cmd('outbound.campaign.briefing.update', { campaign_id: 'c1', name: ' ' }), t.ctx);
  assert.equal(t.doc('outbound_campaigns', 'c1').name, 'Alt');
  assert.equal((await rejects(handle(cmd('outbound.campaign.briefing.update', { campaign_id: 'x' }), t.ctx))).code, 'not_found');
});
