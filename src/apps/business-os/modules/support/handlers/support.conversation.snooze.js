// Origin: CTOX
// License: Apache-2.0
// support.conversation.snooze – ref: support.rs:625-657
import {
  commandConversationId, createRun, ensureConversation, fail, insertConversationEvent, numberField,
  updateConversationFields, withStore,
} from './_shared.js';

export default async function handle(command, ctx) {
  const run = createRun(command, ctx);
  const payload = run.payload;
  const conversationId = commandConversationId(command);
  return withStore(ctx, run, async (store) => {
    await ensureConversation(store, run, conversationId, payload);
    const snoozedUntilMs = numberField(payload, ['snoozed_until_ms', 'until_ms']);
    if (snoozedUntilMs === undefined) fail('invalid', 'snoozed_until_ms is required');
    await updateConversationFields(store, run, conversationId, {
      status: 'snoozed',
      snoozed_until_ms: snoozedUntilMs,
      last_activity_at_ms: run.now,
    });
    await insertConversationEvent(store, run, {
      conversation_id: conversationId,
      event_type: 'support.conversation.snoozed',
      summary: 'Support conversation snoozed.',
      payload: { snoozed_until_ms: snoozedUntilMs },
    });
    return run.result({ conversation_id: conversationId, snoozed_until_ms: snoozedUntilMs });
  });
}
