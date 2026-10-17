// Origin: CTOX
// License: Apache-2.0
// Gemeinsamer Arm von support.agent.apply_suggestion / reject_suggestion – ref: support.rs:1279-1320
import { createRun, fail, firstString, insertConversationEvent, withStore } from './_shared.js';

export async function decideSuggestion(command, ctx, status) {
  const run = createRun(command, ctx);
  const payload = run.payload;
  const suggestionId = firstString(payload, ['suggestion_id', 'id']);
  if (suggestionId === undefined) fail('invalid', 'suggestion_id is required');
  return withStore(ctx, run, async (store) => {
    const suggestion = await store.get('support_agent_suggestions', suggestionId);
    if (suggestion) {
      // Rust: UPDATE … SET status, updated_at_ms; danach project_agent_suggestion.
      await store.put('support_agent_suggestions', { ...suggestion, status, updated_at_ms: run.now });
    }
    const conversationId = (suggestion ? suggestion.conversation_id : undefined)
      ?? firstString(payload, ['conversation_id']);
    if (conversationId === undefined) fail('not_found', 'support agent suggestion does not exist');
    const applied = status === 'applied';
    await insertConversationEvent(store, run, {
      conversation_id: conversationId,
      event_type: applied ? 'support.agent.suggestion_applied' : 'support.agent.suggestion_rejected',
      source_task_id: firstString(payload, ['task_id']) ?? '',
      summary: applied ? 'Support agent suggestion applied.' : 'Support agent suggestion rejected.',
      payload: { suggestion_id: suggestionId, status },
    });
    return run.result({ conversation_id: conversationId, suggestion_id: suggestionId, status });
  });
}
