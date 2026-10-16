// knowledge.task.failed – on_failed-Rückruf der Knowledge-Aufträge (Kern §16.30 Nr. 3), auch Abbruch
// (status 'cancelled'). Vermerkt den Stand in knowledge_tasks; die App meldet ihn in der Zustandszone. Idempotent.
import { callbackOf, failKnowledgeTask } from './_lib/callback.js';

export default async function handle(command, ctx) {
  return failKnowledgeTask(ctx, callbackOf(command));
}
