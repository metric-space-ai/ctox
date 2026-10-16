import test from 'node:test';
import assert from 'node:assert/strict';
import { makeCtx, run, runError } from './_ctx.mjs';

test('support.inbox.upsert applies defaults and keeps created_at on update', async () => {
  const env = makeCtx({ now: 100 });
  const { inbox_id: id } = await run(env, 'support.inbox.upsert', { channel_filters: { channel: 'mail' }, is_default: 'yes' });
  assert.ok(id.startsWith('support_inbox_'));
  let inbox = env.doc('support_inboxes', id);
  assert.equal(inbox.name, 'Support Inbox');
  assert.equal(inbox.sort_key, 'Support Inbox');
  assert.equal(inbox.status, 'active');
  assert.equal(inbox.is_default, true);
  assert.deepEqual(inbox.channel_filters_json, { channel: 'mail' });
  assert.deepEqual(inbox.policy_json, {});
  env.setNow(200);
  await run(env, 'support.inbox.upsert', { id, name: 'Billing', policy: { resolve_required_fields: ['ticket_case_id'] } });
  inbox = env.doc('support_inboxes', id);
  assert.equal(inbox.name, 'Billing');
  assert.equal(inbox.created_at_ms, 100);
  assert.equal(inbox.updated_at_ms, 200);
  assert.equal(inbox.is_default, false, 'upsert replaces, not merges');
});

test('support.view.upsert and support.view_filter.upsert (support.rs:5056)', async () => {
  const env = makeCtx();
  assert.equal((await runError(env, 'support.view.upsert', { id: 'v' })).message, 'support view title is required');
  await run(env, 'support.view.upsert', { id: 'view_urgent', title: 'Urgent open', scope: 'team', filters: { priority: 'urgent', status: 'open' } });
  const view = env.doc('support_views', 'view_urgent');
  assert.equal(view.owner_id, 'support-admin');
  assert.deepEqual(view.filters_json, { priority: 'urgent', status: 'open' });
  assert.deepEqual(view.sorts_json, {});
  assert.equal((await runError(env, 'support.view_filter.upsert', { id: 'f', field: 'x' })).message, 'view_id is required');
  assert.equal((await runError(env, 'support.view_filter.upsert', { id: 'f', view_id: 'v' })).message, 'filter field is required');
  await run(env, 'support.view_filter.upsert', { id: 'view_filter_urgent', view_id: 'view_urgent', field: 'priority', value: 'urgent' });
  const filter = env.doc('support_view_filters', 'view_filter_urgent');
  assert.equal(filter.operator, 'eq');
  assert.equal(filter.value, 'urgent');
  await run(env, 'support.view_filter.upsert', { id: 'f2', view_id: 'view_urgent', field: 'labels', value: null });
  assert.equal(env.doc('support_view_filters', 'f2').value, null);
});

test('support.assignment_policy.upsert stores strategy and limits', async () => {
  const env = makeCtx();
  const { policy_id: id } = await run(env, 'support.assignment_policy.upsert', { strategy: 'round_robin', fair_distribution_limit: 5, payload: { members: ['a'] } });
  const p = env.doc('support_assignment_policies', id);
  assert.equal(p.name, 'Support assignment policy');
  assert.equal(p.strategy, 'round_robin');
  assert.equal(p.fair_distribution_limit, 5);
  assert.equal(p.fair_distribution_window_ms, 0);
  assert.deepEqual(p.payload, { members: ['a'] });
});

test('support.macro.upsert validates title and action array', async () => {
  const env = makeCtx();
  assert.equal((await runError(env, 'support.macro.upsert', { actions: [] })).message, 'macro title is required');
  assert.equal((await runError(env, 'support.macro.upsert', { title: 'm', actions: { a: 1 } })).message, 'macro actions must be an array');
  await run(env, 'support.macro.upsert', { id: 'm1', name: 'Escalate', actions_json: [{ type: 'priority', priority: 'high' }] }, { actor: { id: 'u7', role: 'chef' } });
  const m = env.doc('support_macros', 'm1');
  assert.equal(m.title, 'Escalate');
  assert.equal(m.visibility, 'team');
  assert.equal(m.owner_id, 'u7');
  assert.deepEqual(m.actions_json, [{ type: 'priority', priority: 'high' }]);
});

test('support.automation_rule.upsert validates and defaults', async () => {
  const env = makeCtx();
  assert.equal((await runError(env, 'support.automation_rule.upsert', { event_name: 'manual' })).message, 'automation rule name is required');
  assert.equal((await runError(env, 'support.automation_rule.upsert', { name: 'r' })).message, 'event_name is required');
  assert.equal((await runError(env, 'support.automation_rule.upsert', { name: 'r', event_name: 'e', conditions: 'x' })).message, 'automation conditions must be an array');
  await run(env, 'support.automation_rule.upsert', { id: 'r1', name: 'r', event_name: 'manual', active: 'off' });
  const r = env.doc('support_automation_rules', 'r1');
  assert.equal(r.active, false);
  assert.equal(r.query_operator, 'all');
  assert.deepEqual(r.conditions_json, []);
  assert.deepEqual(r.actions_json, []);
});

test('support.sla_policy.upsert stores integer targets only', async () => {
  const env = makeCtx();
  assert.equal((await runError(env, 'support.sla_policy.upsert', {})).message, 'SLA policy name is required');
  await run(env, 'support.sla_policy.upsert', { id: 's1', name: 'Fast', first_response_target_ms: 60000, resolution_target_ms: 1.5, business_hours: { tz: 'Europe/Berlin' } });
  const s = env.doc('support_sla_policies', 's1');
  assert.equal(s.active, true);
  assert.equal(s.first_response_target_ms, 60000);
  assert.equal(s.resolution_target_ms, 0);
  assert.deepEqual(s.business_hours_json, { tz: 'Europe/Berlin' });
});
