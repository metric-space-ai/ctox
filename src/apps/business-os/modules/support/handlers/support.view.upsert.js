// Origin: CTOX
// License: Apache-2.0
// support.view.upsert – ref: support.rs:961-967, 1880-1923, 3440-3471
import { createRun, fail, firstString, jsonField, numberField, upsertWithCreatedAt, withStore } from './_shared.js';

export default async function handle(command, ctx) {
  const run = createRun(command, ctx);
  const payload = run.payload;
  const viewId = firstString(payload, ['id', 'view_id']) ?? run.newId('support_view_');
  return withStore(ctx, run, async (store) => {
    const title = firstString(payload, ['title', 'name']);
    if (title === undefined) fail('invalid', 'support view title is required');
    await upsertWithCreatedAt(store, run, 'support_views', viewId, {
      title,
      owner_id: firstString(payload, ['owner_id']) ?? run.actorId,
      scope: firstString(payload, ['scope']) ?? 'personal',
      position: numberField(payload, ['position']) ?? 0,
      filters_json: jsonField(payload, ['filters_json', 'filters']) ?? {},
      sorts_json: jsonField(payload, ['sorts_json', 'sorts']) ?? {},
    });
    return run.result({ view_id: viewId });
  });
}
