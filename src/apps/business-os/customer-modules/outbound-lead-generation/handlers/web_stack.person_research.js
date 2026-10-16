// PORTIERT (Strang OLG3, Agentenauftrag seit OLG4): web_stack.person_research
// ref: command_plane.rs:1744 -> person_research_command.rs:59 start / :271 spawn_worker
// Prüfungen, Fence je Lead und Phase A als Agentenauftrag (prompts/person-research.md, Paket-Skill
// outbound-lead-generation/person-research); Auswertung im Rückruf web_stack.person_research.done. Logik: _lib/person-research.js.
import { handlePersonResearch } from './_lib/person-research.js';

export default async function handle(command, ctx) {
  return handlePersonResearch(command, ctx);
}
