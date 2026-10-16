// Origin: CTOX
// License: Apache-2.0
// support.conversation.resolve – ref: support.rs:658-673, 2076-2116, 2762-2792
import {
  commandConversationId, createRun, ensureConversation, ensureResolveRequirements, markAppliedSlasResolved,
  updateConversationStatus, withStore,
} from './_shared.js';

export default async function handle(command, ctx) {
  const run = createRun(command, ctx);
  const payload = run.payload;
  const conversationId = commandConversationId(command);
  return withStore(ctx, run, async (store) => {
    await ensureConversation(store, run, conversationId, payload);
    await ensureResolveRequirements(store, conversationId, payload);
    await updateConversationStatus(store, run, conversationId, 'resolved');
    await markAppliedSlasResolved(store, run, conversationId);
    return run.result({ conversation_id: conversationId, status: 'resolved' });
  });
}
