import test from 'node:test';
import assert from 'node:assert/strict';
import { makeCtx, run, runError } from './_ctx.mjs';
import { NAMESPACE_URL, uuidV5 } from '../_shared.js';

const NOW = 1_760_000_000_000;

test('Rust SLA test (support.rs:4849): breach, recalc, resolve gate, resolved status', async () => {
  const env = makeCtx({ now: NOW });
  const startedAt = NOW - 10_000;
  await run(env, 'support.conversation.open_from_thread', { conversation_id: 'conv_sla_1', thread_key: 'mail:sla-1', waiting_since_ms: startedAt, search_text: 'SLA case' });
  await run(env, 'support.sla_policy.upsert', { id: 'sla_1', name: 'Fast resolution', active: true, resolution_target_ms: 1 });
  const applied = await run(env, 'support.sla.apply', { conversation_id: 'conv_sla_1', policy_id: 'sla_1', started_at_ms: startedAt });
  assert.equal(applied.applied_sla_id, `support_applied_sla_${uuidV5(NAMESPACE_URL, 'conv_sla_1:sla_1')}`);
  let sla = env.doc('support_applied_slas', applied.applied_sla_id);
  assert.equal(sla.status, 'breached');
  assert.equal(sla.breached_at_ms, NOW);
  assert.equal(sla.resolution_due_at_ms, startedAt + 1);
  assert.equal(sla.first_response_due_at_ms, 0);
  assert.deepEqual(sla.payload, { started_at_ms: startedAt, source: 'support.sla.apply' });
  assert.deepEqual(env.docs('support_sla_events').map((e) => e.event_type).sort(), ['support.sla.applied', 'support.sla.breached']);
  env.setNow(NOW + 5);
  const recalc = await run(env, 'support.sla.recalculate', { conversation_id: 'conv_sla_1' });
  assert.equal(recalc.recalculated, 1);
  assert.equal(env.doc('support_applied_slas', applied.applied_sla_id).breached_at_ms, NOW, 'breach time is sticky');
  const blocked = await runError(env, 'support.conversation.resolve', { conversation_id: 'conv_sla_1', required_fields: ['ticket_case_id'] });
  assert.equal(blocked.code, 'conflict');
  await run(env, 'support.ticket.link', { conversation_id: 'conv_sla_1', ticket_case_id: 'ticket_1' });
  await run(env, 'support.conversation.resolve', { conversation_id: 'conv_sla_1', required_fields: ['ticket_case_id'] });
  sla = env.doc('support_applied_slas', applied.applied_sla_id);
  assert.equal(sla.status, 'resolved');
  assert.ok(sla.breached_at_ms > 0);
  assert.ok(env.count('support_sla_events') >= 2);
});

test('sla.apply resolves policy from inbox, then latest active; errors for missing/inactive', async () => {
  const env = makeCtx({ now: NOW, collections: {
    support_sla_policies: [
      { id: 'old', active: true, first_response_target_ms: 1000, updated_at_ms: 1 },
      { id: 'new', active: true, first_response_target_ms: 2000, updated_at_ms: 5 },
      { id: 'off', active: false, updated_at_ms: 9 },
    ],
    support_inboxes: [{ id: 'inbox_off', sla_policy_id: 'off' }],
  } });
  await run(env, 'support.conversation.status', { conversation_id: 'c1', status: 'open' });
  const latest = await run(env, 'support.sla.apply', { conversation_id: 'c1' });
  assert.equal(latest.policy_id, 'new');
  const sla = env.doc('support_applied_slas', latest.applied_sla_id);
  assert.equal(sla.first_response_due_at_ms, NOW + 2000, 'started_at = created_at without waiting_since');
  assert.equal(sla.status, 'active');
  const missing = await runError(env, 'support.sla.apply', { conversation_id: 'c1', policy_id: 'ghost' });
  assert.equal(missing.code, 'not_found');
  assert.equal(missing.message, 'support SLA policy `ghost` does not exist');
  const off = await runError(env, 'support.sla.apply', { conversation_id: 'c1', policy_id: 'off' });
  assert.equal(off.message, 'support SLA policy `off` is inactive');
  // open_from_thread with an inbox pointing to an inactive policy fails as in Rust
  const viaInbox = await runError(env, 'support.conversation.open_from_thread', { conversation_id: 'c2', inbox_id: 'inbox_off' });
  assert.equal(viaInbox.message, 'support SLA policy `off` is inactive');
  const empty = makeCtx();
  await run(empty, 'support.conversation.reopen', { conversation_id: 'c3' });
  assert.equal((await runError(empty, 'support.sla.apply', { conversation_id: 'c3' })).message, 'policy_id is required');
});

