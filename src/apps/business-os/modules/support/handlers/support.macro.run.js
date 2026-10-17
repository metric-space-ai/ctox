// Origin: CTOX
// License: Apache-2.0
// support.macro.run – ref: support.rs:1123-1155, 2147-2157, 2159-2419
import {
  applySupportActions, commandConversationId, createRun, ensureConversation, fail, firstString,
  insertConversationEvent, withStore,
} from './_shared.js';

export default async function handle(command, ctx) {
  const run = createRun(command, ctx);
  const payload = run.payload;
  const conversationId = commandConversationId(command);
  return withStore(ctx, run, async (store) => {
    await ensureConversation(store, run, conversationId, payload);
    const macroId = firstString(payload, ['macro_id', 'id']);
    if (macroId === undefined) fail('invalid', 'macro_id is required');
    const macro = await store.get('support_macros', macroId);
    if (!macro) fail('not_found', `support macro \`${macroId}\` does not exist`);
    const actions = macro.actions_json === undefined ? [] : macro.actions_json;
    const applied = await applySupportActions(store, run, conversationId, actions, 'support.macro.run');
    run.touch('support_macros', macroId); // Rust: project_macro (unveränderte Neuprojektion)
    await insertConversationEvent(store, run, {
      conversation_id: conversationId,
      event_type: 'support.macro.ran',
      summary: 'Support macro applied.',
      payload: { macro_id: macroId, applied_actions: applied },
    });
    return run.result({ conversation_id: conversationId, macro_id: macroId, applied_actions: applied });
  });
}
