// Origin: CTOX
// License: Apache-2.0
// ctox.ticket.execute – ref: mission/tickets/cases.rs:1291-1296 → record_execution_action (cases.rs:415-464)
// Nur ausführbare Fälle; Zustand → executing. Die Ausführungsnotiz (Rust: ticket_execution_actions, nie gelesen)
// steht am Fall als last_execution_action und im Audit.
import { payloadOf, requiredString } from './_base.js';
import {
  COLLECTIONS, caseOutcome, clock, enforceStateTransition, ensureCaseIsExecutable, loadCase, recordAudit, seedOf,
  setCaseState, stableDigest,
} from './_tickets.js';

export default async function handle(command, ctx) {
  const p = payloadOf(command);
  const caseId = requiredString(p, 'case_id');
  const summary = requiredString(p, 'summary');
  const time = clock(ctx);
  const actionId = `execution:${caseId}:${await stableDigest(seedOf(command))}`;
  const { before, after } = await ctx.transaction(async (tx) => {
    const caseDoc = await loadCase(tx, caseId);
    ensureCaseIsExecutable(caseDoc);
    enforceStateTransition(caseDoc, 'executing');
    await setCaseState(tx, caseDoc, 'executing', time, {
      last_execution_action: { action_id: actionId, summary, created_at: time.iso },
    });
    return { before: caseDoc, after: await tx.get(COLLECTIONS.cases, caseId) };
  });
  await recordAudit(ctx, time, {
    case: before, ticket_key: before.ticket_key, actor_type: 'agent', action_type: 'execution_case', details: { summary },
  });
  return caseOutcome(after);
}
