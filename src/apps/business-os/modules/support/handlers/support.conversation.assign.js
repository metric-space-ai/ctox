// Origin: CTOX
// License: Apache-2.0
// support.conversation.assign – ref: support.rs:516-571
import {
  commandConversationId, createRun, ensureConversation, fail, firstString, insertAssignmentEvent,
  insertConversationEvent, loadConversationRequired, updateConversationFields, withStore,
} from './_shared.js';

export default async function handle(command, ctx) {
  const run = createRun(command, ctx);
  const payload = run.payload;
  const conversationId = commandConversationId(command);
  return withStore(ctx, run, async (store) => {
    await ensureConversation(store, run, conversationId, payload);
    const previous = await loadConversationRequired(store, conversationId);
    const assigneeId = firstString(payload, ['assignee_id', 'user_id']);
    const teamId = firstString(payload, ['team_id']);
    if (assigneeId === undefined && teamId === undefined) fail('invalid', 'assignee_id or team_id is required');
    const conversation = await updateConversationFields(store, run, conversationId, {
      assignee_id: assigneeId,
      team_id: teamId,
      last_activity_at_ms: run.now,
    });
    await insertAssignmentEvent(store, run, conversationId, assigneeId ?? conversation.assignee_id,
      previous.assignee_id, 'support.assignment.assigned', { team_id: teamId ?? null, source_command_id: run.commandId });
    await insertConversationEvent(store, run, {
      conversation_id: conversationId,
      event_type: 'support.conversation.assigned',
      summary: 'Support conversation assigned.',
      payload: {
        assignee_id: conversation.assignee_id,
        team_id: conversation.team_id,
        previous_assignee_id: previous.assignee_id,
      },
    });
    return run.result({ conversation_id: conversationId, assignee_id: conversation.assignee_id, team_id: conversation.team_id });
  });
}
