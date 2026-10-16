// ctox.knowledge.book.create – Knopf der Knowledge-App → Agenten-Auftrag (Kern §16.30, Strang SKL-B2).
// Template prompts/book-create.md, Skill knowledge/knowledge-maintenance, Rückruf knowledge.task.complete / .failed.
// Vorher ohne Handler (generischer Weg Befehl → Queue-Task, store.rs:26700 create_ctox_queue_task).
import { enqueueKnowledgeTask } from './_lib/agent.js';

export default async function handle(command, ctx) {
  return enqueueKnowledgeTask(command, ctx);
}
