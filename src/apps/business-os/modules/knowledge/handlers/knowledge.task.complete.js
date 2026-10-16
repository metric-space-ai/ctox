// knowledge.task.complete – on_complete-Rückruf der Knowledge-Aufträge (Kern §16.30 Nr. 3), kein UI-Sender.
// Liest knowledge/writeback.json (Skill knowledge/knowledge-maintenance), wendet es auf knowledge_items /
// knowledge_runbooks an und schließt den Auftrag in knowledge_tasks ab. Idempotent bei Doppelzustellung.
import { callbackOf, completeKnowledgeTask } from './_lib/callback.js';

export default async function handle(command, ctx) {
  return completeKnowledgeTask(ctx, callbackOf(command));
}
