// Origin: CTOX
// License: Apache-2.0
// support.conversation.claim – ref: support.rs:478-515, 2045-2074
import {
  boolField, claimConversationAtomic, commandConversationId, createRun, ensureConversation, firstString,
  insertAssignmentEvent, insertConversationEvent, loadConversationRequired, withStore,
} from './_shared.js';

export default async function handle(command, ctx) {
  const run = createRun(command, ctx);
  const payload = run.payload;
  const conversationId = commandConversationId(command);
  return withStore(ctx, run, async (store) => {
    await ensureConversation(store, run, conversationId, payload);
    const nextAssigneeId = firstString(payload, ['assignee_id', 'user_id']) ?? run.actorId;
    const force = boolField(payload, ['force']) ?? false;
    const previousAssigneeId = await claimConversationAtomic(store, run, conversationId, nextAssigneeId, force);
    const conversation = await loadConversationRequired(store, conversationId);
    await insertAssignmentEvent(store, run, conversationId, nextAssigneeId, previousAssigneeId,
      'support.assignment.claimed', { source_command_id: run.commandId, force });
    await insertConversationEvent(store, run, {
      conversation_id: conversationId,
      event_type: 'support.conversation.claimed',
      summary: 'Support conversation claimed.',
      payload: { assignee_id: nextAssigneeId, previous_assignee_id: previousAssigneeId },
    });
    return run.result({ conversation_id: conversationId, assignee_id: conversation.assignee_id });
  });
}
