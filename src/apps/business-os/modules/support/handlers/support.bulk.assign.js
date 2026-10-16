// Origin: CTOX
// License: Apache-2.0
// support.bulk.assign – ref: support.rs:975-1016
import {
  commandConversationIds, createRun, ensureConversation, fail, firstString, insertAssignmentEvent,
  loadConversationRequired, updateConversationFields, withStore,
} from './_shared.js';

export default async function handle(command, ctx) {
  const run = createRun(command, ctx);
  const payload = run.payload;
  const conversationIds = commandConversationIds(command);
  const assigneeId = firstString(payload, ['assignee_id', 'user_id']);
  const teamId = firstString(payload, ['team_id']);
  if (assigneeId === undefined && teamId === undefined) fail('invalid', 'assignee_id or team_id is required');
  return withStore(ctx, run, async (store) => {
    for (const conversationId of conversationIds) {
      await ensureConversation(store, run, conversationId, payload);
      const previous = await loadConversationRequired(store, conversationId);
      const conversation = await updateConversationFields(store, run, conversationId, {
        assignee_id: assigneeId,
        team_id: teamId,
        last_activity_at_ms: run.now,
      });
      await insertAssignmentEvent(store, run, conversationId, assigneeId ?? conversation.assignee_id, previous.assignee_id,
        'support.assignment.bulk_assigned', { team_id: teamId ?? null, source_command_id: run.commandId });
    }
    return run.result({ conversation_ids: conversationIds, updated: conversationIds.length });
  });
}
