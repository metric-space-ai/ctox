import test from 'node:test';
import assert from 'node:assert/strict';
import { makeCtx, run, runError } from './_ctx.mjs';
import { NAMESPACE_URL, uuidV5 } from '../_shared.js';

const NOW = 1_760_000_000_000;

async function opened(env, id = 'conv_1', extra = {}) {
  return run(env, 'support.conversation.open_from_thread', {
    conversation_id: id, thread_key: `mail:${id}`, channel: 'mail', search_text: `Case ${id}`, ...extra,
  });
}

test('support.conversation.open_from_thread creates conversation, primary link and opened event', async () => {
  const env = makeCtx({ now: NOW });
  const result = await opened(env, 'conv_support_1', { status: 'open', priority: 'normal', search_text: 'Printer is offline', account_key: 'mail:inbox' });
  assert.equal(result.ok, true);
  assert.equal(result.command_type, 'support.conversation.open_from_thread');
  assert.equal(result.status, 'open');
  const conv = env.doc('support_conversations', 'conv_support_1');
  assert.deepEqual(Object.keys(conv), [
    'id', 'is_deleted', 'created_at_ms', 'updated_at_ms', 'inbox_id', 'primary_thread_key', 'status', 'priority',
    'assignee_id', 'team_id', 'customer_account_id', 'customer_contact_id', 'ticket_case_id', 'last_message_key',
    'last_activity_at_ms', 'waiting_since_ms', 'snoozed_until_ms', 'unread_count', 'label_ids', 'custom_attributes',
    'search_text',
  ]);
  assert.equal(conv.primary_thread_key, 'mail:conv_support_1');
  assert.equal(conv.search_text, 'Printer is offline');
  assert.equal(conv.created_at_ms, NOW);
  const [link] = env.docs('support_thread_links');
  assert.equal(link.id, `support_thread_link_${uuidV5(NAMESPACE_URL, 'mail:conv_support_1')}`);
  assert.equal(link.thread_key, 'mail:conv_support_1');
  assert.equal(link.channel, 'mail');
  assert.equal(link.account_key, 'mail:inbox');
  assert.equal(link.link_role, 'primary');
  const [event] = env.docs('support_conversation_events');
  assert.equal(event.event_type, 'support.conversation.opened');
  assert.equal(event.actor_id, 'support-admin');
  assert.equal(event.payload.payload.search_text, 'Printer is offline');
  assert.ok(result.projections.some((p) => p.collection === 'support_thread_links'));
});

test('open_from_thread without conversation_id is rejected like Rust (UI import path, U1)', async () => {
  const env = makeCtx();
  const error = await runError(env, 'support.conversation.open_from_thread', { thread_key: 'mail:x', inbox_id: 'inbox_1' });
  assert.equal(error.code, 'invalid');
  assert.equal(error.message, 'conversation_id is required');
  assert.equal(env.count('support_conversations'), 0);
});

test('open_from_thread applies the inbox SLA policy and keeps created_at on re-open', async () => {
  const env = makeCtx({ now: NOW, collections: {
    support_inboxes: [{ id: 'inbox_1', sla_policy_id: 'sla_1', updated_at_ms: 1 }],
    support_sla_policies: [{ id: 'sla_1', active: true, first_response_target_ms: 60_000, next_response_target_ms: 0, resolution_target_ms: 0, updated_at_ms: 1 }],
  } });
  await opened(env, 'conv_1', { inbox_id: 'inbox_1', waiting_since_ms: NOW - 1000 });
  const [sla] = env.docs('support_applied_slas');
  assert.equal(sla.policy_id, 'sla_1');
  assert.equal(sla.status, 'active');
  assert.equal(sla.first_response_due_at_ms, NOW - 1000 + 60_000);
  assert.equal(env.docs('support_sla_events')[0].event_type, 'support.sla.applied');
  env.setNow(NOW + 5);
  await opened(env, 'conv_1', { search_text: 'renamed' });
  const conv = env.doc('support_conversations', 'conv_1');
  assert.equal(conv.created_at_ms, NOW);
  assert.equal(conv.updated_at_ms, NOW + 5);
  assert.equal(conv.inbox_id, 'inbox_1', 'missing payload fields keep existing values');
  assert.equal(conv.search_text, 'renamed');
});

