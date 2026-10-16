// Origin: CTOX
// License: Apache-2.0
// support.agent.task – „CTOX fragen“ (Strang SKL-A1): Template prompts/agent-task.md → agent.task.enqueue mit Skill
// support/support-workflow, Abschluss über support.agent.task.complete / .failed (Logik in _agent-task.js).
// Ersetzt den rohen Kernbefehl business_os.chat.task (vorher support-commands.mjs buildSupportAgentTaskCommand).
import { enqueueAgentTask } from './_agent-task.js';

export default async function handle(command, ctx) {
  return enqueueAgentTask(command, ctx);
}
