// Origin: CTOX
// License: Apache-2.0
// support.macro.upsert – ref: support.rs:1116-1122, 1741-1783, 3815-3845
import { createRun, fail, firstString, jsonField, upsertWithCreatedAt, withStore } from './_shared.js';

export default async function handle(command, ctx) {
  const run = createRun(command, ctx);
  const payload = run.payload;
  const macroId = firstString(payload, ['id', 'macro_id']) ?? run.newId('support_macro_');
  return withStore(ctx, run, async (store) => {
    const title = firstString(payload, ['title', 'name']);
    if (title === undefined) fail('invalid', 'macro title is required');
    const actions = jsonField(payload, ['actions_json', 'actions']) ?? [];
    if (!Array.isArray(actions)) fail('invalid', 'macro actions must be an array');
    await upsertWithCreatedAt(store, run, 'support_macros', macroId, {
      title,
      visibility: firstString(payload, ['visibility']) ?? 'team',
      owner_id: firstString(payload, ['owner_id']) ?? run.actorId,
      actions_json: actions,
      payload: jsonField(payload, ['payload']) ?? {},
    });
    return run.result({ macro_id: macroId });
  });
}
