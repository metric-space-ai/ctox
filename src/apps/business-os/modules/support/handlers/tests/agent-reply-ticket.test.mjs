import test from 'node:test';
import assert from 'node:assert/strict';
import { makeCtx, run, runError } from './_ctx.mjs';

async function opened(env, id = 'conv_1', extra = {}) {
  return run(env, 'support.conversation.open_from_thread', { conversation_id: id, thread_key: `mail:${id}`, search_text: 'Reply case', ...extra });
}

test('support.agent.writeback stores a proposed suggestion with Rust defaults', async () => {
  const env = makeCtx();
  await opened(env);
  const result = await run(env, 'support.agent.writeback', {
    conversation_id: 'conv_1', source_command_id: 'cmd_src', task_id: 'task_9', suggestion_kind: 'draft_reply',
    confidence: 0.82, summary: 'Antwortentwurf', payload: { body: 'Hallo' },
  });
  assert.equal(result.status, 'proposed');
  const s = env.doc('support_agent_suggestions', result.suggestion_id);
  assert.equal(s.status, 'proposed');
  assert.equal(s.confidence, 0, 'number_field drops floats (U6)');
  assert.equal(s.required_human_action, 'review');
  assert.equal(s.task_id, 'task_9');
  assert.deepEqual(s.payload, { body: 'Hallo' });
  const event = env.docs('support_conversation_events').find((e) => e.event_type === 'support.agent.writeback');
  assert.equal(event.source_task_id, 'task_9');
  const raw = await run(env, 'support.agent.writeback', { conversation_id: 'conv_1', confidence: 1 });
  const s2 = env.doc('support_agent_suggestions', raw.suggestion_id);
  assert.equal(s2.suggestion_kind, 'summary');
  assert.equal(s2.payload.raw_payload.conversation_id, 'conv_1');
  assert.equal(s2.confidence, 1);
});

test('support.agent.apply_suggestion / reject_suggestion set status and log decision', async () => {
  const env = makeCtx({ now: 1000 });
  await opened(env);
  const { suggestion_id: id } = await run(env, 'support.agent.writeback', { conversation_id: 'conv_1', summary: 's' });
  env.setNow(2000);
  const applied = await run(env, 'support.agent.apply_suggestion', { suggestion_id: id, task_id: 't1' });
  assert.equal(applied.status, 'applied');
  assert.equal(applied.conversation_id, 'conv_1');
  const s = env.doc('support_agent_suggestions', id);
  assert.equal(s.status, 'applied');
  assert.equal(s.updated_at_ms, 2000);
  assert.equal(s.created_at_ms, 1000);
  const rejected = await run(env, 'support.agent.reject_suggestion', { id });
  assert.equal(rejected.status, 'rejected');
  const types = env.docs('support_conversation_events').map((e) => e.event_type);
  assert.ok(types.includes('support.agent.suggestion_applied'));
  assert.ok(types.includes('support.agent.suggestion_rejected'));
  assert.equal(env.docs('support_conversation_events').find((e) => e.event_type === 'support.agent.suggestion_applied').source_task_id, 't1');
});

test('apply_suggestion: unknown suggestion falls back to payload conversation_id, else not_found', async () => {
  const env = makeCtx();
  const fallback = await run(env, 'support.agent.apply_suggestion', { suggestion_id: 'ghost', conversation_id: 'conv_x' });
  assert.equal(fallback.conversation_id, 'conv_x');
  assert.equal(env.count('support_agent_suggestions'), 0);
  const error = await runError(env, 'support.agent.reject_suggestion', { suggestion_id: 'ghost' });
  assert.equal(error.code, 'not_found');
  assert.equal(error.message, 'support agent suggestion does not exist');
  assert.equal((await runError(env, 'support.agent.apply_suggestion', {})).message, 'suggestion_id is required');
});

test('support.reply.draft creates a draft suggestion trimmed to 180 chars', async () => {
  const env = makeCtx();
  await opened(env);
  const body = 'x'.repeat(200);
  const result = await run(env, 'support.reply.draft', { conversation_id: 'conv_1', draft: body });
  assert.equal(result.status, 'draft');
  const s = env.doc('support_agent_suggestions', result.suggestion_id);
  assert.equal(s.summary.length, 180);
  assert.equal(s.payload.body, body);
  assert.equal(s.required_human_action, 'human_send_required');
  assert.equal(s.confidence, 1);
  assert.equal((await runError(env, 'support.reply.draft', { conversation_id: 'conv_1' })).message, 'draft body is required');
});

