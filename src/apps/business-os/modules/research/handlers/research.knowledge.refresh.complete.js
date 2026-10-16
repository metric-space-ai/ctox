// research.knowledge.refresh.complete – on_complete-Rückruf des Knowledge-Auftrags (Kern §16.30 Nr. 3).
// Liest knowledge-refresh/result.json (Skill research/knowledge-refresh) über agent.task.workspace.read, prüft
// Kennungen und Quellen-Lineage fail-closed, legt das Book im Format ctox.knowledge.book.v1 als files-Datensatz an
// und übergibt es per ctx.command('knowledge.book.ingest', {book:{ref}}) an die App knowledge (Eigentümerin der
// Wissensdaten, Strang KNOW2). research_notes behält nur den Verweis (kind knowledge_book), der Stand steht in
// research_tasks.payload.knowledge_refresh. Idempotent bei Doppelzustellung.
import { callbackOf, completeKnowledgeRefresh } from './_lib/callback.js';

export default async function handle(command, ctx) {
  return completeKnowledgeRefresh(ctx, callbackOf(command));
}
