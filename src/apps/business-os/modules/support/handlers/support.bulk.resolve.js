// Origin: CTOX
// License: Apache-2.0
// support.bulk.resolve – ref: support.rs:1091-1108
import {
  commandConversationIds, createRun, ensureConversation, ensureResolveRequirements, markAppliedSlasResolved,
  updateConversationStatus, withStore,
} from './_shared.js';

export default async function handle(command, ctx) {
  const run = createRun(command, ctx);
  const payload = run.payload;
  const conversationIds = commandConversationIds(command);
  return withStore(ctx, run, async (store) => {
    for (const conversationId of conversationIds) {
      await ensureConversation(store, run, conversationId, payload);
      await ensureResolveRequirements(store, conversationId, payload);
      await updateConversationStatus(store, run, conversationId, 'resolved');
      await markAppliedSlasResolved(store, run, conversationId);
    }
    return run.result({ conversation_ids: conversationIds, status: 'resolved', updated: conversationIds.length });
  });
}
