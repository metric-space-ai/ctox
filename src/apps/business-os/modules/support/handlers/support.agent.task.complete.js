// Origin: CTOX
// License: Apache-2.0
// support.agent.task.complete – on_complete-Befehl von support.agent.task (Kern §16.30), kein UI-Sender.
// payload {task_id, status, result, workspace_refs?}; schreibt die Vorschläge in support_agent_suggestions,
// idempotent bei Doppelzustellung (Logik in _agent-task.js).
import { completeAgentTask } from './_agent-task.js';

export default async function handle(command, ctx) {
  return completeAgentTask(command, ctx);
}
