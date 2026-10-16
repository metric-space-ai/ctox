// Origin: CTOX
// License: Apache-2.0
// ctox.ticket.request_clarification – ref: mission/tickets/cases.rs:1308-1359 → create_ticket_clarification_request
// (cases.rs:525-623). Rückfrage als Entwurf; ein zugehöriger Fall geht nach blocked_needs_clarification.
import { fail, payloadOf, payloadString, requiredString } from './_base.js';
import {
  COLLECTIONS, canonicalClarificationResumeState, canonicalClarificationTargetChannel,
  canonicalClarificationTargetType, clock, enforceStateTransition, ensureCaseCanRequestClarification, loadCase,
  loadTicket, normalizeClarificationInputs, parseCsv, recordAudit, seedOf, setCaseState, stableDigest,
} from './_tickets.js';

export default async function handle(command, ctx) {
  const p = payloadOf(command);
  const caseId = payloadString(p, 'case_id');
  const requestedKey = payloadString(p, 'ticket_key');
  const question = requiredString(p, 'question');
  const missingRaw = Array.isArray(p.missing_inputs)
    ? p.missing_inputs.filter((item) => typeof item === 'string')
    : (payloadString(p, 'missing_inputs_csv') !== undefined ? parseCsv(payloadString(p, 'missing_inputs_csv')) : []);
  const workId = payloadString(p, 'work_id') ?? null;
  const unblockCriteria = payloadString(p, 'unblock_criteria') ?? null;
  const createdBy = payloadString(p, 'created_by') ?? 'business-os';
  const metadata = p.metadata !== undefined && p.metadata !== null ? p.metadata : {};
  const time = clock(ctx);
  const seed = seedOf(command);
  const outcome = await ctx.transaction(async (tx) => {
    const caseDoc = caseId !== undefined ? await loadCase(tx, caseId) : null;
    let ticketKey;
    if (caseDoc && requestedKey !== undefined && caseDoc.ticket_key !== requestedKey) {
      fail('invalid', `clarification ticket_key ${requestedKey} does not match case ${caseDoc.case_id} ticket_key ${caseDoc.ticket_key}`);
    } else if (caseDoc) ticketKey = caseDoc.ticket_key;
    else if (requestedKey !== undefined) ticketKey = requestedKey;
    else fail('invalid', 'case_id or ticket_key is required for clarification');
    const ticket = await loadTicket(tx, ticketKey, 'ticket not found for clarification');
    const targetType = canonicalClarificationTargetType(payloadString(p, 'target_type') ?? 'requester');
    const targetChannel = canonicalClarificationTargetChannel(payloadString(p, 'target_channel') ?? 'ticket');
    const resumeState = canonicalClarificationResumeState(payloadString(p, 'resume_state') ?? 'executable');
    const missingInputs = normalizeClarificationInputs(missingRaw);
    const clarificationId = `clarification:${ticketKey}:${await stableDigest(`${question}${seed}`)}`;
    if (caseDoc) {
      ensureCaseCanRequestClarification(caseDoc);
      enforceStateTransition(caseDoc, 'blocked_needs_clarification');
    }
    const doc = {
      id: clarificationId, clarification_id: clarificationId, ticket_key: ticketKey, case_id: caseDoc?.case_id ?? null,
      work_id: workId, target_type: targetType, target_channel: targetChannel, question, missing_inputs: missingInputs,
      unblock_criteria: unblockCriteria, status: 'draft', outbound_message_key: null, inbound_response_key: null,
      inbound_response_body: null, resume_state: resumeState, created_by: createdBy, created_at: time.iso,
      updated_at: time.iso, sent_at: null, resolved_at: null, metadata,
      updated_at_ms: time.ms, is_deleted: false,
    };
    await tx.put(COLLECTIONS.clarifications, doc);
    if (caseDoc) await setCaseState(tx, caseDoc, 'blocked_needs_clarification', time);
    return { caseDoc, ticket, doc };
  });
  const { caseDoc, ticket, doc } = outcome;
  await recordAudit(ctx, time, {
    case: caseDoc, ticket_key: ticket.ticket_key, case_id: caseDoc?.case_id ?? null,
    actor_type: 'clarification_engine', action_type: 'clarification_requested',
    details: {
      clarification_id: doc.clarification_id, target_type: doc.target_type, target_channel: doc.target_channel,
      question: doc.question, missing_inputs: doc.missing_inputs, unblock_criteria: doc.unblock_criteria,
    },
  });
  return { clarification: doc, clarification_id: doc.clarification_id, case_id: doc.case_id, ticket_key: doc.ticket_key };
}