test('support.conversation.claim assigns actor, rejects competing claim, honours force', async () => {
  const env = makeCtx();
  await opened(env, 'conv_claim_1');
  const first = await run(env, 'support.conversation.claim', { conversation_id: 'conv_claim_1' }, { actor: { id: 'support-agent-1', role: 'user' } });
  assert.equal(first.assignee_id, 'support-agent-1');
  const error = await runError(env, 'support.conversation.claim', { conversation_id: 'conv_claim_1' }, { actor: { id: 'support-agent-2', role: 'user' } });
  assert.equal(error.code, 'conflict');
  assert.equal(error.message, 'support conversation `conv_claim_1` is already assigned to `support-agent-1`');
  assert.equal(env.doc('support_conversations', 'conv_claim_1').assignee_id, 'support-agent-1');
  assert.equal(env.count('support_assignment_events'), 1, 'failed claim leaves no event (transaction)');
  await run(env, 'support.conversation.claim', { conversation_id: 'conv_claim_1', force: 'true' }, { actor: { id: 'support-agent-2', role: 'user' } });
  assert.equal(env.doc('support_conversations', 'conv_claim_1').assignee_id, 'support-agent-2');
  const claimed = env.docs('support_assignment_events').find((e) => e.previous_assignee_id === 'support-agent-1');
  assert.equal(claimed.event_type, 'support.assignment.claimed');
  assert.deepEqual(claimed.payload.force, true);
});

test('claim preserves an existing waiting status', async () => {
  const env = makeCtx();
  await opened(env, 'conv_wait', { status: 'waiting' });
  await run(env, 'support.conversation.claim', { conversation_id: 'conv_wait' });
  const conv = env.doc('support_conversations', 'conv_wait');
  assert.equal(conv.status, 'waiting');
  assert.equal(conv.assignee_id, 'support-admin');
});

test('support.conversation.assign requires assignee or team and writes both events', async () => {
  const env = makeCtx();
  await opened(env);
  assert.equal((await runError(env, 'support.conversation.assign', { conversation_id: 'conv_1' })).message, 'assignee_id or team_id is required');
  const result = await run(env, 'support.conversation.assign', { conversation_id: 'conv_1', team_id: 'support-team' });
  assert.equal(result.team_id, 'support-team');
  assert.equal(result.assignee_id, '');
  const [assignment] = env.docs('support_assignment_events');
  assert.equal(assignment.event_type, 'support.assignment.assigned');
  assert.deepEqual(assignment.payload, { team_id: 'support-team', source_command_id: assignment.payload.source_command_id });
  assert.ok(env.docs('support_conversation_events').some((e) => e.event_type === 'support.conversation.assigned'));
});

test('support.conversation.assign on an unknown id creates it from the payload (ensure_conversation)', async () => {
  const env = makeCtx({ now: NOW });
  await run(env, 'support.conversation.assign', { conversation_id: 'conv_new', assignee_id: 'u1' });
  const conv = env.doc('support_conversations', 'conv_new');
  assert.equal(conv.assignee_id, 'u1');
  assert.equal(conv.primary_thread_key, 'conv_new');
  assert.equal(conv.search_text, 'conv_new');
});

test('support.conversation.status normalizes and records status + reporting events', async () => {
  const env = makeCtx();
  await opened(env);
  const result = await run(env, 'support.conversation.status', { conversation_id: 'conv_1', status: 'Pending' });
  assert.equal(result.status, 'waiting');
  assert.equal(env.doc('support_conversations', 'conv_1').status, 'waiting');
  assert.equal(env.docs('support_reporting_events')[0].event_name, 'support.conversation.status_changed');
  assert.equal((await runError(env, 'support.conversation.status', { conversation_id: 'conv_1' })).code, 'invalid');
});

test('support.conversation.priority normalizes and records event', async () => {
  const env = makeCtx();
  await opened(env);
  const result = await run(env, 'support.conversation.priority', { conversation_id: 'conv_1', priority: 'critical' });
  assert.equal(result.priority, 'urgent');
  assert.equal(env.doc('support_conversations', 'conv_1').priority, 'urgent');
  assert.ok(env.docs('support_conversation_events').some((e) => e.event_type === 'support.conversation.priority_changed'));
});

