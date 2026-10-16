// Origin: CTOX
// License: Apache-2.0
// ctox.ticket.resolve_clarification – ref: mission/tickets/cases.rs:1381-1401 → resolve_ticket_clarification_request
// (cases.rs:776-849). Rückfrage resolved; ein blockierter Fall kehrt in den resume_state zurück.
import { payloadOf, payloadString, requiredString } from './_base.js';
import {
  COLLECTIONS, clock, enforceStateTransition, loadClarification, parseCaseState, recordAudit, setCaseState,
} from './_tickets.js';

export default async function handle(command, ctx) {
  const p = payloadOf(command);
  const clarificationId = requiredString(p, 'clarification_id');
  const responseKey = requiredString(p, 'response_key');
  const body = payloadString(p, 'body') ?? null;
  const resolvedBy = payloadString(p, 'resolved_by') ?? 'business-os';
  const time = clock(ctx);
  const after = await ctx.transaction(async (tx) => {
    const clarification = await loadClarification(tx, clarificationId);
    await tx.patch(COLLECTIONS.clarifications, clarification.id, {
      status: 'resolved', inbound_response_key: responseKey, inbound_response_body: body, updated_at: time.iso,
      resolved_at: time.iso, updated_at_ms: time.ms, metadata: { previous: clarification.metadata ?? {}, resolved_by: resolvedBy },
    });
    if (clarification.case_id) {
      const caseDoc = await tx.get(COLLECTIONS.cases, clarification.case_id);
      if (caseDoc && !caseDoc.is_deleted && (caseDoc.state === 'blocked' || caseDoc.state === 'blocked_needs_clarification')) {
        const resume = parseCaseState(clarification.resume_state);
        enforceStateTransition(caseDoc, resume);
        await setCaseState(tx, caseDoc, resume, time);
      }
    }
    return tx.get(COLLECTIONS.clarifications, clarificationId);
  });
  await recordAudit(ctx, time, {
    ticket_key: after.ticket_key, case_id: after.case_id ?? null, actor_type: 'clarification_engine',
    action_type: 'clarification_resolved',
    details: { clarification_id: clarificationId, response_key: responseKey, resolved_by: resolvedBy },
  });
  return { clarification: after, clarification_id: clarificationId, case_id: after.case_id ?? null, ticket_key: after.ticket_key };
}
