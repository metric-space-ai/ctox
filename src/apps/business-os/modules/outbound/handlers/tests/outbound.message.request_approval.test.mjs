import test from 'node:test';
import assert from 'node:assert/strict';
import { createTestCtx, cmd, rejects } from './_ctx.mjs';
import { message, engagement } from './_fixtures.mjs';
import handle from '../outbound.message.request_approval.js';

test('moves message to awaiting_approval and engagement too', async () => {
  const t = createTestCtx({ data: {
    outbound_messages: { msg_1: message({ approval_status: 'draft', send_status: 'not_scheduled' }) },
    outbound_engagements: { eng_1: engagement() },
  } });
  await handle(cmd('outbound.message.request_approval', { message_id: 'msg_1' }), t.ctx);
  const doc = t.doc('outbound_messages', 'msg_1');
  assert.equal(doc.approval_status, 'awaiting_approval');
  assert.equal(doc.send_status, 'awaiting_approval');
  assert.equal(doc.draft_status, 'ready_for_review');
  assert.equal(t.doc('outbound_engagements', 'eng_1').status, 'awaiting_approval');
});

test('requires body', async () => {
  const t = createTestCtx({ data: { outbound_messages: { msg_1: message({ body_text: ' ' }) } } });
  const error = await rejects(handle(cmd('outbound.message.request_approval', { message_id: 'msg_1' }), t.ctx));
  assert.equal(error.code, 'invalid');
});
