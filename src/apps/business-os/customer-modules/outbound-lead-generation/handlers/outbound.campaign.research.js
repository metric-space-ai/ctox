// outbound.campaign.research — Kampagnenrecherche (Strang SKL-C): der Handler fächert je Lead einen outbound.lead.research auf
// (idempotency_key je Lauf und Lead); der frühere Agenten-Planer (campaignResearchPrompt, MCP-Fan-out) entfällt.
import { handleCampaignResearch } from './_lib/lead-research.js';

export default async function handle(command, ctx) {
  return handleCampaignResearch(command, ctx);
}