test('re-apply of an unbreached SLA after breach keeps breached_at 0 and emits no breach event (Rust quirk U7)', async () => {
  const env = makeCtx({ now: NOW, collections: { support_sla_policies: [{ id: 'p', active: true, resolution_target_ms: 1000, updated_at_ms: 1 }] } });
  await run(env, 'support.conversation.reopen', { conversation_id: 'c' });
  const first = await run(env, 'support.sla.apply', { conversation_id: 'c', policy_id: 'p', started_at_ms: NOW });
  assert.equal(env.doc('support_applied_slas', first.applied_sla_id).status, 'active');
  env.setNow(NOW + 5000);
  await run(env, 'support.sla.apply', { conversation_id: 'c', policy_id: 'p', started_at_ms: NOW });
  const sla = env.doc('support_applied_slas', first.applied_sla_id);
  assert.equal(sla.status, 'breached');
  assert.equal(sla.breached_at_ms, 0);
  assert.equal(env.docs('support_sla_events').filter((e) => e.event_type === 'support.sla.breached').length, 0);
  // recalculate then fills the breach time and emits the event
  await run(env, 'support.sla.recalculate', {});
  assert.equal(env.doc('support_applied_slas', first.applied_sla_id).breached_at_ms, NOW + 5000);
  assert.equal(env.docs('support_sla_events').filter((e) => e.event_type === 'support.sla.breached').length, 1);
});

test('sla.recalculate: all rows without id, none for record_id ""', async () => {
  const env = makeCtx({ now: NOW, collections: { support_applied_slas: [
    { id: 'a1', conversation_id: 'x', status: 'active', first_response_due_at_ms: NOW - 1, next_response_due_at_ms: 0, resolution_due_at_ms: 0, breached_at_ms: 0 },
    { id: 'a2', conversation_id: 'y', status: 'resolved', first_response_due_at_ms: 0, next_response_due_at_ms: 0, resolution_due_at_ms: NOW + 100, breached_at_ms: 0 },
  ] } });
  const viaRecord = await run(env, 'support.sla.recalculate', {}, { record_id: '' });
  assert.equal(viaRecord.recalculated, 0);
  assert.equal(viaRecord.conversation_id, '');
  const all = await run(env, 'support.sla.recalculate', {});
  assert.equal(all.recalculated, 2);
  assert.equal(env.doc('support_applied_slas', 'a1').status, 'breached');
  assert.equal(env.doc('support_applied_slas', 'a2').status, 'resolved');
});

test('support.reporting.rebuild_rollups aggregates per day/event/metric and keeps created_at', async () => {
  const day = 86_400_000;
  const env = makeCtx({ now: 50 * day, collections: { support_reporting_events: [
    { id: 'e1', event_name: 'support.conversation.status_changed', metric_name: 'count', value_ms: 1, occurred_at_ms: 10 * day + 5 },
    { id: 'e2', event_name: 'support.conversation.status_changed', metric_name: 'count', value_ms: 1, occurred_at_ms: 10 * day + 900 },
    { id: 'e3', event_name: 'support.conversation.status_changed', metric_name: 'count', value_ms: 1, occurred_at_ms: 11 * day },
    { id: 'e4', event_name: 'support.reply.pending_approval', metric_name: 'count', value_ms: 1, occurred_at_ms: 1 * day + 7 },
  ] } });
  const result = await run(env, 'support.reporting.rebuild_rollups', {});
  assert.equal(result.rebuilt, 3);
  const id = `support_rollup_${uuidV5(NAMESPACE_URL, `support:support.reply.pending_approval:count:${day}`)}`;
  assert.equal(id, 'support_rollup_f937252c-4fa7-5b7a-9909-aad0955888ed');
  const rollup = env.doc('support_reporting_rollups', id);
  assert.equal(rollup.count, 1);
  assert.equal(rollup.bucket_end_ms, 2 * day);
  assert.deepEqual(rollup.dimensions, { event_name: 'support.reply.pending_approval' });
  const tenth = env.docs('support_reporting_rollups').find((r) => r.bucket_start_ms === 10 * day);
  assert.equal(tenth.count, 2);
  assert.equal(tenth.value, 2);
  env.setNow(60 * day);
  await run(env, 'support.reporting.rebuild_rollups', {});
  assert.equal(env.doc('support_reporting_rollups', id).created_at_ms, 50 * day);
  assert.equal(env.doc('support_reporting_rollups', id).updated_at_ms, 60 * day);
});
