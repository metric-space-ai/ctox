// kundenpipeline.decision.resolve – Port aus decision_hub.rs handle_decision_resolve :292
// und resolve_agent_decision :441. Nimmt genau eine deklarierte Option einer
// agent_escalation-Entscheidung an; Wiederholung mit gleicher Antwort ist idempotent,
// abweichende Antwort -> conflict, abgelaufen -> expired.

import { resolveAgentDecision } from './_lib/hub.js';

export default async function handle(command, ctx) {
  return resolveAgentDecision(ctx, command.payload ?? {}, command);
}
