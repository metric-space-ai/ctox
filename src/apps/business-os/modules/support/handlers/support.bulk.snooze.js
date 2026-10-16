// Origin: CTOX
// License: Apache-2.0
// support.bulk.snooze – ref: support.rs:1068-1090 (Rust schreibt hier kein Ereignis)
import {
  commandConversationIds, createRun, ensureConversation, fail, numberField, updateConversationFields, withStore,
} from './_shared.js';

export default async function handle(command, ctx) {
  const run = createRun(command, ctx);
  const payload = run.payload;
  const conversationIds = commandConversationIds(command);
  const snoozedUntilMs = numberField(payload, ['snoozed_until_ms', 'until_ms']);
  if (snoozedUntilMs === undefined) fail('invalid', 'snoozed_until_ms is required');
  return withStore(ctx, run, async (store) => {
    for (const conversationId of conversationIds) {
      await ensureConversation(store, run, conversationId, payload);
      await updateConversationFields(store, run, conversationId, {
        status: 'snoozed',
        snoozed_until_ms: snoozedUntilMs,
        last_activity_at_ms: run.now,
      });
    }
    return run.result({ conversation_ids: conversationIds, snoozed_until_ms: snoozedUntilMs, updated: conversationIds.length });
  });
}
