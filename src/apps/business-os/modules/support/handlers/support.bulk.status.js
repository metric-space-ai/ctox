// Origin: CTOX
// License: Apache-2.0
// support.bulk.status – ref: support.rs:1017-1043
import {
  commandConversationIds, createRun, ensureConversation, ensureResolveRequirements, fail, firstString,
  markAppliedSlasResolved, normalizeStatus, updateConversationStatus, withStore,
} from './_shared.js';

export default async function handle(command, ctx) {
  const run = createRun(command, ctx);
  const payload = run.payload;
  const conversationIds = commandConversationIds(command);
  const raw = firstString(payload, ['status']);
  if (raw === undefined) fail('invalid', 'status is required');
  const status = normalizeStatus(raw);
  return withStore(ctx, run, async (store) => {
    for (const conversationId of conversationIds) {
      await ensureConversation(store, run, conversationId, payload);
      if (status === 'resolved') await ensureResolveRequirements(store, conversationId, payload);
      await updateConversationStatus(store, run, conversationId, status);
      if (status === 'resolved') await markAppliedSlasResolved(store, run, conversationId);
    }
    return run.result({ conversation_ids: conversationIds, status, updated: conversationIds.length });
  });
}
