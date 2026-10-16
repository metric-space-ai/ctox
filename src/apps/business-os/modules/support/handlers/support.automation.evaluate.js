// Origin: CTOX
// License: Apache-2.0
// support.automation.evaluate – ref: support.rs:1163-1181, 2421-2539
import {
  commandConversationId, createRun, ensureConversation, evaluateAutomationRules, firstString, withStore,
} from './_shared.js';

export default async function handle(command, ctx) {
  const run = createRun(command, ctx);
  const payload = run.payload;
  const conversationId = commandConversationId(command);
  return withStore(ctx, run, async (store) => {
    await ensureConversation(store, run, conversationId, payload);
    const eventName = firstString(payload, ['event_name']) ?? 'manual';
    const matches = await evaluateAutomationRules(store, run, conversationId, eventName, 'support.automation.evaluate');
    return run.result({ conversation_id: conversationId, event_name: eventName, matched_rules: matches });
  });
}
