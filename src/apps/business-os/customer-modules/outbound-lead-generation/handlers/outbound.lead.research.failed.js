// outbound.lead.research.failed — Rückruf des Kerns (on_failed, auch Abbruch status 'cancelled'): Lead-Fehlerstand ohne
// Ergebnisverlust, nur wenn der Lead noch an diesem Task hängt. Idempotent je task_id.
import { handleResearchFailed } from './_lib/lead-research.js';

export default async function handle(command, ctx) {
  return handleResearchFailed(command, ctx);
}
