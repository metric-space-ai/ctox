// Origin: CTOX
// License: Apache-2.0
// support.reply.draft – ref: support.rs:881-916
import {
  commandConversationId, createRun, ensureConversation, fail, firstString, insertConversationEvent,
  trimToChars, upsertAgentSuggestion, withStore,
} from './_shared.js';

export default async function handle(command, ctx) {
  const run = createRun(command, ctx);
  const payload = run.payload;
  const conversationId = commandConversationId(command);
  return withStore(ctx, run, async (store) => {
    await ensureConversation(store, run, conversationId, payload);
    const body = firstString(payload, ['body', 'text', 'draft']);
    if (body === undefined) fail('invalid', 'draft body is required');
    const suggestionId = firstString(payload, ['id', 'suggestion_id']) ?? run.newId('support_suggestion_');
    await upsertAgentSuggestion(store, run, {
      id: suggestionId,
      conversation_id: conversationId,
      source_command_id: run.commandId,
      task_id: '',
      suggestion_kind: 'draft_reply',
      status: 'draft',
      confidence: 1,
      required_human_action: 'human_send_required',
      summary: trimToChars(body, 180),
      payload: { body, source: 'support.reply.draft' },
    });
    await insertConversationEvent(store, run, {
      conversation_id: conversationId,
      event_type: 'support.reply.draft_created',
      summary: 'Support reply draft created.',
      payload: { suggestion_id: suggestionId },
    });
    return run.result({ conversation_id: conversationId, suggestion_id: suggestionId, status: 'draft' });
  });
}
