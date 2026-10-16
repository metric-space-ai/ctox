// Origin: CTOX
// License: Apache-2.0
// support.sla_policy.upsert – ref: support.rs:1182-1188, 1836-1878, 3886-3925
// Rust-Treue: business_hours wird gespeichert, aber bei der Fristberechnung nicht beachtet (§3 U10).
import { boolField, createRun, fail, firstString, jsonField, numberField, upsertWithCreatedAt, withStore } from './_shared.js';

export default async function handle(command, ctx) {
  const run = createRun(command, ctx);
  const payload = run.payload;
  const policyId = firstString(payload, ['id', 'policy_id']) ?? run.newId('support_sla_policy_');
  return withStore(ctx, run, async (store) => {
    const name = firstString(payload, ['name', 'title']);
    if (name === undefined) fail('invalid', 'SLA policy name is required');
    await upsertWithCreatedAt(store, run, 'support_sla_policies', policyId, {
      name,
      active: boolField(payload, ['active']) ?? true,
      first_response_target_ms: numberField(payload, ['first_response_target_ms']) ?? 0,
      next_response_target_ms: numberField(payload, ['next_response_target_ms']) ?? 0,
      resolution_target_ms: numberField(payload, ['resolution_target_ms']) ?? 0,
      business_hours_json: jsonField(payload, ['business_hours_json', 'business_hours']) ?? {},
      payload: jsonField(payload, ['payload']) ?? {},
    });
    return run.result({ policy_id: policyId });
  });
}
