import test from 'node:test';
import assert from 'node:assert/strict';
import { createTestCtx, cmd, rejects } from './_ctx.mjs';
import { engagement, message } from './_fixtures.mjs';
import handle from '../outbound.reply.match.js';

test('marks reply, cancels pending outbound messages, annotates', async () => {
  const annotations = [];
  const t = createTestCtx({ now: 70, data: {
    communication_messages: { k1: { id: 'k1', message_key: 'k1', metadata_json: { source: 'mail' } } },
    outbound_engagements: { eng_1: engagement() },
    outbound_messages: {
      a: message({ id: 'a', send_status: 'awaiting_approval' }),
      b: message({ id: 'b', send_status: 'sent' }),
      c: message({ id: 'c', send_status: 'paused' }),
      d: message({ id: 'd', direction: 'inbound', send_status: 'draft' }),
    },
  } });
  const result = await handle(cmd('outbound.reply.match', { engagement_id: 'eng_1', communication_message_key: 'k1', outbound_message_id: 'b' }), t.ctx);
  assert.equal(result.classification, 'unclear');
  assert.deepEqual(result.cancelled_message_ids, ['a']);
  assert.equal(t.doc('outbound_messages', 'a').payload.cancelled_reason, 'reply_received');
  const eng = t.doc('outbound_engagements', 'eng_1');
  assert.equal(eng.status, 'reply_received');
  assert.equal(eng.payload.reply_message_id, 'k1');
  // Annotation direkt in communication_messages (App conversations, foreign write), Metadaten zusammengeführt.
  assert.deepEqual(t.doc('communication_messages', 'k1').metadata_json, { source: 'mail', outbound_engagement_id: 'eng_1', outbound_reply_classification: 'unclear', outbound_message_id: 'b' });
  assert.equal(annotations.length, 0);
});

test('unsubscribe reply stops engagement; annotate failure ignored; key required', async () => {
  const t = createTestCtx({ data: { outbound_engagements: { eng_1: engagement() } } });
  const result = await handle(cmd('outbound.reply.match', { engagement_id: 'eng_1', reply_message_id: 'r', classification: 'unsubscribe' }), t.ctx);
  assert.ok(result.suppression_id);
  assert.equal(t.doc('outbound_engagements', 'eng_1').status, 'stopped');
  assert.equal(t.calls.logs[0].level, 'warn');
  assert.equal((await rejects(handle(cmd('outbound.reply.match', { engagement_id: 'eng_1' }), t.ctx))).code, 'invalid');
});
