// Origin: CTOX
// License: Apache-2.0
// ctox.ticket.writeback_transition – ref: mission/tickets/cases.rs:1413-1424 → writeback_transition (cases.rs:1062-1193)
// Nur aus writeback_pending, Abschluss-Prüfung vor und nach dem Rückschrieb, danach Fall closed.
// Fremdsystem (zammad): Rückschrieb als agent.task; Fall schließt erst der Befehl ctox.ticket.writeback.complete.
import { asBool, fail, payloadOf, payloadString, requiredString } from './_base.js';
import {
  COLLECTIONS, WRITEBACK_SYSTEMS, caseOutcome, clock, closureEvidence, enforceCloseTransition, enqueueRemoteWriteback,
  ensureCaseReadyForWriteback, loadCase, loadTicket, localWriteback, recordAudit, seedOf, setCaseState, stableDigest,
} from './_tickets.js';

export default async function handle(command, ctx) {
  const p = payloadOf(command);
  const caseId = requiredString(p, 'case_id');
  const state = requiredString(p, 'state');
  const noteBody = payloadString(p, 'body') ?? null;
  const internalNote = asBool(p.internal) ?? false;
  const time = clock(ctx);
  const seed = seedOf(command);
  const writebackId = `writeback:${caseId}:${await stableDigest(`${state}${seed}`)}`;
  const request = { operation: 'transition', state, note_body: noteBody, internal_note: internalNote };
  const outcome = await ctx.transaction(async (tx) => {
    const caseDoc = await loadCase(tx, caseId);
    ensureCaseReadyForWriteback(caseDoc);
    const ticket = await loadTicket(tx, caseDoc.ticket_key, 'ticket not found for case');
    if (!WRITEBACK_SYSTEMS.includes(ticket.source_system)) fail('invalid', `unsupported ticket system for writeback: ${ticket.source_system}`);
    const evidence = await closureEvidence(tx, caseId);
    enforceCloseTransition(caseDoc, evidence);
    const local = ticket.source_system === 'local';
    const result = local ? await localWriteback(tx, time, seed, ticket, request) : null;
    await tx.put(COLLECTIONS.writebacks, {
      id: writebackId, writeback_id: writebackId, case_id: caseId, ticket_key: caseDoc.ticket_key, operation: 'transition',
      payload: { state, note_body: noteBody, internal_note: internalNote, ...(local ? { remote_event_ids: result.remote_event_ids } : {}) },
      status: local ? 'ok' : 'queued', created_at: time.iso, updated_at_ms: time.ms, is_deleted: false,
    });
    if (local) {
      enforceCloseTransition(caseDoc, evidence);
      await setCaseState(tx, caseDoc, 'closed', time, { closed_at: time.iso });
    }
    return { caseDoc, ticket, local, after: await tx.get(COLLECTIONS.cases, caseId) };
  });
  if (!outcome.local) {
    const taskId = await enqueueRemoteWriteback(ctx, outcome.ticket, writebackId, request);
    await ctx.patch(COLLECTIONS.writebacks, writebackId, { task_id: taskId });
    return { ...caseOutcome(outcome.after), writeback_id: writebackId, writeback_status: 'queued', task_id: taskId };
  }
  const caseDoc = outcome.caseDoc;
  await recordAudit(ctx, time, {
    case: caseDoc, ticket_key: caseDoc.ticket_key, actor_type: 'writeback_engine', action_type: 'writeback_record',
    details: { operation: 'transition', state, note_body: noteBody, internal_note: internalNote },
  });
  await recordAudit(ctx, time, {
    case: caseDoc, ticket_key: caseDoc.ticket_key, actor_type: 'control_plane', action_type: 'case_closed',
    details: { reason: 'writeback transition completed' },
  });
  return caseOutcome(outcome.after);
}
