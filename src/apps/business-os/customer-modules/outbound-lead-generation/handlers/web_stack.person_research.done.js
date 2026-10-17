// web_stack.person_research.done — Rückruf des Kerns (on_complete, §16.30) der Personenrecherche Phase A: JSON-Block der
// Schlussantwort → Phase-A-Ergebnis → Sellify-Grundlinie, Lückenaufgabe, Lead-Projektion (ref person_research_command.rs
// :402/:463, gap_closure.rs:755); gibt den Fence je Lead frei. Idempotent je task_id. Logik: _lib/person-research.js.
import { handlePersonResearchDone } from './_lib/person-research.js';

export default async function handle(command, ctx) {
  return handlePersonResearchDone(command, ctx);
}
