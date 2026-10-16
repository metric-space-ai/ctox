// threads.ai.complete – Rückruf-Befehl (Kern §16.30 on_complete), kein UI-Sender.
// payload {task_id, status, result, error?, workspace_refs?}; idempotent bei Doppelzustellung (siehe _agent.js).
// Antwort wird als Nachricht (Autor CTOX) in den Thread geschrieben.
import { settleAiRequest } from './_agent.js';

export default async function handle(command, ctx) {
  return settleAiRequest(command, ctx, 'completed');
}
