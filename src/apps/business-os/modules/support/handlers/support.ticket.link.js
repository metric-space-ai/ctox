// Origin: CTOX
// License: Apache-2.0
// support.ticket.link – ref: support.rs:783-816
import {
  commandConversationId, createRun, ensureConversation, fail, firstString, insertConversationEvent,
  updateConversationFields, withStore,
} from './_shared.js';

export default async function handle(command, ctx) {
  const run = createRun(command, ctx);
  const payload = run.payload;
  const conversationId = commandConversationId(command);
  return withStore(ctx, run, async (store) => {
    await ensureConversation(store, run, conversationId, payload);
    const ticketCaseId = firstString(payload, ['ticket_case_id', 'case_id', 'ticket_key']);
    if (ticketCaseId === undefined) fail('invalid', 'ticket_case_id is required');
    await updateConversationFields(store, run, conversationId, { ticket_case_id: ticketCaseId, last_activity_at_ms: run.now });
    await insertConversationEvent(store, run, {
      conversation_id: conversationId,
      event_type: 'support.ticket.linked',
      summary: 'Support conversation linked to a CTOX ticket.',
      payload: { ticket_case_id: ticketCaseId },
    });
    return run.result({ conversation_id: conversationId, ticket_case_id: ticketCaseId });
  });
}
