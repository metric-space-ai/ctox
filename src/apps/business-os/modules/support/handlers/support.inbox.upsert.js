// Origin: CTOX
// License: Apache-2.0
// support.inbox.upsert – ref: support.rs:435-441, 1640-1696, 3402-3438
import { boolField, createRun, firstString, jsonField, upsertWithCreatedAt, withStore } from './_shared.js';

export default async function handle(command, ctx) {
  const run = createRun(command, ctx);
  const payload = run.payload;
  const inboxId = firstString(payload, ['id', 'inbox_id']) ?? run.newId('support_inbox_');
  return withStore(ctx, run, async (store) => {
    const name = firstString(payload, ['name', 'title']) ?? 'Support Inbox';
    await upsertWithCreatedAt(store, run, 'support_inboxes', inboxId, {
      name,
      description: firstString(payload, ['description']) ?? '',
      status: firstString(payload, ['status']) ?? 'active',
      channel_filters_json: jsonField(payload, ['channel_filters_json', 'channel_filters']) ?? {},
      team_id: firstString(payload, ['team_id']) ?? '',
      assignment_policy_id: firstString(payload, ['assignment_policy_id']) ?? '',
      sla_policy_id: firstString(payload, ['sla_policy_id']) ?? '',
      policy_json: jsonField(payload, ['policy_json', 'policy']) ?? {},
      is_default: boolField(payload, ['is_default']) ?? false,
      sort_key: firstString(payload, ['sort_key']) ?? name,
    });
    return run.result({ inbox_id: inboxId });
  });
}
