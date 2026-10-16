import test from 'node:test';
import assert from 'node:assert/strict';
import { makeCtx, run, runError } from './_ctx.mjs';

async function opened(env, id = 'conv_macro_1', extra = {}) {
  return run(env, 'support.conversation.open_from_thread', { conversation_id: id, thread_key: `mail:${id}`, priority: 'normal', search_text: 'Macro case', ...extra });
}

test('Rust macro + automation test (support.rs:4734)', async () => {
  const env = makeCtx();
  await opened(env);
  await run(env, 'support.macro.upsert', {
    id: 'macro_1', title: 'Escalate and draft',
    actions: [
      { type: 'support.note.create', payload: { body: 'Macro note' } },
      { type: 'support.conversation.priority', payload: { priority: 'high' } },
      { type: 'support.reply.draft', payload: { body: 'We are checking this now.' } },
    ],
  });
  const ran = await run(env, 'support.macro.run', { conversation_id: 'conv_macro_1', macro_id: 'macro_1' });
  assert.deepEqual(ran.applied_actions.map((a) => a.type), ['support.note.create', 'support.conversation.priority', 'support.reply.draft']);
  assert.ok(ran.projections.some((p) => p.collection === 'support_macros' && p.record_id === 'macro_1'));
  await run(env, 'support.automation_rule.upsert', {
    id: 'rule_1', name: 'High priority waits', event_name: 'manual', active: true,
    conditions: [{ field: 'priority', operator: 'eq', value: 'high' }],
    actions: [{ type: 'support.conversation.status', payload: { status: 'waiting' } }],
  });
  const automation = await run(env, 'support.automation.evaluate', { conversation_id: 'conv_macro_1', event_name: 'manual' });
  assert.equal(automation.matched_rules.length, 1);
  const conv = env.doc('support_conversations', 'conv_macro_1');
  assert.equal(conv.priority, 'high');
  assert.equal(conv.status, 'waiting');
  assert.equal(env.count('support_notes'), 1);
  assert.equal(env.count('support_agent_suggestions'), 1);
  const note = env.docs('support_notes')[0];
  assert.equal(note.source, 'support.macro.run');
  const types = env.docs('support_conversation_events').map((e) => e.event_type);
  assert.ok(types.includes('support.macro.ran'));
  assert.ok(types.includes('support.automation.rule_matched'));
});

test('macro.run: short action names, flat payloads, unknown macro, unsupported action rolls back', async () => {
  const env = makeCtx();
  await opened(env);
  await run(env, 'support.macro.upsert', { id: 'm_flat', title: 'flat', actions: [
    { action: 'assign', team_id: 'tier2' },
    { type: 'snooze', until_ms: 500 },
    { type: 'ticket.link', case_id: 'C-1' },
  ] });
  const ran = await run(env, 'support.macro.run', { conversation_id: 'conv_macro_1', macro_id: 'm_flat' });
  assert.equal(ran.applied_actions[0].team_id, 'tier2');
  const conv = env.doc('support_conversations', 'conv_macro_1');
  assert.equal(conv.status, 'snoozed');
  assert.equal(conv.snoozed_until_ms, 500);
  assert.equal(conv.ticket_case_id, 'C-1');
  assert.equal(env.docs('support_assignment_events')[0].event_type, 'support.assignment.automated');
  const missing = await runError(env, 'support.macro.run', { conversation_id: 'conv_macro_1', macro_id: 'nope' });
  assert.equal(missing.code, 'not_found');
  assert.equal(missing.message, 'support macro `nope` does not exist');
  await run(env, 'support.macro.upsert', { id: 'm_bad', title: 'bad', actions: [{ type: 'priority', priority: 'low' }, { type: 'mail.send' }] });
  const bad = await runError(env, 'support.macro.run', { conversation_id: 'conv_macro_1', macro_id: 'm_bad' });
  assert.equal(bad.message, 'unsupported closed Support action `mail.send`');
  assert.equal(env.doc('support_conversations', 'conv_macro_1').priority, 'normal', 'earlier action rolled back (D1)');
  assert.equal((await runError(env, 'support.macro.run', { conversation_id: 'conv_macro_1' })).message, 'macro_id is required');
});

test('automation.evaluate: conditions see the pre-evaluation snapshot, any/all, operators, order', async () => {
  const env = makeCtx({ now: 10 });
  await opened(env, 'conv_a', { label_ids: ['vip'], custom_attributes: { plan: 'Gold' } });
  await run(env, 'support.automation_rule.upsert', { id: 'r_first', name: 'raise', event_name: 'inbound', actions: [{ type: 'priority', priority: 'urgent' }] });
  env.setNow(20);
  await run(env, 'support.automation_rule.upsert', {
    id: 'r_second', name: 'needs urgent', event_name: 'inbound',
    conditions: [{ field: 'priority', value: 'urgent' }],
    actions: [{ type: 'status', status: 'waiting' }],
  });
  await run(env, 'support.automation_rule.upsert', {
    id: 'r_any', name: 'vip or gold', event_name: 'inbound', query_operator: 'ANY',
    conditions: [{ field: 'label_ids', operator: 'contains', value: 'VIP' }, { field: 'plan', operator: 'eq', value: 'silver' }],
    actions: [],
  });
  await run(env, 'support.automation_rule.upsert', { id: 'r_off', name: 'off', event_name: 'inbound', active: false });
  const result = await run(env, 'support.automation.evaluate', { conversation_id: 'conv_a', event_name: 'inbound' });
  assert.deepEqual(result.matched_rules.map((m) => m.rule_id), ['r_first', 'r_any'], 'r_second sees snapshot priority=normal');
  assert.equal(env.doc('support_conversations', 'conv_a').priority, 'urgent');
  assert.equal(env.doc('support_conversations', 'conv_a').status, 'open');
  const none = await run(env, 'support.automation.evaluate', { conversation_id: 'conv_a' });
  assert.equal(none.event_name, 'manual');
  assert.deepEqual(none.matched_rules, []);
});

test('automation operators in/neq/empty/not_empty and unknown operator', async () => {
  const env = makeCtx();
  await opened(env, 'conv_b', { custom_attributes: { tier: '' } });
  const rules = [
    ['r_in', [{ field: 'status', operator: 'in', value: ['open', 'waiting'] }]],
    ['r_neq', [{ field: 'priority', operator: 'neq', value: 'high' }]],
    ['r_empty', [{ field: 'tier', operator: 'empty' }]],
    ['r_not_empty', [{ field: 'search_text', operator: 'not_empty' }]],
    ['r_unknown', [{ field: 'status', operator: 'regex', value: '.*' }]],
    ['r_nofield', [{ operator: 'eq', value: 'x' }]],
  ];
  for (const [id, conditions] of rules) {
    await run(env, 'support.automation_rule.upsert', { id, name: id, event_name: 'ops', conditions });
  }
  const result = await run(env, 'support.automation.evaluate', { conversation_id: 'conv_b', event_name: 'ops' });
  assert.deepEqual(result.matched_rules.map((m) => m.rule_id).sort(), ['r_empty', 'r_in', 'r_neq']);
});
