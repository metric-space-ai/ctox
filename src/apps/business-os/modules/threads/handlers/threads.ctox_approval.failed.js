// threads.ctox_approval.failed – Rückruf-Befehl (Kern §16.30 on_failed), kein UI-Sender.
// payload {task_id, status, result, error?, workspace_refs?}; idempotent bei Doppelzustellung (siehe _agent.js).
// Abbruch kommt mit status 'cancelled' und wird als beendete Arbeit geschrieben.
import { settleApprovalCallback } from './_agent.js';

export default async function handle(command, ctx) {
  return settleApprovalCallback(command, ctx, 'failed');
}
