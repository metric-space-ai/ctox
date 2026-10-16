// Origin: CTOX
// License: Apache-2.0
// support.automation_rule.upsert – ref: support.rs:1156-1162, 1785-1834, 3847-3884
import { boolField, createRun, fail, firstString, jsonField, upsertWithCreatedAt, withStore } from './_shared.js';

export default async function handle(command, ctx) {
  const run = createRun(command, ctx);
  const payload = run.payload;
  const ruleId = firstString(payload, ['id', 'rule_id']) ?? run.newId('support_automation_rule_');
  return withStore(ctx, run, async (store) => {
    const name = firstString(payload, ['name', 'title']);
    if (name === undefined) fail('invalid', 'automation rule name is required');
    const eventName = firstString(payload, ['event_name']);
    if (eventName === undefined) fail('invalid', 'event_name is required');
    const conditions = jsonField(payload, ['conditions_json', 'conditions']) ?? [];
    const actions = jsonField(payload, ['actions_json', 'actions']) ?? [];
    if (!Array.isArray(conditions)) fail('invalid', 'automation conditions must be an array');
    if (!Array.isArray(actions)) fail('invalid', 'automation actions must be an array');
    await upsertWithCreatedAt(store, run, 'support_automation_rules', ruleId, {
      name,
      event_name: eventName,
      active: boolField(payload, ['active']) ?? true,
      query_operator: firstString(payload, ['query_operator']) ?? 'all',
      conditions_json: conditions,
      actions_json: actions,
    });
    return run.result({ rule_id: ruleId });
  });
}
