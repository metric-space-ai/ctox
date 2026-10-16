// Origin: CTOX
// License: Apache-2.0
// support.view_filter.upsert – ref: support.rs:968-974, 1925-1961, 3473-3508
import { createRun, fail, firstString, jsonField, numberField, upsertWithCreatedAt, withStore } from './_shared.js';

export default async function handle(command, ctx) {
  const run = createRun(command, ctx);
  const payload = run.payload;
  const filterId = firstString(payload, ['id', 'filter_id']) ?? run.newId('support_view_filter_');
  return withStore(ctx, run, async (store) => {
    const viewId = firstString(payload, ['view_id']);
    if (viewId === undefined) fail('invalid', 'view_id is required');
    const field = firstString(payload, ['field']);
    if (field === undefined) fail('invalid', 'filter field is required');
    await upsertWithCreatedAt(store, run, 'support_view_filters', filterId, {
      view_id: viewId,
      field,
      operator: firstString(payload, ['operator']) ?? 'eq',
      value: jsonField(payload, ['value']) ?? null,
      position: numberField(payload, ['position']) ?? 0,
    });
    return run.result({ filter_id: filterId });
  });
}
