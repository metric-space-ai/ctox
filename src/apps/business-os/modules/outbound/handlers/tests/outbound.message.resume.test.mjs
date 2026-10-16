import test from 'node:test';
import assert from 'node:assert/strict';
import { createTestCtx, cmd, rejects } from './_ctx.mjs';
import { message, engagement } from './_fixtures.mjs';
import handle from '../outbound.message.resume.js';

test('resume approved message -> approved_not_sent / approved_for_send', async () => {
  const t = createTestCtx({ data: {
    outbound_messages: { msg_1: message({ send_status: 'paused', approval_status: 'approved' }) },
    outbound_engagements: { eng_1: engagement({ status: 'paused' }) },
  } });
  await handle(cmd('outbound.message.resume', { message_id: 'msg_1' }), t.ctx);
  const doc = t.doc('outbound_messages', 'msg_1');
  assert.equal(doc.send_status, 'approved_not_sent');
  assert.equal(doc.payload.resume_reason, 'manual_resume');
  assert.equal(t.doc('outbound_engagements', 'eng_1').status, 'approved_for_send');
});

test('resume draft -> not_scheduled / draft_prepared', async () => {
  const t = createTestCtx({ data: {
    outbound_messages: { msg_1: message({ send_status: 'paused', approval_status: 'draft' }) },
    outbound_engagements: { eng_1: engagement() },
  } });
  await handle(cmd('outbound.message.resume', { message_id: 'msg_1', reason: 'r' }), t.ctx);
  assert.equal(t.doc('outbound_messages', 'msg_1').send_status, 'not_scheduled');
  assert.equal(t.doc('outbound_engagements', 'eng_1').status, 'draft_prepared');
});

test('only paused', async () => {
  const t = createTestCtx({ data: { outbound_messages: { msg_1: message() } } });
  assert.equal((await rejects(handle(cmd('outbound.message.resume', { message_id: 'msg_1' }), t.ctx))).code, 'conflict');
});
