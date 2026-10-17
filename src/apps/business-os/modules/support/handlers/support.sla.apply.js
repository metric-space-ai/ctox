// Origin: CTOX
// License: Apache-2.0
// support.sla.apply – ref: support.rs:1189-1217, 2541-2696
import { applySlaFromPayload, commandConversationId, createRun, ensureConversation, withStore } from './_shared.js';

export default async function handle(command, ctx) {
  const run = createRun(command, ctx);
  const payload = run.payload;
  const conversationId = commandConversationId(command);
  return withStore(ctx, run, async (store) => {
    await ensureConversation(store, run, conversationId, payload);
    const { policyId, appliedSlaId } = await applySlaFromPayload(store, run, conversationId, payload, 'policy_id is required');
    return run.result({ conversation_id: conversationId, policy_id: policyId, applied_sla_id: appliedSlaId });
  });
}