test('support.reply.send blocks direct mode and queues approval with attachment refs (support.rs:4963)', async () => {
  const env = makeCtx();
  await opened(env, 'conv_reply_1');
  const direct = await runError(env, 'support.reply.send', { conversation_id: 'conv_reply_1', body: 'Direct', send_mode: 'direct' });
  assert.equal(direct.code, 'unavailable');
  const outcome = await run(env, 'support.reply.send', {
    conversation_id: 'conv_reply_1', body: 'We will follow up after checking the logs.', attachment_file_ids: ['desktop_file_1'], channel: 'mail',
  });
  assert.equal(outcome.status, 'pending_approval');
  assert.ok(outcome.approval_id.startsWith('support_reply_approval_'));
  const event = env.docs('support_conversation_events').find((e) => e.event_type === 'support.reply.pending_approval');
  assert.equal(event.payload.attachment_refs[0].file_collection, 'desktop_files');
  assert.equal(event.payload.attachment_refs[0].chunk_collection, 'desktop_file_chunks');
  assert.equal(event.payload.channel, 'mail');
  assert.equal(event.payload.thread_key, '');
  assert.equal(env.count('support_reporting_events'), 1);
  const rebuilt = await run(env, 'support.reporting.rebuild_rollups', { id: 'support-reporting-rebuild' });
  assert.equal(rebuilt.rebuilt, 1);
  assert.equal(env.count('support_reporting_rollups'), 1);
  assert.equal(env.capCalls.length, 0, 'no transport capability is used');
});

test('support.ticket.create_from_conversation sends ctox.ticket.local.create to the tickets app and links the ticket', async () => {
  const env = makeCtx();
  await opened(env, 'conv_1', { priority: 'high', search_text: 'Ö'.repeat(130) });
  const result = await run(env, 'support.ticket.create_from_conversation', { conversation_id: 'conv_1' }, { id: 'cmd_ticket_1' });
  assert.match(result.ticket_case_id, /^LT-[0-9a-f]{12}$/);
  const sent = env.commands.filter((c) => c.type === 'ctox.ticket.local.create');
  assert.equal(sent.length, 1);
  assert.deepEqual(sent[0].payload, {
    title: 'Ö'.repeat(120), body: 'Created from Business OS Support conversation `conv_1`.', status: 'open', priority: 'high',
    ticket_id: result.ticket_case_id,
  });
  assert.deepEqual(sent[0].opts, { id: 'cmd_ticket_1:ticket', module: 'tickets', record_id: 'conv_1' });
  assert.equal(env.doc('support_conversations', 'conv_1').ticket_case_id, result.ticket_case_id);
  const event = env.docs('support_conversation_events').find((e) => e.event_type === 'support.ticket.created');
  assert.equal(event.payload.ticket_outcome.ticket_key, result.ticket_case_id);
  assert.equal(event.payload.ticket_outcome.command_id, 'cmd_ticket_1:ticket');
  assert.equal(env.capCalls.length, 0, 'keine Core-Capability, insbesondere kein ctox.ticket.run');
});

test('ticket.create_from_conversation: deterministische Ticket-ID je Befehl, abgelehnter Befehl → unavailable ohne Verknüpfung', async () => {
  const a = makeCtx();
  await opened(a);
  const first = await run(a, 'support.ticket.create_from_conversation', { conversation_id: 'conv_1', title: 'Explicit' }, { id: 'same' });
  const b = makeCtx();
  await opened(b);
  const second = await run(b, 'support.ticket.create_from_conversation', { conversation_id: 'conv_1', title: 'Explicit' }, { id: 'same' });
  assert.equal(first.ticket_case_id, second.ticket_case_id);
  assert.equal(a.commands.at(-1).payload.title, 'Explicit');
  const rejected = makeCtx({ commandResult: { status: 'rejected', reason: 'forbidden' } });
  await opened(rejected);
  const error = await runError(rejected, 'support.ticket.create_from_conversation', { conversation_id: 'conv_1' });
  assert.equal(error.code, 'unavailable');
  assert.equal(error.message, 'ticket command rejected: forbidden');
  assert.equal(rejected.doc('support_conversations', 'conv_1').ticket_case_id, '');
});
