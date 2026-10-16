// PORTIERT (Strang OLG3): outbound.lead.research_writeback
// ref: command_plane.rs:1746 -> person_research_gap_closure.rs:1745 handle_research_writeback
// Fachlogik vollständig in JS (_lib/research-*.js); native Fähigkeiten über ctx.cap (V4-PORT.md §14, §15a).
import { handleResearchWriteback } from './_lib/research-writeback.js';

export default async function handle(command, ctx) {
  return handleResearchWriteback(command, ctx);
}
