// threads.ctox_approval.complete – Rückruf-Befehl (Kern §16.30 on_complete), kein UI-Sender.
// payload {task_id, status, result, error?, workspace_refs?}; idempotent bei Doppelzustellung (siehe _agent.js).
// Antwort wird als Nachricht (Autor CTOX) in den Thread geschrieben.
import { settleApprovalCallback } from './_agent.js';

export default async function handle(command, ctx) {
  return settleApprovalCallback(command, ctx, 'completed');
}
