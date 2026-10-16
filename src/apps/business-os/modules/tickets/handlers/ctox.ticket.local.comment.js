// Origin: CTOX
// License: Apache-2.0
// ctox.ticket.local.comment – ref: mission/tickets/cases.rs:1249-1261 → ticket_local_native.rs:142-164 add_local_comment
import { fail, payloadOf, payloadString, requiredString } from './_base.js';
import { COLLECTIONS, clock, insertLocalEvent, loadLocalTicket, seedOf } from './_tickets.js';

export default async function handle(command, ctx) {
  const p = payloadOf(command);
  const ticketId = payloadString(p, 'ticket_id') ?? payloadString(p, 'ticket_key');
  if (ticketId === undefined) fail('invalid', 'ticket_key is required');
  const body = requiredString(p, 'body');
  const time = clock(ctx);
  const event = await ctx.transaction(async (tx) => {
    const ticket = await loadLocalTicket(tx, ticketId);
    const created = await insertLocalEvent(tx, time, seedOf(command), { ticketId, eventType: 'comment', summary: 'comment', body });
    await tx.patch(COLLECTIONS.items, ticket.id, { updated_at: time.iso, updated_at_ms: time.ms });
    return created;
  });
  return { event, ticket_key: ticketId, source_system: 'local' };
}
