// Origin: CTOX
// License: Apache-2.0
// support.ticket.create_from_conversation – ref: support.rs:817-880
// Fachlogik hier. Die Ticketanlage gehört der Tickets-App (Eigentümerin der ctox_ticket_*-Collections, Strang
// TICKAPP): support schickt ihr den Befehl ctox.ticket.local.create über ctx.command, keine Core-Capability.
// Die lokale Ticket-ID gibt support selbst vor (deterministisch aus der Befehls-ID), weil ctx.command nur
// {id, status} liefert; Rust speicherte ohne case_id ohnehin den ticket_key = LT-Id (cases.rs:1230-1248).
// Ablauf: (1) Konversation sicherstellen/lesen, (2) Ticket-Befehl absetzen, (3) verknüpfen.
import {
  NAMESPACE_URL, commandConversationId, createRun, ensureConversation, fail, firstString, insertConversationEvent,
  trimToChars, updateConversationFields, uuidV5, withStore,
} from './_shared.js';

export const TICKET_CREATE_COMMAND = 'ctox.ticket.local.create';

export function supportTicketId(commandId) {
  return `LT-${uuidV5(NAMESPACE_URL, `ctox.support.ticket:${commandId}`).replace(/-/g, '').slice(0, 12)}`;
}

export default async function handle(command, ctx) {
  const run = createRun(command, ctx);
  const payload = run.payload;
  const conversationId = commandConversationId(command);
  const conversation = await withStore(ctx, run, (store) => ensureConversation(store, run, conversationId, payload));
  const title = firstString(payload, ['title'])
    ?? (conversation.search_text.trim() === ''
      ? `Support conversation ${conversation.id}`
      : trimToChars(conversation.search_text, 120));
  const body = firstString(payload, ['body', 'summary'])
    ?? `Created from Business OS Support conversation \`${conversation.id}\`.`;
  if (typeof ctx.command !== 'function') fail('unavailable', 'ctx.command is not available');
  const ticketId = supportTicketId(run.commandId);
  const sent = await ctx.command(
    TICKET_CREATE_COMMAND,
    { title, body, status: 'open', priority: conversation.priority, ticket_id: ticketId },
    { id: `${run.commandId}:ticket`, module: 'tickets', record_id: conversationId },
  );
  const status = sent && typeof sent === 'object' ? sent.status : undefined;
  if (status === 'rejected' || status === 'failed') {
    fail('unavailable', `ticket command ${status}: ${sent.reason ?? 'no reason'}`);
  }
  const outcome = {
    command_id: sent && typeof sent.id === 'string' ? sent.id : null,
    command_status: status ?? 'accepted',
    ticket_key: ticketId,
    source_system: 'local',
  };
  const ticketCaseId = ticketId;
  return withStore(ctx, run, async (store) => {
    await updateConversationFields(store, run, conversationId, { ticket_case_id: ticketCaseId, last_activity_at_ms: run.now });
    await insertConversationEvent(store, run, {
      conversation_id: conversationId,
      event_type: 'support.ticket.created',
      summary: 'CTOX ticket created from Support conversation.',
      payload: { ticket_case_id: ticketCaseId, ticket_outcome: outcome },
    });
    return run.result({ conversation_id: conversationId, ticket_case_id: ticketCaseId });
  });
}
