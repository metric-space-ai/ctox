import test from 'node:test';
import assert from 'node:assert/strict';
import { createTestCtx, cmd } from './_ctx.mjs';
import { message, engagement } from './_fixtures.mjs';
import handle from '../outbound.message.cancel.js';

test('cancel writes cancel_reason and closed_reason', async () => {
  const t = createTestCtx({ data: { outbound_messages: { msg_1: message() }, outbound_engagements: { eng_1: engagement() } } });
  await handle(cmd('outbound.message.cancel', { message_id: 'msg_1', reason: 'falsch' }), t.ctx);
  assert.equal(t.doc('outbound_messages', 'msg_1').send_status, 'cancelled');
  assert.equal(t.doc('outbound_messages', 'msg_1').payload.cancel_reason, 'falsch');
  assert.equal(t.doc('outbound_engagements', 'eng_1').status, 'cancelled');
  assert.equal(t.doc('outbound_engagements', 'eng_1').closed_reason, 'falsch');
});

test('cancel without reason leaves reasons untouched', async () => {
  const t = createTestCtx({ data: { outbound_messages: { msg_1: message() }, outbound_engagements: { eng_1: engagement() } } });
  await handle(cmd('outbound.message.cancel', { message_id: 'msg_1' }), t.ctx);
  assert.equal(t.doc('outbound_messages', 'msg_1').payload.cancel_reason, undefined);
  assert.equal(t.doc('outbound_engagements', 'eng_1').closed_reason, undefined);
});
