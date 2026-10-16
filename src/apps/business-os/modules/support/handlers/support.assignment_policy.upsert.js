// Origin: CTOX
// License: Apache-2.0
// support.assignment_policy.upsert – ref: support.rs:1109-1115, 1698-1739, 3778-3813
// Rust-Treue: die Strategie wird nur gespeichert; keine Zuweisung liest sie (§3 U10).
import { createRun, firstString, jsonField, numberField, upsertWithCreatedAt, withStore } from './_shared.js';

export default async function handle(command, ctx) {
  const run = createRun(command, ctx);
  const payload = run.payload;
  const policyId = firstString(payload, ['id', 'policy_id']) ?? run.newId('support_assignment_policy_');
  return withStore(ctx, run, async (store) => {
    await upsertWithCreatedAt(store, run, 'support_assignment_policies', policyId, {
      name: firstString(payload, ['name', 'title']) ?? 'Support assignment policy',
      strategy: firstString(payload, ['strategy']) ?? 'manual',
      fair_distribution_limit: numberField(payload, ['fair_distribution_limit']) ?? 0,
      fair_distribution_window_ms: numberField(payload, ['fair_distribution_window_ms']) ?? 0,
      payload: jsonField(payload, ['payload']) ?? {},
    });
    return run.result({ policy_id: policyId });
  });
}
