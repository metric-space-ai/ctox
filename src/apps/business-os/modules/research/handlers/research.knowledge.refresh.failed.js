// research.knowledge.refresh.failed – on_failed-Rückruf des Knowledge-Auftrags (Kern §16.30 Nr. 3), auch Abbruch
// (status 'cancelled'). Vermerkt den Stand in research_tasks.payload.knowledge_refresh. Idempotent.
import { callbackOf, failKnowledgeRefresh } from './_lib/callback.js';

export default async function handle(command, ctx) {
  return failKnowledgeRefresh(ctx, callbackOf(command));
}
