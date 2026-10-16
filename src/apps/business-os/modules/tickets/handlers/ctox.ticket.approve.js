// Origin: CTOX
// License: Apache-2.0
// ctox.ticket.approve – ref: mission/tickets/cases.rs:1277-1290 → decide_case_approval (cases.rs:354-413)
// approved → executable, sonst blocked; Übergang gegen den Ticket-Katalog (core_state/mod.rs:1172).
import { payloadOf, payloadString, requiredString } from './_base.js';
import {
  COLLECTIONS, canonicalApprovalStatus, caseOutcome, clock, enforceStateTransition, loadCase, recordAudit, seedOf,
  setCaseState, stableDigest,
} from './_tickets.js';

export default async function handle(command, ctx) {
  const p = payloadOf(command);
  const caseId = requiredString(p, 'case_id');
  const status = requiredString(p, 'status');
  const decidedBy = payloadString(p, 'decided_by') ?? 'owner';
  const rationale = payloadString(p, 'rationale') ?? null;
  const time = clock(ctx);
  const approvalId = `approval:${caseId}:${await stableDigest(seedOf(command))}`;
  const { before, after, canonical } = await ctx.transaction(async (tx) => {
    const caseDoc = await loadCase(tx, caseId);
    const canonicalStatus = canonicalApprovalStatus(status);
    const next = canonicalStatus === 'approved' ? 'executable' : 'blocked';
    enforceStateTransition(caseDoc, next);
    await tx.put(COLLECTIONS.approvals, {
      id: approvalId, approval_id: approvalId, case_id: caseId, status: canonicalStatus, decided_by: decidedBy,
      rationale, created_at: time.iso, updated_at_ms: time.ms, is_deleted: false,
    });
    await setCaseState(tx, caseDoc, next, time);
    return { before: caseDoc, after: await tx.get(COLLECTIONS.cases, caseId), canonical: canonicalStatus };
  });
  await recordAudit(ctx, time, {
    case: before, ticket_key: before.ticket_key, actor_type: 'approver', action_type: 'approval_decision',
    details: { status: canonical, decided_by: decidedBy, rationale },
  });
  return caseOutcome(after);
}
