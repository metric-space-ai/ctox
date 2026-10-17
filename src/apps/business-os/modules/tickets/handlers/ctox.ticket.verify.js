// Origin: CTOX
// License: Apache-2.0
// ctox.ticket.verify – ref: mission/tickets/cases.rs:1297-1307 → record_verification (cases.rs:466-523)
// passed → writeback_pending, failed → blocked.
import { payloadOf, payloadString, requiredString } from './_base.js';
import {
  COLLECTIONS, canonicalVerificationStatus, caseOutcome, clock, enforceStateTransition, loadCase, recordAudit, seedOf,
  setCaseState, stableDigest,
} from './_tickets.js';

export default async function handle(command, ctx) {
  const p = payloadOf(command);
  const caseId = requiredString(p, 'case_id');
  const status = requiredString(p, 'status');
  const summary = payloadString(p, 'summary') ?? null;
  const time = clock(ctx);
  const verificationId = `verification:${caseId}:${await stableDigest(seedOf(command))}`;
  const { before, after, canonical } = await ctx.transaction(async (tx) => {
    const caseDoc = await loadCase(tx, caseId);
    const canonicalStatus = canonicalVerificationStatus(status);
    const next = canonicalStatus === 'passed' ? 'writeback_pending' : 'blocked';
    enforceStateTransition(caseDoc, next);
    await tx.put(COLLECTIONS.verifications, {
      id: verificationId, verification_id: verificationId, case_id: caseId, status: canonicalStatus, summary,
      created_at: time.iso, updated_at_ms: time.ms, is_deleted: false,
    });
    await setCaseState(tx, caseDoc, next, time);
    return { before: caseDoc, after: await tx.get(COLLECTIONS.cases, caseId), canonical: canonicalStatus };
  });
  await recordAudit(ctx, time, {
    case: before, ticket_key: before.ticket_key, actor_type: 'verification_engine', action_type: 'verification_record',
    details: { status: canonical, summary },
  });
  return caseOutcome(after);
}
