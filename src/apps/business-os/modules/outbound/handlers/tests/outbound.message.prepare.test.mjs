import test from 'node:test';
import assert from 'node:assert/strict';
import { createTestCtx, cmd, rejects } from './_ctx.mjs';
import { engagement } from './_fixtures.mjs';
import handle from '../outbound.message.prepare.js';
import { messageRevision } from '../_lib/outbound.js';

test('prepares message, campaign from engagement, engagement -> draft_prepared', async () => {
  const t = createTestCtx({ now: 20, data: { outbound_engagements: { eng_1: engagement() } } });
  const result = await handle(cmd('outbound.message.prepare', { message_id: 'm1', engagement_id: 'eng_1', subject: 'S', body_text: 'B' }), t.ctx);
  const doc = t.doc('outbound_messages', 'm1');
  assert.equal(doc.campaign_id, 'camp_1');
  assert.equal(doc.message_type, 'initial');
  assert.equal(doc.approval_status, 'draft');
  assert.equal(doc.send_status, 'not_scheduled');
  assert.equal(doc.revision_id, messageRevision(doc));
  assert.equal(t.doc('outbound_engagements', 'eng_1').status, 'draft_prepared');
  assert.equal(result.collection, 'outbound_messages');
});

test('missing engagement without campaign_id -> invalid', async () => {
  const t = createTestCtx();
  const error = await rejects(handle(cmd('outbound.message.prepare', { message_id: 'm', engagement_id: 'nope' }), t.ctx));
  assert.equal(error.code, 'invalid');
});

test('missing engagement with campaign_id still writes message', async () => {
  const t = createTestCtx();
  await handle(cmd('outbound.message.prepare', { message_id: 'm', engagement_id: 'nope', campaign_id: 'c' }), t.ctx);
  assert.ok(t.doc('outbound_messages', 'm'));
});
