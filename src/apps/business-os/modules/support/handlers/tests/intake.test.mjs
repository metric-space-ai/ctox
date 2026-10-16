import test from 'node:test';
import assert from 'node:assert/strict';
import { makeCtx, run } from './_ctx.mjs';
import { NAMESPACE_URL, uuidV5 } from '../_shared.js';

const T = 1_781_697_600_000; // 2026-06-17T12:00:00Z

function channelData() {
  return {
    communication_threads: [
      { id: 'mail:thread-intake', thread_key: 'mail:thread-intake', channel: 'mail', account_key: 'mail:inbox', subject: 'Printengine is stuck', last_message_key: 'mail-msg-1', unread_count: 1, updated_at_ms: T },
      { id: 'business-os/support/conv_existing', thread_key: 'business-os/support/conv_existing', channel: 'queue', account_key: 'business-os', subject: 'Support: existing', last_message_key: 'queue-msg-1', unread_count: 1, updated_at_ms: T },
      { id: 'mail:outbound-only', thread_key: 'mail:outbound-only', channel: 'mail', subject: 'Newsletter', updated_at_ms: T },
    ],
    communication_messages: [
      { id: 'mail-msg-1', message_key: 'mail-msg-1', thread_key: 'mail:thread-intake', direction: 'inbound', updated_at_ms: T },
      { id: 'queue-msg-1', message_key: 'queue-msg-1', thread_key: 'business-os/support/conv_existing', direction: 'inbound', updated_at_ms: T },
      { id: 'out-1', message_key: 'out-1', thread_key: 'mail:outbound-only', direction: 'outbound', updated_at_ms: T },
    ],
  };
}

test('intake projects inbound threads, ignores internal and outbound-only threads, is idempotent (support.rs:4520/4599)', async () => {
  const env = makeCtx({ now: T + 1000, collections: channelData() });
  const first = await run(env, 'support.intake.project', { since_ms: 0 });
  assert.ok(first.changed_count >= 2);
  assert.equal(first.max_updated_at_ms, T);
  const conversations = env.docs('support_conversations');
  assert.equal(conversations.length, 1);
  const conv = conversations[0];
  assert.equal(conv.id, `support_conv_${uuidV5(NAMESPACE_URL, 'mail:thread-intake')}`);
  assert.equal(conv.primary_thread_key, 'mail:thread-intake');
  assert.equal(conv.search_text, 'Printengine is stuck');
  assert.equal(conv.last_message_key, 'mail-msg-1');
  assert.equal(conv.unread_count, 1);
  assert.equal(env.count('support_thread_links'), 1);
  const [event] = env.docs('support_conversation_events');
  assert.equal(event.event_type, 'support.intake.communication_thread');
  assert.equal(event.actor_id, 'ctox');
  assert.equal(event.source_command_id, '');
  const again = await run(env, 'support.intake.project', { since_ms: 0 });
  assert.equal(again.changed_count, 0);
  const later = await run(env, 'support.intake.project', { since_ms: first.max_updated_at_ms + 1 });
  assert.equal(later.changed_count, 0);
  assert.equal(later.max_updated_at_ms, first.max_updated_at_ms + 1);
});

test('intake updates an existing linked conversation without a second import event', async () => {
  const data = channelData();
  const env = makeCtx({ now: T + 1000, collections: data });
  await run(env, 'support.intake.project', {});
  await env.ctx.put('communication_threads', { ...data.communication_threads[0], subject: 'Printengine is stuck again', unread_count: 2, updated_at_ms: T + 60_000 });
  await env.ctx.put('communication_messages', { id: 'mail-msg-2', message_key: 'mail-msg-2', thread_key: 'mail:thread-intake', direction: 'inbound', updated_at_ms: T + 60_000 });
  const result = await run(env, 'support.intake.project', { since_ms: T });
  assert.ok(result.changed_count > 0);
  const conv = env.docs('support_conversations')[0];
  assert.equal(conv.search_text, 'Printengine is stuck again');
  assert.equal(conv.unread_count, 2);
  assert.equal(env.docs('support_conversation_events').filter((e) => e.event_type === 'support.intake.communication_thread').length, 1);
});
