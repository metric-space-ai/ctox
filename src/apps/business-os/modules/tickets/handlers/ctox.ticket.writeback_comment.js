// Origin: CTOX
// License: Apache-2.0
// ctox.ticket.writeback_comment – ref: mission/tickets/cases.rs:1402-1412 → writeback_comment (cases.rs:954-1060)
// Nur aus writeback_pending. Lokales System: Kommentar-Ereignis in der App (ticket_local_native.rs:256-278);
// Fremdsystem (zammad): Rückschrieb als agent.task, Abschluss in _writeback.js.
import { asBool, fail, payloadOf, requiredString } from './_base.js';
import {
  COLLECTIONS, WRITEBACK_SYSTEMS, caseOutcome, clock, enqueueRemoteWriteback, ensureCaseReadyForWriteback, loadCase,
  loadTicket, localWriteback, recordAudit, seedOf, stableDigest,
} from './_tickets.js';

export default async function handle(command, ctx) {
  const p = payloadOf(command);
  const caseId = requiredString(p, 'case_id');
  const body = requiredString(p, 'body');
  const internal = asBool(p.internal) ?? false;
  const time = clock(ctx);
  const seed = seedOf(command);
  const writebackId = `writeback:${caseId}:${await stableDigest(seed)}`;
  const request = { operation: 'comment', body, internal };
  const outcome = await ctx.transaction(async (tx) => {
    const caseDoc = await loadCase(tx, caseId);
    ensureCaseReadyForWriteback(caseDoc);
    const ticket = await loadTicket(tx, caseDoc.ticket_key, 'ticket not found for case');
    if (!WRITEBACK_SYSTEMS.includes(ticket.source_system)) fail('invalid', `unsupported ticket system for writeback: ${ticket.source_system}`);
    const local = ticket.source_system === 'local';
    const result = local ? await localWriteback(tx, time, seed, ticket, request) : null;
    await tx.put(COLLECTIONS.writebacks, {
      id: writebackId, writeback_id: writebackId, case_id: caseId, ticket_key: caseDoc.ticket_key, operation: 'comment',
      payload: { body, internal, ...(local ? { remote_event_ids: result.remote_event_ids } : {}) },
      status: local ? 'ok' : 'queued', created_at: time.iso, updated_at_ms: time.ms, is_deleted: false,
    });
    await tx.patch(COLLECTIONS.cases, caseId, { updated_at: time.iso, updated_at_ms: time.ms });
    return { caseDoc, ticket, local, after: await tx.get(COLLECTIONS.cases, caseId) };
  });
  if (!outcome.local) {
    const taskId = await enqueueRemoteWriteback(ctx, outcome.ticket, writebackId, request);
    await ctx.patch(COLLECTIONS.writebacks, writebackId, { task_id: taskId });
    return { ...caseOutcome(outcome.after), writeback_id: writebackId, writeback_status: 'queued', task_id: taskId };
  }
  await recordAudit(ctx, time, {
    case: outcome.caseDoc, ticket_key: outcome.caseDoc.ticket_key, actor_type: 'writeback_engine', action_type: 'writeback_record',
    details: { operation: 'comment', body, internal },
  });
  return caseOutcome(outcome.after);
}
