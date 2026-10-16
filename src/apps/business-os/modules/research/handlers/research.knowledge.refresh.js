// research.knowledge.refresh – Knowledge Book aus einem abgeschlossenen Research-Lauf (Kern §16.30).
// Vorher: Queue-Aufnahme mit dem Phantom-Skill `knowledge` (store.rs suggested_skill_for_command :27751) und ohne
// Abschluss in der App. Jetzt: prompts/knowledge-refresh.md, Skill research/knowledge-refresh aus dem Paket,
// Rückruf research.knowledge.refresh.complete / .failed.
import { enqueueKnowledgeRefresh } from './_lib/enqueue.js';

export default async function handle(command, ctx) {
  return enqueueKnowledgeRefresh(command, ctx);
}
