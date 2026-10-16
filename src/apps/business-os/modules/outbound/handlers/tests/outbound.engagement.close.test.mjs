import test from 'node:test';
import assert from 'node:assert/strict';
import { createTestCtx, cmd } from './_ctx.mjs';
import { message, engagement } from './_fixtures.mjs';
import handle from '../outbound.engagement.close.js';

test('closes engagement and cancels open messages only', async () => {
  const t = createTestCtx({ now: 77, data: {
    outbound_engagements: { eng_1: engagement() },
    outbound_messages: {
      a: message({ id: 'a', send_status: 'awaiting_approval' }),
      b: message({ id: 'b', send_status: 'sent' }),
      c: message({ id: 'c', send_status: 'paused' }),
      d: message({ id: 'd', engagement_id: 'other', send_status: 'paused' }),
    },
  } });
  const result = await handle(cmd('outbound.engagement.close', { engagement_id: 'eng_1', reason: 'kein Bedarf' }), t.ctx);
  const eng = t.doc('outbound_engagements', 'eng_1');
  assert.equal(eng.status, 'closed');
  assert.equal(eng.closed_reason, 'kein Bedarf');
  assert.equal(eng.closed_at_ms, 77);
  assert.deepEqual(result.closed_message_ids.sort(), ['a', 'c']);
  assert.equal(t.doc('outbound_messages', 'a').payload.close_reason, 'kein Bedarf');
  assert.equal(t.doc('outbound_messages', 'b').send_status, 'sent');
  assert.equal(t.doc('outbound_messages', 'd').send_status, 'paused');
});
