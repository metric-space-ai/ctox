// Origin: CTOX
// License: Apache-2.0
// support.bulk.priority – ref: support.rs:1044-1067 (Rust schreibt hier kein Ereignis)
import {
  commandConversationIds, createRun, ensureConversation, fail, firstString, normalizePriority,
  updateConversationFields, withStore,
} from './_shared.js';

export default async function handle(command, ctx) {
  const run = createRun(command, ctx);
  const payload = run.payload;
  const conversationIds = commandConversationIds(command);
  const raw = firstString(payload, ['priority']);
  if (raw === undefined) fail('invalid', 'priority is required');
  const priority = normalizePriority(raw);
  return withStore(ctx, run, async (store) => {
    for (const conversationId of conversationIds) {
      await ensureConversation(store, run, conversationId, payload);
      await updateConversationFields(store, run, conversationId, { priority, last_activity_at_ms: run.now });
    }
    return run.result({ conversation_ids: conversationIds, priority, updated: conversationIds.length });
  });
}
