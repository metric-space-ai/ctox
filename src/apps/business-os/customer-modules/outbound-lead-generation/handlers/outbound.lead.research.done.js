// outbound.lead.research.done — Rückruf des Kerns (on_complete, §16.30) für Lead-Recherche und Lückenschluss: JSON-Block der
// Schlussantwort → outbound.lead.research_writeback (gebunden an Task und Recherchebefehl). Idempotent je task_id.
import { handleResearchDone } from './_lib/lead-research.js';

export default async function handle(command, ctx) {
  return handleResearchDone(command, ctx);
}
