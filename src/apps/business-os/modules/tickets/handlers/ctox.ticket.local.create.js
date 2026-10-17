// Origin: CTOX
// License: Apache-2.0
// ctox.ticket.local.create – ref: mission/tickets/cases.rs:1230-1248 → ticket_local_native.rs:106-140 create_local_ticket
// Lokales Ticket direkt als ctox_ticket_items-Dokument (Schlüssel local:<ticket_id>) plus Ereignis „created“.
// Optionales ticket_id (LT-…): vom aufrufenden Handler vorgegeben (support über ctx.command), damit der Aufrufer
// den Schlüssel kennt, ohne auf das Ergebnis zu warten.
import { fail, payloadOf, payloadString, requiredString } from './_base.js';
import { COLLECTIONS, clock, insertLocalEvent, localRecord, seedOf, stableDigest, ticketKeyOf } from './_tickets.js';

export default async function handle(command, ctx) {
  const p = payloadOf(command);
  const title = requiredString(p, 'title');
  const body = payloadString(p, 'body') ?? payloadString(p, 'body_text') ?? '';
  const status = payloadString(p, 'status') ?? 'open';
  const priority = payloadString(p, 'priority') ?? null;
  const time = clock(ctx);
  const seed = seedOf(command);
  const requested = payloadString(p, 'ticket_id');
  if (requested !== undefined && !/^LT-[A-Za-z0-9_-]{1,64}$/.test(requested)) fail('invalid', 'ticket_id must match LT-<id>');
  const ticketId = requested ?? `LT-${await stableDigest(`${title}${seed}`)}`;
  const key = ticketKeyOf('local', ticketId);
  const item = await ctx.transaction(async (tx) => {
    const existing = await tx.get(COLLECTIONS.items, key);
    if (existing && !existing.is_deleted) {
      if (existing.title === title && existing.created_by_command === seed) return existing;
      fail('conflict', `local ticket already exists: ${ticketId}`);
    }
    const doc = {
      id: key,
      ticket_key: key,
      source_system: 'local',
      remote_ticket_id: ticketId,
      title,
      body_text: body,
      remote_status: status,
      priority,
      requester: null,
      created_at: time.iso,
      updated_at: time.iso,
      last_synced_at: time.iso,
      metadata: {},
      created_by_command: seed,
      updated_at_ms: time.ms,
      is_deleted: false,
    };
    await tx.put(COLLECTIONS.items, doc);
    await insertLocalEvent(tx, time, seed, { ticketId, eventType: 'created', summary: title, body });
    return doc;
  });
  return { ticket: localRecord(item), ticket_key: ticketId, source_system: 'local' };
}
