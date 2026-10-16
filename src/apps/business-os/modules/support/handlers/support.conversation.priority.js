// Origin: CTOX
// License: Apache-2.0
// support.conversation.priority – ref: support.rs:591-624
import {
  commandConversationId, createRun, ensureConversation, fail, firstString, insertConversationEvent,
  normalizePriority, updateConversationFields, withStore,
} from './_shared.js';

export default async function handle(command, ctx) {
  const run = createRun(command, ctx);
  const payload = run.payload;
  const conversationId = commandConversationId(command);
  return withStore(ctx, run, async (store) => {
    await ensureConversation(store, run, conversationId, payload);
    const raw = firstString(payload, ['priority']);
    if (raw === undefined) fail('invalid', 'priority is required');
    const priority = normalizePriority(raw);
    await updateConversationFields(store, run, conversationId, { priority, last_activity_at_ms: run.now });
    await insertConversationEvent(store, run, {
      conversation_id: conversationId,
      event_type: 'support.conversation.priority_changed',
      summary: 'Support conversation priority changed.',
      payload: { priority },
    });
    return run.result({ conversation_id: conversationId, priority });
  });
}
