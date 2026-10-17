import test from 'node:test';
import assert from 'node:assert/strict';
import { createTestCtx, cmd } from './_ctx.mjs';
import { message, engagement } from './_fixtures.mjs';
import handle from '../outbound.provider.reconcile.js';

test('promotes delivered to sent and failed to failed', async () => {
  const t = createTestCtx({ now: 50, caps: {
    // Kernform caps/mail.rs DeliveryQuery: {message_ids} → {deliveries:[…]}; je id gilt die jüngste Zustellung.
    'mail.delivery_log.query': async ({ message_ids: ids }) => ({ deliveries: [
      { message_id: 'q1', outcome: 'deferred', error_text: 'later', completed_at: 30 },
      { message_id: 'q1', outcome: 'delivered', error_text: null, completed_at: 40 },
      { message_id: 'q2', outcome: 'failed', error_text: '550', completed_at: 41 },
    ].filter((d) => ids.includes(d.message_id)) }),
  }, data: {
    outbound_engagements: { eng_1: engagement() },
    outbound_messages: {
      a: message({ id: 'a', send_status: 'queued_for_provider', provider_message_id: 'q1' }),
      b: message({ id: 'b', send_status: 'queued', payload: { provider_queue_id: 'q2' } }),
      c: message({ id: 'c', send_status: 'sent', provider_message_id: 'q3' }),
      d: message({ id: 'd', send_status: 'queued_for_provider' }),
    },
  } });
  const result = await handle(cmd('outbound.provider.reconcile'), t.ctx);
  assert.equal(result.checked, 2);
  assert.equal(result.updated.length, 2);
  const a = t.doc('outbound_messages', 'a');
  assert.equal(a.send_status, 'sent');
  assert.equal(a.sent_at_ms, 40);
  assert.equal(a.payload.delivered_at_ms, 40);
  assert.equal(t.doc('outbound_messages', 'b').send_status, 'failed');
  assert.equal(t.doc('outbound_messages', 'b').payload.provider_error_text, '550');
  assert.equal(t.doc('outbound_engagements', 'eng_1').status, 'sent');
});

test('missing log capability -> log_available false, no writes', async () => {
  const t = createTestCtx({ data: { outbound_messages: { a: message({ id: 'a', send_status: 'queued', provider_message_id: 'q' }) } } });
  const result = await handle(cmd('outbound.provider.reconcile'), t.ctx);
  assert.equal(result.log_available, false);
  assert.equal(result.checked, 1);
  assert.equal(t.calls.writes.length, 0);
});
