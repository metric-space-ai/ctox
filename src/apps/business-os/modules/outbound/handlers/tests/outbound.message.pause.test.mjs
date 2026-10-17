import test from 'node:test';
import assert from 'node:assert/strict';
import { createTestCtx, cmd } from './_ctx.mjs';
import { message, engagement } from './_fixtures.mjs';
import handle from '../outbound.message.pause.js';

test('pause stores reason on message and paused_reason on engagement', async () => {
  const t = createTestCtx({ data: { outbound_messages: { msg_1: message() }, outbound_engagements: { eng_1: engagement() } } });
  await handle(cmd('outbound.message.pause', { message_id: 'msg_1', reason: 'Urlaub' }), t.ctx);
  assert.equal(t.doc('outbound_messages', 'msg_1').send_status, 'paused');
  assert.equal(t.doc('outbound_messages', 'msg_1').payload.pause_reason, 'Urlaub');
  const eng = t.doc('outbound_engagements', 'eng_1');
  assert.equal(eng.status, 'paused');
  assert.equal(eng.paused_reason, 'Urlaub');
});
