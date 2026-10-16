// web_stack.person_research.failed — Rückruf des Kerns (on_failed, auch Abbruch status 'cancelled') der Personenrecherche
// Phase A: Lead failed (ref person_research_command.rs:2075), Fence je Lead frei. Idempotent je task_id, wirkungslos, wenn der
// Lead an einem neueren Auftrag hängt. Logik: _lib/person-research.js.
import { handlePersonResearchFailed } from './_lib/person-research.js';

export default async function handle(command, ctx) {
  return handlePersonResearchFailed(command, ctx);
}
