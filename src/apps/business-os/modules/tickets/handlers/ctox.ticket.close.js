// Origin: CTOX
// License: Apache-2.0
// ctox.ticket.close – ref: mission/tickets/cases.rs:1425-1433 → close_case (cases.rs:1195-1222)
// Abschluss nur aus verified/writeback_pending mit bestandener Prüfung und Review-Beleg (cases.rs:1457-1495).
import { payloadOf, payloadString, requiredString } from './_base.js';
import {
  COLLECTIONS, caseOutcome, clock, closureEvidence, enforceCloseTransition, loadCase, recordAudit, setCaseState,
} from './_tickets.js';

export default async function handle(command, ctx) {
  const p = payloadOf(command);
  const caseId = requiredString(p, 'case_id');
  const summary = payloadString(p, 'summary') ?? null;
  const time = clock(ctx);
  const { before, after } = await ctx.transaction(async (tx) => {
    const caseDoc = await loadCase(tx, caseId);
    enforceCloseTransition(caseDoc, await closureEvidence(tx, caseId));
    await setCaseState(tx, caseDoc, 'closed', time, { closed_at: time.iso });
    return { before: caseDoc, after: await tx.get(COLLECTIONS.cases, caseId) };
  });
  await recordAudit(ctx, time, {
    case: before, ticket_key: before.ticket_key, actor_type: 'control_plane', action_type: 'case_closed', details: { summary },
  });
  return caseOutcome(after);
}
