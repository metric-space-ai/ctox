// Origin: CTOX
// License: Apache-2.0
// support.conversation.open_from_thread – ref: support.rs:442-477, 1508-1626, 2570-2593, 3139-3165
// Rust liest KEINEN Thread über einen Kanal: alle Felder kommen aus dem Payload. Damit reine
// Fachlogik; die in E vermutete Capability cap.mail.read wird hier nicht gebraucht (§3 U1).
import {
  applySlaForConversationIfAvailable, commandConversationId, createRun, firstString, insertConversationEvent,
  upsertConversationFromPayload, upsertThreadLink, withStore,
} from './_shared.js';

export default async function handle(command, ctx) {
  const run = createRun(command, ctx);
  const payload = run.payload;
  const conversationId = commandConversationId(command);
  return withStore(ctx, run, async (store) => {
    const conversation = await upsertConversationFromPayload(store, run, conversationId, payload);
    await applySlaForConversationIfAvailable(store, run, conversation);
    if (conversation.primary_thread_key !== '') {
      await upsertThreadLink(store, run, conversation.id, conversation.primary_thread_key,
        firstString(payload, ['channel', 'inbound_channel']) ?? '',
        firstString(payload, ['account_key']) ?? '',
        'primary');
    }
    await insertConversationEvent(store, run, {
      conversation_id: conversation.id,
      event_type: 'support.conversation.opened',
      summary: 'Support conversation opened from an inbound thread.',
      payload: { payload },
    });
    return run.result({ conversation_id: conversation.id, status: conversation.status });
  });
}
