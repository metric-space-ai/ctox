// Origin: CTOX
// License: Apache-2.0
// support.identity.link – ref: support.rs:688-732, 3167-3221
import {
  createRun, ensureConversation, firstString, insertConversationEvent, recordIdOf, updateConversationFields,
  upsertIdentityLink, withStore,
} from './_shared.js';

export default async function handle(command, ctx) {
  const run = createRun(command, ctx);
  const payload = run.payload;
  const identityLinkId = firstString(payload, ['id', 'identity_link_id']) ?? run.newId('support_identity_');
  // Rust: payload.conversation_id, sonst record_id (ungetrimmt), sonst "".
  const conversationId = firstString(payload, ['conversation_id']) ?? recordIdOf(command) ?? '';
  return withStore(ctx, run, async (store) => {
    await upsertIdentityLink(store, run, identityLinkId, payload);
    if (conversationId !== '') {
      await ensureConversation(store, run, conversationId, payload);
      await updateConversationFields(store, run, conversationId, {
        customer_account_id: firstString(payload, ['customer_account_id']),
        customer_contact_id: firstString(payload, ['customer_contact_id']),
        last_activity_at_ms: run.now,
      });
      await insertConversationEvent(store, run, {
        conversation_id: conversationId,
        event_type: 'support.identity.linked',
        summary: 'Support identity linked.',
        payload: { identity_link_id: identityLinkId },
      });
    }
    return run.result({ identity_link_id: identityLinkId, conversation_id: conversationId });
  });
}
