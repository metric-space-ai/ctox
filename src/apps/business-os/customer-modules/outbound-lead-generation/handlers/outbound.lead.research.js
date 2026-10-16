// outbound.lead.research — Lead-Nach-/Neurecherche (Strang SKL-C): Template prompts/lead-research*.md füllen, Agenten-Task
// mit Paket-Skill outbound-lead-generation/research anlegen, Rückruf outbound.lead.research.done/.failed. Logik: _lib/lead-research.js.
import { handleLeadResearch } from './_lib/lead-research.js';

export default async function handle(command, ctx) {
  return handleLeadResearch(command, ctx);
}
