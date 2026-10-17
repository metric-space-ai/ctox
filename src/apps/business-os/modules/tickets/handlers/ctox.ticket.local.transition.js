// Origin: CTOX
// License: Apache-2.0
// ctox.ticket.local.transition – ref: mission/tickets/cases.rs:1262-1276 → ticket_local_native.rs:166-205
import { fail, payloadOf, payloadString } from './_base.js';
import { clock, seedOf, transitionLocalTicket } from './_tickets.js';

export default async function handle(command, ctx) {
  const p = payloadOf(command);
  const ticketId = payloadString(p, 'ticket_id') ?? payloadString(p, 'ticket_key');
  if (ticketId === undefined) fail('invalid', 'ticket_key is required');
  const status = payloadString(p, 'status') ?? payloadString(p, 'state');
  if (status === undefined) fail('invalid', 'state is required');
  const time = clock(ctx);
  const ticket = await ctx.transaction((tx) => transitionLocalTicket(tx, time, seedOf(command), ticketId, status));
  return { ticket, ticket_key: ticketId, source_system: 'local' };
}