test('support.conversation.snooze sets status and until; reopen clears the snooze', async () => {
  const env = makeCtx();
  await opened(env);
  assert.equal((await runError(env, 'support.conversation.snooze', { conversation_id: 'conv_1', snoozed_until_ms: 1.5 })).message, 'snoozed_until_ms is required');
  await run(env, 'support.conversation.snooze', { conversation_id: 'conv_1', until_ms: NOW + 3600 });
  let conv = env.doc('support_conversations', 'conv_1');
  assert.equal(conv.status, 'snoozed');
  assert.equal(conv.snoozed_until_ms, NOW + 3600);
  await run(env, 'support.conversation.status', { conversation_id: 'conv_1', status: 'waiting' });
  assert.equal(env.doc('support_conversations', 'conv_1').snoozed_until_ms, NOW + 3600, 'non-open status keeps snooze');
  const reopened = await run(env, 'support.conversation.reopen', { conversation_id: 'conv_1' });
  assert.equal(reopened.status, 'open');
  conv = env.doc('support_conversations', 'conv_1');
  assert.equal(conv.status, 'open');
  assert.equal(conv.snoozed_until_ms, 0);
});

test('support.conversation.resolve enforces payload, custom and inbox required fields', async () => {
  const env = makeCtx({ collections: { support_inboxes: [{ id: 'inbox_1', policy_json: { resolve_required_fields: ['customer_account_id'] } }] } });
  await opened(env, 'conv_1', { inbox_id: 'inbox_1', custom_attributes: { resolve_required_fields: ['root_cause'], root_cause: '' } });
  const error = await runError(env, 'support.conversation.resolve', { conversation_id: 'conv_1', required_fields: ['ticket_case_id'] });
  assert.equal(error.code, 'conflict');
  assert.equal(error.message, 'cannot resolve support conversation; missing required fields: customer_account_id, root_cause, ticket_case_id');
  assert.equal(env.doc('support_conversations', 'conv_1').status, 'open');
});

test('support.conversation.resolve marks applied SLAs resolved', async () => {
  const env = makeCtx({ collections: { support_sla_policies: [{ id: 'sla_1', active: true, resolution_target_ms: 10_000, updated_at_ms: 1 }] } });
  await opened(env);
  const result = await run(env, 'support.conversation.resolve', { conversation_id: 'conv_1' });
  assert.equal(result.status, 'resolved');
  assert.equal(env.docs('support_applied_slas')[0].status, 'resolved');
  assert.ok(env.docs('support_sla_events').some((e) => e.event_type === 'support.sla.resolved'));
});

test('support.note.create stores note, bumps activity, rejects empty body atomically', async () => {
  const env = makeCtx({ now: NOW });
  await opened(env);
  env.setNow(NOW + 10);
  const result = await run(env, 'support.note.create', { conversation_id: 'conv_1', body: ' Asked customer for logs. ', visibility: 'customer' });
  const note = env.doc('support_notes', result.note_id);
  assert.equal(note.body, 'Asked customer for logs.');
  assert.equal(note.visibility, 'public');
  assert.equal(note.author_id, 'support-admin');
  assert.equal(note.source, 'business-os.support');
  assert.ok(result.note_id.startsWith('support_note_'));
  assert.equal(env.doc('support_conversations', 'conv_1').last_activity_at_ms, NOW + 10);
  const error = await runError(env, 'support.note.create', { conversation_id: 'conv_missing', body: '' });
  assert.equal(error.message, 'body is required');
  assert.equal(env.doc('support_conversations', 'conv_missing'), null, 'rollback: no implicit conversation (D1)');
});

test('support.note.create upsert keeps author and created_at (ON CONFLICT)', async () => {
  const env = makeCtx({ now: NOW });
  await opened(env);
  await run(env, 'support.note.create', { conversation_id: 'conv_1', note_id: 'n1', body: 'a' }, { actor: { id: 'u1', role: 'user' } });
  env.setNow(NOW + 1);
  await run(env, 'support.note.create', { conversation_id: 'conv_1', note_id: 'n1', body: 'b' }, { actor: { id: 'u2', role: 'user' } });
  const note = env.doc('support_notes', 'n1');
  assert.equal(note.body, 'b');
  assert.equal(note.author_id, 'u1');
  assert.equal(note.created_at_ms, NOW);
});

