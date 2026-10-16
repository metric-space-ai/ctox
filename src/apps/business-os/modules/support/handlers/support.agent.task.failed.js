// Origin: CTOX
// License: Apache-2.0
// support.agent.task.failed – on_failed-Befehl von support.agent.task (Kern §16.30), kein UI-Sender.
// payload {task_id, status: 'failed'|'cancelled', error?}; Auftrag failed bzw. cancelled, idempotent (_agent-task.js).
import { failAgentTask } from './_agent-task.js';

export default async function handle(command, ctx) {
  return failAgentTask(command, ctx);
}
