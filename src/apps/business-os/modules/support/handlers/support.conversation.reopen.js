// Origin: CTOX
// License: Apache-2.0
// support.conversation.reopen – ref: support.rs:674-687
import { commandConversationId, createRun, ensureConversation, updateConversationStatus, withStore } from './_shared.js';

export default async function handle(command, ctx) {
  const run = createRun(command, ctx);
  const conversationId = commandConversationId(command);
  return withStore(ctx, run, async (store) => {
    await ensureConversation(store, run, conversationId, run.payload);
    await updateConversationStatus(store, run, conversationId, 'open');
    return run.result({ conversation_id: conversationId, status: 'open' });
  });
}