test('support.ticket.link sets ticket_case_id from any alias', async () => {
  const env = makeCtx();
  await opened(env);
  const result = await run(env, 'support.ticket.link', { conversation_id: 'conv_1', ticket_key: 'T-7' });
  assert.equal(result.ticket_case_id, 'T-7');
  assert.equal(env.doc('support_conversations', 'conv_1').ticket_case_id, 'T-7');
  assert.equal((await runError(env, 'support.ticket.link', { conversation_id: 'conv_1' })).message, 'ticket_case_id is required');
});

test('support.identity.link stores normalized identity and links the customer', async () => {
  const env = makeCtx();
  await opened(env);
  const result = await run(env, 'support.identity.link', {
    conversation_id: 'conv_1', channel: 'mail', external_identity: ' Customer@Example.com ',
    customer_account_id: 'acct_1', customer_contact_id: 'contact_1', confidence: 0.9,
  });
  const link = env.doc('support_identity_links', result.identity_link_id);
  assert.equal(link.external_identity, 'Customer@Example.com');
  assert.equal(link.normalized_identity, 'customer@example.com');
  assert.equal(link.confidence, 1, 'Rust number_field drops 0.9 → default 1 (U6)');
  assert.equal(link.payload.channel, 'mail');
  const conv = env.doc('support_conversations', 'conv_1');
  assert.equal(conv.customer_account_id, 'acct_1');
  assert.equal(conv.customer_contact_id, 'contact_1');
  const only = await run(env, 'support.identity.link', { email: 'x@y.z' });
  assert.equal(only.conversation_id, '');
  assert.equal(env.count('support_identity_links'), 2);
});

test('Rust core workflow test (support.rs:4402) passes end-to-end', async () => {
  const env = makeCtx();
  await opened(env, 'conv_support_1', { status: 'open', priority: 'normal', search_text: 'Printer is offline' });
  await run(env, 'support.conversation.claim', {}, { record_id: 'conv_support_1' });
  await run(env, 'support.conversation.assign', { assignee_id: 'support-admin', team_id: 'support-team' }, { record_id: 'conv_support_1' });
  await run(env, 'support.conversation.status', { status: 'waiting' }, { record_id: 'conv_support_1' });
  await run(env, 'support.note.create', { body: 'Asked customer for logs.', visibility: 'internal' }, { record_id: 'conv_support_1' });
  await run(env, 'support.identity.link', { conversation_id: 'conv_support_1', channel: 'mail', external_identity: 'customer@example.com', customer_account_id: 'acct_1', customer_contact_id: 'contact_1' });
  await run(env, 'support.conversation.resolve', {}, { record_id: 'conv_support_1' });
  const conv = env.doc('support_conversations', 'conv_support_1');
  assert.equal(conv.status, 'resolved');
  assert.equal(conv.assignee_id, 'support-admin');
  assert.equal(conv.customer_contact_id, 'contact_1');
  assert.equal(env.count('support_notes'), 1);
  assert.equal(env.count('support_identity_links'), 1);
  assert.ok(env.count('support_assignment_events') >= 2);
});

test('event ids are deterministic per command id (retry-idempotent, D2)', async () => {
  const a = makeCtx();
  const b = makeCtx();
  for (const env of [a, b]) {
    await opened(env, 'conv_1');
    await run(env, 'support.note.create', { conversation_id: 'conv_1', body: 'x' }, { id: 'cmd_fixed' });
  }
  assert.deepEqual(a.docs('support_notes').map((n) => n.id), b.docs('support_notes').map((n) => n.id));
});

test('missing command id is rejected', async () => {
  const env = makeCtx();
  const { default: handle } = await import('../support.conversation.reopen.js');
  await assert.rejects(handle({ type: 'support.conversation.reopen', payload: { conversation_id: 'c' } }, env.ctx), { code: 'invalid', message: 'command id is required' });
});
