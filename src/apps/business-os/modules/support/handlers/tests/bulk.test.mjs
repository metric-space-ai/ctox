import test from 'node:test';
import assert from 'node:assert/strict';
import { makeCtx, run, runError } from './_ctx.mjs';

async function two(env) {
  for (const id of ['conv_bulk_1', 'conv_bulk_2']) {
    await run(env, 'support.conversation.open_from_thread', { conversation_id: id, thread_key: `mail:${id}`, search_text: id });
  }
}

test('bulk ids: dedup + sort, record_id fallback, required', async () => {
  const env = makeCtx();
  await two(env);
  const result = await run(env, 'support.bulk.priority', { conversation_ids: ['conv_bulk_2', ' conv_bulk_1 ', 'conv_bulk_2'], priority: 'urgent' });
  assert.deepEqual(result.conversation_ids, ['conv_bulk_1', 'conv_bulk_2']);
  assert.equal(result.updated, 2);
  assert.equal((await runError(env, 'support.bulk.priority', { priority: 'low' })).message, 'conversation_ids is required');
  const viaRecord = await run(env, 'support.bulk.priority', { priority: 'low' }, { record_id: 'conv_bulk_1' });
  assert.deepEqual(viaRecord.conversation_ids, ['conv_bulk_1']);
  assert.equal(env.docs('support_conversation_events').filter((e) => e.event_type === 'support.conversation.priority_changed').length, 0, 'Rust bulk.priority writes no event');
});

test('support.bulk.assign writes bulk_assigned assignment events', async () => {
  const env = makeCtx();
  await two(env);
  assert.equal((await runError(env, 'support.bulk.assign', { ids: ['conv_bulk_1'] })).message, 'assignee_id or team_id is required');
  await run(env, 'support.bulk.assign', { ids: ['conv_bulk_1', 'conv_bulk_2'], user_id: 'agent_7' });
  assert.deepEqual(env.docs('support_conversations').map((c) => c.assignee_id), ['agent_7', 'agent_7']);
  const events = env.docs('support_assignment_events');
  assert.equal(events.length, 2);
  assert.ok(events.every((e) => e.event_type === 'support.assignment.bulk_assigned' && e.payload.team_id === null));
});

test('support.bulk.status resolves with gate and SLA resolution', async () => {
  const env = makeCtx();
  await two(env);
  const blocked = await runError(env, 'support.bulk.status', { conversation_ids: ['conv_bulk_1', 'conv_bulk_2'], status: 'done', required_fields: ['assignee_id'] });
  assert.equal(blocked.code, 'conflict');
  assert.deepEqual(env.docs('support_conversations').map((c) => c.status), ['open', 'open'], 'all-or-nothing (D1)');
  const ok = await run(env, 'support.bulk.status', { conversation_ids: ['conv_bulk_1', 'conv_bulk_2'], status: 'waiting' });
  assert.equal(ok.status, 'waiting');
  assert.equal(env.count('support_reporting_events'), 2);
});

test('support.bulk.snooze snoozes without events', async () => {
  const env = makeCtx();
  await two(env);
  assert.equal((await runError(env, 'support.bulk.snooze', { conversation_ids: ['conv_bulk_1'] })).message, 'snoozed_until_ms is required');
  await run(env, 'support.bulk.snooze', { conversation_ids: ['conv_bulk_1', 'conv_bulk_2'], snoozed_until_ms: 99 });
  assert.deepEqual(env.docs('support_conversations').map((c) => [c.status, c.snoozed_until_ms]), [['snoozed', 99], ['snoozed', 99]]);
  assert.equal(env.docs('support_conversation_events').filter((e) => e.event_type === 'support.conversation.snoozed').length, 0);
});

test('Rust bulk test (support.rs:5056): priority + resolve on two conversations', async () => {
  const env = makeCtx();
  await two(env);
  await run(env, 'support.bulk.priority', { conversation_ids: ['conv_bulk_1', 'conv_bulk_2'], priority: 'urgent' }, { record_id: '' });
  const result = await run(env, 'support.bulk.resolve', { conversation_ids: ['conv_bulk_1', 'conv_bulk_2'] }, { record_id: '' });
  assert.equal(result.status, 'resolved');
  assert.equal(env.docs('support_conversations').filter((c) => c.status === 'resolved' && c.priority === 'urgent').length, 2);
});
