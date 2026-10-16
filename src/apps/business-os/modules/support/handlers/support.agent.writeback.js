// Origin: CTOX
// License: Apache-2.0
// support.agent.writeback – ref: support.rs:1229-1278
import {
  clone, commandConversationId, createRun, ensureConversation, firstString, insertConversationEvent,
  numberField, upsertAgentSuggestion, withStore,
} from './_shared.js';

export default async function handle(command, ctx) {
  const run = createRun(command, ctx);
  const payload = run.payload;
  const conversationId = commandConversationId(command);
  return withStore(ctx, run, async (store) => {
    await ensureConversation(store, run, conversationId, payload);
    const suggestionId = firstString(payload, ['id', 'suggestion_id']) ?? run.newId('support_suggestion_');
    const taskId = firstString(payload, ['task_id']) ?? '';
    const suggestionKind = firstString(payload, ['suggestion_kind', 'kind']) ?? 'summary';
    // Rust: number_field → nur ganze Zahlen; 0.82 wird zu 0 (§3 U6).
    const confidence = numberField(payload, ['confidence']) ?? 0;
    const inner = Object.prototype.hasOwnProperty.call(payload, 'payload')
      ? clone(payload.payload)
      : { raw_payload: clone(payload) };
    await upsertAgentSuggestion(store, run, {
      id: suggestionId,
      conversation_id: conversationId,
      source_command_id: firstString(payload, ['source_command_id']) ?? '',
      task_id: taskId,
      suggestion_kind: suggestionKind,
      status: 'proposed',
      confidence,
      required_human_action: firstString(payload, ['required_human_action']) ?? 'review',
      summary: firstString(payload, ['summary']) ?? '',
      payload: inner,
    });
    await insertConversationEvent(store, run, {
      conversation_id: conversationId,
      event_type: 'support.agent.writeback',
      source_task_id: taskId,
      summary: 'CTOX Agent wrote a structured Support suggestion.',
      payload: { suggestion_id: suggestionId, suggestion_kind: suggestionKind },
    });
    return run.result({ conversation_id: conversationId, suggestion_id: suggestionId, status: 'proposed' });
  });
}
